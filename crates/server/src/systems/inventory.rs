//! Inventory, equipment, ground items and drops. OWNER: inventory system (P1, Opus C).
//! Port of InvUsuario.bas and Modulo_InventANDobj.bas (InvNpc); rules in `ao_rules::items`.
use crate::world::inventory::{InvItem, MAX_INVENTORY_SLOTS};
use crate::world::World;
use ao_formats::csm::{BLOCK_ALL, FLAG_WATER};
use ao_formats::dat::obj::{ObjData, ObjType};
use ao_protocol::{Tone, S2C};
use ao_rules::items::{self, potion, Role, GOLD_OBJ, MAX_FOOD, MAX_STACK};

/// Pirates may throw gold into the water (TirarOro).
const PIRATE: u8 = 10;
/// Key of the generic use interval in `inv.cooldowns` (obj.dat CD types are >= 0).
const USE_TIMER: i32 = -1;
const USE_INTERVAL_MS: u64 = 276;

/// What `CanUseObject` needs to know about a player, faction and rank included.
fn item_user(p: &crate::world::entities::Player) -> items::User<'_> {
    use ao_rules::faction::{is_chaos, is_royal};
    let f = &p.faction.rec;
    let rank = if is_royal(f.status) { f.royal_rank } else if is_chaos(f.status) { f.chaos_rank } else { 0 };
    items::User { race: p.race, class: p.class, gender: p.gender, level: p.level, skills: &p.stats.skills, faction_status: f.status, faction_rank: rank }
}

impl World {
    // ---------------------------------------------------------------- hooks (contract)

    /// Called by combat when a creature dies: drops its loot (npcs.dat drops, gold).
    /// `Npc.def` is used as the npcs.dat number until the NPC system stores the real one.
    #[allow(dead_code)]
    pub(crate) fn drop_npc_loot(&mut self, map: u16, npc: u32, killer: u32) {
        let Some(n) = self.maps.get(&map).and_then(|i| i.npcs.get(&npc)) else { return };
        let (x, y, dat) = (n.x, n.y, n.def as u32);
        self.drop_creature_loot(map, x, y, dat, killer);
    }

    /// NPCTirarOro + NpcDropObj for the npcs.dat creature `dat` that died at (x, y).
    pub(crate) fn drop_creature_loot(&mut self, map: u16, x: i32, y: i32, dat: u32, killer: u32) {
        let ao = self.ao.clone();
        let Some(def) = ao.npcs.get(dat) else { return };
        // TSAO NPCTirarOro: RandomNumber(GiveGLDMin, GiveGLDMax) * OroMult, straight into the
        // killer's purse.
        let (gmin, gmax) = def.gold_range;
        if self.player(killer).is_some() && gmin > 0 && gmax > 0 {
            let mult = self.cfg.world.gold_mult.max(1) as i32;
            let gold = self.roll(gmin.saturating_mul(mult), gmax.saturating_mul(mult)).max(0) as u32;
            // The tunics of wealth (`GoldBonusPct`, systems/creatures.rs).
            let gold = self.gold_with_items(killer, gold);
            let p = self.player_mut(killer);
            p.inv.gold = p.inv.gold.saturating_add(gold);
            self.send_inventory_vitals(killer);
            self.console(killer, format!("Has ganado {gold} monedas de oro."), Tone::Good);
        }
        // TSAO NPC_TIRAR_ITEMS: each `ObjN=obj-amount-prob` of a creature falls with chance
        // `prob * (2 + DropMult)` in 200, moved by the killer's charisma (18 or less -1,
        // 20 +1, 21 +2, 22+ +(CHA - 21)). The VB drew that many distinct numbers and
        // compared one roll against them; the chance is the same.
        if def.trades != 1 && self.player(killer).is_some() {
            let cha = self.player(killer).map_or(18, |p| p.stats.attributes[ao_rules::character::ATTR_CHARISMA] as i32);
            let mult = self.cfg.world.drop_mult.max(1) as i32;
            let luck = self.drop_luck(killer);
            for it in def.inventory.iter().filter(|i| i.drop_prob > 0 && i.obj > 0 && i.amount > 0) {
                let mut n = (it.drop_prob * 2 + it.drop_prob * mult).min(200);
                n += match cha {
                    c if c <= 18 => -1,
                    19 => 0,
                    20 => 1,
                    21 => 2,
                    c => c - 21,
                };
                // The tunics of luck (`DropLuck`): `If NumerosUsados < 200 Then + 3`.
                if n < 200 {
                    n += luck;
                }
                if ao.objs.get(it.obj as u32).is_some() && self.roll(1, 200) <= n.clamp(0, 200) {
                    self.drop_to_floor(map, x, y, InvItem { obj: it.obj as u16, amount: it.amount as u32 }, false);
                }
            }
        }
        // NPCTirarOro: only when a player killed it; stacks of MaxInventoryObjs.
        if self.player(killer).is_some() && def.gold > 0 {
            let mut gold = def.gold as u64 * self.cfg.world.gold_mult.max(1) as u64;
            while gold > 0 {
                let chunk = gold.min(MAX_STACK as u64) as u32;
                gold -= chunk as u64;
                self.drop_to_floor(map, x, y, InvItem { obj: GOLD_OBJ, amount: chunk }, false);
            }
        }
        // NpcDropObj: each drop falls with probability 1 in (DropChance / DropMult).
        let drop_mult = self.cfg.world.drop_mult.max(1) as i32;
        for d in &def.drops {
            let one_in = (d.chance / drop_mult).max(1);
            if d.chance <= 0 || d.obj <= 0 || ao.objs.get(d.obj as u32).is_none() {
                continue;
            }
            if self.rng.range(1, one_in) != 1 {
                continue;
            }
            let (lo, hi) = (d.min_quantity.min(d.max_quantity), d.min_quantity.max(d.max_quantity));
            let amount = self.rng.range(lo, hi).min(MAX_STACK as i32);
            if amount > 0 {
                self.drop_to_floor(map, x, y, InvItem { obj: d.obj as u16, amount: amount as u32 }, false);
            }
        }
    }

    /// Called when a player enters the world: re-equips `inv.pending_equipped` deciding
    /// each slot's role (weapon, armor...) from obj.dat, then sends the inventory, the gold
    /// and the objects on the floor of the map.
    pub(crate) fn restore_equipment(&mut self, id: u32) {
        if self.player(id).is_none() {
            return;
        }
        let mut pending = std::mem::take(&mut self.player_mut(id).inv.pending_equipped);
        // The backpack first: it unlocks the slots above 24.
        let ao = self.ao.clone();
        pending.sort_by_key(|s| {
            let obj = self.player(id).and_then(|p| p.inv.get(*s)).map_or(0, |i| i.obj);
            ao.objs.get(obj as u32).is_none_or(|o| o.obj_type != ObjType::Backpack)
        });
        for slot in pending {
            self.equip(id, slot, true);
        }
        self.send_inventory(id);
        self.send_inventory_vitals(id);
        self.send_ground_items(id);
    }

    /// Visible look of a player from its equipment: (body, head, weapon anim, shield
    /// anim, helmet anim), as obj.dat defines them. Used by `send_look`.
    pub(crate) fn look_of(&self, id: u32) -> (u16, u16, u16, u16, u16) {
        let Some(p) = self.player(id) else { return (0, 0, 0, 0, 0) };
        let anim = |slot: Option<u8>, f: fn(&ObjData) -> i32| -> u16 {
            slot.and_then(|s| p.inv.get(s)).and_then(|i| self.ao.objs.get(i.obj as u32)).map_or(0, |o| f(o).clamp(0, u16::MAX as i32) as u16)
        };
        let e = &p.inv.equipped;
        let weapon = match anim(e.weapon, |o| o.weapon_anim) {
            0 => anim(e.tool, |o| o.weapon_anim),
            w => w,
        };
        (p.body, p.head, weapon, anim(e.shield, |o| o.shield_anim), anim(e.helmet, |o| o.helmet_anim))
    }

    /// UserDie -> TirarTodosLosItems + the Desequipar calls after it. Called by combat
    /// when a player dies. Safe maps keep everything (stand-in for `MapInfo.DropItems`).
    #[allow(dead_code)]
    pub(crate) fn drop_inventory_on_death(&mut self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let (map, x, y, level, gold) = (p.map, p.x, p.y, p.level, p.inv.gold);
        let safe = self.maps.get(&map).is_none_or(|i| i.map.safe);
        if !safe {
            // The wallet keeps OroPorNivelBilletera per level; the rest falls.
            let keep = items::WALLET_GOLD_PER_LEVEL.saturating_mul(level);
            if gold > keep {
                self.throw_gold(id, gold - keep);
            }
            let ao = self.ao.clone();
            let newbie = level <= self.data.balance.newbie_max_level;
            for slot in 1..=MAX_INVENTORY_SLOTS as u8 {
                let Some(item) = self.player(id).and_then(|p| p.inv.get(slot)) else { continue };
                let Some(o) = ao.objs.get(item.obj as u32) else { continue };
                if !items::falls_on_death(o) || (newbie && o.newbie == 1) {
                    continue;
                }
                // DropAmmount: the amulet that protects ores/wood (EfectoMagico 12) is not
                // modelled yet, so the whole stack falls.
                match self.free_tile(map, x, y, item, false) {
                    Some((tx, ty)) => self.drop_obj(id, slot, item.amount, map, tx, ty),
                    None => self.remove_from_slot(id, slot, item.amount),
                }
            }
        }
        // UserDie unequips everything but the backpack and the ship.
        let equipped: Vec<u8> = self
            .player(id)
            .map(|p| {
                let e = p.inv.equipped;
                [e.armor, e.weapon, e.shield, e.helmet, e.ring, e.tool, e.saddle, e.ammo, e.amulet].into_iter().flatten().collect()
            })
            .unwrap_or_default();
        for slot in equipped {
            self.unequip(id, slot);
        }
        self.send_look(id);
    }

    /// Development command `/dar <obj> <amount>` (only with `cfg.dev_login`): puts objects
    /// in the inventory. Returns false if refused. The chat system hooks it up.
    #[allow(dead_code)]
    pub(crate) fn dev_give(&mut self, id: u32, obj: u16, amount: u32) -> bool {
        if !self.cfg.dev_login || self.player(id).is_none() || amount == 0 || amount > MAX_STACK || self.ao.objs.get(obj as u32).is_none() {
            return false;
        }
        self.add_item(id, InvItem { obj, amount })
    }

    // ---------------------------------------------------------------- output

    pub(crate) fn send_slot(&self, id: u32, slot: u8) {
        let Some(p) = self.player(id) else { return };
        let item = p.inv.get(slot);
        self.send(id, &S2C::InventorySlot {
            slot,
            obj: item.map_or(0, |i| i.obj),
            amount: item.map_or(0, |i| i.amount),
            equipped: item.is_some() && p.inv.equipped.is_equipped(slot),
        });
    }

    /// Every slot (UpdateUserInv with UpdateAll).
    pub(crate) fn send_inventory(&self, id: u32) {
        for slot in 1..=MAX_INVENTORY_SLOTS as u8 {
            self.send_slot(id, slot);
        }
    }

    /// `S2C::Vitals` (stamina, hunger, thirst and gold): WriteUpdateGold /
    /// WriteUpdateHungerAndThirst. Named apart from progression's vitals on purpose.
    pub(crate) fn send_inventory_vitals(&self, id: u32) {
        if let Some(p) = self.player(id) {
            let s = &p.stats;
            self.send(id, &S2C::Vitals { sta: s.sta, max_sta: s.max_sta, food: s.food, water: s.water, gold: p.inv.gold });
        }
    }

    /// The objects on the player's current map. Called on login; the session should also
    /// call it from `enter_map` (teleports).
    pub(crate) fn send_ground_items(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let Some(inst) = self.maps.get(&p.map) else { return };
        let mut tiles: Vec<_> = inst.ground.tiles.iter().collect();
        tiles.sort_unstable_by_key(|(k, _)| **k);
        for ((x, y), it) in tiles {
            self.send(id, &S2C::GroundItem { x: *x, y: *y, obj: it.obj, amount: it.amount });
        }
    }

    // ---------------------------------------------------------------- inventory core

    /// GetSlotForItemInInventory: a stack with room, else the first empty slot.
    fn slot_for(&self, id: u32, item: InvItem) -> Option<u8> {
        let inv = &self.player(id)?.inv;
        let mut empty = None;
        for slot in 1..=inv.usable_slots() {
            match inv.get(slot) {
                None if empty.is_none() => empty = Some(slot),
                Some(it) if it.obj == item.obj && it.amount + item.amount <= MAX_STACK => return Some(slot),
                _ => {}
            }
        }
        empty
    }

    /// MeterItemEnInventario: gold goes to the purse, the rest to a slot.
    pub(crate) fn add_item(&mut self, id: u32, item: InvItem) -> bool {
        if self.player(id).is_none() || item.amount == 0 {
            return false;
        }
        if item.obj == GOLD_OBJ {
            let p = self.player_mut(id);
            p.inv.gold = p.inv.gold.saturating_add(item.amount);
            self.send_inventory_vitals(id);
            return true;
        }
        let Some(slot) = self.slot_for(id, item) else {
            self.console(id, "No podes cargar mas objetos.", Tone::Combat);
            return false;
        };
        let cell = self.player_mut(id).inv.cell_mut(slot).unwrap();
        let amount = cell.map_or(0, |c| c.amount) + item.amount;
        *cell = Some(InvItem { obj: item.obj, amount: amount.min(MAX_STACK) });
        self.send_slot(id, slot);
        true
    }

    /// QuitarUserInvItem + UpdateUserInv: an emptied equipped stack is unequipped first.
    pub(crate) fn remove_from_slot(&mut self, id: u32, slot: u8, amount: u32) {
        let Some(item) = self.player(id).and_then(|p| p.inv.get(slot)) else { return };
        let unequipped = item.amount <= amount && self.player(id).unwrap().inv.equipped.is_equipped(slot);
        if unequipped {
            self.unequip(id, slot);
        }
        let cell = self.player_mut(id).inv.cell_mut(slot).unwrap();
        *cell = (item.amount > amount).then(|| InvItem { obj: item.obj, amount: item.amount - amount });
        self.send_slot(id, slot);
        if unequipped {
            self.send_look(id);
        }
    }

    // ---------------------------------------------------------------- equipment

    /// Desequipar: frees the equipment piece that uses `slot`.
    pub(crate) fn unequip(&mut self, id: u32, slot: u8) {
        if self.player(id).is_none() {
            return;
        }
        let prof = ao_formats::profile::for_recursos(&self.recursos);
        let p = self.player_mut(id);
        if !p.inv.equipped.is_equipped(slot) {
            return;
        }
        if p.inv.equipped.armor == Some(slot) {
            // SetNakedBody.
            p.body = prof.naked_body(p.race, p.gender);
        }
        for f in p.inv.equipped.fields_mut() {
            if *f == Some(slot) {
                *f = None;
            }
        }
        self.send_slot(id, slot);
    }

    /// EquiparInvItem. `logging_in` re-equips saved equipment: no toggling off and no
    /// look broadcast (the session sends the look after).
    pub(crate) fn equip(&mut self, id: u32, slot: u8, logging_in: bool) {
        let ao = self.ao.clone();
        let Some(p) = self.player(id) else { return };
        let Some(item) = p.inv.get(slot) else { return };
        let Some(o) = ao.objs.get(item.obj as u32) else {
            return self.console(id, "Ese objeto no existe.", Tone::Warn);
        };
        let user = item_user(p);
        if let Err(why) = items::can_use(o, &user) {
            return self.console(id, why.message(), Tone::Info);
        }
        let Some(role) = items::role_of(o.obj_type) else {
            return self.console(id, "Ese objeto no se puede equipar.", Tone::Info);
        };
        let (race, gender) = (p.race, p.gender);
        let e = p.inv.equipped;
        if e.is_equipped(slot) {
            if !logging_in {
                self.unequip(id, slot);
                self.send_look(id);
            }
            return;
        }
        let weapon_two_handed = e.weapon.and_then(|s| p.inv.get(s)).and_then(|i| ao.objs.get(i.obj as u32)).is_some_and(|w| w.two_handed == 1);
        let mut body = None;
        match role {
            Role::Weapon => {
                for s in [e.weapon, e.tool].into_iter().flatten() {
                    self.unequip(id, s);
                }
                if o.two_handed == 1 {
                    if let Some(s) = e.shield {
                        self.unequip(id, s);
                        self.console(id, "No puedes usar armas de dos manos si tienes un escudo equipado.", Tone::Warn);
                    }
                }
            }
            Role::Tool => {
                for s in [e.tool, e.weapon].into_iter().flatten() {
                    self.unequip(id, s);
                }
            }
            Role::Shield => {
                if let Some(s) = e.shield {
                    self.unequip(id, s);
                }
                if weapon_two_handed {
                    self.unequip(id, e.weapon.unwrap());
                    self.console(id, "No puedes equipar un escudo si tienes un arma de dos manos.", Tone::Warn);
                }
            }
            Role::Armor | Role::Backpack => {
                let ropaje = items::clothing_for(o, race, gender);
                if ropaje <= 0 {
                    return self.console(id, "Hay un error con este objeto, informale a un administrador.", Tone::Warn);
                }
                if let Some(s) = *self.player_mut(id).inv.equipped.role_mut(role) {
                    self.unequip(id, s);
                }
                if role == Role::Armor {
                    body = Some(ropaje as u16);
                }
            }
            _ => {
                if let Some(s) = *self.player_mut(id).inv.equipped.role_mut(role) {
                    self.unequip(id, s);
                }
            }
        }
        let p = self.player_mut(id);
        *p.inv.equipped.role_mut(role) = Some(slot);
        if let Some(b) = body {
            p.body = b;
        }
        self.send_slot(id, slot);
        if !logging_in {
            self.send_look(id);
        }
    }

    // ---------------------------------------------------------------- floor

    /// MakeObj: adds a stack to a tile (merging the same object) and tells the map.
    pub(crate) fn make_obj(&mut self, map: u16, x: i32, y: i32, item: InvItem) {
        let Some(inst) = self.maps.get_mut(&map) else { return };
        let cell = inst.ground.tiles.entry((x, y)).or_insert(InvItem { obj: item.obj, amount: 0 });
        if cell.obj == item.obj {
            cell.amount = (cell.amount + item.amount).min(MAX_STACK);
        } else {
            *cell = item;
        }
        let it = *cell;
        let now = self.now();
        self.maps.get_mut(&map).unwrap().ground.dropped_at.insert((x, y), now);
        self.broadcast(map, &S2C::GroundItem { x, y, obj: it.obj, amount: it.amount }, None);
    }

    /// Floor cleanup (an improvement: the official server keeps everything forever): once a
    /// minute, stacks dropped more than `floor_cleanup_minutes` ago vanish.
    pub(crate) fn tick_floor_cleanup(&mut self, now: crate::clock::GameTime) {
        let minutes = self.cfg.world.floor_cleanup_minutes;
        if minutes == 0 || now < self.next_floor_cleanup {
            return;
        }
        self.next_floor_cleanup = now.after_secs(60);
        let ttl = minutes * 60_000;
        let maps: Vec<u16> = self.maps.keys().copied().collect();
        for map in maps {
            let expired: Vec<(i32, i32)> = self.maps[&map].ground.dropped_at.iter().filter(|(_, t)| now.since(**t) >= ttl).map(|(k, _)| *k).collect();
            for (x, y) in expired {
                let g = &mut self.maps.get_mut(&map).unwrap().ground;
                g.dropped_at.remove(&(x, y));
                if g.tiles.remove(&(x, y)).is_some() {
                    self.broadcast(map, &S2C::GroundItemRemoved { x, y }, None);
                }
            }
        }
    }

    /// EraseObj: takes `amount` from a tile; an emptied tile is removed for everyone.
    pub(crate) fn erase_obj(&mut self, map: u16, x: i32, y: i32, amount: u32) {
        let Some(inst) = self.maps.get_mut(&map) else { return };
        let Some(cell) = inst.ground.tiles.get_mut(&(x, y)) else { return };
        if cell.amount > amount {
            cell.amount -= amount;
            let it = *cell;
            self.broadcast(map, &S2C::GroundItem { x, y, obj: it.obj, amount: it.amount }, None);
        } else {
            inst.ground.tiles.remove(&(x, y));
            inst.ground.dropped_at.remove(&(x, y));
            self.broadcast(map, &S2C::GroundItemRemoved { x, y }, None);
        }
    }

    /// LegalPos for floor objects: inside the border, not a wall, no character on it,
    /// water only if allowed.
    fn legal_floor(&self, map: u16, x: i32, y: i32, water: bool) -> bool {
        let Some(inst) = self.maps.get(&map) else { return false };
        if !ao_rules::walk::in_bounds(x, y) {
            return false;
        }
        let b = inst.map.blocked(x, y);
        b & BLOCK_ALL != BLOCK_ALL && (water || b & FLAG_WATER == 0) && !self.occupied(map, x, y)
    }

    /// Tilelibre: the closest legal tile (rings up to 15) whose floor is empty or holds
    /// the same object with room, and is not a map exit.
    pub(crate) fn free_tile(&self, map: u16, x: i32, y: i32, item: InvItem, water: bool) -> Option<(i32, i32)> {
        let inst = self.maps.get(&map)?;
        for r in 0..=items::FREE_TILE_RINGS {
            for ty in y - r..=y + r {
                for tx in x - r..=x + r {
                    if !self.legal_floor(map, tx, ty, water) || inst.map.exit(tx, ty).is_some() {
                        continue;
                    }
                    let fits = inst.ground.at(tx, ty).is_none_or(|g| g.obj == item.obj && g.amount + item.amount <= MAX_STACK);
                    if fits {
                        return Some((tx, ty));
                    }
                }
            }
        }
        None
    }

    /// TirarItemAlPiso: a stack on the closest free tile. Returns where it fell.
    pub(crate) fn drop_to_floor(&mut self, map: u16, x: i32, y: i32, item: InvItem, water: bool) -> Option<(i32, i32)> {
        let (tx, ty) = self.free_tile(map, x, y, item, water)?;
        self.make_obj(map, tx, ty, item);
        Some((tx, ty))
    }

    /// DropObj: `num` units of `slot` onto (x, y), if the floor there takes them.
    fn drop_obj(&mut self, id: u32, slot: u8, num: u32, map: u16, x: i32, y: i32) {
        let Some(item) = self.player(id).and_then(|p| p.inv.get(slot)) else { return };
        let num = num.min(item.amount);
        if num == 0 {
            return;
        }
        let destroys = self.ao.objs.get(item.obj as u32).is_some_and(|o| o.destroys != 0);
        if !destroys {
            let ground = self.maps.get(&map).and_then(|i| i.ground.at(x, y));
            let fits = ground.is_none_or(|g| g.obj == item.obj && g.amount + num <= MAX_STACK);
            if !fits {
                return self.console(id, "No hay espacio en el piso.", Tone::Warn);
            }
            self.make_obj(map, x, y, InvItem { obj: item.obj, amount: num });
        }
        self.remove_from_slot(id, slot, num);
    }

    /// TirarOro: stacks of MaxInventoryObjs onto free tiles around the player.
    fn throw_gold(&mut self, id: u32, amount: u32) {
        let Some(p) = self.player(id) else { return };
        let (map, x, y, water) = (p.map, p.x, p.y, p.class == PIRATE);
        let mut left = amount.min(p.inv.gold);
        while left > 0 {
            let chunk = left.min(MAX_STACK);
            if self.drop_to_floor(map, x, y, InvItem { obj: GOLD_OBJ, amount: chunk }, water).is_none() {
                break;
            }
            left -= chunk;
            let p = self.player_mut(id);
            p.inv.gold -= chunk;
        }
        self.send_inventory_vitals(id);
    }

    // ---------------------------------------------------------------- validation

    /// Slot number check shared by every message: 0 or above the maximum is something no
    /// honest client sends (strike); a locked slot (no backpack) is a plain refusal. An
    /// occupied slot is always reachable, so objects left above 24 after taking the
    /// backpack off are never lost: the limit only decides where new things go.
    fn checked_slot(&mut self, id: u32, slot: u8) -> bool {
        if slot == 0 || slot as usize > MAX_INVENTORY_SLOTS {
            self.strike(id, "casilla de inventario invalida");
            return false;
        }
        self.player(id).is_some_and(|p| slot <= p.inv.usable_slots() || p.inv.get(slot).is_some())
    }

    fn dead(&self, id: u32) -> bool {
        self.player(id).is_none_or(|p| p.combat.dead)
    }

    // ---------------------------------------------------------------- messages

    /// HandleUseItem -> UseInvItem.
    pub(crate) fn on_use_item(&mut self, id: u32, slot: u8) {
        if !self.checked_slot(id, slot) {
            return;
        }
        let Some(item) = self.player(id).unwrap().inv.get(slot) else { return };
        if self.dead(id) {
            return self.console(id, "Estas muerto!!", Tone::Info);
        }
        // Craft tools (pot, saw, sewing box, hammer) open their trade (UseInvItem).
        if self.craft_use_tool(id, slot) {
            return;
        }
        let ao = self.ao.clone();
        let Some(o) = ao.objs.get(item.obj as u32) else { return };
        let p = self.player(id).unwrap();
        let user = item_user(p);
        if let Err(why) = items::can_use(o, &user) {
            return self.console(id, why.message(), Tone::Info);
        }
        // IntervaloPermiteUsarClick (intervalos.ini IntervaloUserPuedeUsarClic = 276 ms).
        let now = self.now();
        if p.inv.cooldowns.get(&USE_TIMER).is_some_and(|t| now < *t) {
            return;
        }
        self.player_mut(id).inv.cooldowns.insert(USE_TIMER, now.after_ms(USE_INTERVAL_MS));
        let p = self.player(id).unwrap();
        // Per-type cooldown (CdTimes(obj.cdType) vs obj.Cooldown, in ms).
        if o.cooldown > 0 {
            if p.inv.cooldowns.get(&o.cooldown_type).is_some_and(|t| now < *t) {
                return;
            }
            self.player_mut(id).inv.cooldowns.insert(o.cooldown_type, now.after_ms(o.cooldown as u64));
        }
        let free = items::consumable_free_map(self.player(id).unwrap().map);
        match o.obj_type {
            ObjType::Ships => self.use_boat(id, slot, self.data.balance.boat_min_level),
            _ if o.tsao_type == 38 => self.read_treasure_map(id),
            _ if o.tsao_type == super::tsao_progress::TSAO_OCTARINE_GEM => self.tsao_upgrades(id),
            ObjType::Chest if o.chest_kind > 0 => self.open_chest(id, slot, o.chest_kind as u32, o.chest_key),
            ObjType::Chest => self.open_ao20_chest(id, slot, o),
            ObjType::UseOnce => {
                let s = &mut self.player_mut(id).stats;
                s.food = (s.food + o.min_hunger).min(MAX_FOOD);
                self.remove_from_slot(id, slot, 1);
                self.send_inventory_vitals(id);
            }
            ObjType::GoldCoin => {
                let p = self.player_mut(id);
                p.inv.gold = p.inv.gold.saturating_add(item.amount);
                *p.inv.cell_mut(slot).unwrap() = None;
                self.send_slot(id, slot);
                self.send_inventory_vitals(id);
            }
            ObjType::Drinks => {
                let s = &mut self.player_mut(id).stats;
                s.water = (s.water + o.min_thirst).min(MAX_FOOD);
                self.remove_from_slot(id, slot, 1);
                self.send_inventory_vitals(id);
            }
            ObjType::FullBottle => {
                let s = &mut self.player_mut(id).stats;
                s.water = (s.water + o.min_thirst).min(MAX_FOOD);
                self.remove_from_slot(id, slot, 1);
                if o.closed_index > 0 {
                    let empty = InvItem { obj: o.closed_index as u16, amount: 1 };
                    if !self.add_item(id, empty) {
                        let p = self.player(id).unwrap();
                        let (map, x, y) = (p.map, p.x, p.y);
                        self.drop_to_floor(map, x, y, empty, true);
                    }
                }
                self.send_inventory_vitals(id);
            }
            ObjType::Potions => self.use_potion(id, slot, o, free),
            ObjType::Keys => self.use_key(id, slot),
            ObjType::EmptyBottle => self.fill_bottle(id, slot, o),
            ObjType::MusicalInstruments => self.play_instrument(id, o),
            ObjType::Parchment => {
                let p = self.player(id).unwrap();
                if !items::class_can_use(o, p.class) || !items::race_can_use(o, p.race, p.gender) {
                    return self.console(id, "Por mas que lo intentas, no podes comprender el manuescrito.", Tone::Warn);
                }
                if p.stats.food <= 0 || p.stats.water <= 0 {
                    return self.console(id, "Estas demasiado hambriento y sediento.", Tone::Warn);
                }
                if self.learn_spell_from_item(id, item.obj) {
                    self.remove_from_slot(id, slot, 1);
                }
            }
            // Working tools, minerals, ships, saddles, instruments, keys, bottles to fill,
            // chests...: trades and travel (P2).
            _ => {}
        }
    }

    /// UseInvItem, `Case otPotions`.
    fn use_potion(&mut self, id: u32, slot: u8, o: &ObjData, free: bool) {
        let roll = |w: &mut World| {
            let (lo, hi) = (o.min_modifier.min(o.max_modifier), o.min_modifier.max(o.max_modifier));
            w.rng.range(lo, hi)
        };
        let consume = |w: &mut World| {
            if !free {
                w.remove_from_slot(id, slot, 1);
            }
        };
        match o.potion_type {
            potion::HP => {
                let amount = roll(self);
                self.heal_player(id, amount);
                consume(self);
            }
            potion::MP => {
                let p = self.player_mut(id);
                p.mana = (p.mana + p.max_mana * o.percentage / 100).min(p.max_mana);
                consume(self);
                self.send_stats(id);
            }
            potion::STAMINA => {
                let amount = roll(self);
                let s = &mut self.player_mut(id).stats;
                s.sta = (s.sta + amount).min(s.max_sta);
                consume(self);
                self.send_inventory_vitals(id);
            }
            potion::HEALS_POISON => {
                let st = &mut self.player_mut(id).status;
                if st.poisoned_until.take().is_some() {
                    self.console(id, "Te has curado del envenenamiento.", Tone::Good);
                    self.remove_from_slot(id, slot, 1);
                } else {
                    self.console(id, "No te encuentras envenenado.", Tone::Combat);
                }
            }
            potion::HEALS_PARALYSIS => {
                let st = &mut self.player_mut(id).status;
                if st.paralyzed_until.is_some() || st.immobilized_until.is_some() {
                    st.paralyzed_until = None;
                    st.immobilized_until = None;
                    self.remove_from_slot(id, slot, 1);
                    self.console(id, "Te has removido la paralisis.", Tone::Good);
                } else {
                    self.console(id, "No estas paralizado.", Tone::Warn);
                }
            }
            potion::HEALS_ALL => {
                self.remove_from_slot(id, slot, 1);
                let p = self.player_mut(id);
                p.status.poisoned_until = None;
                p.status.paralyzed_until = None;
                p.status.immobilized_until = None;
                p.status.blind_until = None;
                p.stats.sta = p.stats.max_sta;
                p.stats.water = MAX_FOOD;
                p.stats.food = MAX_FOOD;
                p.mana = p.max_mana;
                let full = p.max_hp;
                self.heal_player(id, full);
                self.send_inventory_vitals(id);
                self.console(id, "Te sentis sano y lleno.", Tone::Good);
            }
            potion::AGILITY | potion::STRENGTH => {
                // UserAtributos + RandomNumber(Min, Max), capped at twice the base, for
                // DuracionEfecto seconds (DuracionPociones restores the backup).
                let attr = if o.potion_type == potion::AGILITY { ao_rules::character::ATTR_AGILITY } else { ao_rules::character::ATTR_STRENGTH };
                let add = roll(self);
                let until = self.now().after_secs(o.effect_duration.max(1) as u64);
                let s = &mut self.player_mut(id).stats;
                let base = *s.attr_backup.get_or_insert(s.attributes);
                s.attributes[attr] = (s.attributes[attr] as i32 + add).min(base[attr] as i32 * 2).clamp(0, 255) as u8;
                s.potion_until = Some(until);
                consume(self);
                self.send_attributes(id);
            }
            potion::HEAD_RANDOM | potion::SEX => {
                let p = self.player(id).unwrap();
                let (race, mut gender, old) = (p.race, p.gender, p.head);
                if o.potion_type == potion::SEX {
                    gender = if gender == 1 { 2 } else { 1 };
                }
                // A random head of the race and (new) gender (the head ranges the account
                // screen also validates).
                let heads: Vec<u16> = ao_formats::heads::for_recursos(&self.recursos).heads(race, gender).collect();
                if heads.is_empty() {
                    return self.console(id, "Esta pocion no tiene efecto en vos.", Tone::Warn);
                }
                let head = heads[self.rng.range(0, heads.len() as i32 - 1) as usize];
                let p = self.player_mut(id);
                p.gender = gender;
                p.head = head;
                if head != old || o.potion_type == potion::SEX {
                    self.remove_from_slot(id, slot, 1);
                }
                self.send_look(id);
                self.console(id, if o.potion_type == potion::SEX { "Has cambiado de sexo." } else { "Has cambiado tu apariencia." }, Tone::Good);
            }
            potion::INVISIBLE => {
                let now = self.now();
                if self.player(id).unwrap().status.invisible_until.is_some_and(|u| now < u) {
                    return self.console(id, "Ya estas invisible.", Tone::Warn);
                }
                let until = now.after_secs(o.effect_duration.max(1) as u64);
                self.player_mut(id).status.invisible_until = Some(until);
                self.remove_from_slot(id, slot, 1);
                self.refresh_status(id);
                self.console(id, "Te has escondido entre las sombras...", Tone::Good);
            }
            potion::SUICIDE => {
                self.remove_from_slot(id, slot, 1);
                let hp = self.player(id).unwrap().hp;
                self.damage_player(id, hp, "una pocion negra");
            }
            // Marriage, legendary heads, particles and effects over time (types 16-23)
            // belong to systems not ported yet.
            _ => self.console(id, "Ese efecto todavia no esta disponible.", Tone::Warn),
        }
    }

    /// `otChest` of AO20 (`Subtipo`): 1 gives every item, 2 gives `CantEntrega` items at
    /// random, 3 gives each item with its own chance (`RandomNumber(1, data) = 1`).
    fn open_ao20_chest(&mut self, id: u32, slot: u8, o: &ObjData) {
        if o.contents.is_empty() {
            return self.console(id, "Este cofre esta vacio.", Tone::Info);
        }
        self.remove_from_slot(id, slot, 1);
        let p = self.player(id).unwrap();
        let (map, x, y) = (p.map, p.x, p.y);
        if o.snd1 > 0 {
            self.broadcast(map, &S2C::Sound { wav: o.snd1 as u16, x, y }, None);
        }
        let mut got: Vec<(u16, u32)> = Vec::new();
        match o.subtype {
            1 => got.extend(o.contents.iter().map(|c| (c.obj as u16, c.amount.max(1) as u32))),
            2 => {
                for _ in 0..o.deliver_count.max(1) {
                    let c = &o.contents[self.rng.range(0, o.contents.len() as i32 - 1) as usize];
                    got.push((c.obj as u16, c.amount.max(1) as u32));
                }
            }
            _ => {
                for c in &o.contents {
                    if self.rng.range(1, c.drop_roll.max(1)) == 1 {
                        got.push((c.obj as u16, c.amount.max(1) as u32));
                    }
                }
            }
        }
        if got.is_empty() {
            return self.console(id, "El cofre no tenia nada esta vez.", Tone::Info);
        }
        for (obj, amount) in got {
            let it = InvItem { obj, amount };
            if !self.add_item(id, it) {
                self.drop_to_floor(map, x, y, it, true);
            }
            let name = self.ao.objs.get(obj as u32).map(|d| d.name.clone()).unwrap_or_default();
            self.console(id, format!("{name} ({amount})"), Tone::Good);
        }
    }

    /// `otEmptyBottle`: filled with water standing within two tiles of it (the official one
    /// needs the water tile clicked first; any water that close is enough here).
    fn fill_bottle(&mut self, id: u32, slot: u8, o: &ObjData) {
        let p = self.player(id).unwrap();
        let (map, px, py) = (p.map, p.x, p.y);
        let m = &self.instance(map).map;
        let near_water = (-2..=2).any(|dy| (-2..=2).any(|dx| ao_formats::csm::on_map(px + dx, py + dy) && m.blocked(px + dx, py + dy) & ao_formats::csm::FLAG_WATER != 0));
        if !near_water {
            return self.console(id, "Debes acercarte mas al agua.", Tone::Info);
        }
        if o.open_index <= 0 {
            return;
        }
        self.remove_from_slot(id, slot, 1);
        let full = InvItem { obj: o.open_index as u16, amount: 1 };
        if !self.add_item(id, full) {
            self.drop_to_floor(map, px, py, full, true);
        }
        self.broadcast(map, &S2C::Sound { wav: 15, x: px, y: py }, None);
    }

    /// `otMusicalInstruments`: plays its sound; the faction horns (`Real` / `Caos`) only
    /// for their army, outside safe maps, and the whole map hears them.
    fn play_instrument(&mut self, id: u32, o: &ObjData) {
        let p = self.player(id).unwrap();
        let (map, x, y, status) = (p.map, p.x, p.y, self.faction_status(id) as i32);
        if o.snd1 <= 0 {
            return;
        }
        let wav = o.snd1 as u16;
        if o.royal != 0 || o.chaos != 0 {
            // Status 3 royal army, 2 chaos legion.
            let member = if o.royal != 0 { status == 3 } else { status == 2 };
            if !member {
                let who = if o.royal != 0 { "la Armada Real" } else { "la Legion Oscura" };
                return self.console(id, format!("Solo los miembros de {who} pueden usar este cuerno."), Tone::Warn);
            }
            if self.instance(map).map.safe {
                return self.console(id, "No hay peligro aqui. Es zona segura.", Tone::Info);
            }
        }
        self.broadcast(map, &S2C::Sound { wav, x, y }, None);
    }

    /// `/hogar` (`goHome`): a ghost starts the trip home; `tick_potions` brings it there.
    pub(crate) fn go_home(&mut self, id: u32) {
        if !self.is_dead(id) {
            return self.console(id, "Debes estar muerto para volver a tu hogar.", Tone::Info);
        }
        if self.player(id).is_some_and(|p| p.going_home.is_some()) {
            return self.console(id, "Ya estas viajando a tu hogar.", Tone::Info);
        }
        let gm = self.gm_rank(id) != ao_protocol::p4::gm::GmRank::User;
        let secs = if gm { 5 } else { self.ao.balance.extra.home_timer.max(1) as u64 };
        let at = self.now().after_secs(secs);
        self.player_mut(id).going_home = Some(at);
        self.console(id, format!("Viajaras a tu hogar en {secs} segundos."), Tone::Info);
    }

    /// `HomeArrival`: the home city of `Ciudades.dat` (Ullathorpe: characters have no other
    /// home city here yet).
    fn home_arrival(&mut self, id: u32) {
        self.player_mut(id).going_home = None;
        if !self.is_dead(id) {
            return; // resurrected on the way: the trip is off.
        }
        let (map, x, y) = home_city(&self.recursos);
        self.teleport(id, map, x, y);
        self.console(id, "Has llegado a tu hogar.", Tone::Info);
    }

    /// `DuracionPociones`: strength/agility potions wear off.
    pub(crate) fn tick_potions(&mut self, now: crate::clock::GameTime) {
        let due: Vec<u32> = self.sessions.iter().filter_map(|(id, s)| s.player.as_ref().filter(|p| p.stats.potion_until.is_some_and(|u| now >= u)).map(|_| *id)).collect();
        let home: Vec<u32> = self.sessions.iter().filter_map(|(id, s)| s.player.as_ref().filter(|p| p.going_home.is_some_and(|u| now >= u)).map(|_| *id)).collect();
        for id in home {
            self.home_arrival(id);
        }
        for id in due {
            let s = &mut self.player_mut(id).stats;
            s.potion_until = None;
            if let Some(b) = s.attr_backup.take() {
                s.attributes = b;
            }
            self.send_attributes(id);
            self.console(id, "El efecto de la pocion ha terminado.", Tone::Info);
        }
    }

    pub(crate) fn send_attributes(&self, id: u32) {
        if let Some(p) = self.player(id) {
            self.send(id, &S2C::Attributes { values: p.stats.attributes });
        }
    }

    /// HandleEquipItem -> EquiparInvItem.
    pub(crate) fn on_equip_item(&mut self, id: u32, slot: u8) {
        if !self.checked_slot(id, slot) {
            return;
        }
        if self.dead(id) {
            return self.console(id, "Estas muerto!! Solo podes usar items cuando estas vivo.", Tone::Combat);
        }
        if self.player(id).unwrap().inv.get(slot).is_none() {
            return;
        }
        self.equip(id, slot, false);
    }

    /// HandleDrop (item): newbie and `Intirable` objects stay; DropObj on the own tile.
    pub(crate) fn on_drop_item(&mut self, id: u32, slot: u8, amount: u32) {
        // TirarItem ends the automated work (Trabajo.bas).
        self.work_cancel(id);
        if !self.checked_slot(id, slot) || amount == 0 || self.dead(id) {
            return;
        }
        let p = self.player(id).unwrap();
        let Some(item) = p.inv.get(slot) else { return };
        let (map, x, y, level) = (p.map, p.x, p.y, p.level);
        let ao = self.ao.clone();
        let Some(o) = ao.objs.get(item.obj as u32) else { return };
        if level <= self.data.balance.newbie_max_level && o.newbie == 1 {
            return self.console(id, "No se pueden tirar los objetos Newbies.", Tone::Warn);
        }
        if o.non_droppable == 1 {
            return self.console(id, "Accion no permitida.", Tone::Warn);
        }
        self.drop_obj(id, slot, amount, map, x, y);
    }

    /// HandleDrop (gold slot) -> TirarOro: at most 100.000 per request, never more than
    /// the purse holds.
    pub(crate) fn on_drop_gold(&mut self, id: u32, amount: u32) {
        if amount == 0 || self.dead(id) {
            return;
        }
        let amount = amount.min(items::MAX_GOLD_DROP);
        if amount > self.player(id).unwrap().inv.gold {
            return self.console(id, "No tenes esa cantidad de oro.", Tone::Warn);
        }
        self.throw_gold(id, amount);
    }

    /// HandlePickUp -> PickObjAt on the own tile.
    pub(crate) fn on_pick_up(&mut self, id: u32) {
        if self.dead(id) {
            return self.console(id, "Estas muerto!!", Tone::Info);
        }
        let p = self.player(id).unwrap();
        let (map, x, y) = (p.map, p.x, p.y);
        let Some(item) = self.maps.get(&map).and_then(|i| i.ground.at(x, y)) else {
            return self.console(id, "No hay nada aqui.", Tone::Info);
        };
        // `Agarrable=1` marks objects that cannot be picked up (doors, signs, trees...).
        let ao = self.ao.clone();
        if ao.objs.get(item.obj as u32).is_some_and(|o| o.grabbable == 1) {
            return;
        }
        if self.add_item(id, item) {
            self.erase_obj(map, x, y, item.amount);
            if item.obj != GOLD_OBJ {
                let name = ao.objs.get(item.obj as u32).map_or("objeto", |o| o.name.as_str());
                self.console(id, format!("Has agarrado {} {name}.", item.amount), Tone::Info);
            }
        }
    }

    /// HandleMoveItem: same object merges (up to the stack limit), otherwise swaps;
    /// equipment follows its object.
    pub(crate) fn on_swap_slots(&mut self, id: u32, a: u8, b: u8) {
        if !self.checked_slot(id, a) || self.player(id).is_none() {
            return;
        }
        if !self.checked_slot(id, b) {
            if self.player(id).is_some() {
                self.console(id, "Espacio no desbloqueado.", Tone::Warn);
            }
            return;
        }
        if a == b {
            return;
        }
        let p = self.player_mut(id);
        let (ia, ib) = (p.inv.get(a), p.inv.get(b));
        match (ia, ib) {
            (Some(x), Some(y)) if x.obj == y.obj => {
                let total = x.amount + y.amount;
                if total > MAX_STACK {
                    *p.inv.cell_mut(b).unwrap() = Some(InvItem { obj: y.obj, amount: MAX_STACK });
                    *p.inv.cell_mut(a).unwrap() = Some(InvItem { obj: x.obj, amount: total - MAX_STACK });
                } else {
                    *p.inv.cell_mut(b).unwrap() = Some(InvItem { obj: y.obj, amount: total });
                    *p.inv.cell_mut(a).unwrap() = None;
                    for f in p.inv.equipped.fields_mut() {
                        if *f == Some(a) {
                            *f = Some(b);
                        }
                    }
                }
            }
            _ => {
                *p.inv.cell_mut(a).unwrap() = ib;
                *p.inv.cell_mut(b).unwrap() = ia;
                for f in p.inv.equipped.fields_mut() {
                    if *f == Some(a) {
                        *f = Some(b);
                    } else if *f == Some(b) {
                        *f = Some(a);
                    }
                }
            }
        }
        self.send_slot(id, a);
        self.send_slot(id, b);
    }
}

/// In-crate tests: they need objects in the inventory, which no client message can create
/// yet (`/dar` is not hooked to the chat). A fake connection drives the world directly.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Event;
    use crate::Settings;
    use ao_protocol::{FrameReader, C2S, PROTOCOL_VERSION};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    struct Conn {
        id: u32,
        rx: UnboundedReceiver<Vec<u8>>,
        reader: FrameReader,
        log: Vec<S2C>,
    }

    impl Conn {
        fn pump(&mut self) -> &[S2C] {
            while let Ok(bytes) = self.rx.try_recv() {
                self.reader.push(&bytes);
                while let Some(Ok(m)) = self.reader.next::<S2C>(1 << 20) {
                    self.log.push(m);
                }
            }
            &self.log
        }
    }

    fn connect(w: &mut World, id: u32) -> Conn {
        let (tx, rx) = unbounded_channel();
        w.on_event(Event::Connected { id, addr: "127.0.0.1:9".parse().unwrap(), tx });
        Conn { id, rx, reader: FrameReader::default(), log: Vec::new() }
    }

    fn login(w: &mut World, id: u32, name: &str) -> Conn {
        let c = connect(w, id);
        w.on_event(Event::Frame { id, msg: C2S::Login { name: name.into(), version: PROTOCOL_VERSION }, at: None });
        assert!(w.player(id).is_some(), "dev login de {name}");
        c
    }

    fn world() -> World {
        World::new(Settings::defaults())
    }

    fn user(w: &World, id: u32) -> (u8, u8, u8, u32, [u8; 24]) {
        let p = w.player(id).unwrap();
        (p.race, p.class, p.gender, p.level, p.stats.skills)
    }

    /// First obj.dat object of a type this player can use, matching `extra`.
    fn find(w: &World, id: u32, t: ObjType, extra: impl Fn(&ObjData) -> bool) -> u16 {
        let (race, class, gender, level, skills) = user(w, id);
        let u = items::User { race, class, gender, level, skills: &skills, faction_status: ao_rules::faction::CITIZEN, faction_rank: 0 };
        w.ao.objs
            .iter()
            .find(|(_, o)| o.obj_type == t && extra(o) && items::can_use(o, &u).is_ok() && o.destroys == 0 && o.non_droppable == 0 && o.newbie == 0)
            .map(|(i, _)| i as u16)
            .unwrap_or_else(|| panic!("no hay un {t:?} usable en obj.dat"))
    }

    fn slot_of(w: &World, id: u32, obj: u16) -> u8 {
        (1..=MAX_INVENTORY_SLOTS as u8).find(|s| w.player(id).unwrap().inv.get(*s).is_some_and(|i| i.obj == obj)).expect("en el inventario")
    }

    fn send(w: &mut World, id: u32, msg: C2S) {
        w.on_event(Event::Frame { id, msg, at: None });
    }

    #[test]
    fn equipping_a_weapon_changes_the_look_for_others() {
        let mut w = world();
        let mut a = login(&mut w, 1, "Armado");
        let mut b = login(&mut w, 2, "Miron");
        let sword = find(&w, 1, ObjType::Weapon, |o| o.weapon_anim > 0 && o.two_handed == 0);
        assert!(w.dev_give(1, sword, 1));
        let slot = slot_of(&w, 1, sword);
        a.pump();
        b.pump();
        send(&mut w, 1, C2S::EquipItem { slot });
        let anim = w.ao.objs.get(sword as u32).unwrap().weapon_anim as u16;
        assert!(b.pump().iter().any(|m| matches!(m, S2C::Look { id: 1, weapon, .. } if *weapon == anim)), "el otro ve el arma");
        assert!(a.pump().iter().any(|m| matches!(m, S2C::InventorySlot { slot: s, equipped: true, .. } if *s == slot)));
        // Toggling it off clears the animation.
        send(&mut w, 1, C2S::EquipItem { slot });
        assert_eq!(w.look_of(1).2, 0);
        let _ = (a.id, b.id);
    }

    #[test]
    fn armour_changes_the_body_and_back_to_naked() {
        let mut w = world();
        let _a = login(&mut w, 1, "Vestido");
        let (race, _, gender, ..) = user(&w, 1);
        let armour = find(&w, 1, ObjType::Armor, |o| items::clothing_for(o, race, gender) > 0);
        w.dev_give(1, armour, 1);
        let slot = slot_of(&w, 1, armour);
        send(&mut w, 1, C2S::EquipItem { slot });
        let body = items::clothing_for(w.ao.objs.get(armour as u32).unwrap(), race, gender) as u16;
        assert_eq!(w.look_of(1).0, body);
        send(&mut w, 1, C2S::EquipItem { slot });
        assert_eq!(w.look_of(1).0, ao_rules::character::naked_body(race, gender), "SetNakedBody al quitarla");
    }

    #[test]
    fn requirements_refuse_with_a_message_not_a_strike() {
        let mut w = world();
        let mut a = login(&mut w, 1, "Novato");
        let high = w.ao.objs.iter().find(|(_, o)| o.obj_type == ObjType::Weapon && o.min_level > 1).map(|(i, _)| i as u16).unwrap();
        w.dev_give(1, high, 1);
        let slot = slot_of(&w, 1, high);
        send(&mut w, 1, C2S::EquipItem { slot });
        assert!(!w.player(1).unwrap().inv.equipped.is_equipped(slot));
        assert!(a.pump().iter().any(|m| matches!(m, S2C::Console { text, .. } if text.contains("nivel"))));
        let now = w.now();
        assert_eq!(w.player_mut(1).strikes.add(now), 1, "sin strikes previos");
    }

    #[test]
    fn red_potion_heals_and_food_feeds() {
        let mut w = world();
        let _a = login(&mut w, 1, "Herido");
        let red = find(&w, 1, ObjType::Potions, |o| o.potion_type == potion::HP && o.min_modifier > 0);
        w.dev_give(1, red, 3);
        w.player_mut(1).hp = 1;
        let slot = slot_of(&w, 1, red);
        send(&mut w, 1, C2S::UseItem { slot });
        assert!(w.player(1).unwrap().hp > 1, "la pocion cura");
        assert_eq!(w.player(1).unwrap().inv.get(slot_of(&w, 1, red)).unwrap().amount, 2, "se consume una");

        let food = find(&w, 1, ObjType::UseOnce, |o| o.min_hunger > 0);
        w.dev_give(1, food, 1);
        w.player_mut(1).stats.food = 10;
        // Right after the potion the use interval (276 ms) refuses it.
        let slot = slot_of(&w, 1, food);
        send(&mut w, 1, C2S::UseItem { slot });
        assert_eq!(w.player(1).unwrap().stats.food, 10, "IntervaloPermiteUsarClick");
        w.player_mut(1).inv.cooldowns.clear();
        let slot = slot_of(&w, 1, food);
        send(&mut w, 1, C2S::UseItem { slot });
        assert!(w.player(1).unwrap().stats.food > 10);
        assert!(w.player(1).unwrap().inv.get(slot).is_none(), "comida consumida");
    }

    #[test]
    fn drop_and_pick_up_seen_by_another_player() {
        let mut w = world();
        let mut a = login(&mut w, 1, "Tirador");
        let mut b = login(&mut w, 2, "Testigo");
        let food = find(&w, 1, ObjType::UseOnce, |_| true);
        w.dev_give(1, food, 5);
        let slot = slot_of(&w, 1, food);
        let (x, y) = { let p = w.player(1).unwrap(); (p.x, p.y) };
        b.pump();
        send(&mut w, 1, C2S::DropItem { slot, amount: 2 });
        assert!(b.pump().iter().any(|m| matches!(m, S2C::GroundItem { x: gx, y: gy, obj, amount: 2 } if (*gx, *gy) == (x, y) && *obj == food)));
        assert_eq!(w.player(1).unwrap().inv.get(slot).unwrap().amount, 3);

        // A third player entering the map receives what lies on the floor.
        let mut c = login(&mut w, 3, "Recien");
        assert!(c.pump().iter().any(|m| matches!(m, S2C::GroundItem { obj, .. } if *obj == food)));

        send(&mut w, 1, C2S::PickUp);
        assert_eq!(w.player(1).unwrap().inv.get(slot).unwrap().amount, 5, "se apila de vuelta");
        assert!(b.pump().iter().any(|m| matches!(m, S2C::GroundItemRemoved { .. })));
        a.pump();
        send(&mut w, 1, C2S::PickUp);
        assert!(a.pump().iter().any(|m| matches!(m, S2C::Console { text, .. } if text.contains("nada"))), "recoger donde no hay nada");
    }

    #[test]
    fn gold_falls_next_to_the_player_and_is_bounded() {
        let mut w = world();
        let mut a = login(&mut w, 1, "Rico");
        w.player_mut(1).inv.gold = 500;
        send(&mut w, 1, C2S::DropGold { amount: 900 });
        assert_eq!(w.player(1).unwrap().inv.gold, 500, "no tira mas de lo que tiene");
        assert!(a.pump().iter().any(|m| matches!(m, S2C::Console { text, .. } if text.contains("oro"))));
        send(&mut w, 1, C2S::DropGold { amount: 200 });
        assert_eq!(w.player(1).unwrap().inv.gold, 300);
        let p = w.player(1).unwrap();
        let ground = &w.maps[&p.map].ground;
        let (&(gx, gy), it) = ground.tiles.iter().next().unwrap();
        assert_eq!((it.obj, it.amount), (GOLD_OBJ, 200));
        assert_ne!((gx, gy), (p.x, p.y), "Tilelibre no usa la casilla ocupada");
    }

    #[test]
    fn traps_change_nothing() {
        let mut w = world();
        let _a = login(&mut w, 1, "Tramposo");
        let food = find(&w, 1, ObjType::UseOnce, |_| true);
        w.dev_give(1, food, 3);
        let slot = slot_of(&w, 1, food);
        // More than the stack: clamped to what there is, nothing duplicated.
        send(&mut w, 1, C2S::DropItem { slot, amount: 1_000_000 });
        assert!(w.player(1).unwrap().inv.get(slot).is_none());
        let p = w.player(1).unwrap();
        assert_eq!(w.maps[&p.map].ground.at(p.x, p.y).unwrap().amount, 3);
        // Empty slot, locked slot (no backpack): ignored.
        send(&mut w, 1, C2S::UseItem { slot: 5 });
        send(&mut w, 1, C2S::EquipItem { slot: 30 });
        send(&mut w, 1, C2S::SwapSlots { a: 1, b: 30 });
        assert!(w.player(1).is_some());
        // Slot 0 / out of range: strikes until the kick.
        for _ in 0..20 {
            if w.player(1).is_none() {
                break;
            }
            send(&mut w, 1, C2S::DropItem { slot: 200, amount: 1 });
        }
        assert!(w.player(1).is_none(), "expulsado por casillas invalidas");
        assert!(!w.dev_give(9, food, 1), "dar a quien no existe");
    }

    #[test]
    fn dropping_the_equipped_weapon_updates_the_look() {
        let mut w = world();
        let _a = login(&mut w, 1, "Desarmado");
        let mut b = login(&mut w, 2, "Observa");
        let sword = find(&w, 1, ObjType::Weapon, |o| o.weapon_anim > 0 && o.two_handed == 0);
        w.dev_give(1, sword, 1);
        let slot = slot_of(&w, 1, sword);
        send(&mut w, 1, C2S::EquipItem { slot });
        b.pump();
        send(&mut w, 1, C2S::DropItem { slot, amount: 1 });
        assert!(b.pump().iter().any(|m| matches!(m, S2C::Look { id: 1, weapon: 0, .. })));
        assert_eq!(w.player(1).unwrap().inv.equipped.weapon, None);
    }

    #[test]
    fn backpack_unlocks_42_slots_and_nothing_gets_stuck() {
        let mut w = world();
        let _a = login(&mut w, 1, "Viajero");
        let (race, _, gender, ..) = user(&w, 1);
        let bag = find(&w, 1, ObjType::Backpack, |o| items::clothing_for(o, race, gender) > 0);
        w.dev_give(1, bag, 1);
        let slot = slot_of(&w, 1, bag);
        assert_eq!(w.player(1).unwrap().inv.usable_slots(), 24);
        send(&mut w, 1, C2S::EquipItem { slot });
        assert_eq!(w.player(1).unwrap().inv.usable_slots(), 42);
        // Move the bag to slot 40, take it off: slot 40 is still reachable.
        send(&mut w, 1, C2S::SwapSlots { a: slot, b: 40 });
        send(&mut w, 1, C2S::EquipItem { slot: 40 });
        assert_eq!(w.player(1).unwrap().inv.usable_slots(), 24);
        send(&mut w, 1, C2S::SwapSlots { a: 40, b: 2 });
        assert_eq!(w.player(1).unwrap().inv.get(2).unwrap().obj, bag);
    }

    #[test]
    fn swap_moves_equipment_with_its_object() {
        let mut w = world();
        let _a = login(&mut w, 1, "Ordenado");
        let sword = find(&w, 1, ObjType::Weapon, |o| o.weapon_anim > 0);
        w.dev_give(1, sword, 1);
        let slot = slot_of(&w, 1, sword);
        send(&mut w, 1, C2S::EquipItem { slot });
        send(&mut w, 1, C2S::SwapSlots { a: slot, b: 10 });
        let inv = &w.player(1).unwrap().inv;
        assert_eq!(inv.get(10).unwrap().obj, sword);
        assert_eq!(inv.equipped.weapon, Some(10));
    }

    #[test]
    fn creature_drops_gold_and_items() {
        let mut w = world();
        let _a = login(&mut w, 1, "Cazador");
        let ao = w.ao.clone();
        let (dat, def) = ao.npcs.iter().find(|(_, n)| n.gold > 0 && n.drops.iter().any(|d| d.chance == 1 && d.min_quantity > 0 && ao.objs.get(d.obj as u32).is_some())).expect("npc con oro y drop seguro");
        let sure: Vec<i32> = def.drops.iter().filter(|d| d.chance == 1 && d.min_quantity > 0).map(|d| d.obj).collect();
        let p = w.player(1).unwrap();
        let (map, x, y) = (p.map, p.x + 3, p.y);
        w.drop_creature_loot(map, x, y, dat, 1);
        let ground = &w.maps[&map].ground;
        let gold: u32 = ground.tiles.values().filter(|i| i.obj == GOLD_OBJ).map(|i| i.amount).sum();
        assert_eq!(gold, def.gold as u32, "GiveGLD");
        for obj in sure {
            assert!(ground.tiles.values().any(|i| i.obj as i32 == obj), "drop 1 en 1 del obj {obj}");
        }
    }

    #[test]
    fn death_drops_what_falls_and_keeps_the_wallet() {
        let mut w = world();
        let _a = login(&mut w, 1, "Caido");
        let food = find(&w, 1, ObjType::UseOnce, items::falls_on_death);
        w.dev_give(1, food, 4);
        w.player_mut(1).inv.gold = 1_500;
        // Death drops only off safe maps: move to the first unsafe loaded map if needed.
        let map = w.player(1).unwrap().map;
        if w.maps[&map].map.safe {
            w.maps.get_mut(&map).unwrap().map.safe = false;
        }
        w.drop_inventory_on_death(1);
        let p = w.player(1).unwrap();
        assert_eq!(p.inv.gold, 1_000, "la billetera guarda 1000 por nivel");
        assert!(p.inv.slots.iter().all(|s| s.is_none_or(|i| i.obj != food)));
        let ground = &w.maps[&map].ground;
        assert!(ground.tiles.values().any(|i| i.obj == food && i.amount == 4));
    }

    #[test]
    fn inventory_and_equipment_persist_across_reconnects() {
        let mut w = world();
        let mut c = connect(&mut w, 1);
        send(&mut w, 1, C2S::CreateAccount { user: "guardian".into(), password: "secreta1".into(), version: PROTOCOL_VERSION });
        send(&mut w, 1, C2S::CreateCharacter { name: "Guardado".into(), race: 1, class: 3, gender: 1, head: 1 });
        send(&mut w, 1, C2S::PlayCharacter { name: "Guardado".into() });
        assert!(w.player(1).is_some(), "entra con cuenta: {:?}", c.pump().last());
        let sword = find(&w, 1, ObjType::Weapon, |o| o.weapon_anim > 0);
        let food = find(&w, 1, ObjType::UseOnce, |_| true);
        w.dev_give(1, sword, 1);
        w.dev_give(1, food, 7);
        w.player_mut(1).inv.gold = 1234;
        let slot = slot_of(&w, 1, sword);
        send(&mut w, 1, C2S::EquipItem { slot });
        w.on_event(Event::Disconnected { id: 1 });

        let mut c = connect(&mut w, 2);
        send(&mut w, 2, C2S::AccountLogin { user: "guardian".into(), password: "secreta1".into(), version: PROTOCOL_VERSION });
        send(&mut w, 2, C2S::PlayCharacter { name: "Guardado".into() });
        let p = w.player(2).expect("vuelve a entrar");
        assert_eq!(p.inv.gold, 1234);
        assert_eq!(p.inv.get(slot_of(&w, 2, food)).unwrap().amount, 7);
        assert_eq!(p.inv.equipped.weapon, Some(slot), "re-equipada al entrar");
        let anim = w.ao.objs.get(sword as u32).unwrap().weapon_anim as u16;
        assert_eq!(w.look_of(2).2, anim);
        assert!(c.pump().iter().any(|m| matches!(m, S2C::InventorySlot { slot: s, equipped: true, .. } if *s == slot)));
    }
}

/// Ullathorpe in `Dat/Ciudades.dat` (MAPA, X, Y); map 1 (57, 44) without the file.
fn home_city(recursos: &std::path::Path) -> (u16, i32, i32) {
    let text = std::fs::read(recursos.join("Dat").join("Ciudades.dat")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let mut in_city = false;
    let (mut map, mut x, mut y) = (1u16, 57i32, 44i32);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_city = line.eq_ignore_ascii_case("[Ullathorpe]");
            continue;
        }
        let Some((k, v)) = line.split_once('=').filter(|_| in_city) else { continue };
        match k.trim().to_ascii_uppercase().as_str() {
            "MAPA" => map = v.trim().parse().unwrap_or(map),
            "X" => x = v.trim().parse().unwrap_or(x),
            "Y" => y = v.trim().parse().unwrap_or(y),
            _ => {}
        }
    }
    (map, x, y)
}

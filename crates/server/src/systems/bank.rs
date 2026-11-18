//! Vault (bank). OWNER: bank system (P1, Sonnet 2). Opened by the NPC system through
//! `open_bank(player, banker_npc_id)` when a player interacts with a banker, and
//! `clan_vault_open` for the clan vault (TSAO `BoveClan`), which uses the same window and
//! operations on a vault shared by the clan.
//!
//! Port of modBanco.bas plus the bank handlers of Protocol.bas (HandleBankStart,
//! HandleBankDeposit, HandleBankExtractItem, HandleBankDepositGold, HandleBankExtractGold,
//! HandleBankEnd). The VB keeps the "open" state in `flags.Comerciando` and the banker in
//! `flags.TargetNPC`; here both are `p.bank.open_with`.
//!
//! Differences from the VB, on purpose:
//! - The protocol has no destination slot: the vault/inventory slot is always chosen by
//!   `find_slot` (VB with `slotdestino = 0`).
//! - Equipped items cannot be deposited (the VB would unequip them).
//! - Impossible requests (slot out of range, amount above what exists) are cheating and
//!   get a strike, instead of the VB's silent clamp.
//! - `ElementalTags` do not exist in this port.
//! - Clan vault: TSAO let one member in at a time (a linear scan of every user) and wrote
//!   the whole `.bov` file on close, so a crash lost the session and two windows could
//!   overwrite each other. Here every member may have it open, each change is written at
//!   once and shown to everyone looking, and leaving the clan closes it.
use crate::world::bank::{find_slot, ClanVault, MAX_BANK_SLOTS, OPEN_RANGE, OPERATE_RANGE};
use crate::world::inventory::{InvItem, BASE_INVENTORY_SLOTS, MAX_INVENTORY_SLOTS};
use crate::world::World;
use ao_formats::dat::npcs::NpcType;
use ao_protocol::{Tone, S2C};
use tracing::warn;

/// VB `Distancia` on one map: Manhattan distance.
fn distance(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    (ax - bx).abs() + (ay - by).abs()
}

/// Which vault an operation works on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Vault {
    Own,
    Clan(i64),
}

impl World {
    // ---------------------------------------------------------------- helpers

    /// Position of a live banker NPC on `map`.
    /// NPC system: the contract is that `open_bank` is only called for NpcType::Banker.
    fn banker_pos(&self, map: u16, banker: u32) -> Option<(i32, i32)> {
        let n = self.maps.get(&map)?.npcs.get(&banker)?;
        n.alive().then_some((n.x, n.y))
    }

    /// Usable inventory slots (CurrentInventorySlots): 24, or 42 with a backpack.
    fn inventory_capacity(&self, id: u32) -> usize {
        match self.player(id) {
            Some(p) if p.inv.equipped.backpack.is_some() => MAX_INVENTORY_SLOTS,
            _ => BASE_INVENTORY_SLOTS,
        }
    }

    fn send_inv_slot(&self, id: u32, slot: u8) {
        let Some(p) = self.player(id) else { return };
        let (obj, amount) = p.inv.get(slot).map_or((0, 0), |i| (i.obj, i.amount));
        self.send(id, &S2C::InventorySlot { slot, obj, amount, equipped: p.inv.equipped.is_equipped(slot) });
    }

    fn vault_of(&self, id: u32) -> Vault {
        self.player(id).and_then(|p| p.bank.clan).map_or(Vault::Own, Vault::Clan)
    }

    /// Loads a clan vault from the database the first time it is needed.
    fn clan_vault_loaded(&mut self, guild: i64) -> &mut ClanVault {
        if !self.clan_vaults.contains_key(&guild) {
            let (rows, gold) = self.db.clan_vault(guild).unwrap_or_else(|e| {
                warn!(?e, guild, "boveda de clan: no se pudo leer");
                (Vec::new(), 0)
            });
            let mut slots = vec![None; MAX_BANK_SLOTS];
            for (slot, obj, amount) in rows {
                if let Some(s) = slots.get_mut(slot as usize - 1) {
                    *s = Some(InvItem { obj, amount });
                }
            }
            self.clan_vaults.insert(guild, ClanVault { slots, gold });
        }
        self.clan_vaults.get_mut(&guild).unwrap()
    }

    /// Takes the vault out to work on it next to the inventory (put it back with
    /// `vault_put`).
    fn vault_take(&mut self, id: u32) -> (Vault, Vec<Option<InvItem>>, u32) {
        match self.vault_of(id) {
            Vault::Own => {
                let b = &mut self.player_mut(id).bank;
                (Vault::Own, std::mem::take(&mut b.slots), b.gold)
            }
            Vault::Clan(g) => {
                let v = self.clan_vault_loaded(g);
                (Vault::Clan(g), std::mem::take(&mut v.slots), v.gold)
            }
        }
    }

    /// Puts the vault back; a clan vault writes the changed slot / gold and shows it to
    /// every member who has it open.
    fn vault_put(&mut self, id: u32, vault: Vault, slots: Vec<Option<InvItem>>, gold: u32, changed: Option<u8>, gold_changed: bool) {
        match vault {
            Vault::Own => {
                let b = &mut self.player_mut(id).bank;
                b.slots = slots;
                b.gold = gold;
            }
            Vault::Clan(g) => {
                if let Some(s) = changed {
                    let it = slots[s as usize - 1].map(|i| (i.obj, i.amount));
                    if let Err(e) = self.db.clan_vault_set_slot(g, s, it) {
                        warn!(?e, g, "boveda de clan: no se pudo guardar");
                    }
                }
                if gold_changed {
                    if let Err(e) = self.db.clan_vault_set_gold(g, gold) {
                        warn!(?e, g, "boveda de clan: no se pudo guardar el oro");
                    }
                }
                self.clan_vaults.insert(g, ClanVault { slots, gold });
                for other in self.player_ids() {
                    if other != id && self.player(other).is_some_and(|p| p.bank.clan == Some(g)) {
                        if let Some(s) = changed {
                            self.send_bank_slot(other, s);
                        }
                        if gold_changed {
                            self.send(other, &S2C::BankGold { gold });
                        }
                    }
                }
            }
        }
    }

    fn send_bank_slot(&self, id: u32, slot: u8) {
        let Some(p) = self.player(id) else { return };
        let slots = match p.bank.clan {
            Some(g) => match self.clan_vaults.get(&g) {
                Some(v) => &v.slots,
                None => return,
            },
            None => &p.bank.slots,
        };
        let (obj, amount) = slots.get(slot as usize - 1).copied().flatten().map_or((0, 0), |i| (i.obj, i.amount));
        self.send(id, &S2C::BankSlot { slot, obj, amount });
    }

    /// Gold in hand changed: vault gold and the vitals line that carries the hand gold.
    pub(crate) fn send_gold_pub(&self, id: u32) {
        self.send_gold(id);
    }

    fn send_gold(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let gold = match p.bank.clan {
            Some(g) => self.clan_vaults.get(&g).map_or(0, |v| v.gold),
            None => p.bank.gold,
        };
        self.send(id, &S2C::BankGold { gold });
        self.send(id, &S2C::Vitals { sta: p.stats.sta, max_sta: p.stats.max_sta, food: p.stats.food, water: p.stats.water, gold: p.inv.gold });
    }

    /// Closes the vault if it is open (HandleBankEnd). Also for the rest of the systems:
    /// call it when the player dies, changes map or starts something incompatible.
    pub(crate) fn close_bank(&mut self, id: u32) {
        let Some(p) = self.sessions.get_mut(&id).and_then(|s| s.player.as_mut()) else { return };
        p.bank.clan = None;
        if p.bank.open_with.take().is_some() {
            self.send(id, &S2C::BankClosed);
        }
    }

    /// Closes the vault if the player walked away from the banker (or the banker is gone).
    /// The operations check this on their own; a ticking system may call it too.
    #[allow(dead_code)]
    pub(crate) fn close_bank_if_far(&mut self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let Some(banker) = p.bank.open_with else { return };
        let near = self.banker_pos(p.map, banker).is_some_and(|(bx, by)| distance(p.x, p.y, bx, by) <= OPERATE_RANGE);
        if !near {
            self.close_bank(id);
        }
    }

    /// Checks every vault operation starts with: alive, vault open, banker still there and
    /// close enough, and (clan vault) still in that clan. Closes the vault otherwise.
    /// Returns the banker.
    fn bank_guard(&mut self, id: u32) -> Option<u32> {
        let p = self.player(id)?;
        if p.combat.dead {
            self.console(id, "!!Estas muerto!!", Tone::Info);
            self.close_bank(id);
            return None;
        }
        let Some(banker) = p.bank.open_with else {
            self.console(id, "La boveda esta cerrada.", Tone::Warn);
            return None;
        };
        if p.bank.clan.is_some() && p.bank.clan != p.guild.guild {
            self.console(id, "Ya no perteneces a ese clan.", Tone::Warn);
            self.close_bank(id);
            return None;
        }
        match self.banker_pos(p.map, banker) {
            Some((bx, by)) if distance(p.x, p.y, bx, by) <= OPERATE_RANGE => Some(banker),
            _ => {
                self.console(id, "Estas demasiado lejos.", Tone::Warn);
                self.close_bank(id);
                None
            }
        }
    }

    // ---------------------------------------------------------------- open (hook)

    /// Called by the NPC system (interact) when the player talks to a banker.
    /// HandleBankStart + IniciarDeposito: alive, not already trading, banker within
    /// `OPEN_RANGE`; then the whole vault is sent.
    #[allow(dead_code)]
    pub(crate) fn open_bank(&mut self, id: u32, banker: u32) {
        self.open_vault(id, banker, None);
    }

    /// HOOK (interact): the clan vault NPC. Only members of a clan.
    pub(crate) fn clan_vault_open(&mut self, id: u32, npc: u32) {
        let Some(p) = self.player(id) else { return };
        let Some(g) = p.guild.guild else {
            return self.console(id, "Solo los miembros de un clan pueden usar su bóveda.", Tone::Warn);
        };
        let ok = self.instance(p.map).npcs.get(&npc).is_some_and(|n| n.kind == NpcType::ClanVault);
        if ok {
            self.open_vault(id, npc, Some(g));
        }
    }

    fn open_vault(&mut self, id: u32, banker: u32, clan: Option<i64>) {
        let Some(p) = self.player(id) else { return };
        if p.combat.dead {
            return self.console(id, "!!Estas muerto!!", Tone::Info);
        }
        if p.trade.shop.is_some() || p.trade.with.is_some() {
            return self.console(id, "Ya estas comerciando.", Tone::Info);
        }
        if p.bank.open_with.is_some() {
            return self.console(id, "Ya estas comerciando.", Tone::Info);
        }
        let Some((bx, by)) = self.banker_pos(p.map, banker) else {
            return self.console(id, "Primero haz click izquierdo sobre el personaje.", Tone::Info);
        };
        if distance(p.x, p.y, bx, by) > OPEN_RANGE {
            return self.console(id, "Estas demasiado lejos.", Tone::Warn);
        }
        let (slots, gold) = match clan {
            Some(g) => {
                let v = self.clan_vault_loaded(g);
                (v.slots.clone(), v.gold)
            }
            None => (self.player(id).unwrap().bank.slots.clone(), self.player(id).unwrap().bank.gold),
        };
        let p = self.player_mut(id);
        p.bank.open_with = Some(banker);
        p.bank.clan = clan;
        let slots = slots.iter().enumerate().filter_map(|(i, s)| s.map(|it| ((i + 1) as u8, it.obj, it.amount))).collect();
        self.send(id, &S2C::BankOpen { slots, gold });
    }

    pub(crate) fn on_bank_close(&mut self, id: u32) {
        self.close_bank(id);
    }

    // ---------------------------------------------------------------- items

    /// UserDepositaItem + UserDejaObj: inventory slot -> vault.
    pub(crate) fn on_bank_deposit(&mut self, id: u32, slot: u8, amount: u32) {
        if self.bank_guard(id).is_none() {
            return;
        }
        if slot == 0 || slot as usize > self.inventory_capacity(id) {
            return self.strike(id, "banco: casilla de inventario invalida");
        }
        let p = self.player(id).expect("logged player");
        let Some(item) = p.inv.get(slot) else {
            return self.console(id, "No hay nada en esa casilla.", Tone::Warn);
        };
        if amount == 0 {
            return self.console(id, "Cantidad invalida.", Tone::Warn);
        }
        if amount > item.amount {
            return self.strike(id, "banco: deposita mas de lo que tiene");
        }
        if p.inv.equipped.is_equipped(slot) || p.inv.pending_equipped.contains(&slot) {
            return self.console(id, "Desequipa el objeto antes de depositarlo.", Tone::Warn);
        }
        let Some(data) = self.ao.objs.get(item.obj as u32) else { return };
        if data.newbie == 1 {
            return self.console(id, "No puedes comerciar objetos newbie.", Tone::Warn);
        }
        if p.bank.clan.is_some() && data.non_transferable == 1 {
            return self.console(id, "Los objetos intransferibles no pueden ir a la bóveda del clan.", Tone::Warn);
        }
        let (vault, mut slots, gold) = self.vault_take(id);
        let Some(dest) = find_slot(&slots, item.obj, amount) else {
            self.vault_put(id, vault, slots, gold, None, false);
            return self.console(id, "No tienes mas espacio en el banco.", Tone::Warn);
        };
        slots[dest].get_or_insert(InvItem { obj: item.obj, amount: 0 }).amount += amount;
        let left = item.amount - amount;
        self.player_mut(id).inv.slots[slot as usize - 1] = (left > 0).then_some(InvItem { obj: item.obj, amount: left });
        self.vault_put(id, vault, slots, gold, Some(dest as u8 + 1), false);
        if let Vault::Clan(_) = vault {
            // The player's side of a clan move is saved at once too (no dupe on a crash).
            if let Some(p) = self.player(id) {
                self.save_player(p);
            }
        }
        self.send_inv_slot(id, slot);
        self.send_bank_slot(id, dest as u8 + 1);
    }

    /// UserRetiraItem + UserReciveObj: vault -> inventory.
    pub(crate) fn on_bank_withdraw(&mut self, id: u32, slot: u8, amount: u32) {
        if self.bank_guard(id).is_none() {
            return;
        }
        if slot == 0 || slot as usize > MAX_BANK_SLOTS {
            return self.strike(id, "banco: casilla de boveda invalida");
        }
        let (vault, mut slots, gold) = self.vault_take(id);
        let fail = |w: &mut World, slots, msg: Option<&str>, strike: Option<&str>| {
            w.vault_put(id, vault, slots, gold, None, false);
            if let Some(m) = msg {
                w.console(id, m, Tone::Warn);
            }
            if let Some(s) = strike {
                w.strike(id, s);
            }
        };
        let Some(item) = slots[slot as usize - 1] else {
            return fail(self, slots, Some("No hay nada en esa casilla."), None);
        };
        if amount == 0 {
            return fail(self, slots, Some("Cantidad invalida."), None);
        }
        if amount > item.amount {
            // Another clan member may have taken it a moment ago: not a cheat there.
            return if vault == Vault::Own { fail(self, slots, None, Some("banco: retira mas de lo que hay")) } else { fail(self, slots, Some("Ya no hay tanta cantidad."), None) };
        }
        let cap = self.inventory_capacity(id);
        let Some(dest) = find_slot(&self.player(id).unwrap().inv.slots[..cap], item.obj, amount) else {
            return fail(self, slots, Some("No podes tener mas objetos."), None);
        };
        self.player_mut(id).inv.slots[dest].get_or_insert(InvItem { obj: item.obj, amount: 0 }).amount += amount;
        let left = item.amount - amount;
        slots[slot as usize - 1] = (left > 0).then_some(InvItem { obj: item.obj, amount: left });
        if let Vault::Clan(_) = vault {
            if let Some(p) = self.player(id) {
                self.save_player(p);
            }
        }
        self.vault_put(id, vault, slots, gold, Some(slot), false);
        self.send_inv_slot(id, dest as u8 + 1);
        self.send_bank_slot(id, slot);
    }

    // ---------------------------------------------------------------- gold

    fn banker_says(&self, id: u32, banker: u32, text: String) {
        self.send(id, &S2C::NpcSay { id: banker, text });
    }

    /// HandleBankDepositGold: hand -> vault. The hand is debited first, like the VB.
    pub(crate) fn on_bank_deposit_gold(&mut self, id: u32, amount: u32) {
        let Some(banker) = self.bank_guard(id) else { return };
        let hand = self.player(id).expect("logged player").inv.gold;
        if amount > i32::MAX as u32 || amount > hand {
            return self.strike(id, "banco: deposita mas oro del que tiene");
        }
        let (vault, slots, gold) = self.vault_take(id);
        let Some(total) = gold.checked_add(amount).filter(|_| amount > 0) else {
            self.vault_put(id, vault, slots, gold, None, false);
            return self.banker_says(id, banker, "No tenes esa cantidad.".into());
        };
        self.player_mut(id).inv.gold -= amount;
        if vault != Vault::Own {
            if let Some(p) = self.player(id) {
                self.save_player(p);
            }
        }
        self.vault_put(id, vault, slots, total, None, true);
        self.banker_says(id, banker, format!("Tenes {total} monedas de oro en tu cuenta."));
        self.send_gold(id);
    }

    /// HandleBankExtractGold: vault -> hand.
    pub(crate) fn on_bank_withdraw_gold(&mut self, id: u32, amount: u32) {
        let Some(banker) = self.bank_guard(id) else { return };
        let hand = self.player(id).expect("logged player").inv.gold;
        let (vault, slots, gold) = self.vault_take(id);
        if amount > i32::MAX as u32 || amount > gold {
            self.vault_put(id, vault, slots, gold, None, false);
            return if vault == Vault::Own { self.strike(id, "banco: retira mas oro del que hay") } else { self.banker_says(id, banker, "Ya no hay tanto oro en la bóveda.".into()) };
        }
        let Some(new_hand) = hand.checked_add(amount).filter(|_| amount > 0) else {
            self.vault_put(id, vault, slots, gold, None, false);
            return self.banker_says(id, banker, "No tenes esa cantidad.".into());
        };
        let left = gold - amount;
        self.player_mut(id).inv.gold = new_hand;
        if vault != Vault::Own {
            if let Some(p) = self.player(id) {
                self.save_player(p);
            }
        }
        self.vault_put(id, vault, slots, left, None, true);
        self.banker_says(id, banker, format!("Tenes {left} monedas de oro en tu cuenta."));
        self.send_gold(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameTime;
    use crate::db::{CharacterState, NewCharacter};
    use crate::world::bank::MAX_STACK;
    use crate::world::Session;
    use crate::Settings;
    use ao_formats::dat::npcs::NpcType;
    use ao_protocol::{FrameReader, MAX_SERVER_FRAME};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    const BANKER_ID: u32 = 200_000;
    const ME: u32 = 1;

    struct Rig {
        w: World,
        rx: UnboundedReceiver<Vec<u8>>,
        /// Two ordinary objects and a newbie one, from the official obj.dat.
        a: u16,
        b: u16,
        newbie: u16,
    }

    fn drain(rx: &mut UnboundedReceiver<Vec<u8>>) -> Vec<S2C> {
        let mut r = FrameReader::default();
        while let Ok(b) = rx.try_recv() {
            r.push(&b);
        }
        let mut out = Vec::new();
        while let Some(m) = r.next::<S2C>(MAX_SERVER_FRAME) {
            out.push(m.expect("frame"));
        }
        out
    }

    /// A banker of Ullathorpe (map 1) from npcs.dat + the map file.
    fn ullathorpe_banker(w: &World) -> (i32, i32) {
        let m = &w.maps[&1].map;
        let b = m.npcs.iter().find(|n| w.ao.npcs.get(n.npc as u32).is_some_and(|d| d.npc_type == NpcType::Banker));
        let b = b.expect("Ullathorpe has a banker in the official map");
        (b.x, b.y)
    }

    fn place_banker(w: &mut World, at: (i32, i32)) {
        let m = &w.maps[&1].map;
        let def = m.npcs.iter().find(|n| (n.x, n.y) == at).map(|n| n.npc).expect("banker on that tile");
        let npc = w.make_npc(def, at.0, at.1).expect("banker in npcs.dat");
        w.maps.get_mut(&1).unwrap().add_npc(BANKER_ID, npc);
    }

    fn join(w: &mut World, id: u32, name: &str, at: (i32, i32), state: &CharacterState, char_id: Option<i64>) -> UnboundedReceiver<Vec<u8>> {
        let (tx, rx) = unbounded_channel();
        w.sessions.insert(id, Session::new(tx, "127.0.0.1:1".parse().unwrap()));
        let rec = crate::db::CharacterRecord { id: char_id.unwrap_or(0), account_id: 0, name: name.into(), race: 1, class: 1, gender: 1, body: 1, head: 1, level: 1, exp: 0, hp: 15, max_hp: 15, mana: 0, max_mana: 0, map: 1, x: at.0, y: at.1 };
        assert!(w.spawn_player(id, &rec, state, char_id));
        rx
    }

    fn rig() -> Rig {
        let mut w = World::new(Settings::defaults());
        assert!(w.ensure_map(1));
        let banker_at = ullathorpe_banker(&w);
        place_banker(&mut w, banker_at);
        let mut rx = join(&mut w, ME, "Cliente", banker_at, &CharacterState::default(), None);
        drain(&mut rx);
        let plain: Vec<u16> = w.ao.objs.iter().filter(|(_, o)| o.newbie == 0 && !o.name.is_empty()).map(|(i, _)| i as u16).take(2).collect();
        let newbie = w.ao.objs.iter().find(|(_, o)| o.newbie == 1).map(|(i, _)| i as u16).expect("a newbie object");
        Rig { w, rx, a: plain[0], b: plain[1], newbie }
    }

    impl Rig {
        fn give(&mut self, slot: u8, obj: u16, amount: u32) {
            self.w.player_mut(ME).inv.slots[slot as usize - 1] = Some(InvItem { obj, amount });
        }
        fn inv(&self, slot: u8) -> Option<InvItem> {
            self.w.player(ME).unwrap().inv.get(slot)
        }
        fn bank(&self, slot: u8) -> Option<InvItem> {
            self.w.player(ME).unwrap().bank.slots[slot as usize - 1]
        }
        fn open(&mut self) {
            self.w.open_bank(ME, BANKER_ID);
            assert_eq!(self.w.player(ME).unwrap().bank.open_with, Some(BANKER_ID));
            drain(&mut self.rx);
        }
        fn msgs(&mut self) -> Vec<S2C> {
            drain(&mut self.rx)
        }
        fn alive(&self) -> bool {
            self.w.player(ME).is_some()
        }
    }

    #[test]
    fn opening_sends_the_vault_and_needs_a_near_live_banker() {
        let mut r = rig();
        r.w.player_mut(ME).bank.slots[2] = Some(InvItem { obj: r.a, amount: 7 });
        r.w.player_mut(ME).bank.gold = 55;
        r.w.open_bank(ME, BANKER_ID);
        assert!(matches!(r.msgs().as_slice(), [S2C::BankOpen { slots, gold: 55 }] if slots == &vec![(3, r.a, 7)]));
        r.w.open_bank(ME, BANKER_ID);
        assert!(r.msgs().iter().all(|m| matches!(m, S2C::Console { .. })), "already open: no second BankOpen");

        let mut r = rig();
        r.w.open_bank(ME, BANKER_ID + 1);
        assert!(r.w.player(ME).unwrap().bank.open_with.is_none(), "no such NPC");
        r.w.player_mut(ME).x += 8; // Manhattan distance 8 > 6
        r.w.open_bank(ME, BANKER_ID);
        assert!(r.w.player(ME).unwrap().bank.open_with.is_none(), "too far to open");
        r.w.player_mut(ME).x -= 8;
        r.w.maps.get_mut(&1).unwrap().npcs.get_mut(&BANKER_ID).unwrap().dead_until = Some(GameTime(1));
        r.w.open_bank(ME, BANKER_ID);
        assert!(r.w.player(ME).unwrap().bank.open_with.is_none(), "dead banker");
        assert!(!r.msgs().iter().any(|m| matches!(m, S2C::BankOpen { .. })));
    }

    #[test]
    fn dead_players_cannot_open_or_operate() {
        let mut r = rig();
        r.w.player_mut(ME).combat.dead = true;
        r.w.open_bank(ME, BANKER_ID);
        assert!(r.w.player(ME).unwrap().bank.open_with.is_none());
        r.w.player_mut(ME).combat.dead = false;
        r.give(1, r.a, 5);
        r.open();
        r.w.player_mut(ME).combat.dead = true;
        r.w.on_bank_deposit(ME, 1, 5);
        assert_eq!(r.inv(1), Some(InvItem { obj: r.a, amount: 5 }));
        assert!(r.msgs().iter().any(|m| matches!(m, S2C::BankClosed)), "dying closes the vault");
        assert!(r.alive(), "being dead is not cheating");
    }

    #[test]
    fn items_stack_and_find_free_slots() {
        let mut r = rig();
        let (a, b) = (r.a, r.b);
        r.give(1, a, 30);
        r.give(2, b, 4);
        r.open();

        r.w.on_bank_deposit(ME, 1, 10);
        assert_eq!(r.bank(1), Some(InvItem { obj: a, amount: 10 }));
        assert_eq!(r.inv(1), Some(InvItem { obj: a, amount: 20 }));
        let m = r.msgs();
        assert!(m.iter().any(|m| matches!(m, S2C::InventorySlot { slot: 1, obj, amount: 20, equipped: false } if *obj == a)));
        assert!(m.iter().any(|m| matches!(m, S2C::BankSlot { slot: 1, obj, amount: 10 } if *obj == a)));

        r.w.on_bank_deposit(ME, 1, 10);
        assert_eq!(r.bank(1), Some(InvItem { obj: a, amount: 20 }), "same object stacks in the same vault slot");

        r.w.on_bank_deposit(ME, 2, 4);
        assert_eq!(r.bank(2), Some(InvItem { obj: b, amount: 4 }), "another object takes the first free slot");
        assert_eq!(r.inv(2), None, "emptied inventory slot");
        assert!(r.msgs().iter().any(|m| matches!(m, S2C::InventorySlot { slot: 2, obj: 0, amount: 0, .. })));

        // Withdraw: stacks onto the inventory slot that still has `a`, else first free slot.
        r.w.on_bank_withdraw(ME, 1, 5);
        assert_eq!(r.inv(1), Some(InvItem { obj: a, amount: 15 }));
        assert_eq!(r.bank(1), Some(InvItem { obj: a, amount: 15 }));
        r.w.on_bank_withdraw(ME, 2, 4);
        assert_eq!(r.bank(2), None);
        assert_eq!(r.inv(2), Some(InvItem { obj: b, amount: 4 }));
        assert!(r.msgs().iter().any(|m| matches!(m, S2C::BankSlot { slot: 2, obj: 0, amount: 0 })));
        assert!(r.alive());
    }

    #[test]
    fn full_stacks_overflow_into_the_next_slot_and_a_full_vault_refuses() {
        let mut r = rig();
        let a = r.a;
        r.w.player_mut(ME).bank.slots[0] = Some(InvItem { obj: a, amount: MAX_STACK });
        r.give(1, a, 3);
        r.open();
        r.w.on_bank_deposit(ME, 1, 3);
        assert_eq!(r.bank(1), Some(InvItem { obj: a, amount: MAX_STACK }));
        assert_eq!(r.bank(2), Some(InvItem { obj: a, amount: 3 }));

        // Fill every vault slot with other objects: nothing fits any more.
        for i in 0..MAX_BANK_SLOTS {
            let cell = &mut r.w.player_mut(ME).bank.slots[i];
            if cell.is_none() {
                *cell = Some(InvItem { obj: 9000 + i as u16, amount: 1 });
            }
        }
        r.give(2, r.b, 1);
        r.w.on_bank_deposit(ME, 2, 1);
        assert_eq!(r.inv(2), Some(InvItem { obj: r.b, amount: 1 }), "item stays in hand");
        assert!(r.alive());
    }

    #[test]
    fn withdraw_with_a_full_inventory_is_refused() {
        let mut r = rig();
        for s in 1..=BASE_INVENTORY_SLOTS as u8 {
            r.give(s, r.b, 1);
        }
        r.w.player_mut(ME).bank.slots[0] = Some(InvItem { obj: r.a, amount: 5 });
        r.open();
        r.w.on_bank_withdraw(ME, 1, 5);
        assert_eq!(r.bank(1), Some(InvItem { obj: r.a, amount: 5 }));
        assert!(r.w.player(ME).unwrap().inv.slots[BASE_INVENTORY_SLOTS..].iter().all(Option::is_none), "slots above 24 need a backpack");
        // With a backpack the extra slots are usable.
        r.w.player_mut(ME).inv.equipped.backpack = Some(1);
        r.w.on_bank_withdraw(ME, 1, 5);
        assert_eq!(r.bank(1), None);
        assert_eq!(r.inv(BASE_INVENTORY_SLOTS as u8 + 1), Some(InvItem { obj: r.a, amount: 5 }));
    }

    #[test]
    fn equipped_and_newbie_items_stay_in_hand() {
        let mut r = rig();
        r.give(1, r.a, 1);
        r.give(2, r.newbie, 1);
        r.w.player_mut(ME).inv.equipped.weapon = Some(1);
        r.open();
        r.w.on_bank_deposit(ME, 1, 1);
        r.w.on_bank_deposit(ME, 2, 1);
        assert_eq!(r.inv(1), Some(InvItem { obj: r.a, amount: 1 }));
        assert_eq!(r.inv(2), Some(InvItem { obj: r.newbie, amount: 1 }));
        assert!(r.w.player(ME).unwrap().bank.slots.iter().all(Option::is_none));
        assert!(r.alive(), "legitimate refusals are not cheating");
        // Pending equipment (not yet restored on login) counts as equipped too.
        r.w.player_mut(ME).inv.equipped.weapon = None;
        r.w.player_mut(ME).inv.pending_equipped.push(1);
        r.w.on_bank_deposit(ME, 1, 1);
        assert_eq!(r.inv(1), Some(InvItem { obj: r.a, amount: 1 }));
    }

    #[test]
    fn closed_vault_or_walking_away_rejects_everything() {
        let mut r = rig();
        r.give(1, r.a, 5);
        r.w.player_mut(ME).inv.gold = 100;
        // Never opened.
        r.w.on_bank_deposit(ME, 1, 5);
        r.w.on_bank_deposit_gold(ME, 50);
        assert_eq!(r.inv(1), Some(InvItem { obj: r.a, amount: 5 }));
        assert_eq!((r.w.player(ME).unwrap().inv.gold, r.w.player(ME).unwrap().bank.gold), (100, 0));
        assert!(r.alive(), "operating with the vault closed is only refused");

        // Open, then walk away more than 10 tiles: refused and closed.
        r.open();
        r.w.player_mut(ME).x += 11;
        r.w.on_bank_deposit(ME, 1, 5);
        assert_eq!(r.inv(1), Some(InvItem { obj: r.a, amount: 5 }));
        assert!(r.w.player(ME).unwrap().bank.open_with.is_none());
        assert!(r.msgs().iter().any(|m| matches!(m, S2C::BankClosed)));

        // Back near: it must be opened again.
        r.w.player_mut(ME).x -= 11;
        r.w.on_bank_deposit(ME, 1, 5);
        assert_eq!(r.inv(1), Some(InvItem { obj: r.a, amount: 5 }));

        // Explicit close, then operate.
        r.open();
        r.w.on_bank_close(ME);
        assert!(r.msgs().iter().any(|m| matches!(m, S2C::BankClosed)));
        r.w.on_bank_withdraw_gold(ME, 1);
        assert!(r.alive());
    }

    #[test]
    fn gold_moves_both_ways_and_updates_the_client() {
        let mut r = rig();
        r.w.player_mut(ME).inv.gold = 1000;
        r.open();
        r.w.on_bank_deposit_gold(ME, 400);
        let p = r.w.player(ME).unwrap();
        assert_eq!((p.inv.gold, p.bank.gold), (600, 400));
        let m = r.msgs();
        assert!(m.iter().any(|m| matches!(m, S2C::BankGold { gold: 400 })));
        assert!(m.iter().any(|m| matches!(m, S2C::Vitals { gold: 600, .. })));
        assert!(m.iter().any(|m| matches!(m, S2C::NpcSay { id: BANKER_ID, .. })));

        r.w.on_bank_withdraw_gold(ME, 100);
        let p = r.w.player(ME).unwrap();
        assert_eq!((p.inv.gold, p.bank.gold), (700, 300));
        // Zero is refused politely.
        r.w.on_bank_deposit_gold(ME, 0);
        r.w.on_bank_withdraw_gold(ME, 0);
        let p = r.w.player(ME).unwrap();
        assert_eq!((p.inv.gold, p.bank.gold), (700, 300));
        assert!(r.alive());
        // Overflow of the vault gold is refused without losing the hand gold.
        r.w.player_mut(ME).bank.gold = u32::MAX - 10;
        r.w.player_mut(ME).inv.gold = 50;
        r.w.on_bank_deposit_gold(ME, 50);
        let p = r.w.player(ME).unwrap();
        assert_eq!((p.inv.gold, p.bank.gold), (50, u32::MAX - 10));
    }

    /// Repeats `attack` until the server kicks the cheater (strikes pile up); the state must
    /// stay untouched on every attempt.
    fn cheat_ends_in_kick(r: &mut Rig, what: &str, attack: &dyn Fn(&mut World), untouched: impl Fn(&Rig) -> bool) {
        for i in 0..40 {
            if !r.alive() {
                assert!(i > 1, "{what}: one try must not kick");
                return;
            }
            attack(&mut r.w);
            if r.alive() {
                assert!(untouched(r), "{what}: state changed on try {i}");
            }
        }
        panic!("{what}: never kicked");
    }

    #[test]
    fn impossible_requests_are_cheating() {
        type Attack = Box<dyn Fn(&mut World)>;
        let attacks: Vec<(&str, Attack)> = vec![
            ("deposit slot 0", Box::new(|w| w.on_bank_deposit(ME, 0, 1))),
            ("deposit slot 25 without backpack", Box::new(|w| w.on_bank_deposit(ME, 25, 1))),
            ("deposit slot 255", Box::new(|w| w.on_bank_deposit(ME, 255, 1))),
            ("withdraw slot 0", Box::new(|w| w.on_bank_withdraw(ME, 0, 1))),
            ("withdraw slot 43", Box::new(|w| w.on_bank_withdraw(ME, 43, 1))),
            ("deposit more than held", Box::new(|w| w.on_bank_deposit(ME, 1, 6))),
            ("deposit u32::MAX", Box::new(|w| w.on_bank_deposit(ME, 1, u32::MAX))),
            ("withdraw more than stored", Box::new(|w| w.on_bank_withdraw(ME, 1, 4))),
            ("withdraw u32::MAX", Box::new(|w| w.on_bank_withdraw(ME, 1, u32::MAX))),
            ("gold deposit above hand", Box::new(|w| w.on_bank_deposit_gold(ME, 101))),
            ("gold deposit 'negative' (u32 wrap)", Box::new(|w| w.on_bank_deposit_gold(ME, (-5i32) as u32))),
            ("gold withdraw above vault", Box::new(|w| w.on_bank_withdraw_gold(ME, 31))),
            ("gold withdraw 'negative'", Box::new(|w| w.on_bank_withdraw_gold(ME, (-1i32) as u32))),
        ];
        for (name, attack) in &attacks {
            let mut r = rig();
            r.give(1, r.a, 5);
            r.w.player_mut(ME).bank.slots[0] = Some(InvItem { obj: r.b, amount: 3 });
            r.w.player_mut(ME).inv.gold = 100;
            r.w.player_mut(ME).bank.gold = 30;
            r.open();
            let (a, b) = (r.a, r.b);
            cheat_ends_in_kick(&mut r, name, attack.as_ref(), |r| {
                let p = r.w.player(ME).unwrap();
                p.inv.get(1) == Some(InvItem { obj: a, amount: 5 })
                    && p.bank.slots[0] == Some(InvItem { obj: b, amount: 3 })
                    && p.bank.slots[1..].iter().all(Option::is_none)
                    && (p.inv.gold, p.bank.gold) == (100, 30)
            });
        }
    }

    #[test]
    fn deposits_and_withdrawals_survive_logout_and_login() {
        let mut w = World::new(Settings::defaults());
        assert!(w.ensure_map(1));
        let at = ullathorpe_banker(&w);
        place_banker(&mut w, at);
        let acc = w.db.create_account("boveda", "secreta1").unwrap();
        let rec = w.db.create_character(acc.id, &NewCharacter { name: "Avaro", race: 1, class: 1, gender: 1, body: 1, head: 1, hp: 15, mana: 0, map: 1, x: at.0, y: at.1 }).unwrap();
        let mut state = w.db.load_state(rec.id).unwrap();
        state.gold = 500;
        state.inventory = vec![(1, 38, 20, false), (2, 39, 3, false)];
        w.db.save_full(&rec, &state).unwrap();

        let state = w.db.load_state(rec.id).unwrap();
        let mut rx = join(&mut w, 7, "Avaro", at, &state, Some(rec.id));
        drain(&mut rx);
        w.open_bank(7, BANKER_ID);
        w.on_bank_deposit(7, 1, 12);
        w.on_bank_deposit(7, 2, 3);
        w.on_bank_deposit_gold(7, 300);
        w.on_bank_close(7);
        w.logout(7); // saves like a normal disconnect

        let rec = w.db.load_character(acc.id, "Avaro").unwrap();
        let state = w.db.load_state(rec.id).unwrap();
        let mut rx = join(&mut w, 8, "Avaro", at, &state, Some(rec.id));
        drain(&mut rx);
        let p = w.player(8).unwrap();
        assert_eq!(p.bank.slots[0], Some(InvItem { obj: 38, amount: 12 }));
        assert_eq!(p.bank.slots[1], Some(InvItem { obj: 39, amount: 3 }));
        assert_eq!((p.bank.gold, p.inv.gold), (300, 200));
        assert_eq!(p.inv.get(1), Some(InvItem { obj: 38, amount: 8 }));
        assert_eq!(p.inv.get(2), None);
        assert!(p.bank.open_with.is_none(), "the vault never comes back open");

        // And what is taken out is taken out for good.
        w.open_bank(8, BANKER_ID);
        w.on_bank_withdraw(8, 2, 3);
        w.on_bank_withdraw_gold(8, 100);
        w.logout(8);
        let state = w.db.load_state(rec.id).unwrap();
        assert_eq!(state.bank, vec![(1, 38, 12)]);
        assert_eq!(state.bank_gold, 200);
        assert_eq!(state.gold, 300);
        assert!(state.inventory.contains(&(2, 39, 3, false)));
    }

    /// Clan vault: two members share one vault; a deposit shows up in the other's window,
    /// the other can take it, and a member who leaves the clan is shut out.
    #[test]
    fn clan_vault_is_shared_by_members() {
        const OTHER: u32 = 2;
        const CLAN: i64 = 7;
        let mut r = rig();
        let at = (r.w.player(ME).unwrap().x, r.w.player(ME).unwrap().y);
        let mut rx2 = join(&mut r.w, OTHER, "Socio", at, &CharacterState::default(), None);
        r.w.player_mut(ME).guild.guild = Some(CLAN);
        r.w.player_mut(OTHER).guild.guild = Some(CLAN);
        r.w.open_vault(ME, BANKER_ID, Some(CLAN));
        r.w.open_vault(OTHER, BANKER_ID, Some(CLAN));
        drain(&mut rx2);
        r.msgs();
        r.give(1, r.a, 5);
        r.w.on_bank_deposit(ME, 1, 5);
        assert_eq!(r.inv(1), None);
        assert!(r.w.player(ME).unwrap().bank.slots.iter().all(Option::is_none), "own vault untouched");
        let seen = drain(&mut rx2);
        assert!(seen.iter().any(|m| matches!(m, S2C::BankSlot { slot: 1, obj, amount: 5 } if *obj == r.a)), "the other member sees it");
        r.w.on_bank_withdraw(OTHER, 1, 5);
        assert_eq!(r.w.player(OTHER).unwrap().inv.slots.iter().flatten().map(|i| i.amount).sum::<u32>(), 5);
        // Taking again what is gone is not a cheat in a shared vault.
        r.w.on_bank_withdraw(OTHER, 1, 5);
        assert!(r.w.player(OTHER).is_some());
        // Leaves the clan: the next operation closes it.
        r.w.player_mut(ME).guild.guild = None;
        r.w.player_mut(ME).inv.gold = 100;
        r.w.on_bank_deposit_gold(ME, 100);
        assert_eq!(r.w.player(ME).unwrap().inv.gold, 100);
        assert!(r.w.player(ME).unwrap().bank.open_with.is_none());
        assert_eq!(r.w.clan_vaults[&CLAN].gold, 0);
    }
}

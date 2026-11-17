//! Vault (bank). OWNER: bank system (P1, Sonnet 2). Opened by the NPC system through
//! `open_bank(player, banker_npc_id)` when a player interacts with a banker.
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
use crate::world::bank::{find_slot, MAX_BANK_SLOTS, OPEN_RANGE, OPERATE_RANGE};
use crate::world::inventory::{InvItem, BASE_INVENTORY_SLOTS, MAX_INVENTORY_SLOTS};
use crate::world::World;
use ao_protocol::{Tone, S2C};

/// VB `Distancia` on one map: Manhattan distance.
fn distance(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    (ax - bx).abs() + (ay - by).abs()
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

    fn send_bank_slot(&self, id: u32, slot: u8) {
        let Some(p) = self.player(id) else { return };
        let (obj, amount) = p.bank.slots.get(slot as usize - 1).copied().flatten().map_or((0, 0), |i| (i.obj, i.amount));
        self.send(id, &S2C::BankSlot { slot, obj, amount });
    }

    /// Gold in hand changed: bank gold and the vitals line that carries the hand gold.
    fn send_gold(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        self.send(id, &S2C::BankGold { gold: p.bank.gold });
        self.send(id, &S2C::Vitals { sta: p.stats.sta, max_sta: p.stats.max_sta, food: p.stats.food, water: p.stats.water, gold: p.inv.gold });
    }

    /// Closes the vault if it is open (HandleBankEnd). Also for the rest of the systems:
    /// call it when the player dies, changes map or starts something incompatible.
    pub(crate) fn close_bank(&mut self, id: u32) {
        let Some(p) = self.sessions.get_mut(&id).and_then(|s| s.player.as_mut()) else { return };
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
    /// close enough. Closes the vault when the player walked away. Returns the banker.
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
        let Some(p) = self.player(id) else { return };
        if p.combat.dead {
            return self.console(id, "!!Estas muerto!!", Tone::Info);
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
        let p = self.player_mut(id);
        p.bank.open_with = Some(banker);
        let slots = p.bank.slots.iter().enumerate().filter_map(|(i, s)| s.map(|it| ((i + 1) as u8, it.obj, it.amount))).collect();
        let gold = p.bank.gold;
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
        let Some(dest) = find_slot(&p.bank.slots, item.obj, amount) else {
            return self.console(id, "No tienes mas espacio en el banco.", Tone::Warn);
        };
        let p = self.player_mut(id);
        let bank_slot = p.bank.slots[dest].get_or_insert(InvItem { obj: item.obj, amount: 0 });
        bank_slot.amount += amount;
        let left = item.amount - amount;
        p.inv.slots[slot as usize - 1] = (left > 0).then_some(InvItem { obj: item.obj, amount: left });
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
        let p = self.player(id).expect("logged player");
        let Some(item) = p.bank.slots[slot as usize - 1] else {
            return self.console(id, "No hay nada en esa casilla.", Tone::Warn);
        };
        if amount == 0 {
            return self.console(id, "Cantidad invalida.", Tone::Warn);
        }
        if amount > item.amount {
            return self.strike(id, "banco: retira mas de lo que hay");
        }
        let cap = self.inventory_capacity(id);
        let Some(dest) = find_slot(&p.inv.slots[..cap], item.obj, amount) else {
            return self.console(id, "No podes tener mas objetos.", Tone::Warn);
        };
        let p = self.player_mut(id);
        let inv_slot = p.inv.slots[dest].get_or_insert(InvItem { obj: item.obj, amount: 0 });
        inv_slot.amount += amount;
        let left = item.amount - amount;
        p.bank.slots[slot as usize - 1] = (left > 0).then_some(InvItem { obj: item.obj, amount: left });
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
        let p = self.player(id).expect("logged player");
        if amount > i32::MAX as u32 || amount > p.inv.gold {
            return self.strike(id, "banco: deposita mas oro del que tiene");
        }
        let total = p.bank.gold.checked_add(amount).filter(|_| amount > 0);
        let Some(total) = total else {
            return self.banker_says(id, banker, "No tenes esa cantidad.".into());
        };
        let p = self.player_mut(id);
        p.inv.gold -= amount;
        p.bank.gold = total;
        self.banker_says(id, banker, format!("Tenes {total} monedas de oro en tu cuenta."));
        self.send_gold(id);
    }

    /// HandleBankExtractGold: vault -> hand.
    pub(crate) fn on_bank_withdraw_gold(&mut self, id: u32, amount: u32) {
        let Some(banker) = self.bank_guard(id) else { return };
        let p = self.player(id).expect("logged player");
        if amount > i32::MAX as u32 || amount > p.bank.gold {
            return self.strike(id, "banco: retira mas oro del que hay");
        }
        let hand = p.inv.gold.checked_add(amount).filter(|_| amount > 0);
        let Some(hand) = hand else {
            return self.banker_says(id, banker, "No tenes esa cantidad.".into());
        };
        let p = self.player_mut(id);
        p.bank.gold -= amount;
        p.inv.gold = hand;
        let left = p.bank.gold;
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
}

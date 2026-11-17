//! Trade system. OWNER: Opus B (docs/P2_CONTRATO.md).
//!
//! Merchant shop: port of `modSistemaComercio` (Comercio.bas: `Comercio`, `IniciarComercioNPC`,
//! `SlotEnNPCInv`, `UpdateNpcInvToAll`, `EsSkinNoComprablePorComerciante`), `InvNpc`
//! (Modulo_InventANDobj.bas: `QuitarNpcInvItem`, `CargarInvent`, `QuedanItems`,
//! `EncontrarCant`) and the handlers `HandleCommerceBuy`, `HandleCommerceSell`,
//! `HandleCommerceEnd` (Protocol.bas).
//!
//! Player trade: port of `mdlCOmercioConUsuario` (`IniciarComercioConUsuario`,
//! `EnviarObjetoTransaccion`, `AceptarComercioUsu`, `FinComerciarUsu`) and the handlers
//! `HandleCommerceStart` (player branch), `HandleUserCommerceOffer`, `HandleUserCommerceOk`,
//! `HandleUserCommerceReject`, `HandleUserCommerceEnd`.
//!
//! Differences from the VB, on purpose:
//! - Double confirmation: both sides `Confirm` an offer revision, then both `Accept` it. Any
//!   change of either offer bumps the revision and clears every confirmation (the VB only
//!   had one `Acepto`, cleared on the other side's change).
//! - The exchange is atomic: everything is checked on copies first (holdings, gold, room);
//!   if anything does not fit nothing moves. The VB dropped the overflow on the floor.
//! - Offered items never leave the inventory until the exchange, and equipped items cannot
//!   be offered (the VB only refused the ship and saddle in use).
//! - Walking away, dying or changing map is checked on every operation (the bank does the
//!   same); `close_trades` is there for the systems that can call it right away.
//! - Impossible requests (slot out of range, more than held, more than a stack) are struck
//!   instead of the VB's clamp or ban.
//! - Not ported: `SubirSkill(Comerciar)` (no raise-by-use in this port yet), the key sale
//!   that rewrites NPCs.dat, the `worker_sale_bonus` / `destroy_npc_bought_items` features
//!   (off by default), ElementalTags, privileges and jail.
use crate::world::inventory::InvItem;
use crate::world::trade::{OfferSide, ShopStock};
use crate::world::World;
use ao_formats::dat::npcs::MAX_INVENTORY;
use ao_formats::dat::obj::ObjType;
use ao_protocol::{Tone, TradeC2S, TradeS2C, S2C};
use ao_rules::items::{GOLD_OBJ, MAX_STACK};
use ao_rules::trade::{self as rules, MAX_GOLD, MAX_OFFER_ITEMS, PLAYER_TRADE_RANGE, SKILL_COMMERCE};

/// Max Manhattan distance to a merchant to open its window (interact uses 4).
pub const SHOP_OPEN_RANGE: i32 = 4;
/// Max distance to keep buying/selling; further away the window closes.
pub const SHOP_RANGE: i32 = 6;
/// `otElse`: a merchant with `TipoItems=100` buys anything.
const ANY_TYPE: i32 = 100;

const DEAD: &str = "!!Estas muerto!!";
const BUSY: &str = "Ya estas comerciando.";
const TOO_FAR: &str = "Estas demasiado lejos.";

fn distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

/// `EsSkinNoComprablePorComerciante`.
fn is_skin(t: ObjType) -> bool {
    matches!(t, ObjType::SkinsWings | ObjType::SkinsArmours | ObjType::SkinsShields | ObjType::SkinsHelmets | ObjType::SkinsWeapons | ObjType::SkinsBoats | ObjType::SkinsSpells)
}

/// GetSlotForItemInInventory on a copy: a stack with room for all of it, else an empty slot.
fn put(slots: &mut [Option<InvItem>], item: InvItem) -> bool {
    let at = slots.iter().position(|s| s.is_some_and(|i| i.obj == item.obj && i.amount + item.amount <= MAX_STACK)).or_else(|| slots.iter().position(Option::is_none));
    match at {
        Some(i) => {
            let cur = slots[i].map_or(0, |c| c.amount);
            slots[i] = Some(InvItem { obj: item.obj, amount: cur + item.amount });
            true
        }
        None => false,
    }
}

impl World {
    pub(crate) fn on_trade(&mut self, id: u32, msg: TradeC2S) {
        match msg {
            TradeC2S::Buy { slot, amount } => self.shop_buy(id, slot, amount),
            TradeC2S::Sell { slot, amount } => self.shop_sell(id, slot, amount),
            TradeC2S::CloseShop => self.close_shop(id),
            TradeC2S::Propose { target } => self.trade_propose(id, target),
            TradeC2S::Decline => self.trade_decline(id),
            TradeC2S::Offer { slot, amount } => self.trade_offer(id, slot, amount),
            TradeC2S::OfferGold { amount } => self.trade_offer_gold(id, amount),
            TradeC2S::Retract { index } => self.trade_retract(id, index),
            TradeC2S::Confirm { rev } => self.trade_confirm(id, rev),
            TradeC2S::Accept { rev } => self.trade_accept(id, rev),
            TradeC2S::Cancel => self.cancel_player_trade(id, true),
        }
    }

    /// Called when the player enters the world (after everything else is sent).
    pub(crate) fn trade_on_enter(&mut self, id: u32) {
        let _ = id;
    }

    /// Called right before the player leaves the world (logout or kick), still present.
    /// Offers never left the inventory, so there is nothing to give back: the other side
    /// only sees the trade cancelled.
    pub(crate) fn trade_on_leave(&mut self, id: u32) {
        self.close_trades(id);
    }

    /// Closes every trade of `id` (shop, player trade, invitations). For the systems that
    /// know at once: death, map change, logout. Safe to call at any time.
    pub(crate) fn close_trades(&mut self, id: u32) {
        self.close_shop(id);
        self.cancel_player_trade(id, true);
        self.clear_invitations(id);
    }

    /// Any kind of trade open (`flags.Comerciando`), the vault included.
    fn busy(&self, id: u32) -> bool {
        self.player(id).is_some_and(|p| p.bank.open_with.is_some() || p.trade.shop.is_some() || p.trade.with.is_some())
    }

    fn skill_commerce(&self, id: u32) -> u8 {
        self.player(id).map_or(0, |p| p.stats.skills[SKILL_COMMERCE])
    }

    // ================================================================ merchant shop

    /// A live merchant on `map`: (position, npcs.dat number).
    fn merchant(&self, map: u16, npc: u32) -> Option<((i32, i32), u16)> {
        let n = self.maps.get(&map)?.npcs.get(&npc)?;
        let d = self.ao.npcs.get(n.def as u32)?;
        (n.alive() && d.trades == 1).then_some(((n.x, n.y), n.def))
    }

    /// `CargarInvent`: the npcs.dat stock (`NROITEMS` + `ObjN=obj-amount`).
    fn original_stock(&self, def: u16) -> ShopStock {
        let mut slots = vec![None; MAX_INVENTORY];
        if let Some(d) = self.ao.npcs.get(def as u32) {
            for (i, it) in d.inventory.iter().take(MAX_INVENTORY).enumerate() {
                if it.obj > 0 && it.amount > 0 && it.obj <= u16::MAX as i32 {
                    slots[i] = Some(InvItem { obj: it.obj as u16, amount: it.amount as u32 });
                }
            }
        }
        ShopStock { slots }
    }

    fn stock(&mut self, npc: u32, def: u16) -> &mut ShopStock {
        if !self.trades.stocks.contains_key(&npc) {
            let s = self.original_stock(def);
            self.trades.stocks.insert(npc, s);
        }
        self.trades.stocks.get_mut(&npc).expect("just inserted")
    }

    fn shop_slot_msg(&self, npc: u32, slot: u8, skill: u8) -> TradeS2C {
        let it = self.trades.stocks.get(&npc).and_then(|s| s.slots.get(slot as usize - 1).copied().flatten());
        match it {
            Some(it) => {
                let value = self.ao.objs.get(it.obj as u32).map_or(0, |o| o.value);
                TradeS2C::ShopSlot { slot, obj: it.obj, amount: it.amount, price: rules::unit_buy_price(value, skill) }
            }
            None => TradeS2C::ShopSlot { slot, obj: 0, amount: 0, price: 0 },
        }
    }

    /// `UpdateNpcInvToAll`: every player with this merchant's window open sees the slots.
    fn shop_changed(&self, npc: u32, slots: &[u8]) {
        for pid in self.player_ids() {
            if self.player(pid).is_some_and(|p| p.trade.shop == Some(npc)) {
                let skill = self.skill_commerce(pid);
                for &s in slots {
                    self.send(pid, &S2C::Trade(self.shop_slot_msg(npc, s, skill)));
                }
            }
        }
    }

    /// HOOK (interact, NPC system): the player double-clicked a merchant (`Comercia=1`)
    /// within range and alive. `HandleCommerceStart` (NPC branch) + `IniciarComercioNPC`.
    pub(crate) fn open_npc_trade(&mut self, id: u32, npc: u32) {
        let Some(p) = self.player(id) else { return };
        if p.combat.dead {
            return self.console(id, DEAD, Tone::Info);
        }
        if self.busy(id) {
            return self.console(id, BUSY, Tone::Info);
        }
        let (map, pos) = (p.map, (p.x, p.y));
        let Some((npos, def)) = self.merchant(map, npc) else {
            return self.console(id, "No hay ningun comerciante disponible.", Tone::Info);
        };
        if distance(pos, npos) > SHOP_OPEN_RANGE {
            return self.console(id, TOO_FAR, Tone::Info);
        }
        self.clear_invitations(id);
        self.stock(npc, def);
        self.player_mut(id).trade.shop = Some(npc);
        let skill = self.skill_commerce(id);
        let stock = &self.trades.stocks[&npc];
        let items = stock
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.map(|it| ((i + 1) as u8, it.obj, it.amount, rules::unit_buy_price(self.ao.objs.get(it.obj as u32).map_or(0, |o| o.value), skill))))
            .collect();
        self.send(id, &S2C::Trade(TradeS2C::ShopOpen { npc, items }));
    }

    /// `HandleCommerceEnd`.
    pub(crate) fn close_shop(&mut self, id: u32) {
        let Some(p) = self.sessions.get_mut(&id).and_then(|s| s.player.as_mut()) else { return };
        if p.trade.shop.take().is_some() {
            self.send(id, &S2C::Trade(TradeS2C::ShopClosed));
        }
    }

    /// What every shop operation checks first: alive, window open, merchant still alive on
    /// this map and near. Closes the window otherwise. Returns (npc, npcs.dat number).
    fn shop_guard(&mut self, id: u32) -> Option<(u32, u16)> {
        let p = self.player(id)?;
        let Some(npc) = p.trade.shop else {
            self.console(id, "No estas comerciando.", Tone::Info);
            return None;
        };
        if p.combat.dead {
            self.console(id, DEAD, Tone::Info);
            self.close_shop(id);
            return None;
        }
        match self.merchant(p.map, npc) {
            Some((npos, def)) if distance((p.x, p.y), npos) <= SHOP_RANGE => Some((npc, def)),
            _ => {
                self.console(id, TOO_FAR, Tone::Info);
                self.close_shop(id);
                None
            }
        }
    }

    /// `Comercio` (Compra).
    fn shop_buy(&mut self, id: u32, slot: u8, amount: u32) {
        let Some((npc, def)) = self.shop_guard(id) else { return };
        if slot == 0 || slot as usize > MAX_INVENTORY {
            return self.strike(id, "comercio: casilla del comerciante invalida");
        }
        if amount > MAX_STACK {
            // The VB bans for this ("Quiso comprar demasiados items").
            return self.strike(id, "comercio: compra demasiados objetos");
        }
        if amount == 0 {
            return;
        }
        let Some(item) = self.stock(npc, def).slots[slot as usize - 1] else {
            return self.console(id, "Ese objeto ya no esta a la venta.", Tone::Info);
        };
        let Some(data) = self.ao.objs.get(item.obj as u32) else { return };
        let original = self.original_stock(def);
        let infinite = data.crucial == 1 && original.slots.iter().flatten().any(|o| o.obj == item.obj);
        let amount = if infinite { amount } else { amount.min(item.amount) };
        let price = rules::buy_price(data.value, self.skill_commerce(id), amount);
        let p = self.player(id).expect("logged player");
        if (p.inv.gold as u64) < price {
            return self.console(id, "No tienes suficiente dinero.", Tone::Warn);
        }
        if !self.add_item(id, InvItem { obj: item.obj, amount }) {
            return;
        }
        let p = self.player_mut(id);
        p.inv.gold -= price as u32;
        self.send_inventory_vitals(id);
        let changed = self.take_from_stock(npc, def, slot, amount);
        self.shop_changed(npc, &changed);
    }

    /// `QuitarNpcInvItem`: takes from a merchant slot. A `Crucial` object the merchant sells
    /// comes back to its npcs.dat amount when it runs out (`QuedanItems` / `EncontrarCant`),
    /// and an empty merchant restocks everything unless `InvReSpawn=1`. Returns the changed
    /// slots.
    fn take_from_stock(&mut self, npc: u32, def: u16, slot: u8, amount: u32) -> Vec<u8> {
        let original = self.original_stock(def);
        let crucial = |w: &World, obj: u16| w.ao.objs.get(obj as u32).is_some_and(|o| o.crucial == 1);
        let respawn_off = self.ao.npcs.get(def as u32).is_some_and(|d| d.inventory_respawn == 1);
        let Some(item) = self.stock(npc, def).slots[slot as usize - 1] else { return vec![] };
        let is_crucial = crucial(self, item.obj);
        let stock = self.stock(npc, def);
        let i = slot as usize - 1;
        if item.amount > amount {
            stock.slots[i] = Some(InvItem { obj: item.obj, amount: item.amount - amount });
            return vec![slot];
        }
        stock.slots[i] = None;
        if is_crucial && !stock.slots.iter().flatten().any(|s| s.obj == item.obj) {
            if let Some(o) = original.slots.iter().flatten().find(|o| o.obj == item.obj) {
                stock.slots[i] = Some(*o);
            }
        }
        if stock.slots.iter().all(Option::is_none) && !respawn_off {
            *stock = original;
            return (1..=MAX_INVENTORY as u8).collect();
        }
        vec![slot]
    }

    /// `Comercio` (Venta).
    fn shop_sell(&mut self, id: u32, slot: u8, amount: u32) {
        let Some((npc, def)) = self.shop_guard(id) else { return };
        let p = self.player(id).expect("logged player");
        if slot == 0 || slot > p.inv.usable_slots() {
            return self.strike(id, "comercio: casilla de inventario invalida");
        }
        let Some(item) = p.inv.get(slot) else {
            return self.console(id, "No hay nada en esa casilla.", Tone::Info);
        };
        if amount == 0 {
            return;
        }
        if amount > item.amount {
            return self.strike(id, "comercio: vende mas de lo que tiene");
        }
        let ao = self.ao.clone();
        let Some(data) = ao.objs.get(item.obj as u32) else { return };
        const NO_BUY: &str = "Lo siento, no puedo comprarte ese item.";
        if is_skin(data.obj_type) || data.destroys == 1 {
            return self.console(id, NO_BUY, Tone::Warn);
        }
        if data.newbie == 1 {
            return self.console(id, "Lo siento, no comercio objetos para newbies.", Tone::Warn);
        }
        if data.non_transferable == 1 || data.no_tradeable_to_npc {
            return self.console(id, "Lo siento, no puedo comprarte ese item. Es intransferible.", Tone::Warn);
        }
        let wants = self.ao.npcs.get(def as u32).map_or(0, |d| d.item_type);
        if (wants != data.obj_type.id() && wants != ANY_TYPE) || item.obj == GOLD_OBJ {
            let sells = self.stock(npc, def).slots.iter().flatten().any(|s| s.obj == item.obj);
            if !sells {
                return self.console(id, "Lo siento, no estoy interesado en este tipo de objetos.", Tone::Warn);
            }
        }
        let earned = rules::sale_price(data.value, data.newbie == 1, amount);
        self.remove_from_slot(id, slot, amount);
        let p = self.player_mut(id);
        p.inv.gold = rules::add_gold(p.inv.gold, earned);
        self.send_inventory_vitals(id);
        // SlotEnNPCInv: a stack of the same object with room, else (only if none) a free slot.
        let stock = self.stock(npc, def);
        let same: Vec<usize> = (0..MAX_INVENTORY).filter(|&i| stock.slots[i].is_some_and(|s| s.obj == item.obj)).collect();
        let dest = if same.is_empty() { stock.slots.iter().position(Option::is_none) } else { same.into_iter().find(|&i| stock.slots[i].is_some_and(|s| s.amount < MAX_STACK)) };
        if let Some(i) = dest {
            let cur = stock.slots[i].map_or(0, |s| s.amount);
            stock.slots[i] = Some(InvItem { obj: item.obj, amount: (cur + amount).min(MAX_STACK) });
            self.shop_changed(npc, &[i as u8 + 1]);
        }
    }

    // ================================================================ player trade

    fn rev_key(a: u32, b: u32) -> u32 {
        a.min(b)
    }

    fn rev_of(&self, id: u32) -> u32 {
        let Some(o) = self.player(id).and_then(|p| p.trade.with) else { return 0 };
        self.trades.revs.get(&Self::rev_key(id, o)).copied().unwrap_or(0)
    }

    /// Forgets invitations sent by or to `id`.
    fn clear_invitations(&mut self, id: u32) {
        let Some(p) = self.sessions.get_mut(&id).and_then(|s| s.player.as_mut()) else { return };
        let (to, from) = (p.trade.proposed_to.take(), p.trade.invited_by.take());
        if let Some(t) = to.and_then(|t| self.sessions.get_mut(&t)).and_then(|s| s.player.as_mut()) {
            if t.trade.invited_by == Some(id) {
                t.trade.invited_by = None;
            }
        }
        if let Some(f) = from.and_then(|f| self.sessions.get_mut(&f)).and_then(|s| s.player.as_mut()) {
            if f.trade.proposed_to == Some(id) {
                f.trade.proposed_to = None;
            }
        }
    }

    /// `HandleCommerceStart` (player branch) + `IniciarComercioConUsuario`: proposes, or
    /// opens the trade when `target` had already proposed to `id`.
    fn trade_propose(&mut self, id: u32, target: u32) {
        let Some(p) = self.player(id) else { return };
        if target == id {
            return self.console(id, "No podes comerciar con vos mismo.", Tone::Info);
        }
        if p.combat.dead {
            return self.console(id, DEAD, Tone::Info);
        }
        if self.busy(id) {
            return self.console(id, BUSY, Tone::Info);
        }
        let (map, pos) = (p.map, (p.x, p.y));
        let Some(t) = self.player(target) else {
            return self.console(id, "Primero haz click izquierdo sobre el personaje.", Tone::Info);
        };
        if t.combat.dead {
            return self.console(id, "No podes comerciar con los muertos.", Tone::Info);
        }
        if t.map != map || distance(pos, (t.x, t.y)) > PLAYER_TRADE_RANGE {
            return self.console(id, TOO_FAR, Tone::Info);
        }
        if !self.instance(map).map.safe {
            return self.console(id, "No se puede usar el comercio seguro en zona insegura.", Tone::Warn);
        }
        if self.busy(target) {
            return self.console(id, "No podes comerciar con ese usuario en este momento.", Tone::Info);
        }
        let (my_name, their_name) = (p.name.clone(), t.name.clone());
        let accepting = t.trade.proposed_to == Some(id);
        self.clear_invitations(id);
        if accepting {
            self.clear_invitations(target);
            for (a, b) in [(id, target), (target, id)] {
                let pa = self.player_mut(a);
                pa.trade.with = Some(b);
                pa.trade.offer = OfferSide::default();
            }
            self.trades.revs.insert(Self::rev_key(id, target), 1);
            self.send(id, &S2C::Trade(TradeS2C::Opened { with: target, name: their_name }));
            self.send(target, &S2C::Trade(TradeS2C::Opened { with: id, name: my_name }));
            self.send_offers(id);
        } else {
            self.player_mut(id).trade.proposed_to = Some(target);
            let t = self.player_mut(target);
            if let Some(old) = t.trade.invited_by.replace(id) {
                if let Some(o) = self.sessions.get_mut(&old).and_then(|s| s.player.as_mut()) {
                    if o.trade.proposed_to == Some(target) {
                        o.trade.proposed_to = None;
                    }
                }
            }
            self.send(target, &S2C::Trade(TradeS2C::Invited { from: id, name: my_name.clone() }));
            self.console(target, format!("{my_name} desea comerciar contigo. Aceptas?"), Tone::Info);
            self.console(id, format!("Le pediste comerciar a {their_name}."), Tone::Info);
        }
    }

    /// `HandleUserCommerceReject` for a pending invitation.
    fn trade_decline(&mut self, id: u32) {
        let Some(from) = self.player(id).and_then(|p| p.trade.invited_by) else { return };
        let name = self.player(id).map(|p| p.name.clone()).unwrap_or_default();
        self.clear_invitations(id);
        self.console(from, format!("{name} ha rechazado tu oferta."), Tone::Info);
    }

    /// Ends the player trade of `id` and its partner (`FinComerciarUsu` on both).
    /// `notify`: tell the partner it was cancelled (`HandleUserCommerceEnd`).
    fn cancel_player_trade(&mut self, id: u32, notify: bool) {
        let Some(other) = self.player(id).and_then(|p| p.trade.with) else { return };
        let name = self.player(id).map(|p| p.name.clone()).unwrap_or_default();
        self.end_player_trade(id, other, false);
        if notify {
            self.console(other, format!("{name} ha dejado de comerciar con vos."), Tone::Info);
        }
    }

    fn end_player_trade(&mut self, a: u32, b: u32, done: bool) {
        self.trades.revs.remove(&Self::rev_key(a, b));
        for (x, y) in [(a, b), (b, a)] {
            if let Some(p) = self.sessions.get_mut(&x).and_then(|s| s.player.as_mut()) {
                if p.trade.with == Some(y) {
                    p.trade.with = None;
                    p.trade.offer = OfferSide::default();
                    self.send(x, &S2C::Trade(TradeS2C::Closed { done }));
                }
            }
        }
    }

    /// Checks of every player trade operation: partner online and trading back, both alive,
    /// same map and near. Cancels the trade otherwise. Returns the partner.
    fn pair_guard(&mut self, id: u32) -> Option<u32> {
        let p = self.player(id)?;
        let Some(other) = p.trade.with else {
            self.console(id, "No estas comerciando.", Tone::Info);
            return None;
        };
        let ok = match self.player(other) {
            Some(o) if o.trade.with == Some(id) => {
                if p.combat.dead || o.combat.dead {
                    Err("El comercio se cancelo: alguien esta muerto.")
                } else if o.map != p.map || distance((p.x, p.y), (o.x, o.y)) > PLAYER_TRADE_RANGE {
                    Err("El comercio se cancelo porque estan demasiado lejos.")
                } else {
                    Ok(other)
                }
            }
            _ => Err("El comercio se cancelo."),
        };
        match ok {
            Ok(o) => Some(o),
            Err(why) => {
                self.console(id, why, Tone::Warn);
                self.console(other, why, Tone::Warn);
                self.end_player_trade(id, other, false);
                None
            }
        }
    }

    /// Units of `obj` that can be offered: unequipped slots only.
    fn tradeable(&self, id: u32, obj: u16) -> u64 {
        let Some(p) = self.player(id) else { return 0 };
        (1..=p.inv.usable_slots())
            .filter(|s| !p.inv.equipped.is_equipped(*s) && !p.inv.pending_equipped.contains(s))
            .filter_map(|s| p.inv.get(s))
            .filter(|i| i.obj == obj)
            .map(|i| i.amount as u64)
            .sum()
    }

    /// An offer changed: new revision, every confirmation cleared, both sides told.
    fn offer_changed(&mut self, id: u32, other: u32) {
        let rev = self.trades.revs.entry(Self::rev_key(id, other)).or_insert(0);
        *rev += 1;
        let mut was_confirmed = false;
        for x in [id, other] {
            let o = &mut self.player_mut(x).trade.offer;
            was_confirmed |= o.confirmed || o.accepted;
            o.confirmed = false;
            o.accepted = false;
        }
        if was_confirmed {
            let name = self.player(id).map(|p| p.name.clone()).unwrap_or_default();
            self.console(other, format!("{name} ha cambiado su oferta."), Tone::Warn);
        }
        self.send_offers(id);
    }

    /// Both offers and the confirmation state, to both sides.
    fn send_offers(&self, id: u32) {
        let Some(other) = self.player(id).and_then(|p| p.trade.with) else { return };
        let rev = self.rev_of(id);
        for (me, them) in [(id, other), (other, id)] {
            let (Some(a), Some(b)) = (self.player(me), self.player(them)) else { continue };
            for (mine, side) in [(true, &a.trade.offer), (false, &b.trade.offer)] {
                let items = side.items.iter().map(|i| (i.obj, i.amount)).collect();
                self.send(me, &S2C::Trade(TradeS2C::Offer { mine, items, gold: side.gold, rev }));
            }
            let (x, y) = (&a.trade.offer, &b.trade.offer);
            self.send(me, &S2C::Trade(TradeS2C::Status { rev, i_confirmed: x.confirmed, they_confirmed: y.confirmed, i_accepted: x.accepted, they_accepted: y.accepted }));
        }
    }

    /// `HandleUserCommerceOffer` + `EnviarObjetoTransaccion` (items).
    fn trade_offer(&mut self, id: u32, slot: u8, amount: u32) {
        let Some(other) = self.pair_guard(id) else { return };
        let p = self.player(id).expect("logged player");
        if slot == 0 || slot > p.inv.usable_slots() {
            return self.strike(id, "comercio: casilla de inventario invalida");
        }
        let Some(item) = p.inv.get(slot) else {
            return self.console(id, "No hay nada en esa casilla.", Tone::Info);
        };
        if amount == 0 {
            return;
        }
        if amount > item.amount {
            return self.strike(id, "comercio: ofrece mas de lo que tiene");
        }
        if p.inv.equipped.is_equipped(slot) || p.inv.pending_equipped.contains(&slot) {
            return self.console(id, "Desequipa el objeto antes de ofrecerlo.", Tone::Warn);
        }
        let Some(data) = self.ao.objs.get(item.obj as u32) else { return };
        if data.non_transferable == 1 {
            return self.console(id, "No podes comerciar un objeto intransferible.", Tone::Warn);
        }
        if data.newbie == 1 {
            return self.console(id, "No puedes comerciar objetos newbie.", Tone::Warn);
        }
        let offered: u64 = p.trade.offer.items.iter().filter(|i| i.obj == item.obj).map(|i| i.amount as u64).sum();
        if offered + amount as u64 > self.tradeable(id, item.obj) {
            return self.console(id, "No tienes esa cantidad disponible para agregar.", Tone::Warn);
        }
        let offer = &mut self.player_mut(id).trade.offer;
        if let Some(e) = offer.items.iter_mut().find(|e| e.obj == item.obj && e.amount + amount <= MAX_STACK) {
            e.amount += amount;
        } else if offer.items.len() < MAX_OFFER_ITEMS {
            offer.items.push(InvItem { obj: item.obj, amount });
        } else {
            return self.console(id, "No tienes suficiente lugar para agregar esa cantidad o item.", Tone::Warn);
        }
        self.offer_changed(id, other);
    }

    /// `EnviarObjetoTransaccion` (gold, slot `FLAGORO`): sets the offered gold.
    fn trade_offer_gold(&mut self, id: u32, amount: u32) {
        let Some(other) = self.pair_guard(id) else { return };
        let p = self.player(id).expect("logged player");
        if amount > p.inv.gold {
            return self.console(id, "No tienes esa cantidad.", Tone::Warn);
        }
        if p.trade.offer.gold == amount {
            return;
        }
        self.player_mut(id).trade.offer.gold = amount;
        self.offer_changed(id, other);
    }

    fn trade_retract(&mut self, id: u32, index: u8) {
        let Some(other) = self.pair_guard(id) else { return };
        let offer = &mut self.player_mut(id).trade.offer;
        if index as usize >= offer.items.len() {
            return;
        }
        offer.items.remove(index as usize);
        self.offer_changed(id, other);
    }

    /// First confirmation of the current revision.
    fn trade_confirm(&mut self, id: u32, rev: u32) {
        let Some(other) = self.pair_guard(id) else { return };
        if rev != self.rev_of(id) {
            return self.console(id, "La oferta cambio: revisala antes de confirmar.", Tone::Warn);
        }
        let o = &mut self.player_mut(id).trade.offer;
        if o.confirmed {
            return;
        }
        o.confirmed = true;
        if !self.player(other).is_some_and(|p| p.trade.offer.confirmed) {
            self.console(id, "El otro usuario aun no ha confirmado la oferta.", Tone::Info);
        }
        self.send_offers(id);
    }

    /// `HandleUserCommerceOk` / `AceptarComercioUsu`: second confirmation; the exchange
    /// happens when both accepted the same revision.
    fn trade_accept(&mut self, id: u32, rev: u32) {
        let Some(other) = self.pair_guard(id) else { return };
        if rev != self.rev_of(id) {
            return self.console(id, "La oferta cambio: revisala antes de aceptar.", Tone::Warn);
        }
        let both_confirmed = [id, other].iter().all(|x| self.player(*x).is_some_and(|p| p.trade.offer.confirmed));
        if !both_confirmed {
            return self.console(id, "Ambos deben confirmar la oferta antes de aceptar.", Tone::Info);
        }
        self.player_mut(id).trade.offer.accepted = true;
        if !self.player(other).is_some_and(|p| p.trade.offer.accepted) {
            self.console(id, "El otro usuario aun no ha aceptado tu oferta.", Tone::Info);
            return self.send_offers(id);
        }
        self.exchange(id, other);
    }

    /// The exchange, all or nothing: works on copies of both inventories and purses and only
    /// writes them back if every item and coin fits.
    fn exchange(&mut self, a: u32, b: u32) {
        let (Some(pa), Some(pb)) = (self.player(a), self.player(b)) else { return };
        let (oa, ob) = (pa.trade.offer.clone(), pb.trade.offer.clone());
        type Side = (Vec<Option<InvItem>>, u32);
        let plan = |w: &World| -> Result<[Side; 2], (u32, &'static str)> {
            let mut out = Vec::new();
            for (me, give, get) in [(a, &oa, &ob), (b, &ob, &oa)] {
                let p = w.player(me).ok_or((me, "El comercio se cancelo."))?;
                if give.gold > p.inv.gold {
                    return Err((me, "No tienes esa cantidad de oro."));
                }
                let gold = p.inv.gold as u64 - give.gold as u64 + get.gold as u64;
                if gold > MAX_GOLD as u64 {
                    return Err((me, "No podes tener tanto oro."));
                }
                let cap = p.inv.usable_slots() as usize;
                let mut slots = p.inv.slots.clone();
                for it in &give.items {
                    if w.tradeable(me, it.obj) < it.amount as u64 {
                        return Err((me, "No tienes esa cantidad disponible para ofrecer."));
                    }
                }
                // Take what is given from unequipped slots, last slots first.
                for it in &give.items {
                    let mut left = it.amount;
                    for s in (1..=cap as u8).rev() {
                        if left == 0 {
                            break;
                        }
                        if p.inv.equipped.is_equipped(s) || p.inv.pending_equipped.contains(&s) {
                            continue;
                        }
                        let cell = &mut slots[s as usize - 1];
                        if let Some(c) = cell.filter(|c| c.obj == it.obj) {
                            let take = left.min(c.amount);
                            left -= take;
                            *cell = (c.amount > take).then_some(InvItem { obj: c.obj, amount: c.amount - take });
                        }
                    }
                    if left > 0 {
                        return Err((me, "No tienes esa cantidad disponible para ofrecer."));
                    }
                }
                for it in &get.items {
                    if !put(&mut slots[..cap], *it) {
                        return Err((me, "No tienes lugar en el inventario para este comercio."));
                    }
                }
                out.push((slots, gold as u32));
            }
            let mut it = out.into_iter();
            Ok([it.next().unwrap(), it.next().unwrap()])
        };
        match plan(self) {
            Err((who, why)) => {
                let other = if who == a { b } else { a };
                self.console(who, why, Tone::Warn);
                self.console(other, "El comercio se cancelo: el otro usuario no puede completarlo.", Tone::Warn);
                self.end_player_trade(a, b, false);
            }
            Ok([(sa, ga), (sb, gb)]) => {
                for (me, slots, gold) in [(a, sa, ga), (b, sb, gb)] {
                    let p = self.player_mut(me);
                    p.inv.slots = slots;
                    p.inv.gold = gold;
                }
                self.end_player_trade(a, b, true);
                for me in [a, b] {
                    self.send_inventory(me);
                    self.send_inventory_vitals(me);
                    self.console(me, "Comercio finalizado con exito!", Tone::Good);
                    if let Some(p) = self.player(me) {
                        self.save_player(p);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::CharacterState;
    use crate::world::Session;
    use crate::Settings;
    use ao_protocol::{FrameReader, MAX_SERVER_FRAME};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    const MERCHANT: u32 = 200_000;
    const A: u32 = 1;
    const B: u32 = 2;

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

    struct Rig {
        w: World,
        rx: Vec<UnboundedReceiver<Vec<u8>>>,
        at: (i32, i32),
        def: u16,
    }

    fn join(w: &mut World, id: u32, at: (i32, i32)) -> UnboundedReceiver<Vec<u8>> {
        let (tx, rx) = unbounded_channel();
        w.sessions.insert(id, Session::new(tx, "127.0.0.1:1".parse().unwrap()));
        let rec = crate::db::CharacterRecord { id: 0, account_id: 0, name: format!("P{id}"), race: 1, class: 1, gender: 1, body: 1, head: 1, level: 1, exp: 0, hp: 15, max_hp: 15, mana: 0, max_mana: 0, map: 1, x: at.0, y: at.1 };
        assert!(w.spawn_player(id, &rec, &CharacterState::default(), None));
        rx
    }

    /// A merchant of Ullathorpe from the official map with a stock in npcs.dat, and two
    /// players next to it.
    fn rig() -> Rig {
        let mut w = World::new(Settings::defaults());
        assert!(w.ensure_map(1));
        let m = w.maps[&1].map.npcs.iter().find(|n| w.ao.npcs.get(n.npc as u32).is_some_and(|d| d.trades == 1 && !d.inventory.is_empty())).copied().expect("a merchant in Ullathorpe");
        let npc = w.make_npc(m.npc, m.x, m.y).unwrap();
        w.maps.get_mut(&1).unwrap().add_npc(MERCHANT, npc);
        let at = w.free_tile_near(1, m.x, m.y + 1, 2);
        let mut rx = vec![join(&mut w, A, at)];
        let at2 = w.free_tile_near(1, at.0, at.1, 2);
        rx.push(join(&mut w, B, at2));
        for r in &mut rx {
            drain(r);
        }
        Rig { w, rx, at, def: m.npc }
    }

    impl Rig {
        fn p(&self, id: u32) -> &crate::world::entities::Player {
            self.w.player(id).unwrap()
        }
        fn give(&mut self, id: u32, slot: u8, obj: u16, amount: u32) {
            self.w.player_mut(id).inv.slots[slot as usize - 1] = Some(InvItem { obj, amount });
        }
        fn first_stock(&self) -> (u8, InvItem) {
            let s = &self.w.trades.stocks[&MERCHANT];
            s.slots.iter().enumerate().find_map(|(i, s)| s.map(|it| ((i + 1) as u8, it))).unwrap()
        }
        fn msgs(&mut self, i: usize) -> Vec<S2C> {
            drain(&mut self.rx[i])
        }
        fn open_pair(&mut self) {
            self.w.trade_propose(A, B);
            self.w.trade_propose(B, A);
            assert_eq!(self.p(A).trade.with, Some(B));
            assert_eq!(self.p(B).trade.with, Some(A));
        }
        fn rev(&self) -> u32 {
            self.w.rev_of(A)
        }
        fn confirm_all(&mut self) {
            let r = self.rev();
            self.w.trade_confirm(A, r);
            self.w.trade_confirm(B, r);
            self.w.trade_accept(A, r);
            self.w.trade_accept(B, r);
        }
    }

    #[test]
    fn opening_shows_the_official_stock_with_prices() {
        let mut r = rig();
        r.w.open_npc_trade(A, MERCHANT);
        let m = r.msgs(0);
        let Some(S2C::Trade(TradeS2C::ShopOpen { items, .. })) = m.iter().find(|m| matches!(m, S2C::Trade(TradeS2C::ShopOpen { .. }))) else { panic!("{m:?}") };
        let d = r.w.ao.npcs.get(r.def as u32).unwrap();
        assert_eq!(items.len(), d.inventory.iter().filter(|i| i.obj > 0 && i.amount > 0).count());
        let (slot, obj, _, price) = items[0];
        let value = r.w.ao.objs.get(obj as u32).unwrap().value;
        assert_eq!((slot, price), (1, rules::unit_buy_price(value, 0)));
        // Busy: the vault or another shop cannot be opened on top.
        r.w.open_npc_trade(A, MERCHANT);
        assert!(r.msgs(0).iter().all(|m| !matches!(m, S2C::Trade(_))));
    }

    #[test]
    fn buying_charges_the_official_price_and_needs_gold_and_room() {
        let mut r = rig();
        r.w.open_npc_trade(A, MERCHANT);
        let (slot, item) = r.first_stock();
        let value = r.w.ao.objs.get(item.obj as u32).unwrap().value;
        let price = rules::buy_price(value, 0, 1) as u32;
        // No gold: refused.
        r.w.shop_buy(A, slot, 1);
        assert!(r.p(A).inv.slots.iter().all(Option::is_none));
        r.w.player_mut(A).inv.gold = price + 5;
        r.w.shop_buy(A, slot, 1);
        assert_eq!(r.p(A).inv.gold, 5);
        assert!(r.p(A).inv.slots.iter().flatten().any(|i| i.obj == item.obj && i.amount == 1));
        // The skill discounts.
        r.w.player_mut(A).stats.skills[SKILL_COMMERCE] = 100;
        r.w.player_mut(A).inv.gold = 1_000_000;
        r.w.shop_buy(A, slot, 2);
        assert_eq!(r.p(A).inv.gold, 1_000_000 - rules::buy_price(value, 100, 2) as u32);
        // Full inventory: nothing is charged.
        for s in 1..=24 {
            r.give(A, s, 9000 + s as u16, 1);
        }
        let gold = r.p(A).inv.gold;
        r.w.shop_buy(A, slot, 1);
        assert_eq!(r.p(A).inv.gold, gold, "no room, no charge");
        assert!(r.w.player(A).is_some());
    }

    #[test]
    fn stock_runs_out_and_restocks() {
        let mut r = rig();
        r.w.open_npc_trade(A, MERCHANT);
        r.w.player_mut(A).inv.gold = MAX_GOLD;
        let (slot, item) = r.first_stock();
        let crucial = r.w.ao.objs.get(item.obj as u32).unwrap().crucial == 1;
        r.w.shop_buy(A, slot, item.amount.min(MAX_STACK));
        let after = r.w.trades.stocks[&MERCHANT].slots[slot as usize - 1];
        if crucial {
            assert_eq!(after, Some(item), "crucial objects come back");
        } else if r.w.trades.stocks[&MERCHANT].slots.iter().flatten().count() > 0 {
            assert!(after.is_none() || after == Some(item), "slot emptied or restocked");
        }
        // B, watching the same merchant, is told about the change.
        let _ = r.msgs(1);
    }

    #[test]
    fn selling_pays_a_third_and_respects_what_the_merchant_buys() {
        let mut r = rig();
        r.w.open_npc_trade(A, MERCHANT);
        let (_, item) = r.first_stock();
        let data = r.w.ao.objs.get(item.obj as u32).unwrap().clone();
        r.give(A, 1, item.obj, 3);
        r.w.shop_sell(A, 1, 3);
        if data.newbie == 0 && data.non_transferable == 0 && data.destroys == 0 && !data.no_tradeable_to_npc && !is_skin(data.obj_type) {
            assert_eq!(r.p(A).inv.gold, rules::sale_price(data.value, false, 3) as u32);
            assert!(r.p(A).inv.get(1).is_none());
        }
        // Something of another type the merchant does not stock: refused.
        let wants = r.w.ao.npcs.get(r.def as u32).unwrap().item_type;
        if wants != ANY_TYPE {
            let stocked: Vec<u16> = r.w.trades.stocks[&MERCHANT].slots.iter().flatten().map(|i| i.obj).collect();
            let other = r.w.ao.objs.iter().find(|(i, o)| o.obj_type.id() != wants && o.newbie == 0 && o.value > 0 && !stocked.contains(&(*i as u16)) && *i as u16 != GOLD_OBJ).map(|(i, _)| i as u16).unwrap();
            r.give(A, 2, other, 1);
            let gold = r.p(A).inv.gold;
            r.w.shop_sell(A, 2, 1);
            assert_eq!(r.p(A).inv.get(2), Some(InvItem { obj: other, amount: 1 }));
            assert_eq!(r.p(A).inv.gold, gold);
        }
    }

    #[test]
    fn walking_away_or_dying_closes_the_shop() {
        let mut r = rig();
        r.w.open_npc_trade(A, MERCHANT);
        r.w.player_mut(A).y += 10;
        r.w.player_mut(A).inv.gold = 1000;
        let (slot, _) = r.first_stock();
        r.w.shop_buy(A, slot, 1);
        assert!(r.p(A).trade.shop.is_none());
        assert_eq!(r.p(A).inv.gold, 1000);
        r.w.player_mut(A).y -= 10;
        r.w.open_npc_trade(A, MERCHANT);
        r.w.player_mut(A).combat.dead = true;
        r.w.shop_buy(A, slot, 1);
        assert!(r.p(A).trade.shop.is_none());
        assert_eq!(r.p(A).inv.gold, 1000);
    }

    #[test]
    fn player_trade_swaps_items_and_gold_after_double_confirmation() {
        let mut r = rig();
        r.give(A, 1, 38, 10);
        r.w.player_mut(B).inv.gold = 500;
        r.open_pair();
        r.w.trade_offer(A, 1, 4);
        r.w.trade_offer_gold(B, 300);
        // Accepting before both confirmed does nothing.
        let rev = r.rev();
        r.w.trade_accept(A, rev);
        assert!(!r.p(A).trade.offer.accepted);
        r.confirm_all();
        assert_eq!(r.p(A).inv.get(1), Some(InvItem { obj: 38, amount: 6 }));
        assert!(r.p(B).inv.slots.iter().flatten().any(|i| *i == InvItem { obj: 38, amount: 4 }));
        assert_eq!((r.p(A).inv.gold, r.p(B).inv.gold), (300, 200));
        assert!(r.p(A).trade.with.is_none() && r.p(B).trade.with.is_none());
        assert!(r.msgs(1).iter().any(|m| matches!(m, S2C::Trade(TradeS2C::Closed { done: true }))));
    }

    #[test]
    fn changing_the_offer_clears_every_confirmation() {
        let mut r = rig();
        r.give(A, 1, 38, 10);
        r.give(B, 1, 39, 5);
        r.open_pair();
        r.w.trade_offer(A, 1, 4);
        r.w.trade_offer(B, 1, 1);
        let rev = r.rev();
        r.w.trade_confirm(A, rev);
        r.w.trade_confirm(B, rev);
        r.w.trade_accept(A, rev);
        // B changes the offer right before accepting.
        r.w.trade_offer(B, 1, 1);
        assert!(!r.p(A).trade.offer.confirmed && !r.p(A).trade.offer.accepted);
        // A late accept with the old revision is ignored.
        r.w.trade_accept(B, rev);
        r.w.trade_accept(A, rev);
        assert_eq!(r.p(A).inv.get(1), Some(InvItem { obj: 38, amount: 10 }), "nothing moved");
        assert_eq!(r.p(A).trade.with, Some(B), "still trading");
    }

    #[test]
    fn offering_more_than_held_or_twice_the_same_units_is_refused() {
        let mut r = rig();
        r.give(A, 1, 38, 5);
        r.open_pair();
        r.w.trade_offer(A, 1, 5);
        r.w.trade_offer(A, 1, 5);
        assert_eq!(r.p(A).trade.offer.items, vec![InvItem { obj: 38, amount: 5 }], "the same units cannot be offered twice");
        r.w.trade_offer_gold(A, 1);
        assert_eq!(r.p(A).trade.offer.gold, 0, "no gold");
        // Equipped items cannot be offered.
        r.give(A, 2, 39, 1);
        r.w.player_mut(A).inv.equipped.weapon = Some(2);
        r.w.trade_offer(A, 2, 1);
        assert_eq!(r.p(A).trade.offer.items.len(), 1);
    }

    #[test]
    fn exchange_is_all_or_nothing_when_items_vanish_or_do_not_fit() {
        let mut r = rig();
        r.give(A, 1, 38, 5);
        r.open_pair();
        r.w.trade_offer(A, 1, 5);
        // A drops the items after offering: the exchange is refused, nothing duplicates.
        r.w.player_mut(A).inv.slots[0] = None;
        r.confirm_all();
        assert!(r.p(B).inv.slots.iter().all(Option::is_none));
        assert!(r.p(A).trade.with.is_none());

        // B has no room for what it gets.
        let mut r = rig();
        r.give(A, 1, 38, 5);
        for s in 1..=24 {
            r.give(B, s, 9000 + s as u16, 1);
        }
        r.open_pair();
        r.w.trade_offer(A, 1, 5);
        r.confirm_all();
        assert_eq!(r.p(A).inv.get(1), Some(InvItem { obj: 38, amount: 5 }));
        assert!(r.p(B).inv.slots.iter().flatten().all(|i| i.obj != 38));
    }

    #[test]
    fn leaving_dying_or_walking_away_cancels() {
        let mut r = rig();
        r.give(A, 1, 38, 5);
        r.open_pair();
        r.w.trade_offer(A, 1, 5);
        r.w.trade_on_leave(A);
        assert!(r.p(B).trade.with.is_none());
        assert_eq!(r.p(A).inv.get(1), Some(InvItem { obj: 38, amount: 5 }), "offers never left the inventory");

        let mut r = rig();
        r.open_pair();
        r.w.player_mut(B).combat.dead = true;
        r.w.trade_offer_gold(A, 0);
        r.w.trade_confirm(A, 1);
        assert!(r.p(A).trade.with.is_none() && r.p(B).trade.with.is_none());

        let mut r = rig();
        r.open_pair();
        r.w.player_mut(B).x += 5;
        r.w.trade_confirm(A, 1);
        assert!(r.p(A).trade.with.is_none());
        let _ = r.at;
    }

    #[test]
    fn cannot_trade_with_self_dead_far_or_busy() {
        let mut r = rig();
        r.w.trade_propose(A, A);
        assert!(r.p(A).trade.proposed_to.is_none());
        r.w.player_mut(B).combat.dead = true;
        r.w.trade_propose(A, B);
        assert!(r.p(A).trade.proposed_to.is_none());
        r.w.player_mut(B).combat.dead = false;
        r.w.open_npc_trade(B, MERCHANT);
        r.w.trade_propose(A, B);
        assert!(r.p(A).trade.proposed_to.is_none(), "busy at a merchant");
        r.w.close_shop(B);
        r.w.trade_propose(A, B);
        r.w.trade_decline(B);
        r.w.trade_propose(B, A);
        assert!(r.p(A).trade.with.is_none(), "declined invitations are gone");
    }
}

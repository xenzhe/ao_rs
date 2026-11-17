//! Client actions and state for inventory, bank, shop and trade actions (`impl Game` blocks, graphics-free and
//! testable with bots). OWNER: Sonnet 2 (docs/P3_CONTRATO.md).
//!
//! Every action first checks what the server would check (the slot holds something, the
//! amount is not more than what is there, the gold is enough...) so an honest player never
//! sends a packet that would count as cheating; a refusal explains itself in the console.
//! The decisions are `plan_*` functions that return the message to send (or the reason), so
//! they are testable without a connection; the `Game` methods send the plan.
//!
//! Also here: the pure helpers the item windows share (what a double click does, which
//! equipment row an object belongs to, tooltips, parsing a typed quantity).
use crate::local::{ItemSlot, BANK_SLOTS, INVENTORY_SLOTS};
use crate::social::trade::PlayerTrade;
use crate::{Ent, Game};
use ao_formats::dat::obj::{ObjData, ObjType};
use ao_protocol::{EntityKind, Tone, TradeC2S, C2S};
use ao_rules::trade as rules;

/// First slots of the inventory without a backpack (AO20: 24, 42 with it).
pub const BASE_SLOTS: usize = 24;
/// `MAX_OFFER_ITEMS`: entries of one side of a player trade.
pub const MAX_OFFER_ITEMS: usize = rules::MAX_OFFER_ITEMS;

// ---------------------------------------------------------------- object helpers

/// What a double click on an object does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primary {
    Use,
    Equip,
}

/// Equipment is put on (and taken off) with `EquipItem`; everything else is used.
pub fn primary_action(t: ObjType) -> Primary {
    use ObjType::*;
    match t {
        Weapon | Armor | Shield | Helmet | Amulets | RingAccessory | Backpack | Arrows | WorkingTools | MagicalInstrument | MusicalInstruments | Saddles | SkinsWings | SkinsArmours | SkinsShields | SkinsHelmets | SkinsWeapons | SkinsBoats | SkinsSpells => Primary::Equip,
        _ => Primary::Use,
    }
}

/// The row of the equipment strip an equipped object shows in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipCat {
    Weapon,
    Shield,
    Helmet,
    Armor,
    Ring,
    Amulet,
    Other,
}

impl EquipCat {
    pub const ALL: [EquipCat; 7] = [EquipCat::Weapon, EquipCat::Shield, EquipCat::Helmet, EquipCat::Armor, EquipCat::Ring, EquipCat::Amulet, EquipCat::Other];

    pub fn label(self) -> &'static str {
        match self {
            EquipCat::Weapon => "Arma",
            EquipCat::Shield => "Escudo",
            EquipCat::Helmet => "Casco",
            EquipCat::Armor => "Armadura",
            EquipCat::Ring => "Anillo",
            EquipCat::Amulet => "Amuleto",
            EquipCat::Other => "Otro",
        }
    }
}

/// Equipment strip category of an object type (None: it is not equipment).
pub fn equip_category(t: ObjType) -> Option<EquipCat> {
    use ObjType::*;
    Some(match t {
        Weapon => EquipCat::Weapon,
        Shield => EquipCat::Shield,
        Helmet => EquipCat::Helmet,
        Armor => EquipCat::Armor,
        RingAccessory => EquipCat::Ring,
        Amulets => EquipCat::Amulet,
        _ if primary_action(t) == Primary::Equip => EquipCat::Other,
        _ => return None,
    })
}

/// Spanish name of an object type, for tooltips.
pub fn type_name(t: ObjType) -> &'static str {
    use ObjType::*;
    match t {
        UseOnce => "Consumible",
        Weapon => "Arma",
        Armor => "Armadura",
        Trees => "Árbol",
        GoldCoin => "Oro",
        Doors => "Puerta",
        Backpack => "Mochila",
        SignBoards => "Cartel",
        Keys => "Llave",
        Potions => "Poción",
        Drinks => "Bebida",
        Wood => "Madera",
        Shield => "Escudo",
        Helmet => "Casco",
        WorkingTools => "Herramienta",
        Decorations => "Decoración",
        Amulets => "Amuleto",
        Minerals => "Mineral",
        Parchment => "Pergamino",
        MusicalInstruments => "Instrumento musical",
        BlacksmithMaterial => "Material de herrería",
        MagicalInstrument => "Instrumento mágico",
        Ships => "Barca",
        Arrows => "Munición",
        EmptyBottle => "Botella vacía",
        FullBottle => "Botella llena",
        RingAccessory => "Anillo",
        PassageTicket => "Pasaje",
        Map => "Mapa",
        Saddles => "Montura",
        RecallStones => "Piedra de recuerdo",
        Mail => "Correo",
        Chest => "Cofre",
        Quest => "Misión",
        UsableOnTarget => "Usable sobre objetivo",
        Plants => "Planta",
        ElementalRune => "Runa elemental",
        _ => "Objeto",
    }
}

/// Lines of the tooltip of an object: name first, then type, numbers and requirements.
/// `my_level` marks a level requirement the player does not meet.
pub fn tooltip_lines(o: &ObjData, amount: u32, equipped: bool, my_level: i32) -> Vec<String> {
    let mut v = vec![if equipped { format!("{} (equipado)", o.name) } else { o.name.clone() }];
    if amount > 1 {
        v.push(format!("Cantidad: {amount}"));
    }
    v.push(type_name(o.obj_type).to_string());
    use ObjType::*;
    match o.obj_type {
        Weapon => {
            v.push(format!("Daño: {}-{}", o.min_hit, o.max_hit));
            if o.two_handed != 0 {
                v.push("A dos manos".into());
            }
        }
        Armor | Shield | Helmet => {
            v.push(format!("Defensa: {}-{}", o.min_def, o.max_def));
            if o.magic_resistance > 0 {
                v.push(format!("Resistencia mágica: {}", o.magic_resistance));
            }
        }
        Arrows => v.push(format!("Daño: {}-{}", o.min_hit, o.max_hit)),
        Potions => {
            if o.max_modifier > 0 {
                v.push(format!("Efecto: {}-{}", o.min_modifier, o.max_modifier));
            }
        }
        UseOnce | Drinks => {
            if o.min_hunger > 0 {
                v.push(format!("Hambre: +{}", o.min_hunger));
            }
            if o.min_thirst > 0 {
                v.push(format!("Sed: +{}", o.min_thirst));
            }
            if o.min_hp > 0 {
                v.push(format!("Vida: +{}-{}", o.min_hp, o.max_hp.max(o.min_hp)));
            }
            if o.min_stamina > 0 {
                v.push(format!("Energía: +{}", o.min_stamina));
            }
        }
        _ => {}
    }
    if o.value > 0 {
        v.push(format!("Valor: {} oro", o.value));
    }
    if o.newbie != 0 {
        v.push("Objeto de novato".into());
    }
    if o.non_droppable != 0 {
        v.push("No se puede tirar".into());
    }
    if o.min_level > 1 {
        v.push(if my_level < o.min_level { format!("Requiere nivel {} (te falta)", o.min_level) } else { format!("Requiere nivel {}", o.min_level) });
    }
    if o.min_skill > 0 {
        v.push(format!("Habilidad mínima: {}", o.min_skill));
    }
    if o.male_only {
        v.push("Solo hombres".into());
    }
    if o.female_only {
        v.push("Solo mujeres".into());
    }
    if !o.forbidden_classes.is_empty() {
        let names: Vec<&str> = o.forbidden_classes.iter().map(|c| c.spanish_name()).collect();
        v.push(format!("No pueden usarlo: {}", names.join(", ")));
    }
    v
}

/// A typed quantity: digits only, at least 1, at most `max` (`None` when empty / zero).
pub fn parse_qty(text: &str, max: u32) -> Option<u32> {
    let n: u64 = text.trim().parse().ok()?;
    (n > 0 && max > 0).then(|| n.min(max as u64) as u32)
}

/// Gold a merchant pays for `amount` units of `o` (0 when it does not buy it).
pub fn sale_value(o: &ObjData, amount: u32) -> u64 {
    if o.no_tradeable_to_npc {
        return 0;
    }
    rules::sale_price(o.value, o.newbie != 0, amount)
}

// ---------------------------------------------------------------- plans

type Plan = Result<C2S, String>;

fn refuse<T>(why: &str) -> Result<T, String> {
    Err(why.to_string())
}

impl Game {
    fn dispatch(&mut self, plan: Plan) -> bool {
        match plan {
            Ok(m) => {
                self.send(&m);
                true
            }
            Err(why) => {
                if !why.is_empty() {
                    self.say(why, Tone::Warn);
                }
                false
            }
        }
    }

    fn held(&self, slot: u8) -> Result<ItemSlot, String> {
        self.local.slot(slot).ok_or_else(|| "Esa casilla está vacía.".to_string())
    }

    // ---- inventory ----

    pub fn plan_use(&self, slot: u8) -> Plan {
        self.held(slot).map(|_| C2S::UseItem { slot })
    }

    pub fn plan_equip(&self, slot: u8) -> Plan {
        self.held(slot).map(|_| C2S::EquipItem { slot })
    }

    pub fn plan_drop(&self, slot: u8, amount: u32) -> Plan {
        let it = self.held(slot)?;
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::DropItem { slot, amount: amount.min(it.amount) })
    }

    pub fn plan_drop_gold(&self, amount: u32) -> Plan {
        if self.local.gold == 0 {
            return refuse("No tienes oro.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::DropGold { amount: amount.min(self.local.gold) })
    }

    pub fn plan_swap(&self, a: u8, b: u8) -> Plan {
        let ok = |s: u8| (1..=INVENTORY_SLOTS as u8).contains(&s);
        if a == b || !ok(a) || !ok(b) {
            return refuse("");
        }
        self.held(a)?;
        Ok(C2S::SwapSlots { a, b })
    }

    pub fn inv_use(&mut self, slot: u8) -> bool {
        let p = self.plan_use(slot);
        self.dispatch(p)
    }

    pub fn inv_equip(&mut self, slot: u8) -> bool {
        let p = self.plan_equip(slot);
        self.dispatch(p)
    }

    /// Throws `amount` units (clamped to what the slot holds) on the floor.
    pub fn inv_drop(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_drop(slot, amount);
        self.dispatch(p)
    }

    pub fn inv_drop_gold(&mut self, amount: u32) -> bool {
        let p = self.plan_drop_gold(amount);
        self.dispatch(p)
    }

    pub fn inv_swap(&mut self, a: u8, b: u8) -> bool {
        let p = self.plan_swap(a, b);
        self.dispatch(p)
    }

    pub fn inv_pick_up(&mut self) {
        self.send(&C2S::PickUp);
    }

    // ---- vault ----

    fn bank_slot(&self, slot: u8) -> Result<ItemSlot, String> {
        let b = self.local.bank.as_ref().ok_or("La bóveda está cerrada.")?;
        b.slots.get((slot as usize).wrapping_sub(1)).copied().flatten().ok_or_else(|| "Esa casilla de la bóveda está vacía.".to_string())
    }

    pub fn plan_deposit(&self, slot: u8, amount: u32) -> Plan {
        if self.local.bank.is_none() {
            return refuse("La bóveda está cerrada.");
        }
        let it = self.held(slot)?;
        if it.equipped {
            return refuse("Quítate el objeto antes de guardarlo.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::BankDeposit { slot, amount: amount.min(it.amount) })
    }

    pub fn plan_withdraw(&self, slot: u8, amount: u32) -> Plan {
        let it = self.bank_slot(slot)?;
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::BankWithdraw { slot, amount: amount.min(it.amount) })
    }

    pub fn plan_deposit_gold(&self, amount: u32) -> Plan {
        if self.local.bank.is_none() {
            return refuse("La bóveda está cerrada.");
        }
        if self.local.gold == 0 {
            return refuse("No llevas oro.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::BankDepositGold { amount: amount.min(self.local.gold) })
    }

    pub fn plan_withdraw_gold(&self, amount: u32) -> Plan {
        let held = self.local.bank.as_ref().ok_or("La bóveda está cerrada.")?.gold;
        if held == 0 {
            return refuse("No hay oro en la bóveda.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::BankWithdrawGold { amount: amount.min(held) })
    }

    pub fn bank_deposit(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_deposit(slot, amount);
        self.dispatch(p)
    }

    pub fn bank_withdraw(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_withdraw(slot, amount);
        self.dispatch(p)
    }

    pub fn bank_deposit_gold(&mut self, amount: u32) -> bool {
        let p = self.plan_deposit_gold(amount);
        self.dispatch(p)
    }

    pub fn bank_withdraw_gold(&mut self, amount: u32) -> bool {
        let p = self.plan_withdraw_gold(amount);
        self.dispatch(p)
    }

    pub fn bank_close(&mut self) {
        self.send(&C2S::BankClose);
    }

    /// Item of the open vault at `slot` (1-based).
    pub fn bank_item(&self, slot: u8) -> Option<ItemSlot> {
        self.bank_slot(slot).ok()
    }

    // ---- merchant ----

    /// Most units of merchant slot `slot` the purse allows (stock and gold).
    pub fn shop_max_buy(&self, slot: u8) -> u32 {
        let Some(it) = self.local.social.trade.shop.as_ref().and_then(|s| s.items.get(&slot)) else { return 0 };
        let by_gold = self.local.gold.checked_div(it.price).unwrap_or(it.amount);
        it.amount.min(by_gold)
    }

    pub fn plan_buy(&self, slot: u8, amount: u32) -> Plan {
        let shop = self.local.social.trade.shop.as_ref().ok_or("El comerciante no está atendiendo.")?;
        let it = shop.items.get(&slot).ok_or("El comerciante ya no tiene eso.")?;
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        let amount = amount.min(it.amount);
        // The server rounds the whole line, not each unit: never refuse for less than that.
        let lowest = (it.price as u64 * amount as u64).saturating_sub(amount as u64 - 1);
        if (self.local.gold as u64) < lowest {
            return refuse("No tienes oro suficiente.");
        }
        Ok(C2S::Trade(TradeC2S::Buy { slot, amount }))
    }

    pub fn plan_sell(&self, slot: u8, amount: u32) -> Plan {
        if self.local.social.trade.shop.is_none() {
            return refuse("El comerciante no está atendiendo.");
        }
        let it = self.held(slot)?;
        if it.equipped {
            return refuse("Quítate el objeto antes de venderlo.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::Trade(TradeC2S::Sell { slot, amount: amount.min(it.amount) }))
    }

    pub fn shop_buy(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_buy(slot, amount);
        self.dispatch(p)
    }

    pub fn shop_sell(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_sell(slot, amount);
        self.dispatch(p)
    }

    pub fn shop_close(&mut self) {
        self.send(&C2S::Trade(TradeC2S::CloseShop));
    }

    // ---- trade between players ----

    /// Players within trading range (Manhattan), nearest first: (entity id, name).
    pub fn nearby_players(&self) -> Vec<(u32, String)> {
        let Some(me) = self.me() else { return Vec::new() };
        let mut v: Vec<(i32, &u32, &Ent)> = self
            .ents
            .iter()
            .filter(|(id, e)| **id != self.my_id && e.kind == EntityKind::Player && (e.x - me.x).abs() + (e.y - me.y).abs() <= rules::PLAYER_TRADE_RANGE)
            .map(|(id, e)| ((e.x - me.x).abs() + (e.y - me.y).abs(), id, e))
            .collect();
        v.sort_by_key(|(d, id, _)| (*d, **id));
        v.into_iter().map(|(_, id, e)| (*id, e.name.clone())).collect()
    }

    pub fn plan_propose(&self, target: u32) -> Plan {
        if self.local.social.trade.player.is_some() {
            return refuse("Ya estás comerciando.");
        }
        let Some(e) = self.ents.get(&target).filter(|e| target != self.my_id && e.kind == EntityKind::Player) else { return refuse("Ese jugador no está a la vista.") };
        let Some(me) = self.me() else { return refuse("") };
        if (e.x - me.x).abs() + (e.y - me.y).abs() > rules::PLAYER_TRADE_RANGE {
            return refuse("Estás demasiado lejos para comerciar.");
        }
        Ok(C2S::Trade(TradeC2S::Propose { target }))
    }

    /// Asks `target` (entity id) to trade.
    pub fn trade_propose(&mut self, target: u32) -> bool {
        let p = self.plan_propose(target);
        if self.dispatch(p) {
            if let Some(name) = self.ents.get(&target).map(|e| e.name.clone()) {
                self.say(format!("Le propones comerciar a {name}."), Tone::Info);
            }
            return true;
        }
        false
    }

    /// The player standing on tile `(tx, ty)`, if any.
    pub fn player_at(&self, tx: i32, ty: i32) -> Option<u32> {
        self.ents.iter().find(|(id, e)| **id != self.my_id && e.kind == EntityKind::Player && e.x == tx && e.y == ty).map(|(id, _)| *id)
    }

    /// Hook for a click on the world: proposes a trade to the player standing on tile
    /// `(tx, ty)`. Returns whether there was a player there to ask.
    pub fn trade_with_tile(&mut self, tx: i32, ty: i32) -> bool {
        match self.player_at(tx, ty) {
            Some(id) => {
                self.trade_propose(id);
                true
            }
            None => false,
        }
    }

    /// Answers the pending invitation with our own `Propose`.
    pub fn plan_accept_invite(&self) -> Plan {
        let (from, _) = self.local.social.trade.invited_by.clone().ok_or("Nadie te ha invitado a comerciar.")?;
        Ok(C2S::Trade(TradeC2S::Propose { target: from }))
    }

    pub fn plan_decline(&self) -> Plan {
        self.local.social.trade.invited_by.as_ref().ok_or("Nadie te ha invitado a comerciar.")?;
        Ok(C2S::Trade(TradeC2S::Decline))
    }

    /// Accepts the pending invitation (answers it with our own `Propose`).
    pub fn trade_accept_invite(&mut self) -> bool {
        let p = self.plan_accept_invite();
        self.dispatch(p)
    }

    pub fn trade_decline(&mut self) -> bool {
        let p = self.plan_decline();
        let ok = self.dispatch(p);
        self.local.social.trade.invited_by = None;
        ok
    }

    fn trade_open(&self) -> Result<&PlayerTrade, String> {
        self.local.social.trade.player.as_ref().ok_or_else(|| "No estás comerciando.".to_string())
    }

    /// Units of inventory slot `slot` not yet in the own offer.
    pub fn offerable(&self, slot: u8) -> u32 {
        let Some(it) = self.local.slot(slot) else { return 0 };
        if it.equipped {
            return 0;
        }
        let offered: u32 = self.local.social.trade.player.as_ref().map_or(0, |t| t.mine.items.iter().filter(|(o, _)| *o == it.obj).map(|(_, n)| *n).sum());
        // Another slot with the same object may hold the rest of the units.
        let total = self.local.count_of(it.obj);
        it.amount.min(total.saturating_sub(offered))
    }

    pub fn plan_offer(&self, slot: u8, amount: u32) -> Plan {
        let t = self.trade_open()?;
        let it = self.held(slot)?;
        if it.equipped {
            return refuse("Quítate el objeto antes de ofrecerlo.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        let free = self.offerable(slot);
        if free == 0 {
            return refuse("Ya has ofrecido todo lo que tienes de eso.");
        }
        if t.mine.items.len() >= MAX_OFFER_ITEMS && !t.mine.items.iter().any(|(o, _)| *o == it.obj) {
            return refuse("No caben más objetos distintos en la oferta.");
        }
        Ok(C2S::Trade(TradeC2S::Offer { slot, amount: amount.min(free) }))
    }

    pub fn plan_offer_gold(&self, amount: u32) -> Plan {
        self.trade_open()?;
        Ok(C2S::Trade(TradeC2S::OfferGold { amount: amount.min(self.local.gold) }))
    }

    pub fn plan_retract(&self, index: u8) -> Plan {
        let t = self.trade_open()?;
        if (index as usize) >= t.mine.items.len() {
            return refuse("");
        }
        Ok(C2S::Trade(TradeC2S::Retract { index }))
    }

    /// First confirmation of the current revision. Only while it is not confirmed yet.
    pub fn plan_confirm(&self) -> Plan {
        let t = self.trade_open()?;
        if t.i_confirmed {
            return refuse("");
        }
        if t.mine.items.is_empty() && t.mine.gold == 0 && t.theirs.items.is_empty() && t.theirs.gold == 0 {
            return refuse("La oferta está vacía.");
        }
        Ok(C2S::Trade(TradeC2S::Confirm { rev: t.rev }))
    }

    /// Second confirmation: only once both sides confirmed the same revision.
    pub fn plan_accept(&self) -> Plan {
        let t = self.trade_open()?;
        if !(t.i_confirmed && t.they_confirmed) || t.i_accepted {
            return refuse("");
        }
        Ok(C2S::Trade(TradeC2S::Accept { rev: t.rev }))
    }

    pub fn trade_offer(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_offer(slot, amount);
        self.dispatch(p)
    }

    pub fn trade_offer_gold(&mut self, amount: u32) -> bool {
        let p = self.plan_offer_gold(amount);
        self.dispatch(p)
    }

    pub fn trade_retract(&mut self, index: u8) -> bool {
        let p = self.plan_retract(index);
        self.dispatch(p)
    }

    pub fn trade_confirm(&mut self) -> bool {
        let p = self.plan_confirm();
        self.dispatch(p)
    }

    pub fn trade_accept(&mut self) -> bool {
        let p = self.plan_accept();
        self.dispatch(p)
    }

    pub fn trade_cancel(&mut self) {
        self.send(&C2S::Trade(TradeC2S::Cancel));
    }
}

/// Where the double confirmation of a player trade stands, for the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradePhase {
    /// Nobody confirmed: offers are being built.
    Building,
    /// Only I confirmed: waiting for them.
    WaitingThem,
    /// Only they confirmed: waiting for me.
    WaitingMe,
    /// Both confirmed: the second step (accept) is open.
    BothConfirmed,
    /// I accepted, waiting for their acceptance.
    WaitingAccept,
    /// They accepted, waiting for mine.
    TheyAccepted,
}

pub fn trade_phase(t: &PlayerTrade) -> TradePhase {
    match (t.i_confirmed, t.they_confirmed, t.i_accepted, t.they_accepted) {
        (true, true, true, _) => TradePhase::WaitingAccept,
        (true, true, false, true) => TradePhase::TheyAccepted,
        (true, true, _, _) => TradePhase::BothConfirmed,
        (true, false, ..) => TradePhase::WaitingThem,
        (false, true, ..) => TradePhase::WaitingMe,
        _ => TradePhase::Building,
    }
}

/// Inventory slots the player can really use: 24, or all of them with a backpack on.
pub fn usable_slots(has_backpack: bool) -> usize {
    if has_backpack {
        INVENTORY_SLOTS
    } else {
        BASE_SLOTS
    }
}

/// Unused here but shared by windows: vault capacity.
pub const VAULT_SLOTS: usize = BANK_SLOTS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::trade::{Offer, Shop, ShopItem};
    use std::path::PathBuf;

    fn game() -> Game {
        let mut g = Game::new("T".into(), "127.0.0.1:1".into(), PathBuf::new());
        g.local.inventory[0] = Some(ItemSlot { obj: 38, amount: 10, equipped: false });
        g.local.inventory[1] = Some(ItemSlot { obj: 3, amount: 1, equipped: true });
        g.local.gold = 500;
        g
    }

    fn msg(p: Plan) -> String {
        format!("{:?}", p)
    }

    #[test]
    fn inventory_plans_clamp_and_refuse() {
        let g = game();
        assert!(matches!(g.plan_use(1), Ok(C2S::UseItem { slot: 1 })));
        assert!(g.plan_use(5).is_err(), "empty slot");
        assert!(matches!(g.plan_drop(1, 99), Ok(C2S::DropItem { slot: 1, amount: 10 })), "clamped to the stack");
        assert!(g.plan_drop(1, 0).is_err());
        assert!(matches!(g.plan_drop_gold(9999), Ok(C2S::DropGold { amount: 500 })));
        assert!(matches!(g.plan_swap(1, 7), Ok(C2S::SwapSlots { a: 1, b: 7 })));
        assert!(g.plan_swap(1, 1).is_err() && g.plan_swap(1, 0).is_err() && g.plan_swap(1, 43).is_err(), "never a packet the server counts as cheating");
        assert!(g.plan_swap(9, 1).is_err(), "empty source");
    }

    #[test]
    fn vault_plans_need_an_open_vault_and_loose_items() {
        let mut g = game();
        assert!(g.plan_deposit(1, 1).is_err(), "closed vault");
        g.local.bank = Some(crate::local::Bank { slots: vec![None; BANK_SLOTS], gold: 40 });
        g.local.bank.as_mut().unwrap().slots[2] = Some(ItemSlot { obj: 38, amount: 5, equipped: false });
        assert!(matches!(g.plan_deposit(1, 4), Ok(C2S::BankDeposit { slot: 1, amount: 4 })));
        assert!(g.plan_deposit(2, 1).is_err(), "equipped item stays on");
        assert!(matches!(g.plan_withdraw(3, 50), Ok(C2S::BankWithdraw { slot: 3, amount: 5 })));
        assert!(g.plan_withdraw(4, 1).is_err());
        assert!(matches!(g.plan_deposit_gold(10_000), Ok(C2S::BankDepositGold { amount: 500 })));
        assert!(matches!(g.plan_withdraw_gold(10_000), Ok(C2S::BankWithdrawGold { amount: 40 })));
    }

    #[test]
    fn shop_plans_check_stock_and_purse() {
        let mut g = game();
        assert!(g.plan_buy(1, 1).is_err(), "no shop open");
        let mut shop = Shop::default();
        shop.items.insert(1, ShopItem { obj: 38, amount: 3, price: 100 });
        g.local.social.trade.shop = Some(shop);
        assert!(matches!(g.plan_buy(1, 2), Ok(C2S::Trade(TradeC2S::Buy { slot: 1, amount: 2 }))));
        assert!(matches!(g.plan_buy(1, 9), Ok(C2S::Trade(TradeC2S::Buy { slot: 1, amount: 3 }))), "clamped to the stock");
        assert_eq!(g.shop_max_buy(1), 3);
        g.local.gold = 150;
        assert!(g.plan_buy(1, 2).is_err(), "not enough gold");
        assert_eq!(g.shop_max_buy(1), 1);
        assert!(g.plan_buy(7, 1).is_err(), "no such slot");
        assert!(matches!(g.plan_sell(1, 3), Ok(C2S::Trade(TradeC2S::Sell { slot: 1, amount: 3 }))));
        assert!(g.plan_sell(2, 1).is_err(), "equipped");
    }

    #[test]
    fn offers_respect_the_units_already_on_the_table() {
        let mut g = game();
        assert!(g.plan_offer(1, 1).is_err(), "no trade open");
        g.local.social.trade.player = Some(PlayerTrade { with: 2, name: "Otro".into(), rev: 3, ..Default::default() });
        assert!(matches!(g.plan_offer(1, 4), Ok(C2S::Trade(TradeC2S::Offer { slot: 1, amount: 4 }))));
        g.local.social.trade.player.as_mut().unwrap().mine = Offer { items: vec![(38, 8)], gold: 0 };
        assert_eq!(g.offerable(1), 2);
        assert!(matches!(g.plan_offer(1, 9), Ok(C2S::Trade(TradeC2S::Offer { slot: 1, amount: 2 }))));
        assert!(matches!(g.plan_offer_gold(10_000), Ok(C2S::Trade(TradeC2S::OfferGold { amount: 500 }))));
        assert!(g.plan_retract(0).is_ok() && g.plan_retract(1).is_err());
        assert!(g.plan_offer(2, 1).is_err(), "equipped");
        g.local.social.trade.player.as_mut().unwrap().mine.items = (1..=6).map(|o| (o, 1)).collect();
        assert!(g.plan_offer(1, 1).is_err(), "seven different entries do not fit");
    }

    #[test]
    fn double_confirmation_order() {
        let mut g = game();
        g.local.social.trade.player = Some(PlayerTrade { with: 2, rev: 7, ..Default::default() });
        assert!(g.plan_confirm().is_err(), "an empty offer is not confirmed");
        g.local.social.trade.player.as_mut().unwrap().mine.gold = 10;
        assert!(msg(g.plan_confirm()).contains("rev: 7"));
        assert!(g.plan_accept().is_err(), "accept needs both confirmations");
        let t = g.local.social.trade.player.as_mut().unwrap();
        t.i_confirmed = true;
        assert_eq!(trade_phase(t), TradePhase::WaitingThem);
        t.they_confirmed = true;
        assert_eq!(trade_phase(t), TradePhase::BothConfirmed);
        assert!(g.plan_confirm().is_err(), "no second Confirm");
        assert!(matches!(g.plan_accept(), Ok(C2S::Trade(TradeC2S::Accept { rev: 7 }))));
        let t = g.local.social.trade.player.as_mut().unwrap();
        t.i_accepted = true;
        assert_eq!(trade_phase(t), TradePhase::WaitingAccept);
        assert!(g.plan_accept().is_err());
    }

    #[test]
    fn quantities_and_primary_actions() {
        assert_eq!(parse_qty("12", 100), Some(12));
        assert_eq!(parse_qty("500", 100), Some(100), "clamped");
        assert_eq!(parse_qty("0", 100), None);
        assert_eq!(parse_qty("", 100), None);
        assert_eq!(parse_qty("5", 0), None);
        assert_eq!(primary_action(ObjType::Weapon), Primary::Equip);
        assert_eq!(primary_action(ObjType::Potions), Primary::Use);
        assert_eq!(equip_category(ObjType::Helmet), Some(EquipCat::Helmet));
        assert_eq!(equip_category(ObjType::Backpack), Some(EquipCat::Other));
        assert_eq!(equip_category(ObjType::Potions), None);
        assert_eq!(usable_slots(false), 24);
        assert_eq!(usable_slots(true), 42);
    }

    #[test]
    fn tooltip_shows_numbers_and_unmet_requirements() {
        let o = ObjData { name: "Espada".into(), obj_type: ObjType::Weapon, min_hit: 4, max_hit: 9, value: 300, min_level: 5, ..Default::default() };
        let t = tooltip_lines(&o, 1, true, 2);
        assert_eq!(t[0], "Espada (equipado)");
        assert!(t.contains(&"Daño: 4-9".to_string()) && t.contains(&"Valor: 300 oro".to_string()));
        assert!(t.iter().any(|l| l.contains("nivel 5") && l.contains("te falta")));
        let t = tooltip_lines(&o, 3, false, 9);
        assert!(t.contains(&"Cantidad: 3".to_string()) && !t.iter().any(|l| l.contains("te falta")));
        assert_eq!(sale_value(&o, 3), 300);
    }

    #[test]
    fn nearby_players_are_the_ones_in_trading_range() {
        let mut g = game();
        g.my_id = 1;
        let ent = |name: &str, kind, x, y| Ent::new(name.into(), kind, 1, 1, x, y, ao_rules::Dir::Down, 10, 10);
        g.ents.insert(1, ent("Yo", EntityKind::Player, 10, 10));
        g.ents.insert(2, ent("Cerca", EntityKind::Player, 11, 12));
        g.ents.insert(3, ent("Lejos", EntityKind::Player, 20, 10));
        g.ents.insert(4, ent("Npc", EntityKind::Creature, 10, 11));
        assert_eq!(g.nearby_players(), vec![(2, "Cerca".to_string())]);
        assert!(g.plan_propose(2).is_ok());
        assert!(g.plan_propose(3).is_err() && g.plan_propose(4).is_err() && g.plan_propose(1).is_err());
    }
}

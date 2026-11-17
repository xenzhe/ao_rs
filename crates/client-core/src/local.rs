//! What the player knows about their own character beyond the basic stats: inventory,
//! equipment, vitals, attributes, skills, spell slots, the open bank and ground items.
//! Filled from server messages only (the server is the authority).
use ao_protocol::S2C;
use std::collections::HashMap;

/// AO20: 24 usable slots (42 with backpack), 1-based on the wire.
pub const INVENTORY_SLOTS: usize = 42;
pub const BANK_SLOTS: usize = 42;
pub const SPELL_SLOTS: usize = 40;
pub const SKILLS: usize = 24;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ItemSlot {
    pub obj: u16,
    pub amount: u32,
    pub equipped: bool,
}

#[derive(Debug, Default)]
pub struct Local {
    /// Index 0 = slot 1. `None` = empty.
    pub inventory: Vec<Option<ItemSlot>>,
    pub gold: u32,
    pub sta: i32,
    pub max_sta: i32,
    pub food: i32,
    pub water: i32,
    pub attributes: [u8; 5],
    pub skills: Vec<u8>,
    pub free_skill_points: u16,
    /// Hechizos.dat ids, index 0 = slot 1, 0 = empty.
    pub spell_slots: Vec<u16>,
    /// Open bank (None when closed).
    pub bank: Option<Bank>,
    /// Items lying on the current map.
    pub ground: HashMap<(i32, i32), (u16, u32)>,
    /// P2: trade, guild, party, faction, quests, chat channels.
    pub social: crate::social::Social,
    /// Server weather: (cloud/fog intensity, precipitation).
    pub weather: (u8, bool),
    /// Bumped on every thunder (the frontend plays it when it changes).
    pub thunders: u32,
    /// P4: work, crafting, GM, events.
    pub p4: crate::p4::P4,
}

#[derive(Debug, Default)]
pub struct Bank {
    pub slots: Vec<Option<ItemSlot>>,
    pub gold: u32,
}

impl Local {
    pub fn new() -> Local {
        Local {
            inventory: vec![None; INVENTORY_SLOTS],
            skills: vec![0; SKILLS],
            spell_slots: vec![0; SPELL_SLOTS],
            ..Default::default()
        }
    }

    pub fn slot(&self, slot: u8) -> Option<ItemSlot> {
        self.inventory.get((slot as usize).wrapping_sub(1)).copied().flatten()
    }

    /// Count of an object across the inventory.
    pub fn count_of(&self, obj: u16) -> u32 {
        self.inventory.iter().flatten().filter(|s| s.obj == obj).map(|s| s.amount).sum()
    }

    /// Applies a message about the own character. Returns true if it was one of ours.
    pub fn apply(&mut self, msg: &S2C) -> bool {
        match msg {
            S2C::InventorySlot { slot, obj, amount, equipped } => set_slot(&mut self.inventory, *slot, *obj, *amount, *equipped),
            S2C::Vitals { sta, max_sta, food, water, gold } => {
                (self.sta, self.max_sta, self.food, self.water, self.gold) = (*sta, *max_sta, *food, *water, *gold);
            }
            S2C::Attributes { values } => self.attributes = *values,
            S2C::Skills { values, free_points } => {
                self.skills = values.clone();
                self.free_skill_points = *free_points;
            }
            S2C::SpellSlots { slots } => self.spell_slots = slots.clone(),
            S2C::GroundItem { x, y, obj, amount } => {
                self.ground.insert((*x, *y), (*obj, *amount));
            }
            S2C::GroundItemRemoved { x, y } => {
                self.ground.remove(&(*x, *y));
            }
            S2C::BankOpen { slots, gold } => {
                let mut bank = Bank { slots: vec![None; BANK_SLOTS], gold: *gold };
                for (slot, obj, amount) in slots {
                    set_slot(&mut bank.slots, *slot, *obj, *amount, false);
                }
                self.bank = Some(bank);
            }
            S2C::BankSlot { slot, obj, amount } => {
                if let Some(b) = &mut self.bank {
                    set_slot(&mut b.slots, *slot, *obj, *amount, false);
                }
            }
            S2C::BankGold { gold } => {
                if let Some(b) = &mut self.bank {
                    b.gold = *gold;
                }
            }
            S2C::BankClosed => self.bank = None,
            S2C::Weather { fog, precipitation } => self.weather = (*fog, *precipitation),
            S2C::Thunder => self.thunders += 1,
            _ => return self.social.apply(msg) || self.p4.apply(msg),
        }
        true
    }

    /// Ground items belong to a map: cleared on every map change.
    pub fn on_map_change(&mut self) {
        self.ground.clear();
        self.bank = None;
    }
}

fn set_slot(slots: &mut [Option<ItemSlot>], slot: u8, obj: u16, amount: u32, equipped: bool) {
    if let Some(s) = slots.get_mut((slot as usize).wrapping_sub(1)) {
        *s = (obj != 0 && amount > 0).then_some(ItemSlot { obj, amount, equipped });
    }
}

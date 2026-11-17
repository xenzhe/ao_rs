//! Inventory and equipment state. OWNER: inventory system (P1, Opus C).
//! Persisted by `db` (slots, equipped flags, gold); add runtime-only fields freely.

/// AO20: 24 usable slots, up to 42 with a backpack (Declares.bas).
pub const MAX_INVENTORY_SLOTS: usize = 42;
pub const BASE_INVENTORY_SLOTS: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InvItem {
    /// obj.dat id.
    pub obj: u16,
    pub amount: u32,
}

/// Equipped items as inventory slot numbers (1-based), like t_Inventario's *Slot fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Equipped {
    pub weapon: Option<u8>,
    pub armor: Option<u8>,
    pub shield: Option<u8>,
    pub helmet: Option<u8>,
    pub ammo: Option<u8>,
    pub tool: Option<u8>,
    pub ship: Option<u8>,
    pub saddle: Option<u8>,
    pub ring: Option<u8>,
    pub amulet: Option<u8>,
    pub backpack: Option<u8>,
}

impl Equipped {
    pub fn all(&self) -> [Option<u8>; 11] {
        [self.weapon, self.armor, self.shield, self.helmet, self.ammo, self.tool, self.ship, self.saddle, self.ring, self.amulet, self.backpack]
    }

    pub fn is_equipped(&self, slot: u8) -> bool {
        self.all().contains(&Some(slot))
    }
}

#[derive(Clone, Debug)]
pub struct Inventory {
    /// Index 0 = slot 1.
    pub slots: Vec<Option<InvItem>>,
    pub equipped: Equipped,
    pub gold: u32,
    /// Slots saved as equipped, loaded from the database: the inventory system re-equips
    /// them (deciding weapon/armor/... from obj.dat) when the player enters the world.
    pub pending_equipped: Vec<u8>,
}

impl Default for Inventory {
    fn default() -> Self {
        Inventory { slots: vec![None; MAX_INVENTORY_SLOTS], equipped: Equipped::default(), gold: 0, pending_equipped: Vec::new() }
    }
}

impl Inventory {
    pub fn get(&self, slot: u8) -> Option<InvItem> {
        self.slots.get((slot as usize).wrapping_sub(1)).copied().flatten()
    }
}

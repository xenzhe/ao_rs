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
    /// Worn skins (`systems::skins::skin_kind`): armour, helmet, shield, weapon, boat,
    /// wings, spells.
    pub skins: [Option<u8>; 7],
}

impl Equipped {
    pub fn all(&self) -> [Option<u8>; 18] {
        let s = self.skins;
        [self.weapon, self.armor, self.shield, self.helmet, self.ammo, self.tool, self.ship, self.saddle, self.ring, self.amulet, self.backpack, s[0], s[1], s[2], s[3], s[4], s[5], s[6]]
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
    /// Runtime only: when each obj.dat `CDType` can be used again (`CdTimes`).
    pub cooldowns: std::collections::HashMap<i32, crate::clock::GameTime>,
}

impl Default for Inventory {
    fn default() -> Self {
        Inventory { slots: vec![None; MAX_INVENTORY_SLOTS], equipped: Equipped::default(), gold: 0, pending_equipped: Vec::new(), cooldowns: Default::default() }
    }
}

impl Inventory {
    pub fn get(&self, slot: u8) -> Option<InvItem> {
        self.slots.get((slot as usize).wrapping_sub(1)).copied().flatten()
    }

    /// Usable slots: 24, 42 with a backpack equipped (P1 contract). The official server
    /// gives the extra slots by account tier instead (`get_num_inv_slots_from_tier`).
    pub fn usable_slots(&self) -> u8 {
        if self.equipped.backpack.is_some() { MAX_INVENTORY_SLOTS as u8 } else { BASE_INVENTORY_SLOTS as u8 }
    }

    pub fn cell_mut(&mut self, slot: u8) -> Option<&mut Option<InvItem>> {
        self.slots.get_mut((slot as usize).wrapping_sub(1))
    }
}

impl Equipped {
    /// The equipment field for a role.
    pub fn role_mut(&mut self, role: ao_rules::items::Role) -> &mut Option<u8> {
        use ao_rules::items::Role::*;
        match role {
            Weapon => &mut self.weapon,
            Armor => &mut self.armor,
            Shield => &mut self.shield,
            Helmet => &mut self.helmet,
            Ammo => &mut self.ammo,
            Tool => &mut self.tool,
            Ship => &mut self.ship,
            Saddle => &mut self.saddle,
            Ring => &mut self.ring,
            Amulet => &mut self.amulet,
            Backpack => &mut self.backpack,
        }
    }

    /// Every equipment field, to remap or clear slots (swaps, emptied stacks).
    pub fn fields_mut(&mut self) -> [&mut Option<u8>; 18] {
        let [s0, s1, s2, s3, s4, s5, s6] = &mut self.skins;
        [
            s0,
            s1,
            s2,
            s3,
            s4,
            s5,
            s6,
            &mut self.weapon,
            &mut self.armor,
            &mut self.shield,
            &mut self.helmet,
            &mut self.ammo,
            &mut self.tool,
            &mut self.ship,
            &mut self.saddle,
            &mut self.ring,
            &mut self.amulet,
            &mut self.backpack,
        ]
    }
}

/// Objects lying on one map's floor, one stack per tile (MapData().ObjInfo).
#[derive(Clone, Debug, Default)]
pub struct GroundItems {
    pub tiles: std::collections::HashMap<(i32, i32), InvItem>,
    /// When each tile last got something dropped on it (floor cleanup).
    pub dropped_at: std::collections::HashMap<(i32, i32), crate::clock::GameTime>,
}

impl GroundItems {
    pub fn at(&self, x: i32, y: i32) -> Option<InvItem> {
        self.tiles.get(&(x, y)).copied()
    }
}

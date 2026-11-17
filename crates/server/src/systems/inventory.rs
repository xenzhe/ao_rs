//! Inventory, equipment, ground items and drops. OWNER: inventory system (P1, Opus C).
use crate::world::World;
use ao_protocol::Tone;

impl World {
    // ---------------------------------------------------------------- hooks (contract)

    /// Called by combat when a creature dies: drops its loot (npcs.dat drops, gold).
    #[allow(dead_code)]
    pub(crate) fn drop_npc_loot(&mut self, _map: u16, _npc: u32, _killer: u32) {}

    /// Called when a player enters the world: re-equips `inv.pending_equipped` deciding
    /// each slot's role (weapon, armor...) from obj.dat, then sends the inventory.
    pub(crate) fn restore_equipment(&mut self, _id: u32) {}

    /// Visible look of a player from its equipment: (body, head, weapon anim, shield
    /// anim, helmet anim), as obj.dat defines them. Used by `send_look`.
    pub(crate) fn look_of(&self, id: u32) -> (u16, u16, u16, u16, u16) {
        self.player(id).map_or((0, 0, 0, 0, 0), |p| (p.body, p.head, 0, 0, 0))
    }

    // ---------------------------------------------------------------- messages

    pub(crate) fn on_use_item(&mut self, id: u32, _slot: u8) {
        self.console(id, "El inventario todavia no esta disponible.", Tone::Warn);
    }
    pub(crate) fn on_equip_item(&mut self, id: u32, _slot: u8) {
        self.console(id, "El inventario todavia no esta disponible.", Tone::Warn);
    }
    pub(crate) fn on_drop_item(&mut self, id: u32, _slot: u8, _amount: u32) {
        self.console(id, "El inventario todavia no esta disponible.", Tone::Warn);
    }
    pub(crate) fn on_drop_gold(&mut self, id: u32, _amount: u32) {
        self.console(id, "El inventario todavia no esta disponible.", Tone::Warn);
    }
    pub(crate) fn on_pick_up(&mut self, id: u32) {
        self.console(id, "El inventario todavia no esta disponible.", Tone::Warn);
    }
    pub(crate) fn on_swap_slots(&mut self, id: u32, _a: u8, _b: u8) {
        self.console(id, "El inventario todavia no esta disponible.", Tone::Warn);
    }
}

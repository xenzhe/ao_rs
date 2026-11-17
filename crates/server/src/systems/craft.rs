//! Craft system: crafting: blacksmithing, carpentry, tailoring, alchemy. OWNER: Opus B (docs/P4_CONTRATO.md).
use crate::clock::GameTime;
use crate::world::World;
use ao_protocol::CraftC2S;

impl World {
    pub(crate) fn on_craft(&mut self, id: u32, msg: CraftC2S) {
        let _ = id;
        match msg {}
    }

    /// HOOK (interact): the player talked to a crafting NPC (`Crafts > 0` in npcs.dat),
    /// alive and near.
    pub(crate) fn craft_npc(&mut self, id: u32, npc: u32) {
        let _ = npc;
        self.console(id, "La artesanía llega en P4.", ao_protocol::Tone::Info);
    }

    /// Every world tick.
    pub(crate) fn tick_craft(&mut self, now: GameTime) {
        let _ = now;
    }

    /// When the player enters the world (after everything else is sent).
    pub(crate) fn craft_on_enter(&mut self, id: u32) {
        let _ = id;
    }

    /// Right before the player leaves the world, still present.
    pub(crate) fn craft_on_leave(&mut self, id: u32) {
        let _ = id;
    }
}

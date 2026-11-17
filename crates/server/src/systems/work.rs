//! Work system: gathering work: mining, woodcutting, fishing, smelting. OWNER: Opus A (docs/P4_CONTRATO.md).
use crate::clock::GameTime;
use crate::world::World;
use ao_protocol::WorkC2S;

impl World {
    pub(crate) fn on_work(&mut self, id: u32, msg: WorkC2S) {
        let _ = id;
        match msg {}
    }

    /// Every world tick.
    pub(crate) fn tick_work(&mut self, now: GameTime) {
        let _ = now;
    }

    /// When the player enters the world (after everything else is sent).
    pub(crate) fn work_on_enter(&mut self, id: u32) {
        let _ = id;
    }

    /// Right before the player leaves the world, still present.
    pub(crate) fn work_on_leave(&mut self, id: u32) {
        let _ = id;
    }
}

//! Gm system: game masters: commands, roles, penalties (jail, silence, ban), logs. OWNER: Opus C (docs/P4_CONTRATO.md).
use crate::clock::GameTime;
use crate::world::World;
use ao_protocol::GmC2S;

impl World {
    pub(crate) fn on_gm(&mut self, id: u32, msg: GmC2S) {
        let _ = id;
        match msg {}
    }

    /// HOOK (chat): a `/` line. Returns true if it was a GM command (handled, not said).
    /// Runs before the development commands (`/dar`, `/ir`).
    pub(crate) fn gm_command(&mut self, id: u32, line: &str) -> bool {
        let _ = (id, line);
        false
    }

    /// HOOK (chat, channels): silenced players cannot talk (`flags.Silenciado`).
    pub(crate) fn gm_is_silenced(&self, id: u32) -> bool {
        let _ = id;
        false
    }

    /// Every world tick.
    pub(crate) fn tick_gm(&mut self, now: GameTime) {
        let _ = now;
    }

    /// When the player enters the world (after everything else is sent).
    pub(crate) fn gm_on_enter(&mut self, id: u32) {
        let _ = id;
    }

    /// Right before the player leaves the world, still present.
    pub(crate) fn gm_on_leave(&mut self, id: u32) {
        let _ = id;
    }
}

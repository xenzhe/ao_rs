//! Clicking on the world: talking to NPCs (banker, priest, merchant...). OWNER: NPC system
//! (P1, Opus D). Opens the bank through `open_bank` (bank system).
use crate::world::World;
use ao_protocol::Tone;

impl World {
    pub(crate) fn on_interact(&mut self, id: u32, _tx: i32, _ty: i32) {
        self.console(id, "No hay nada con que interactuar ahi.", Tone::Info);
    }
}

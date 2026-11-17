//! Melee and ranged combat. OWNER: combat system (P1, Opus A).
use crate::world::World;
use ao_protocol::Tone;

impl World {
    pub(crate) fn on_melee(&mut self, id: u32, _seq: u32) {
        self.console(id, "El combate cuerpo a cuerpo todavia no esta disponible.", Tone::Warn);
    }

    pub(crate) fn on_shoot(&mut self, id: u32, _tx: i32, _ty: i32, _seq: u32) {
        self.console(id, "El combate a distancia todavia no esta disponible.", Tone::Warn);
    }
}

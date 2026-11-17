//! What the client knows about craft (from `S2C::Craft`) and its actions (`impl Game`).
//! OWNER: Opus B (docs/P4_CONTRATO.md).
use ao_protocol::CraftS2C;

#[derive(Debug, Default)]
pub struct CraftState {}

impl CraftState {
    pub fn apply(&mut self, msg: &CraftS2C) {
        match *msg {}
    }
}

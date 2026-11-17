//! What the client knows about work (from `S2C::Work`) and its actions (`impl Game`).
//! OWNER: Opus A (docs/P4_CONTRATO.md).
use ao_protocol::WorkS2C;

#[derive(Debug, Default)]
pub struct WorkState {}

impl WorkState {
    pub fn apply(&mut self, msg: &WorkS2C) {
        match *msg {}
    }
}

//! What the client knows about gm (from `S2C::Gm`) and its actions (`impl Game`).
//! OWNER: Opus C (docs/P4_CONTRATO.md).
use ao_protocol::GmS2C;

#[derive(Debug, Default)]
pub struct GmState {}

impl GmState {
    pub fn apply(&mut self, msg: &GmS2C) {
        match *msg {}
    }
}

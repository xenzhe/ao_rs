//! What the client knows about event (from `S2C::Event`) and its actions (`impl Game`).
//! OWNER: Opus D (docs/P4_CONTRATO.md).
use ao_protocol::EventS2C;

#[derive(Debug, Default)]
pub struct EventState {}

impl EventState {
    pub fn apply(&mut self, msg: &EventS2C) {
        match *msg {}
    }
}

//! What the client knows about party (filled from `S2C::Party` only). OWNER: Opus C.
use ao_protocol::PartyS2C;

#[derive(Debug, Default)]
pub struct PartyState {}

impl PartyState {
    pub fn apply(&mut self, msg: &PartyS2C) {
        match *msg {}
    }
}

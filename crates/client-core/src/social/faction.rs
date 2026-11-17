//! What the client knows about faction (filled from `S2C::Faction` only). OWNER: Opus C.
use ao_protocol::FactionS2C;

#[derive(Debug, Default)]
pub struct FactionState {}

impl FactionState {
    pub fn apply(&mut self, msg: &FactionS2C) {
        match *msg {}
    }
}

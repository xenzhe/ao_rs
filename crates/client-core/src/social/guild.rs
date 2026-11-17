//! What the client knows about guild (filled from `S2C::Guild` only). OWNER: Opus A.
use ao_protocol::GuildS2C;

#[derive(Debug, Default)]
pub struct GuildState {}

impl GuildState {
    pub fn apply(&mut self, msg: &GuildS2C) {
        match *msg {}
    }
}

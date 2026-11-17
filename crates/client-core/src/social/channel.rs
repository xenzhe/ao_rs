//! What the client knows about channel (filled from `S2C::Channel` only). OWNER: Sonnet 2.
use ao_protocol::ChannelS2C;

#[derive(Debug, Default)]
pub struct ChannelState {}

impl ChannelState {
    pub fn apply(&mut self, msg: &ChannelS2C) {
        match *msg {}
    }
}

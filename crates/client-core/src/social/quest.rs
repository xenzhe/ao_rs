//! What the client knows about quest (filled from `S2C::Quest` only). OWNER: Sonnet 1.
use ao_protocol::QuestS2C;

#[derive(Debug, Default)]
pub struct QuestState {}

impl QuestState {
    pub fn apply(&mut self, msg: &QuestS2C) {
        match *msg {}
    }
}

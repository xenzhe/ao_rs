//! What the client knows about faction (filled from `S2C::Faction` only). OWNER: Opus C.
use ao_protocol::FactionS2C;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct FactionState {
    /// Own status (`e_Facciones`); meaningful once `known` (the server sends it on entering).
    pub status: u8,
    pub rank: u8,
    pub title: String,
    pub score: i32,
    pub citizens_killed: u32,
    pub criminals_killed: u32,
    /// Citizen "seguro".
    pub safe: bool,
    pub legion_safe: bool,
    /// Faction status of other players, by entity id.
    pub others: HashMap<u32, u8>,
    /// The server sent the state at least once.
    pub known: bool,
}

impl FactionState {
    pub fn apply(&mut self, msg: &FactionS2C) {
        match msg {
            FactionS2C::State { status, rank, title, score, citizens_killed, criminals_killed } => {
                self.status = *status;
                self.rank = *rank;
                self.title = title.clone();
                self.score = *score;
                self.citizens_killed = *citizens_killed;
                self.criminals_killed = *criminals_killed;
                self.known = true;
            }
            FactionS2C::Safe { on } => self.safe = *on,
            FactionS2C::LegionSafe { on } => self.legion_safe = *on,
            FactionS2C::Status { id, status } => {
                self.others.insert(*id, *status);
            }
        }
    }
}

//! What the client knows about party (filled from `S2C::Party` only). OWNER: Opus C.
use ao_protocol::social::party::PartyMember;
use ao_protocol::PartyS2C;

#[derive(Debug)]
pub struct PartyState {
    /// Leader id (0 = not in a party).
    pub leader: u32,
    /// Leader first.
    pub members: Vec<PartyMember>,
    /// Pending invitation: (inviter id, inviter name).
    pub invitation: Option<(u32, String)>,
    /// Party "seguro" (on when entering, as in the original).
    pub safe: bool,
}

impl Default for PartyState {
    fn default() -> Self {
        PartyState { leader: 0, members: Vec::new(), invitation: None, safe: true }
    }
}

impl PartyState {
    pub fn apply(&mut self, msg: &PartyS2C) {
        match msg {
            PartyS2C::Invited { from, name } => self.invitation = Some((*from, name.clone())),
            PartyS2C::Members { leader, members } => {
                self.leader = *leader;
                self.members = members.clone();
                if !members.is_empty() {
                    self.invitation = None;
                }
            }
            PartyS2C::Safe { on } => self.safe = *on,
        }
    }

    pub fn in_party(&self) -> bool {
        !self.members.is_empty()
    }

    pub fn has(&self, id: u32) -> bool {
        self.members.iter().any(|m| m.id == id)
    }
}

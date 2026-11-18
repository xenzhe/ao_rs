//! Post office and surgeon windows' state (from `S2C::Mail` / `S2C::Surgeon`) and actions.
use crate::Game;
use ao_protocol::p4::mail::{MailC2S, MailHeader, MailLetterView, MailS2C};
use ao_protocol::p4::surgeon::{SurgeonC2S, SurgeonS2C};
use ao_protocol::C2S;

#[derive(Debug, Default)]
pub struct MailState {
    /// `(npc, letters, postage, capacity)` while the post office is open.
    pub open: Option<(u32, Vec<MailHeader>, u32, u8)>,
    /// The letter being read.
    pub letter: Option<MailLetterView>,
    /// Set when the server confirms a letter left (the form clears itself).
    pub sent: bool,
}

impl MailState {
    pub fn apply(&mut self, msg: &MailS2C) {
        match msg {
            MailS2C::Inbox { npc, letters, postage, capacity } => {
                if self.open.as_ref().map(|o| o.0) != Some(*npc) {
                    self.letter = None;
                }
                self.open = Some((*npc, letters.clone(), *postage, *capacity));
            }
            MailS2C::Letter(l) => self.letter = Some(l.clone()),
            MailS2C::Sent => self.sent = true,
            MailS2C::Close => *self = MailState::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct SurgeonState {
    /// `(npc, heads, price, current head)`.
    pub open: Option<(u32, Vec<u16>, u32, u16)>,
}

impl SurgeonState {
    pub fn apply(&mut self, msg: &SurgeonS2C) {
        match msg {
            SurgeonS2C::Open { npc, heads, price, current } => self.open = Some((*npc, heads.clone(), *price, *current)),
            SurgeonS2C::Close => self.open = None,
        }
    }
}

impl Game {
    fn mail_npc(&self) -> Option<u32> {
        self.local.p4.mail.open.as_ref().map(|o| o.0)
    }

    pub fn mail_send(&mut self, to: &str, subject: &str, body: &str, gold: u32, items: Vec<(u8, u32)>) {
        if let Some(npc) = self.mail_npc() {
            self.send(&C2S::Mail(MailC2S::Send { npc, to: to.into(), subject: subject.into(), body: body.into(), gold, items }));
        }
    }

    pub fn mail_read(&mut self, id: i64) {
        if let Some(npc) = self.mail_npc() {
            self.send(&C2S::Mail(MailC2S::Read { npc, id }));
        }
    }

    pub fn mail_take(&mut self, id: i64) {
        if let Some(npc) = self.mail_npc() {
            self.send(&C2S::Mail(MailC2S::Take { npc, id }));
        }
    }

    pub fn mail_delete(&mut self, id: i64) {
        if let Some(npc) = self.mail_npc() {
            self.local.p4.mail.letter = None;
            self.send(&C2S::Mail(MailC2S::Delete { npc, id }));
        }
    }

    pub fn mail_close(&mut self) {
        self.local.p4.mail = MailState::default();
    }

    pub fn surgeon_change(&mut self, head: u16) {
        if let Some((npc, ..)) = self.local.p4.surgeon.open {
            self.send(&C2S::Surgeon(SurgeonC2S::Change { npc, head }));
        }
    }

    pub fn surgeon_close(&mut self) {
        self.local.p4.surgeon.open = None;
    }
}

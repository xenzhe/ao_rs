//! Player mail at a post office NPC (TSAO `Correos`). Wrapped in `C2S::Mail` / `S2C::Mail`.
//! Letters can carry a parcel: gold and a few item stacks taken from the bag when sent.
use serde::{Deserialize, Serialize};

/// Longest subject / body (characters).
pub const SUBJECT_MAX: usize = 40;
pub const BODY_MAX: usize = 600;
/// Item stacks one letter can carry.
pub const PARCEL_MAX: usize = 5;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MailHeader {
    pub id: i64,
    pub sender: String,
    pub subject: String,
    /// Unix seconds.
    pub sent_at: i64,
    pub unread: bool,
    /// The parcel is still there (gold or items).
    pub parcel: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MailLetterView {
    pub id: i64,
    pub sender: String,
    pub subject: String,
    pub body: String,
    pub sent_at: i64,
    pub gold: u32,
    /// `(obj, amount)`.
    pub items: Vec<(u16, u32)>,
}

/// Client -> server. Every request names the post office NPC: it must stay near.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MailC2S {
    /// `items`: `(inventory slot, amount)`.
    Send { npc: u32, to: String, subject: String, body: String, gold: u32, items: Vec<(u8, u32)> },
    Read { npc: u32, id: i64 },
    /// Moves the parcel of letter `id` into the bag (all or nothing).
    Take { npc: u32, id: i64 },
    Delete { npc: u32, id: i64 },
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MailS2C {
    /// The post office window, with the inbox (newest first) and what sending costs.
    Inbox { npc: u32, letters: Vec<MailHeader>, postage: u32, capacity: u8 },
    Letter(MailLetterView),
    /// The letter was sent (the window clears the form).
    Sent,
    Close,
}

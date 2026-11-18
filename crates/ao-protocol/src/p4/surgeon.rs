//! The surgeon NPC (TSAO `cirujano`): pay to change the face. Wrapped in `C2S::Surgeon` /
//! `S2C::Surgeon`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SurgeonC2S {
    Change { npc: u32, head: u16 },
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SurgeonS2C {
    /// Faces this character may take (its race and gender) and the price.
    Open { npc: u32, heads: Vec<u16>, price: u32, current: u16 },
    Close,
}

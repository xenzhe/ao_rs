//! Travelling between cities with a transporter NPC (TSAO `Viajero`, AO20 `Transporter`).
//! Wrapped in `C2S::Travel` / `S2C::Travel`. The destinations, prices and level limits come
//! from npcs.dat (`CityN`); the server validates everything again when one is chosen.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TravelDest {
    pub name: String,
    pub map: u16,
    /// Gold the trip costs this player (already the price for its level).
    pub price: u32,
    /// Lowest level allowed (0 = any).
    pub min_level: u32,
}

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TravelC2S {
    /// Take trip `index` (0-based, as listed in `Open`) with NPC `npc`.
    Go { npc: u32, index: u8 },
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TravelS2C {
    /// The transporter `npc` offers these trips.
    Open { npc: u32, name: String, dests: Vec<TravelDest> },
    /// Close the travel window (done, too far, dead...).
    Close,
}

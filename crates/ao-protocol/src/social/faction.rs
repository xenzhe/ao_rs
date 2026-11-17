//! Factions (armada/caos, ciudadano/criminal). OWNER: Opus C (docs/P2_CONTRATO.md). Wrapped in `C2S::Faction` / `S2C::Faction`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FactionC2S {
    /// `HandleLeaveFaction`: a citizen becomes criminal at once; army/legion members leave
    /// at their recruiter `npc` (the original uses the targeted NPC).
    Leave { npc: Option<u32> },
    /// `HandleSafeToggle`: the citizen "seguro" (no attacking citizens or the army).
    ToggleSafe,
    /// `HandleLegionarySecure`: legionaries do not hit each other while on.
    ToggleLegionSafe,
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FactionS2C {
    /// Own faction state (`e_Facciones` status: 0 criminal, 1 citizen, 2 chaos, 3 army,
    /// 4 chaos council, 5 army council), current rank and its title.
    State { status: u8, rank: u8, title: String, score: i32, citizens_killed: u32, criminals_killed: u32 },
    /// `WriteSafeModeOn/Off`.
    Safe { on: bool },
    /// `WriteLegionarySecure`.
    LegionSafe { on: bool },
    /// `RefreshCharStatus`: another player's faction status changed (or seen).
    Status { id: u32, status: u8 },
}

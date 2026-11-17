//! Factions (armada/caos, ciudadano/criminal). OWNER: Opus C (docs/P2_CONTRATO.md). Wrapped in `C2S::Faction` / `S2C::Faction`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FactionC2S {}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FactionS2C {}

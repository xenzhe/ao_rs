//! Events: duels, tournaments, auctions, invasions, castles. OWNER: Opus D (docs/P4_CONTRATO.md). Wrapped in `C2S::Event` / `S2C::Event`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EventC2S {}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EventS2C {}

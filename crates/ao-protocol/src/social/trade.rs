//! Commerce with NPCs and between players. OWNER: Opus B (docs/P2_CONTRATO.md). Wrapped in `C2S::Trade` / `S2C::Trade`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TradeC2S {}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TradeS2C {}

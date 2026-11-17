//! Parties (grupos). OWNER: Opus C (docs/P2_CONTRATO.md). Wrapped in `C2S::Party` / `S2C::Party`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum PartyC2S {}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum PartyS2C {}

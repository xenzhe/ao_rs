//! Gathering work: mining, woodcutting, fishing, smelting. OWNER: Opus A (docs/P4_CONTRATO.md). Wrapped in `C2S::Work` / `S2C::Work`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WorkC2S {}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WorkS2C {}

//! Guilds (clanes). OWNER: Opus A (docs/P2_CONTRATO.md). Wrapped in `C2S::Guild` / `S2C::Guild`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GuildC2S {}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GuildS2C {}

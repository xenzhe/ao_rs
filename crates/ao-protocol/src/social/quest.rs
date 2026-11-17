//! Quests (misiones). OWNER: Sonnet 1 (docs/P2_CONTRATO.md). Wrapped in `C2S::Quest` / `S2C::Quest`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum QuestC2S {}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum QuestS2C {}

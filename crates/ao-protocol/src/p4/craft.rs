//! Crafting: blacksmithing, carpentry, tailoring, alchemy. OWNER: Opus B (docs/P4_CONTRATO.md). Wrapped in `C2S::Craft` / `S2C::Craft`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CraftC2S {}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CraftS2C {}

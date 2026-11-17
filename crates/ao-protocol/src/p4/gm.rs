//! Game masters: commands, roles, penalties (jail, silence, ban), logs. OWNER: Opus C (docs/P4_CONTRATO.md). Wrapped in `C2S::Gm` / `S2C::Gm`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GmC2S {}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GmS2C {}

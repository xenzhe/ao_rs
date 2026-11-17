//! Chat channels (whisper, party, guild, global...). OWNER: Sonnet 2 (docs/P2_CONTRATO.md). Wrapped in `C2S::Channel` / `S2C::Channel`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ChannelC2S {}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ChannelS2C {}

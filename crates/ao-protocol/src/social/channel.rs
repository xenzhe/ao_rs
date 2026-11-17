//! Chat channels (whisper, yell, party, guild, global). OWNER: Sonnet 2 (docs/P2_CONTRATO.md).
//! Wrapped in `C2S::Channel` / `S2C::Channel`. Map chat (`Say`) stays in `messages.rs`.
//! Source: Protocol.bas HandleWhisper / HandleYell / HandleGuildMessage and
//! Protocol_GmCommands.bas HandleGlobalMessage (AO20 server).
use serde::{Deserialize, Serialize};

/// Which channel a line belongs to.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    Whisper,
    Yell,
    Party,
    Guild,
    Global,
}

/// Client -> server. The client only states an intention; the server validates everything
/// (who may talk, who is in range, flood, cost).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ChannelC2S {
    /// Private line to one online player (by name) who is nearby.
    Whisper { to: String, text: String },
    /// Loud line for everyone who can see you.
    Yell { text: String },
    /// Line for the members of your party.
    Party { text: String },
    /// Line for the members of your guild.
    Guild { text: String },
    /// Line for every player online (costs gold, has its own interval).
    Global { text: String },
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ChannelS2C {
    /// One chat line. `target` is only filled for whispers (the receiver's name), so the
    /// sender's echo and the receiver's copy are told apart by `author` / `target`.
    Message { channel: ChannelKind, author: String, target: String, text: String },
}

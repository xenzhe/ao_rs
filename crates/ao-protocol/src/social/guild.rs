//! Guilds (clanes). OWNER: Opus A (docs/P2_CONTRATO.md). Wrapped in `C2S::Guild` / `S2C::Guild`.
//! Intentions only: the server validates every request (`server/src/systems/guild.rs`).
use serde::{Deserialize, Serialize};

/// `e_RELACIONES_GUILD`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuildRelation {
    War,
    Peace,
    Alliance,
}

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GuildC2S {
    /// `HandleCreateNewGuild`. `alignment` = `e_ALINEACION_GUILD` (0 neutral, 1 Armada,
    /// 2 Legion, 3 citizen, 4 criminal).
    Found { name: String, description: String, alignment: u8 },
    /// `HandleGuildRequestMembership`.
    RequestJoin { guild: String, application: String },
    /// `HandleGuildAcceptNewMember` (leader).
    Accept { name: String },
    /// `HandleGuildRejectNewMember` (leader).
    Reject { name: String, reason: String },
    /// `HandleGuildKickMember` (leader).
    Kick { name: String },
    /// `HandleGuildLeave`.
    Leave,
    /// `HandleGuildUpdateNews` (leader).
    UpdateNews { news: String },
    /// `HandleClanCodexUpdate` (leader): description / codex.
    UpdateDescription { description: String },
    /// `SendGuildNews` / `SendGuildLeaderInfo`: details of the own guild.
    Info,
    /// `PrepareGuildsList`.
    List,
    /// Peace or alliance proposal to another guild (leader).
    Propose { guild: String, relation: GuildRelation, text: String },
    /// Accept a pending proposal from `guild` (leader).
    AcceptProposal { guild: String },
    /// Reject a pending proposal from `guild` (leader).
    RejectProposal { guild: String },
    /// `HandleGuildDeclareWar` (leader): unilateral.
    DeclareWar { guild: String },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GuildMember {
    pub name: String,
    pub online: bool,
    pub founder: bool,
    pub leader: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GuildApplicant {
    pub name: String,
    pub application: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GuildInfo {
    pub name: String,
    pub alignment: u8,
    pub level: u8,
    pub exp: u32,
    /// Exp needed for the next level (0 at the maximum).
    pub exp_next: u32,
    pub max_members: u16,
    pub founder: String,
    pub leader: String,
    pub description: String,
    pub news: String,
    pub members: Vec<GuildMember>,
    /// Only sent to the leader.
    pub applicants: Vec<GuildApplicant>,
    /// Other guilds that are not at peace with this one.
    pub relations: Vec<(String, GuildRelation)>,
    /// Incoming proposals (only sent to the leader): (guild, relation, text).
    pub proposals: Vec<(String, GuildRelation, String)>,
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GuildS2C {
    /// The player's guild (after entering, and after any change to it).
    Info(Box<GuildInfo>),
    /// The player is not in a guild (left, kicked, or never joined).
    NoGuild,
    /// Every guild: (name, alignment).
    List(Vec<(String, u8)>),
    /// Guild tags shown under the names (`<clan>` in `RefreshCharStatus`): every tagged
    /// player online, sent when entering the world.
    Tags(Vec<(u32, String)>),
    /// One player's tag changed (`None`: no guild).
    Tag { id: u32, tag: Option<String> },
}

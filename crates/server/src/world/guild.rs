//! Per-player guild state (`Player.guild`). OWNER: Opus A (docs/P2_CONTRATO.md).
use crate::security::RateLimiter;
use ao_rules::guild::{Alignment, GuildLevels, Relation};
use std::collections::{BTreeMap, HashMap};

/// Guild requests allowed per window (anti-flood, not in VB6).
pub const GUILD_RATE_MAX: usize = 16;
pub const GUILD_RATE_WINDOW_MS: u64 = 2000;

pub struct GuildPiece {
    /// `UserList.GuildIndex`: the guild the player is connected to (database id).
    pub guild: Option<i64>,
    pub rate: RateLimiter,
}

impl Default for GuildPiece {
    fn default() -> Self {
        GuildPiece { guild: None, rate: RateLimiter::new(GUILD_RATE_MAX, GUILD_RATE_WINDOW_MS) }
    }
}

/// One guild (`clsClan`). Members and applicants are characters (database ids).
#[derive(Debug, Clone, PartialEq)]
pub struct Guild {
    pub id: i64,
    pub name: String,
    pub alignment: Alignment,
    pub founder: i64,
    pub leader: i64,
    pub description: String,
    pub news: String,
    pub level: u8,
    pub exp: u32,
    /// char id -> name.
    pub members: BTreeMap<i64, String>,
    /// `guild_request`: (char id, name, application), in arrival order.
    pub applicants: Vec<(i64, String, String)>,
    /// Relation with other guilds (missing = peace).
    pub relations: HashMap<i64, Relation>,
    /// Incoming proposals: from guild -> (relation, text).
    pub proposals: BTreeMap<i64, (Relation, String)>,
}

impl Guild {
    pub fn relation(&self, other: i64) -> Relation {
        self.relations.get(&other).copied().unwrap_or(Relation::Peace)
    }
}

/// World-wide guild registry (`World.guilds`): every guild, loaded from the database on
/// first use (`modGuilds.LoadGuildsDB`).
#[derive(Debug, Default)]
pub struct Guilds {
    pub loaded: bool,
    pub levels: Option<GuildLevels>,
    pub list: BTreeMap<i64, Guild>,
}

impl Guilds {
    pub fn by_name(&self, name: &str) -> Option<&Guild> {
        let n = name.trim();
        self.list.values().find(|g| g.name.eq_ignore_ascii_case(n))
    }

    /// The guild a character belongs to (online or not).
    pub fn of_char(&self, char_id: i64) -> Option<i64> {
        self.list.values().find(|g| g.members.contains_key(&char_id)).map(|g| g.id)
    }

    /// The guild a character has a pending application to.
    pub fn applied_to(&self, char_id: i64) -> Option<i64> {
        self.list.values().find(|g| g.applicants.iter().any(|a| a.0 == char_id)).map(|g| g.id)
    }
}

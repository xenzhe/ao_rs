//! Technical server configuration (`config/server.toml`). Game values live in `data/`.
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: String,
    /// AO20 `Recursos` folder; empty = auto-detect (see `ao_formats::recursos_dir`).
    pub recursos: String,
    pub data_dir: String,
    /// SQLite file (":memory:" = throwaway, for tests).
    pub database: String,
    /// Allow the account-less `C2S::Login` (development and tests only).
    pub dev_login: bool,
    pub security: SecurityConfig,
    pub world: WorldConfig,
    pub ops: OpsConfig,
}

/// Operation: statistics, backups, graceful shutdown (`crate::ops`, docs/OPERACION.md).
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct OpsConfig {
    /// Minutes between statistics lines in the log (0 = never).
    pub stats_interval_min: u64,
    /// JSON file refreshed for a web page to read ("" = do not write it).
    pub stats_file: String,
    /// Seconds between refreshes of the JSON file (and of the rate measurements).
    pub stats_file_secs: u64,
    /// Folder for database backups.
    pub backup_dir: String,
    /// Hours between automatic backups (0 = no periodic backups).
    pub backup_interval_hours: u64,
    /// Days kept: the newest backup of each of the last N days (all of today's are kept).
    pub backup_keep_days: usize,
    /// Take a backup when the server shuts down.
    pub backup_on_shutdown: bool,
}

impl Default for OpsConfig {
    fn default() -> Self {
        OpsConfig {
            stats_interval_min: 5,
            stats_file: "data/stats.json".into(),
            stats_file_secs: 10,
            backup_dir: "data/backups".into(),
            backup_interval_hours: 6,
            backup_keep_days: 7,
            backup_on_shutdown: true,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SecurityConfig {
    pub max_msgs_per_sec: u32,
    pub kick_strikes: u32,
    pub strike_decay_secs: u64,
    pub walk_tolerance: f32,
    /// Failed account logins on one connection before it is closed.
    pub max_failed_logins: u32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct WorldConfig {
    pub tick_ms: u64,
    /// `ExpMult` of AO20 Configuracion.ini: multiplies the experience NPCs give (0 = 1).
    pub exp_mult: u32,
    /// `OroMult`: multiplies the gold NPCs drop (0 = 1).
    pub gold_mult: u32,
    /// `DropMult`: divides the npcs.dat drop chance "1 in N" (0 = 1).
    pub drop_mult: u32,
    /// Minutes an object dropped on the floor stays before it vanishes (0 = forever, as in
    /// the official server, whose cleaner `TLimpiezaItem` is never used).
    pub floor_cleanup_minutes: u64,
    /// Real minutes per in-game day (`DuracionDia` of Configuracion.ini, 120 officially).
    pub day_length_minutes: u64,
    /// Creatures with npcs.dat `Veneno` poison on a hit (`NpcEnvenenarUser`). On, as in AO20.
    pub creature_poison: bool,
    /// Feature toggle `bandit_unequip_bonus` (on in the official feature_toggle.ini): a
    /// disarmed slot stays locked 8 s (EOT23) and the bandit's chance is 0.2 x skill
    /// instead of 0.15 x skill. The disarm itself happens either way.
    pub bandit_unequip_bonus: bool,
    /// Feature toggle `elemental_tags` (off in the official feature_toggle.ini): weapon and
    /// creature `ElementalTags` scale the damage through `[ElementalMatrixForNpcs]`.
    pub elemental_tags: bool,
    /// Traps: `TrapPlacementRange` and `TrapParaliceDuration` of Configuracion.ini.
    pub trap_placement_range: i32,
    pub trap_paralyze_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            bind: "127.0.0.1:7666".into(),
            recursos: String::new(),
            data_dir: "data".into(),
            database: "data/ao.db".into(),
            dev_login: true,
            security: SecurityConfig::default(),
            world: WorldConfig::default(),
            ops: OpsConfig::default(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        SecurityConfig { max_msgs_per_sec: 40, kick_strikes: 8, strike_decay_secs: 10, walk_tolerance: 0.08, max_failed_logins: 3 }
    }
}

impl Default for WorldConfig {
    fn default() -> Self {
        WorldConfig { tick_ms: 50, exp_mult: 1, gold_mult: 1, drop_mult: 1, floor_cleanup_minutes: 15, day_length_minutes: 120, creature_poison: true, bandit_unequip_bonus: true, elemental_tags: false,trap_placement_range: 6, trap_paralyze_secs: 15 }
    }
}

impl ServerConfig {
    /// Reads the file if it exists; missing keys keep their defaults.
    pub fn load(path: &Path) -> Result<ServerConfig, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(_) => Ok(ServerConfig::default()),
        }
    }

    pub fn recursos_path(&self) -> PathBuf {
        if self.recursos.trim().is_empty() {
            ao_formats::recursos_dir()
        } else {
            PathBuf::from(&self.recursos)
        }
    }
}

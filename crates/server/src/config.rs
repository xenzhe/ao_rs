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
    pub events: EventsConfig,
}

/// Phase 6 events (`[events]`): what AO20 read from its own `Configuracion.ini [EVENTOS]`
/// and `Server.ini AUTOEVENT*` (neither file is public: everything is off by default).
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct EventsConfig {
    /// Hours of the server's local time (dates of GlobalQuests.dat, hourly events).
    pub utc_offset_hours: i32,
    /// `[EVENTOS] 0..23`: hour -> "tipo-duración-multiplicación" (types 1 gold, 2 exp,
    /// 3 gathering, 4 drop, 5 gold+exp, 6 +gathering, 7 +drop).
    pub hourly: std::collections::HashMap<String, String>,
    /// `AUTOEVENTHOURCALENDAR_Horarios`: hour of the automatic lobby event (-1 = never).
    pub auto_hour: i32,
    /// `AUTOEVENTDAYCALENDAR_<día>`: Sunday..Saturday, the `e_EventType` of the day
    /// (0 none, 1 capture, 2 hunt, 3 deathmatch).
    pub auto_days: [u8; 7],
    /// `AUTO<TIPO>_*` of the automatic event (same for every type).
    pub auto_min_level: u32,
    pub auto_max_level: u32,
    pub auto_min_players: u32,
    pub auto_max_players: u32,
    pub auto_team_size: u32,
    pub auto_rounds: u32,
    pub auto_fee: u32,
    /// Founding a castle needs Patreon "Noble" in AO20, which does not exist here: off =
    /// nobody can found one (as for the Patreon maps), on = any player can.
    pub castle_without_patreon: bool,
}

impl Default for EventsConfig {
    fn default() -> Self {
        EventsConfig {
            utc_offset_hours: -3,
            hourly: Default::default(),
            auto_hour: -1,
            auto_days: [0; 7],
            auto_min_level: 1,
            auto_max_level: 47,
            auto_min_players: 2,
            auto_max_players: 20,
            auto_team_size: 1,
            auto_rounds: 3,
            auto_fee: 0,
            castle_without_patreon: false,
        }
    }
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
    /// `RecoleccionMult`: multiplies what gathering gives (mining, wood, fishing; 0 = 1).
    pub gather_mult: u32,
    /// Minutes an object dropped on the floor stays before it vanishes (0 = forever, as in
    /// the official server, whose cleaner `TLimpiezaItem` is never used).
    pub floor_cleanup_minutes: u64,
    /// Real minutes per in-game day (`DuracionDia` of Configuracion.ini, 120 officially).
    pub day_length_minutes: u64,
    /// Creatures with npcs.dat `Veneno` poison on a hit (`NpcEnvenenarUser`). On, as in AO20.
    pub creature_poison: bool,
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
            events: EventsConfig::default(),
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
        WorldConfig { tick_ms: 50, exp_mult: 1, gold_mult: 1, drop_mult: 1, gather_mult: 1, floor_cleanup_minutes: 15, day_length_minutes: 120, creature_poison: true }
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

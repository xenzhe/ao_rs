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
    pub security: SecurityConfig,
    pub world: WorldConfig,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SecurityConfig {
    pub max_msgs_per_sec: u32,
    pub kick_strikes: u32,
    pub strike_decay_secs: u64,
    pub walk_tolerance: f32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct WorldConfig {
    pub tick_ms: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig { bind: "127.0.0.1:7666".into(), recursos: String::new(), data_dir: "data".into(), security: SecurityConfig::default(), world: WorldConfig::default() }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        SecurityConfig { max_msgs_per_sec: 40, kick_strikes: 8, strike_decay_secs: 10, walk_tolerance: 0.08 }
    }
}

impl Default for WorldConfig {
    fn default() -> Self {
        WorldConfig { tick_ms: 50 }
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

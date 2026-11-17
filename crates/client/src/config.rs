//! Client configuration (`config/client.toml`), with `$AO_SERVER` / `$AO_RECURSOS`
//! overrides for tests.
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct ClientConfig {
    pub server: String,
    /// AO20 `Recursos` folder; empty = auto-detect.
    pub recursos: String,
    pub video: VideoConfig,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct VideoConfig {
    pub width: i32,
    pub height: i32,
    pub pixel_perfect: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig { server: "127.0.0.1:7666".into(), recursos: String::new(), video: VideoConfig::default() }
    }
}

impl Default for VideoConfig {
    fn default() -> Self {
        VideoConfig { width: 1280, height: 800, pixel_perfect: false }
    }
}

impl ClientConfig {
    pub fn load() -> ClientConfig {
        let path = std::env::var("AO_CLIENT_CONFIG").unwrap_or_else(|_| "config/client.toml".into());
        let mut cfg: ClientConfig = std::fs::read_to_string(&path).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default();
        if let Ok(s) = std::env::var("AO_SERVER") {
            cfg.server = s;
        }
        cfg
    }

    pub fn recursos_path(&self) -> PathBuf {
        if self.recursos.trim().is_empty() {
            ao_formats::recursos_dir()
        } else {
            PathBuf::from(&self.recursos)
        }
    }
}

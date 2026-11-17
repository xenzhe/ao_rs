//! Authoritative game server.
//!
//! Layout:
//! - [`net`]: connections, frame limits, flood control (no game state)
//! - [`world`]: the world actor (all state, one event at a time), map instances, entities
//! - [`systems`]: gameplay, one file per system (session, movement, magic, combat, AI...)
//! - [`security`]: reusable anti-cheat building blocks
//! - [`config`] / [`data`]: `config/server.toml` and `data/*.toml`
//!
//! The client only sends intentions; every rule is enforced here with the official AO20
//! maps and rules from `ao-rules`.
pub mod config;
pub mod data;
pub mod db;
pub mod net;
pub mod rng;
pub mod security;
mod systems;
pub mod world;

use config::ServerConfig;
use data::GameData;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tokio::sync::mpsc::unbounded_channel;

/// Everything the server needs to start.
pub struct Settings {
    pub config: ServerConfig,
    pub data: GameData,
    pub recursos: PathBuf,
}

impl Settings {
    /// Repository defaults: used by tests and when no config file exists.
    pub fn defaults() -> Settings {
        let config = ServerConfig { database: ":memory:".into(), ..ServerConfig::default() };
        let recursos = config.recursos_path();
        Settings { config, data: GameData::defaults(), recursos }
    }
}

/// Runs the server on an already bound listener until the process ends.
pub async fn run(listener: TcpListener, settings: Settings) {
    let (ev_tx, ev_rx) = unbounded_channel();
    let world = world::World::new(settings.config.clone(), settings.data, settings.recursos);
    tokio::spawn(world::run(world, ev_rx));
    net::accept_loop(listener, ev_tx, settings.config.security).await;
}

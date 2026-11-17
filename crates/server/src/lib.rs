//! Authoritative game server.
//!
//! Layout:
//! - [`net`]: connections, frame limits, flood control (no game state)
//! - [`world`]: the world actor (all state, one event at a time), map instances, entities
//! - [`systems`]: gameplay, one file per system (session, movement, magic, combat, AI...)
//! - [`security`]: reusable anti-cheat building blocks
//! - [`clock`]: game time (real, or virtual for tests); [`probe`]: test instrumentation
//! - [`config`] / [`data`]: `config/server.toml` and `data/*.toml`
//!
//! The client only sends intentions; every rule is enforced here with the official AO20
//! maps and rules from `ao-rules`.
pub mod clock;
pub mod config;
pub mod data;
pub mod net;
pub mod probe;
pub mod rng;
pub mod security;
mod systems;
pub mod world;

use clock::Clock;
use config::ServerConfig;
use data::GameData;
use probe::Probe;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use world::Control;

/// Default world RNG seed (fixed: spawns and rolls are reproducible).
pub const DEFAULT_SEED: u32 = 0xA0_2026;

/// Everything the server needs to start.
pub struct Settings {
    pub config: ServerConfig,
    pub data: GameData,
    pub recursos: PathBuf,
    /// Where new characters appear (`None`: data/balance.toml).
    pub start: Option<(u16, i32, i32)>,
    pub seed: u32,
    /// `Clock::Virtual` makes the world tick only through [`ServerHandle::control`].
    pub clock: Clock,
    /// Test instrumentation; `None` in production.
    pub probe: Option<Arc<Probe>>,
}

impl Settings {
    pub fn new(config: ServerConfig, data: GameData, recursos: PathBuf) -> Settings {
        Settings { config, data, recursos, start: None, seed: DEFAULT_SEED, clock: Clock::real(), probe: None }
    }

    /// Repository defaults: used by tests and when no config file exists.
    pub fn defaults() -> Settings {
        let config = ServerConfig::default();
        let recursos = config.recursos_path();
        Settings::new(config, GameData::defaults(), recursos)
    }
}

/// What a caller gets back from [`spawn`].
pub struct ServerHandle {
    /// Present when the world runs on a virtual clock.
    pub control: Option<UnboundedSender<Control>>,
}

/// Starts the world and the accept loop on the current tokio runtime.
pub fn spawn(listener: TcpListener, settings: Settings) -> ServerHandle {
    let (ev_tx, ev_rx) = unbounded_channel();
    let security = settings.config.security.clone();
    let clock = settings.clock.clone();
    let (control, control_rx) = match clock {
        Clock::Virtual(_) => {
            let (tx, rx) = unbounded_channel();
            (Some(tx), Some(rx))
        }
        Clock::Real(_) => (None, None),
    };
    let world = world::World::new(settings);
    tokio::spawn(world::run(world, ev_rx, control_rx));
    tokio::spawn(net::accept_loop(listener, ev_tx, security, clock));
    ServerHandle { control }
}

/// Runs the server on an already bound listener until the process ends.
pub async fn run(listener: TcpListener, settings: Settings) {
    let _handle = spawn(listener, settings);
    std::future::pending::<()>().await;
}

//! `server [config/server.toml]` — logs with RUST_LOG (default: info).
use server::config::ServerConfig;
use server::data::GameData;
use server::Settings;
use std::path::{Path, PathBuf};
use tokio::net::TcpListener;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_target(false)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();

    let cfg_path = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("config/server.toml"));
    let config = match ServerConfig::load(&cfg_path) {
        Ok(c) => c,
        Err(e) => return error!("configuracion invalida: {e}"),
    };
    let data = match GameData::load(Path::new(&config.data_dir)) {
        Ok(d) => d,
        Err(e) => return error!("datos de juego invalidos: {e}"),
    };
    let recursos = config.recursos_path();
    info!(bind = %config.bind, recursos = %recursos.display(), spells = data.spells.len(), creatures = data.creatures.len(), "servidor");

    let listener = match TcpListener::bind(&config.bind).await {
        Ok(l) => l,
        Err(e) => return error!("no se pudo escuchar en {}: {e}", config.bind),
    };
    server::run(listener, Settings { config, data, recursos }).await;
}

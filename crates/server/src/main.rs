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

    // `server backup|backups|restore|stats ...` (same as the `ao-ops` binary).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = server::ops::cli(&args) {
        std::process::exit(code);
    }

    let cfg_path = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("config/server.toml"));
    let mut config = match ServerConfig::load(&cfg_path) {
        Ok(c) => c,
        Err(e) => return error!("configuracion invalida: {e}"),
    };
    // `AO_BIND=127.0.0.1:7701`: another address (several servers side by side, tests).
    if let Ok(b) = std::env::var("AO_BIND") {
        config.bind = b;
    }
    // `AO_DB=path`: another database file (":memory:" for a throwaway one).
    if let Ok(d) = std::env::var("AO_DB") {
        config.database = d;
    }
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
    // Same world loop plus statistics, periodic backups and graceful shutdown (docs/OPERACION.md).
    server::ops::run(listener, Settings::new(config, data, recursos)).await;
}

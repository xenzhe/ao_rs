//! `ao-ops backup | backups | restore <file|latest> | stats [config/server.toml]`
//!
//! Operation tool that works without the game server running (and without touching `main.rs`).
//! Same commands as `server::ops::cli`.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match server::ops::cli(&args) {
        Some(code) => std::process::exit(code),
        None => {
            eprintln!("uso: ao-ops backup [config] | backups [config] | restore <archivo|latest> [config] | stats [config]");
            std::process::exit(2);
        }
    }
}

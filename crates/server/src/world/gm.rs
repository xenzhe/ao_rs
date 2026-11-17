//! Per-player game masters state (`Player.gm`) and world-wide state (`World.gms`).
//! OWNER: Opus C (docs/P4_CONTRATO.md).
use crate::clock::GameTime;
use ao_protocol::p4::gm::GmRank;
use std::collections::HashSet;

/// `Ciudades.dat [Prision]`: where jailed players are sent (Encarcelar).
pub const JAIL: (u16, i32, i32) = (66, 50, 50);
/// `Ciudades.dat [Libertad]`: where they are released (PurgarPenas).
pub const FREEDOM: (u16, i32, i32) = (66, 48, 73);
/// `MaxJailTime` (Configuracion.ini): longest jail sentence, in minutes.
pub const MAX_JAIL_MINUTES: u64 = 518_400;
/// Longest silence accepted, in minutes (the VB reads an Int16).
pub const MAX_SILENCE_MINUTES: u64 = 32_767;
/// Rows of the log sent to the panel.
pub const PANEL_LOG_ROWS: usize = 40;
/// Env var with the first GMs: `Name:admin,Other:dios` (see systems/gm.rs).
pub const SEED_ENV: &str = "AO_GM_SEED";

#[derive(Debug, Default)]
pub struct GmPiece {
    /// `flags.Privilegios`, loaded on entering, never from the client.
    pub rank: GmRank,
    /// `Counters.Pena`: jail time left (ms of online, alive time).
    pub jail_ms: u64,
    /// `flags.Silenciado` + `MinutosRestantes`: silence time left (ms of online time).
    pub silence_ms: u64,
    /// `flags.AdminInvisible`.
    pub invisible: bool,
    /// Refused on entering (ban): leaving must not save anything.
    pub refused: bool,
}

/// A scheduled server shutdown (`/APAGAR`).
#[derive(Debug)]
pub struct Shutdown {
    pub at: GameTime,
    pub by: String,
    /// Next countdown notice.
    pub next_notice: GameTime,
}

#[derive(Debug, Default)]
pub struct GmWorld {
    /// `AO_GM_SEED` already applied to the database.
    pub seeded: bool,
    /// Last tick time (sentences count down by the elapsed time).
    pub last_tick: Option<GameTime>,
    /// NPCs created with `/ACC` (no respawn): removed for good when they die.
    pub no_respawn: HashSet<(u16, u32)>,
    pub shutdown: Option<Shutdown>,
    /// Set when a shutdown countdown ended: everyone was kicked and logins are refused.
    /// The process owner (main / ops) may exit when it sees it.
    pub shut_down: bool,
    /// Refused on entering (ban, shutdown): kicked on the next tick, not inside the
    /// enter fan-out (other systems still run their `*_on_enter` after ours).
    pub refusals: Vec<(u32, String)>,
}

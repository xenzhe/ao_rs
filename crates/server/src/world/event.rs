//! Per-player events state (`Player.event`) and world-wide state (`World.events`).
//! OWNER: Opus D (docs/P4_CONTRATO.md).
use crate::clock::GameTime;
use ao_protocol::p4::event::TournamentSpec;
use std::collections::VecDeque;

/// Per player: what `flags.SolicitudReto`, `flags.AceptoReto`, `flags.EnReto` /
/// `SalaReto` / `EquipoReto` / `LastPos` and `flags.EnTorneo` hold in AO20.
#[derive(Debug, Default)]
pub struct EventPiece {
    /// `flags.SolicitudReto`: the request this player sent.
    pub request: Option<DuelRequest>,
    /// `flags.AceptoReto`: the offerer whose request this player accepted.
    pub accepted: Option<u32>,
    /// `flags.EnReto` + `SalaReto`: the room this player fights in.
    pub room: Option<usize>,
    /// `flags.EnTorneo`.
    pub in_tournament: bool,
}

/// `t_SolicitudReto`.
#[derive(Debug, Clone)]
pub struct DuelRequest {
    pub bet: u32,
    /// < 0: no limit (`PocionesMaximas`).
    pub max_potions: i16,
    /// `Jugadores`: odd positions are the offerer's teammates, even ones the rivals.
    pub players: Vec<Invitee>,
    /// `e_SolicitudRetoEstado.EnCola`: everybody accepted, waiting for a free room.
    pub queued: bool,
}

#[derive(Debug, Clone)]
pub struct Invitee {
    pub name: String,
    pub id: u32,
    pub accepted: bool,
}

/// `Retos.dat`.
#[derive(Debug, Clone)]
pub struct DuelConfig {
    /// `MaximoEquipo` (N vs N).
    pub max_team: usize,
    /// `ApuestaMinima`.
    pub min_bet: u32,
    /// `ImpuestoApuesta` (0.10 = 10 %).
    pub tax: f64,
    /// `DuracionMaxima` in seconds.
    pub max_secs: u64,
    /// `TiempoConteo` in seconds.
    pub countdown_secs: u64,
    /// Rings: map and the two corners inside the ring (`PosIzquierda`, `PosDerecha`).
    pub rooms: Vec<RoomDef>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoomDef {
    pub map: u16,
    pub left: (i32, i32),
    pub right: (i32, i32),
}

/// One fighter of a duel (`Retos.Salas(i).Jugadores`).
#[derive(Debug, Clone)]
pub struct Fighter {
    pub name: String,
    /// None once they left (abandon, disconnection).
    pub id: Option<u32>,
    /// `flags.LastPos`.
    pub back: (u16, i32, i32),
}

/// A duel being fought in a room (`t_SalaReto` while `EnUso`).
#[derive(Debug, Clone)]
pub struct Duel {
    pub room: usize,
    /// Team A = the offerer and their mates, team B = the rivals.
    pub a: Vec<Fighter>,
    pub b: Vec<Fighter>,
    pub bet: u32,
    /// Gold held by the room: every bet, debited at the start (`IniciarReto`).
    pub pot: u64,
    /// `Ronda`, 1-based.
    pub round: u8,
    pub wins_a: u8,
    pub wins_b: u8,
    /// End of the countdown of the current round.
    pub fight_at: GameTime,
    /// `TiempoRestante` reaching 0: a draw by time.
    pub ends_at: GameTime,
    /// Set when a team concedes entirely: that team loses the whole duel.
    pub forfeit: Option<bool>,
}

/// An auction lot (`t_Subastas`, one per seller here).
#[derive(Debug, Clone)]
pub struct Lot {
    pub id: u32,
    pub seller: String,
    pub obj: u16,
    pub amount: u32,
    pub base_price: u32,
    /// `MejorOferta`, already taken from `bidder` (escrow).
    pub best: u32,
    /// `Comprador`.
    pub bidder: Option<String>,
    pub ends_at: GameTime,
}

/// A GM tournament (`t_Torneo`).
#[derive(Debug, Clone)]
pub struct Tournament {
    pub spec: TournamentSpec,
    /// Signed players and the fee each one paid (for refunds).
    pub signed: Vec<(u32, String)>,
    pub started: bool,
}

/// A saved lot (`event_auction`).
#[derive(Debug, Clone, PartialEq)]
pub struct LotRow {
    pub id: u32,
    pub seller: String,
    pub obj: u16,
    pub amount: u32,
    pub base_price: u32,
    pub best: u32,
    pub bidder: Option<String>,
    pub secs_left: u32,
}

/// Something owed to a character (`event_mailbox`).
#[derive(Debug, Clone, PartialEq)]
pub struct MailRow {
    pub id: i64,
    pub gold: u32,
    pub obj: u16,
    pub amount: u32,
    pub note: String,
}

#[derive(Debug, Default)]
pub struct EventWorld {
    /// `Retos.dat`, read on first use.
    pub duel_cfg: Option<DuelConfig>,
    /// Duel per room (`Retos.Salas`), `None` = free.
    pub rooms: Vec<Option<Duel>>,
    /// `ListaDeEspera`: offerers waiting for a free room.
    pub queue: VecDeque<u32>,
    pub lots: Vec<Lot>,
    pub next_lot: u32,
    /// Open lots were read from the database.
    pub lots_loaded: bool,
    /// Last whole second the lots were saved (their time left).
    pub lots_saved_at: u64,
    pub tournament: Option<Tournament>,
    /// Gold kept by the house (duel tax, auction fees and commissions). Only counted.
    pub house_gold: u64,
}

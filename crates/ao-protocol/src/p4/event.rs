//! Events: duels, tournaments, auctions, invasions, castles. OWNER: Opus D (docs/P4_CONTRATO.md). Wrapped in `C2S::Event` / `S2C::Event`.
//!
//! Ported from AO20 `ModRetos.bas` (duels), `ModSubasta.bas` + `Protocol.HandleOfertaInicial`
//! / `HandleOfertaDeSubasta` (auctions) and `ModTorneos.bas` (tournament sign-up).
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EventC2S {
    /// Asks for the whole event state (window opened).
    Refresh,
    /// `HandleDuel` / `CrearReto`: `players` in AO20 order (`Jugadores`): odd positions are
    /// the proposer's teammates, even positions the rivals. 1v1 = `[rival]`, 2v2 =
    /// `[rival1, mate, rival2]`. `max_potions` < 0 = no limit (`PocionesMaximas`).
    DuelPropose { players: Vec<String>, bet: u32, max_potions: i16 },
    /// `HandleAcceptDuel` / `AceptarReto`.
    DuelAccept { offerer: String },
    /// `HandleCancelDuel`: withdraws the own request or the own acceptance.
    DuelCancel,
    /// `HandleQuitDuel` / `AbandonarReto` (`/ABANDONAR`): concede.
    DuelQuit,
    /// Publishes `amount` of the item in inventory `slot` (`IniciarSubasta` +
    /// `HandleOfertaInicial`), next to an auctioneer.
    AuctionPublish { slot: u8, amount: u32, base_price: u32, minutes: u8 },
    /// `HandleOfertaDeSubasta` (`/OFERTAR`) on one lot.
    AuctionBid { lot: u32, amount: u32 },
    /// `ParticiparTorneo` (`/PARTICIPAR`): sign up, paying the fee.
    TournamentJoin,
    /// Leaves the sign-up list (fee refunded while it has not started).
    TournamentLeave,
    /// GM: `HandleCrearTorneo` / `IniciarTorneo`.
    TournamentOpen(TournamentSpec),
    /// GM: `HandleComenzarTorneo` / `ComenzarTorneoOk` (warps everyone signed up).
    TournamentStart,
    /// GM: `ResetearTorneo`.
    TournamentClose,
}

/// A tournament as a GM opens it (`HandleCrearTorneo`).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct TournamentSpec {
    pub name: String,
    pub rules: String,
    pub min_level: u8,
    pub max_level: u8,
    pub slots: u8,
    pub fee: u32,
    /// AO20 class numbers allowed (empty = all).
    pub classes: Vec<u8>,
    pub map: u16,
    pub x: u8,
    pub y: u8,
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EventS2C {
    /// Duels and tournament as this player sees them.
    State(Box<EventView>),
    /// Every open lot (sent on change to everyone, and on `Refresh`).
    Auctions { lots: Vec<AuctionLot> },
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct EventView {
    /// Requests where this player is invited and has not accepted yet.
    pub invites: Vec<DuelInvite>,
    /// The request this player sent or accepted (still waiting for the others).
    pub pending: Option<DuelInvite>,
    /// The duel this player is fighting.
    pub duel: Option<DuelView>,
    pub tournament: Option<TournamentView>,
    /// Duel rules (`Retos.dat`), for the window.
    pub min_bet: u32,
    pub max_team: u8,
    pub tax_percent: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct DuelInvite {
    pub offerer: String,
    pub team_a: Vec<String>,
    pub team_b: Vec<String>,
    pub bet: u32,
    pub max_potions: i16,
    /// Names still to accept.
    pub missing: Vec<String>,
    /// Waiting for a free room.
    pub queued: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct DuelView {
    pub team_a: Vec<String>,
    pub team_b: Vec<String>,
    /// 1-based round (`Ronda`).
    pub round: u8,
    /// Rounds won by each team.
    pub wins_a: u8,
    pub wins_b: u8,
    /// Which team this player is in (true = A).
    pub mine_a: bool,
    pub bet: u32,
    /// Seconds of the countdown left (0 = fighting).
    pub countdown: u16,
    /// Seconds before the duel ends as a draw (`DuracionMaxima`).
    pub time_left: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct TournamentView {
    pub spec: TournamentSpec,
    pub signed: Vec<String>,
    pub me_signed: bool,
    pub started: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AuctionLot {
    pub id: u32,
    pub seller: String,
    pub obj: u16,
    pub amount: u32,
    pub base_price: u32,
    /// Best bid so far (0 = none).
    pub best: u32,
    pub bidder: String,
    /// Minimum valid next bid.
    pub min_bid: u32,
    pub secs_left: u32,
}

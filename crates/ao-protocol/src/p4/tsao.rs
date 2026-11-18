//! Tierras Sagradas progression: tournament points, TS points and donation credits, the
//! exchange window (TSAO `frmCanjes`, `frmDonaciones`, `frmNewTiendaTS`), the class bonus
//! choice of levels 53/56/60 (message `99`) and the popular vote (`/VOTAR`). Wrapped in
//! `C2S::Tsao` / `S2C::Tsao`.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Points {
    pub tournament: u32,
    pub ts: u32,
    pub credits: u32,
}

/// A prize for tournament points (`Premios.dat`): price per unit and the stats it shows.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PrizeView {
    pub obj: u16,
    pub name: String,
    pub price: u32,
    /// `[hit, def, magic hit, magic def]`, each `(min, max)`.
    pub stats: [(i32, i32); 4],
    pub desc: String,
}

/// An item of the TS shop.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TsView {
    pub obj: u16,
    pub amount: u32,
    pub price: u32,
}

/// A donation pack: what it gives, as text lines, and whether it can be bought here.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PackView {
    pub name: String,
    pub desc: String,
    pub price: u32,
    /// Preview body for this character's race (0 = none).
    pub body: u16,
    pub contents: Vec<String>,
    pub available: bool,
}

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TsaoC2S {
    /// Opens the exchange window.
    OpenExchange,
    /// `amount` units of prize `index` (0-based, as sent in `Exchange`).
    BuyPrize { index: u16, amount: u32 },
    BuyTs { index: u16 },
    BuyPack { index: u16 },
    /// Option 0 or 1 of the pending class bonus `tier` (0-based).
    ChooseBonus { tier: u8, option: u8 },
    /// Option (0-based) of the open poll.
    Vote { option: u8 },
    /// Upgrade `index` (of `Mejorados.dat`, as sent in `Upgrades`) with the Octarine gem.
    Upgrade { index: u16 },
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TsaoS2C {
    /// The character's points (on entering and on every change).
    Points(Points),
    /// The exchange window.
    Exchange { prizes: Vec<PrizeView>, ts: Vec<TsView>, packs: Vec<PackView> },
    /// A class bonus to choose (pending until chosen, offered again on entering).
    BonusOffer { tier: u8, level: u32, options: [String; 2] },
    /// The bonuses chosen so far (texts, in tier order).
    Bonuses(Vec<String>),
    /// The poll window; `results` are the votes per option, sent once the player voted.
    Poll { question: String, options: Vec<String>, results: Option<Vec<u32>> },
    CloseExchange,
    /// The Octarine gem window: `(index, object you have, what it becomes)`; empty when
    /// nothing in the bag can be upgraded. Sent again (or empty) after an upgrade.
    Upgrades(Vec<(u16, u16, PrizeView)>),
}

//! Crafting: blacksmithing, carpentry, tailoring, alchemy. OWNER: Opus B (docs/P4_CONTRATO.md). Wrapped in `C2S::Craft` / `S2C::Craft`.
//!
//! AO20 sends one form per trade (`ShowBlacksmithForm` + `BlacksmithWeapons/Armors/
//! ExtraObjects`, `ShowCarpenterForm` + `CarpenterObjects`, ...) and one build request per
//! trade (`CraftBlacksmith`, `CraftCarpenter` with an amount, `CraftAlquimia`,
//! `CraftSastre`). Here it is one `Open` / `Recipes` pair and one `Build` with an amount
//! for every trade; the server runs the order one step per `IntervaloTrabajarConstruir`.
pub use ao_rules::craft::Trade;
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CraftC2S {
    /// Open a trade: the tool must be equipped (and, for the blacksmith, `station` is the
    /// anvil tile, within 2 tiles). Using the tool or clicking the anvil sends this.
    Open { trade: Trade, station: Option<(i32, i32)> },
    /// Build `amount` units of `obj` (an object of the list the server sent).
    Build { obj: u16, amount: u32 },
    /// Stop the running order.
    Stop,
    /// The window was closed.
    Close,
}

/// One object the player can build, with what it costs (per unit).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub obj: u16,
    /// Skill it needs in the trade.
    pub skill: u8,
    /// (object, units) per unit built.
    pub materials: Vec<(u16, u32)>,
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CraftS2C {
    /// The trade is open (window): what can be built now, the units built per step and the
    /// station tile (blacksmith).
    Open { trade: Trade, recipes: Vec<Recipe>, per_step: u32, station: Option<(i32, i32)> },
    /// Progress of the order: `done` of `total` units of `obj` (`done == total`: finished).
    Progress { obj: u16, done: u32, total: u32 },
    /// The order stopped before finishing (no materials, too tired, moved away...).
    Stopped { reason: String },
    /// One step built `amount` units (the "+N" over the head and the work sound).
    Crafted { obj: u16, amount: u32, sound: u16 },
    /// The trade is closed (walked away from the anvil, unequipped the tool...).
    Closed,
}

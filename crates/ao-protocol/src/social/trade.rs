//! Commerce with NPCs and between players. OWNER: Opus B (docs/P2_CONTRATO.md). Wrapped in `C2S::Trade` / `S2C::Trade`.
//!
//! NPC shop: opened by the server when the player double-clicks a merchant; then `Buy`,
//! `Sell` and `CloseShop`. Player trade: `Propose` (the target gets `Invited` and answers
//! with its own `Propose` or `Decline`), `Offer` / `OfferGold` / `Retract` build the offer,
//! then a double confirmation: both `Confirm` the current offer revision, then both
//! `Accept`. Any change of either offer bumps the revision and clears every confirmation.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TradeC2S {
    /// Buys `amount` units of the merchant's slot `slot` (1-based).
    Buy { slot: u8, amount: u32 },
    /// Sells `amount` units of the own inventory slot `slot` (1-based).
    Sell { slot: u8, amount: u32 },
    /// Closes the merchant window.
    CloseShop,
    /// Asks the player `target` (entity id) to trade, or accepts their invitation.
    Propose { target: u32 },
    /// Turns down an invitation.
    Decline,
    /// Adds `amount` units of the object in inventory slot `slot` to the own offer.
    Offer { slot: u8, amount: u32 },
    /// Sets the gold of the own offer (replaces the previous amount; 0 removes it).
    OfferGold { amount: u32 },
    /// Removes the entry `index` (0-based, as in `Offer.items`) from the own offer.
    Retract { index: u8 },
    /// First confirmation: "this offer revision is fine for me".
    Confirm { rev: u32 },
    /// Second confirmation: only once both confirmed the same revision.
    Accept { rev: u32 },
    /// Leaves the trade with a player.
    Cancel,
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TradeS2C {
    /// Merchant window open: (slot, obj, amount, unit price) for each stocked slot.
    ShopOpen { npc: u32, items: Vec<(u8, u16, u32, u32)> },
    /// One merchant slot changed (obj 0 = empty).
    ShopSlot { slot: u8, obj: u16, amount: u32, price: u32 },
    ShopClosed,
    /// `from` wants to trade; answer with `Propose { target: from }` or `Decline`.
    Invited { from: u32, name: String },
    /// Trade window open with `with`.
    Opened { with: u32, name: String },
    /// One side's offer (`mine` = own offer). `rev` changes with every change of either side.
    Offer { mine: bool, items: Vec<(u16, u32)>, gold: u32, rev: u32 },
    /// Confirmation state of both sides for revision `rev`.
    Status { rev: u32, i_confirmed: bool, they_confirmed: bool, i_accepted: bool, they_accepted: bool },
    /// Trade over: `done` = the exchange happened.
    Closed { done: bool },
}

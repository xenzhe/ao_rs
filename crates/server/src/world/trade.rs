//! Per-player trade state (`Player.trade`). OWNER: Opus B (docs/P2_CONTRATO.md).
//! Nothing here is persisted: offers never leave the inventory until the exchange, so a
//! disconnection in the middle has nothing to give back.
use super::inventory::InvItem;
use std::collections::HashMap;

/// One side of a trade between players (`ComUsu`): what it offers. Items stay in the
/// inventory; they are only checked and moved when both sides accept.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct OfferSide {
    /// `itemsAenviar`: (obj, amount), at most `MAX_OFFER_ITEMS` entries.
    pub items: Vec<InvItem>,
    /// `ComUsu.Oro`.
    pub gold: u32,
    pub confirmed: bool,
    pub accepted: bool,
}

#[derive(Debug, Default)]
pub struct TradePiece {
    /// Merchant NPC whose window is open (`flags.Comerciando` + `flags.TargetNPC`).
    pub shop: Option<u32>,
    /// Player trade partner once both agreed (`ComUsu.DestUsu`).
    pub with: Option<u32>,
    /// Invitation sent to this player, waiting for an answer.
    pub proposed_to: Option<u32>,
    /// Invitation received from this player (`flags.pregunta = 4`).
    pub invited_by: Option<u32>,
    pub offer: OfferSide,
}

/// A merchant's live stock (`NpcList().invent`): index 0 = slot 1, `MAX_INVENTORY` slots.
#[derive(Debug, Clone)]
pub struct ShopStock {
    pub slots: Vec<Option<InvItem>>,
}

/// World-wide trade state (`World.trades`).
#[derive(Debug, Default)]
pub struct Trades {
    /// Merchant stocks by NPC instance id, loaded from npcs.dat on first use.
    pub stocks: HashMap<u32, ShopStock>,
    /// Revision of each open player trade, keyed by the lower of both ids.
    pub revs: HashMap<u32, u32>,
}

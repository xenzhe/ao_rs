//! Per-player crafting state (`Player.craft`) and world-wide state (`World.crafts`).
//! OWNER: Opus B (docs/P4_CONTRATO.md).
//!
//! Runtime only: what is built goes to the inventory and the skills raised live in
//! `CharStats`, both persisted by their owners, so crafting has no table of its own.
use crate::clock::GameTime;
use ao_formats::dat::craft::{CraftLists, MapObject};
use ao_rules::craft::Trade;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

/// The trade the player has open (the AO20 form: `ShowBlacksmithForm`...).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenTrade {
    pub trade: Trade,
    /// Blacksmith: the anvil (map, x, y) and its `Subtipo` (2 = blodium anvil).
    pub station: Option<(u16, i32, i32, i32)>,
}

/// A build order running one step per `IntervaloTrabajarConstruir`
/// (`UserList.Trabajo.Cantidad` / `.Item` of the carpenter macro, for every trade here).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Job {
    pub obj: u16,
    pub total: u32,
    pub done: u32,
    pub next_at: GameTime,
}

#[derive(Debug, Default)]
pub struct CraftPiece {
    pub open: Option<OpenTrade>,
    pub job: Option<Job>,
    /// `Counters.TimerPuedeTrabajar`: per player, not per order, so stopping, reopening or
    /// sending new orders never builds faster than one step per interval.
    pub next_step_at: GameTime,
}

#[derive(Debug, Default)]
pub struct CraftWorld {
    /// Official trade lists, read on first use.
    pub lists: Option<Arc<CraftLists>>,
    /// Objects of each map's `.csm` (anvils), read on first use.
    pub map_objects: HashMap<u16, Arc<Vec<MapObject>>>,
    /// Players with a trade open (stable order, checked every tick).
    pub active: BTreeSet<u32>,
}

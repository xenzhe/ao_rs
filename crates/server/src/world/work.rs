//! Per-player gathering work state (`Player.work`) and world-wide state (`World.works`).
//! OWNER: Opus A (docs/P4_CONTRATO.md).
use crate::clock::GameTime;
use ao_protocol::p4::work::WorkKind;
use ao_rules::work::FishTable;
use std::collections::{BTreeSet, HashMap};

/// `t_AutomatedAction` plus what it was started with: the run stops when any of it changed
/// (the VB resets it on walking, equipping, dying...; here it is checked at every run).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveWork {
    pub kind: WorkKind,
    pub map: u16,
    /// Target tile.
    pub x: i32,
    pub y: i32,
    /// Where the player stood.
    pub from: (i32, i32),
    /// Equipped tool (slot, obj) for gathering; the ore stack (slot, obj) for smelting.
    pub item: (u8, u16),
}

#[derive(Debug, Default)]
pub struct WorkPiece {
    pub active: Option<ActiveWork>,
}

/// A map object as `MapData().ObjInfo` holds it: amount left and last use (`ObjInfo.data`,
/// None = `&H7FFFFFFF`, never used).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resource {
    pub obj: u16,
    pub amount: u32,
    pub last_use: Option<GameTime>,
}

#[derive(Debug, Default)]
pub struct WorkWorld {
    /// Map objects per map, read lazily from the `.csm` (trees and deposits start at
    /// `VidaUtil`, like `CargarMapaFormatoCSM`). Not saved: the VB rebuilds them at boot.
    pub resources: HashMap<u16, HashMap<(i32, i32), Resource>>,
    /// pesca.dat + RecursosEspeciales.dat, read on first use.
    pub fish: Option<FishTable>,
    /// `RunAutomatedActions`: one global run every `IntervalAutomatedAction`.
    pub next_run: GameTime,
    /// Players with an active work (stable order).
    pub workers: BTreeSet<u32>,
}

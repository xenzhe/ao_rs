//! Gathering work: mining, woodcutting, fishing, smelting. OWNER: Opus A (docs/P4_CONTRATO.md). Wrapped in `C2S::Work` / `S2C::Work`.
//!
//! `Start` is the official `StartAutomatedAction(x, y, skill)`; the server then repeats the
//! work every 4 s until something stops it (moving, changing the tool, no stamina, the
//! resource running out, `Stop`).
use serde::{Deserialize, Serialize};

/// What to work at a tile. The server checks the tool, the tile and the distance.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkKind {
    /// Axe on a tree.
    Chop,
    /// Pickaxe on an ore deposit.
    Mine,
    /// Rod or net on water.
    Fish,
    /// Ore of inventory `slot` in a forge (`flags.TargetObjInvSlot`).
    Smelt { slot: u8 },
}

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WorkC2S {
    Start { kind: WorkKind, x: i32, y: i32 },
    Stop,
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WorkS2C {
    /// The work at (x, y) is running.
    Started { kind: WorkKind, x: i32, y: i32 },
    /// Refused or ended (the reason, if any, went to the console).
    Stopped,
    /// One attempt gave `amount` of `obj` (`WriteTextCharDrop "+N"` and the particle).
    Gathered { obj: u16, amount: u32 },
    /// One attempt failed (failure sound / `GRH_FALLO_PESCA`).
    Missed,
}

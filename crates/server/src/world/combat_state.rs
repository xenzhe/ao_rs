//! Combat state of a player. OWNER: combat system (P1, Opus A). Runtime only (death is
//! persisted through hp = 0 by `db`). Add fields freely.
use crate::clock::GameTime;

#[derive(Clone, Debug, Default)]
pub struct CombatState {
    /// Ghost until resurrected.
    pub dead: bool,
    /// Attack interval (melee/ranged), like IntervaloUserPuedeAtacar.
    pub next_attack: GameTime,
}

//! Combat state of a player. OWNER: combat system (P1, Opus A). Runtime only (death is
//! persisted through hp = 0 by `db`). Add fields freely.
use crate::clock::GameTime;

#[derive(Clone, Debug, Default)]
pub struct CombatState {
    /// Ghost until resurrected (`flags.Muerto`).
    pub dead: bool,
    /// Last melee attack (`Counters.TimerPuedeAtacar`, also `TimerGolpeMagia` and
    /// `TimerGolpeUsar`): magic checks `IntervaloGolpeMagia` against it.
    pub last_attack: Option<GameTime>,
    /// Last bow/throwable shot (`Counters.TimerPuedeUsarArco`).
    pub last_shot: Option<GameTime>,
    /// Body and head before dying, restored by `resurrect` (`OrigChar`).
    pub alive_look: Option<(u16, u16)>,
}

//! Active skills of TSAO that are not spells or work: hide, steal, tame, dig up a treasure.
//! Wrapped in `C2S::Skill`. The server answers with console lines and status changes.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SkillC2S {
    /// `UK Ocultarse`.
    Hide,
    /// `UK Robar` + click: the player standing at (x, y).
    Steal { x: i32, y: i32 },
    /// `UK Domar` + click: the creature at (x, y).
    Tame { x: i32, y: i32 },
    /// `/DESENTERRAR` with a treasure map.
    Dig,
}

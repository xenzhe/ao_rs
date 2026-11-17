//! Known spells and status effects. OWNER: magic system (P1, Opus B).
//! `slots` is persisted by `db`; status effects are runtime only. Add fields freely.
use crate::clock::GameTime;

/// AO20 MAXUSERHECHIZOS.
pub const MAX_SPELL_SLOTS: usize = 40;

#[derive(Clone, Debug)]
pub struct Spellbook {
    /// Hechizos.dat ids, index 0 = slot 1, 0 = empty.
    pub slots: Vec<u16>,
}

impl Default for Spellbook {
    fn default() -> Self {
        Spellbook { slots: vec![0; MAX_SPELL_SLOTS] }
    }
}

impl Spellbook {
    pub fn knows(&self, spell: u16) -> bool {
        spell != 0 && self.slots.contains(&spell)
    }
}

/// Timed and toggled states (paralysis, invisibility, poison, meditation...).
#[derive(Clone, Debug, Default)]
pub struct StatusEffects {
    pub meditating: bool,
    pub paralyzed_until: Option<GameTime>,
    pub immobilized_until: Option<GameTime>,
    pub invisible_until: Option<GameTime>,
    pub poisoned_until: Option<GameTime>,
    pub blind_until: Option<GameTime>,
    pub dumb_until: Option<GameTime>,
}

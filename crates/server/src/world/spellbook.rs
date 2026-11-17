//! Known spells and status effects. OWNER: magic system (P1, Opus B).
//! `slots` is persisted by `db`; status effects are runtime only. Add fields freely.
use crate::clock::GameTime;
use std::collections::HashMap;

/// AO20 MAXUSERHECHIZOS.
pub const MAX_SPELL_SLOTS: usize = 40;

#[derive(Clone, Debug)]
pub struct Spellbook {
    /// Hechizos.dat ids, index 0 = slot 1, 0 = empty.
    pub slots: Vec<u16>,
    /// `Counters.TimerLanzarSpell` (`IntervaloPermiteLanzarSpell`): next cast allowed.
    pub next_cast: GameTime,
    /// `Counters.UserHechizosInterval` + `Hechizos.CoolDown`: per-spell cooldowns.
    pub cooldowns: HashMap<u16, GameTime>,
}

impl Default for Spellbook {
    fn default() -> Self {
        Spellbook { slots: vec![0; MAX_SPELL_SLOTS], next_cast: GameTime::ZERO, cooldowns: HashMap::new() }
    }
}

impl Spellbook {
    pub fn knows(&self, spell: u16) -> bool {
        spell != 0 && self.slots.contains(&spell)
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(|s| *s == 0)
    }

    /// `AgregarHechizo`: first free slot. None when the book is full.
    pub fn add(&mut self, spell: u16) -> Option<usize> {
        let i = self.slots.iter().position(|s| *s == 0)?;
        self.slots[i] = spell;
        Some(i)
    }
}

/// An `EffectsOverTime.dat` health/mana effect running on a player (`UpdateHpOverTime`,
/// `UpdateManaOverTime`).
#[derive(Clone, Debug)]
pub struct ActiveEot {
    pub eot: u16,
    /// Caster connection id (0: gone / none).
    pub source: u32,
    pub ticks_left: u32,
    pub next_tick: GameTime,
}

/// Timed and toggled states (paralysis, invisibility, poison, meditation...).
#[derive(Clone, Debug, Default)]
pub struct StatusEffects {
    pub meditating: bool,
    /// `DoMeditar`: next mana pulse (warm-up first).
    pub meditate_next: GameTime,
    /// FX shown while meditating (0 = none).
    pub meditate_fx: u16,
    /// Where meditation started and the attack timer then: moving or attacking stops it.
    pub meditate_anchor: (u16, i32, i32, GameTime),
    pub paralyzed_until: Option<GameTime>,
    pub immobilized_until: Option<GameTime>,
    pub invisible_until: Option<GameTime>,
    /// Poison never wears off by itself in AO20 (only `CuraVeneno`): `poisoned` is the
    /// flag, `poison_next` the next `EfectoVeneno` hit.
    pub poisoned: bool,
    pub poison_next: GameTime,
    /// Kept for the contract (unused: poison has no duration in AO20).
    pub poisoned_until: Option<GameTime>,
    pub blind_until: Option<GameTime>,
    pub dumb_until: Option<GameTime>,
    /// `Counters.TiempoDeInmunidadParalisisNoMagicas`.
    pub paralysis_immune_until: Option<GameTime>,
    pub eots: Vec<ActiveEot>,
    /// Last flags broadcast with `S2C::Status` (to send only changes).
    pub sent_flags: u32,
}

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

/// Who cast an effect over time (`IBaseEffectOverTime.Caster*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EotCaster {
    #[default]
    None,
    /// Connection id of a player.
    User(u32),
    /// A creature: map and NPC id.
    Npc(u16, u32),
}

impl EotCaster {
    /// Opaque key for `eSingleByCaster`.
    pub fn key(self) -> u64 {
        match self {
            EotCaster::None => 0,
            EotCaster::User(id) => 1 << 32 | id as u64,
            EotCaster::Npc(map, nid) => 2 << 48 | (map as u64) << 32 | nid as u64,
        }
    }
}

/// An `EffectsOverTime.dat` effect running on a player or a creature (one per
/// `IBaseEffectOverTime` instance; systems/eot.rs runs them).
#[derive(Clone, Debug, Default)]
pub struct ActiveEot {
    /// `UniqueId`.
    pub uid: u32,
    pub eot: u16,
    pub caster: EotCaster,
    /// Ticks still to run (health, mana, party songs, transforms).
    pub ticks_left: u32,
    pub next_tick: GameTime,
    /// End of the effects that last `TickTime` in total (modifiers, shields...).
    pub ends: Option<GameTime>,
    /// `ProtectEffect.DamageProtection` left.
    pub shield: i32,
    /// Hits left (`BonusDamageEffect.TickCount`, `ApplyEffectOnHit.HitLimit`,
    /// `MultipleAttacks.mTicks`); 0 on an on-hit effect = unlimited.
    pub hits_left: u32,
    /// `StatModifier.Modifier`: the caster's weapon `MagicDamageBonus` / 100.
    pub scale: f32,
    /// `MultipleAttacks`: extra blows still to throw and when the next one goes.
    pub pending_blows: u8,
    pub blow_at: GameTime,
}

/// A player turned into a creature by `TransformEffect` (Forma de Lobo / Oso...).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Transform {
    /// npcs.dat number whose look is worn (`flags.ActiveTransform`).
    pub npc: u16,
    /// Look before the change (saved with the character instead of the creature's).
    pub body: u16,
    pub head: u16,
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
    /// Active `TransformEffect` (systems/eot.rs).
    pub transform: Option<Transform>,
    /// TSAO `flags.Oculto` and when it is checked again (`DoPermanecerOculto`, 3 s).
    pub hidden: bool,
    pub hidden_check: GameTime,
    /// TSAO `flags.Navegando`: the boat object in use.
    pub sailing: Option<u16>,
    /// Speed multiplier of a spell effect (`flags.VelocidadHechizada`), None = none.
    pub speed_spell: Option<f32>,
    /// Next steal / hide attempt allowed (TSAO `PuedoTrabajar`).
    pub skill_ready: GameTime,
    /// Last flags broadcast with `S2C::Status` (to send only changes).
    pub sent_flags: u32,
    /// Map those flags were sent to.
    pub sent_map: u16,
}

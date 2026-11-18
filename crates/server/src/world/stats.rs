//! Attributes, skills and secondary stats. OWNER: progression system (P1, Sonnet 1).
//! Persisted by `db` (attributes, skills, skill_points, sta, food, water); add runtime-only
//! fields freely.

pub const NUM_ATTRIBUTES: usize = 5;
pub const NUM_SKILLS: usize = 24;

/// Official skill indexes (`e_Skill` minus one) used by the progression and regen rules.
pub const SKILL_MEDITATE: usize = 4;
pub const SKILL_SURVIVAL: usize = 7;

#[derive(Clone, Debug, PartialEq)]
pub struct CharStats {
    /// Strength, Agility, Intelligence, Constitution, Charisma (VB6 `e_Atributos` order;
    /// see `ao_rules::character::ATTR_*`).
    pub attributes: [u8; NUM_ATTRIBUTES],
    /// Official skill order (e_Skill, 1-based in VB; index 0 = skill 1).
    pub skills: [u8; NUM_SKILLS],
    pub skill_points: u16,
    pub sta: i32,
    pub max_sta: i32,
    /// Hunger/thirst 0..100 (MinHam/MinAGU).
    pub food: i32,
    pub water: i32,
    /// Runtime-only counters of the regeneration rules (not persisted).
    pub timers: RegenTimers,
    /// Attributes before a strength/agility potion (`UserAtributosBackUP`): restored when
    /// it wears off, and what is saved meanwhile.
    pub attr_backup: Option<[u8; NUM_ATTRIBUTES]>,
    /// When the potion bonus wears off (`flags.DuracionEfecto`, in seconds).
    pub potion_until: Option<crate::clock::GameTime>,
}

/// Counters of `General.bas` (`Counters.HPCounter`, `STACounter`, `AGUACounter`,
/// `COMCounter`, `EnCombate`). Health and stamina count 40 ms game ticks, hunger and thirst
/// count seconds, like the VB timers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegenTimers {
    pub hp: u32,
    pub sta: u32,
    pub thirst: u32,
    pub hunger: u32,
    /// Seconds of "in combat" left (`Counters.EnCombate`): no health regeneration meanwhile.
    pub combat_secs: u32,
    /// Resting (`flags.Descansar`): faster regeneration. There is no client message for it
    /// yet; whoever adds the rest command sets this.
    pub resting: bool,
}

impl Default for CharStats {
    fn default() -> Self {
        CharStats { attributes: [18; NUM_ATTRIBUTES], skills: [0; NUM_SKILLS], skill_points: 0, sta: 40, max_sta: 40, food: 100, water: 100, timers: RegenTimers::default(), attr_backup: None, potion_until: None }
    }
}

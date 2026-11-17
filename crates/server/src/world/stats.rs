//! Attributes, skills and secondary stats. OWNER: progression system (P1, Sonnet 1).
//! Persisted by `db` (attributes, skills, skill_points, sta, food, water); add runtime-only
//! fields freely.

pub const NUM_ATTRIBUTES: usize = 5;
pub const NUM_SKILLS: usize = 24;

#[derive(Clone, Debug, PartialEq)]
pub struct CharStats {
    /// Strength, Agility, Intelligence, Charisma, Constitution (e_Atributos order).
    pub attributes: [u8; NUM_ATTRIBUTES],
    /// Official skill order (e_Skill, 1-based in VB; index 0 = skill 1).
    pub skills: [u8; NUM_SKILLS],
    pub skill_points: u16,
    pub sta: i32,
    pub max_sta: i32,
    /// Hunger/thirst 0..100 (MinHam/MinAGU).
    pub food: i32,
    pub water: i32,
}

impl Default for CharStats {
    fn default() -> Self {
        CharStats { attributes: [18; NUM_ATTRIBUTES], skills: [0; NUM_SKILLS], skill_points: 0, sta: 40, max_sta: 40, food: 100, water: 100 }
    }
}

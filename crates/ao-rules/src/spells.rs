//! Spell definitions. Loaded by the server from `data/spells.toml` and sent to clients in
//! the spell book, so adding or tuning a spell needs no recompilation.
use serde::{Deserialize, Serialize};

/// Visual effect the client plays (the server never cares how it looks).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SpellEffect {
    Fireball,
    Lightning,
    Heal,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SpellTarget {
    /// Damages creatures.
    Enemy,
    /// Heals characters (self included).
    Ally,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SpellDef {
    pub id: u16,
    pub name: String,
    pub mana: i32,
    pub range: i32,
    pub cooldown_ms: u64,
    /// Base damage or healing; the caster level adds `2 * level`.
    pub power: i32,
    pub target: SpellTarget,
    pub effect: SpellEffect,
}

impl SpellDef {
    /// Damage or healing before the random ±15% variation.
    pub fn base_amount(&self, level: u32) -> i32 {
        self.power + level as i32 * 2
    }
}

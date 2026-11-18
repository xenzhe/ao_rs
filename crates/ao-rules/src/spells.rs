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
    /// A quick bright magic missile (Dardo / Destello / Flecha mágica).
    Dart,
    /// An arrow of crackling energy (Flecha eléctrica).
    ElectricDart,
    /// Fire raining on the target (Tormenta de fuego, Inferno, Apocalipsis).
    FireStorm,
    /// A whirlwind (Viento huracanado, Tornado).
    Wind,
    /// Sparks and arcs around the target (Descarga / Tormenta eléctrica).
    Electric,
    /// Frost and chains: paralysis, immobilisation, ice prison.
    Paralyze,
    /// Golden light from above (Fuego divino, Furia divina, Honor sagrado).
    Holy,
    /// Dark smoke (Insurrección sombría, Calavera, ceguera, estupidez).
    Dark,
    /// Soft white motes: cures and removals.
    Cure,
    /// Rising sparkles: strength, agility, speed buffs.
    Buff,
    /// The target fades in a shimmer.
    Invisibility,
    /// A pillar of light on a ghost.
    Resurrect,
    /// A portal circle where creatures appear.
    Summon,
    /// A flash and a swirl (teleports).
    Teleport,
}

impl SpellEffect {
    /// For the interface: what kind of spell it is.
    pub fn family(self) -> SpellFamily {
        use SpellEffect::*;
        match self {
            Heal | Cure | Resurrect => SpellFamily::Healing,
            Buff | Invisibility | Summon | Teleport => SpellFamily::Support,
            Paralyze | Dark => SpellFamily::Control,
            Fireball | Lightning | Dart | ElectricDart | FireStorm | Wind | Electric | Holy => SpellFamily::Damage,
        }
    }

    /// Name used in data files (`init/profile.ini [SpellFx] 2=dart`).
    pub fn parse(s: &str) -> Option<SpellEffect> {
        use SpellEffect::*;
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "fireball" => Fireball,
            "lightning" => Lightning,
            "heal" => Heal,
            "dart" => Dart,
            "electric_dart" => ElectricDart,
            "fire_storm" => FireStorm,
            "wind" => Wind,
            "electric" => Electric,
            "paralyze" => Paralyze,
            "holy" => Holy,
            "dark" => Dark,
            "cure" => Cure,
            "buff" => Buff,
            "invisibility" => Invisibility,
            "resurrect" => Resurrect,
            "summon" => Summon,
            "teleport" => Teleport,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpellFamily {
    Damage,
    Healing,
    Control,
    Support,
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

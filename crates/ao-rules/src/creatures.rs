//! Creature definitions, loaded by the server from `data/creatures.toml`.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CreatureDef {
    pub id: u16,
    pub name: String,
    /// Official AO body and head (cuerpos.dat / cabezas.ini; npcs.dat for reference).
    pub body: u16,
    #[serde(default)]
    pub head: u16,
    pub hp: i32,
    /// Melee damage range [min, max].
    pub damage: [i32; 2],
    pub attack_ms: u64,
    pub step_ms: u64,
    /// Tiles at which it notices a player in line of sight.
    pub aggro_range: i32,
    /// Max tiles it wanders from its spawn point when idle.
    #[serde(default = "default_wander")]
    pub wander_range: i32,
    pub exp: u32,
    pub respawn_secs: u64,
    /// Relative chance to be picked when populating a map.
    #[serde(default = "default_weight")]
    pub spawn_weight: u32,
}

fn default_wander() -> i32 {
    5
}

fn default_weight() -> u32 {
    1
}

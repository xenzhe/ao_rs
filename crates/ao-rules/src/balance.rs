//! Global balance values, loaded by the server from `data/balance.toml`.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Balance {
    /// Every skill of a new character starts here (AO20 0; TSAO 100).
    #[serde(default)]
    pub start_skills: u8,
    /// Highest newbie level (AO20 12, TSAO `LimiteNewbie` 9): newbie items, newbie priest,
    /// kills that do not count.
    #[serde(default = "default_newbie")]
    pub newbie_max_level: u32,
    /// `/regresar` destination (TSAO: Tanaris 54,36). None = the command is off.
    #[serde(default)]
    pub home: Option<(u16, i32, i32)>,
    /// Tiles around the start point that hostile creatures never step into (0 = off).
    /// TSAO starts new characters in the newbie dungeon with 400-life giant ants 11 tiles
    /// away, which killed level-1 characters on arrival.
    #[serde(default)]
    pub start_sanctuary: i32,
    /// Lowest level allowed to sail (TSAO 30).
    #[serde(default)]
    pub boat_min_level: u32,
    /// Jail and where released prisoners go (AO20 Ciudades.dat Prision/Libertad: 66,50,50 /
    /// 66,48,73; TSAO General.bas: 78,50,50 / 28,50,50).
    #[serde(default = "default_jail")]
    pub jail: (u16, i32, i32),
    #[serde(default = "default_freedom")]
    pub freedom: (u16, i32, i32),
    pub start_map: u16,
    pub start_x: i32,
    pub start_y: i32,
    pub start_hp: i32,
    pub start_mana: i32,
    /// Regeneration per second.
    pub hp_regen: i32,
    pub mana_regen: i32,
    /// Experience for level L is `exp_factor * L^2`.
    pub exp_factor: u32,
    pub level_hp: i32,
    pub level_mana: i32,
    pub creatures_per_map: usize,
    /// Human bodies and heads given to new characters.
    pub player_bodies: Vec<u16>,
    pub player_heads: Vec<u16>,
}

fn default_jail() -> (u16, i32, i32) {
    (66, 50, 50)
}

fn default_freedom() -> (u16, i32, i32) {
    (66, 48, 73)
}

fn default_newbie() -> u32 {
    12
}

impl Default for Balance {
    fn default() -> Self {
        Balance {
            start_skills: 0,
            newbie_max_level: 12,
            home: None,
            boat_min_level: 0,
            start_sanctuary: 0,
            jail: default_jail(),
            freedom: default_freedom(),
            start_map: 1,
            start_x: 50,
            start_y: 50,
            start_hp: 100,
            start_mana: 120,
            hp_regen: 1,
            mana_regen: 4,
            exp_factor: 40,
            level_hp: 12,
            level_mana: 10,
            creatures_per_map: 14,
            player_bodies: vec![1, 2, 3, 4],
            player_heads: vec![1, 2, 3, 4, 5, 6, 7, 8],
        }
    }
}

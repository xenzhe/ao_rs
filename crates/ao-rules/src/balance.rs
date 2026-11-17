//! Global balance values, loaded by the server from `data/balance.toml`.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Balance {
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

impl Default for Balance {
    fn default() -> Self {
        Balance {
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

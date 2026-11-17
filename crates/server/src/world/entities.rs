//! Players and creatures as the server sees them.
use super::bank::BankState;
use super::combat_state::CombatState;
use super::inventory::Inventory;
use super::spellbook::{Spellbook, StatusEffects};
use super::stats::CharStats;
use crate::security::{RateLimiter, SeqGuard, Strikes, WalkLimiter};
use ao_protocol::{EntityKind, S2C};
use ao_rules::Dir;
use crate::clock::GameTime;
use std::collections::HashMap;

pub type EntityId = u32;

pub struct Player {
    /// Database id when the character belongs to an account (None for dev logins).
    pub char_id: Option<i64>,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub name: String,
    pub map: u16,
    pub x: i32,
    pub y: i32,
    pub dir: Dir,
    pub body: u16,
    pub head: u16,
    pub hp: i32,
    pub max_hp: i32,
    pub mana: i32,
    pub max_mana: i32,
    pub level: u32,
    pub exp: u32,
    pub walk: WalkLimiter,
    pub seq: SeqGuard,
    pub cooldowns: HashMap<u16, GameTime>,
    pub strikes: Strikes,
    pub chat: RateLimiter,
    /// One piece per system, each owned by its system's file (see docs/P1_CONTRATO.md).
    pub stats: CharStats,
    pub inv: Inventory,
    pub bank: BankState,
    pub spells: Spellbook,
    pub status: StatusEffects,
    pub combat: CombatState,
}

impl Player {
    pub fn spawn_msg(&self, id: EntityId) -> S2C {
        S2C::Spawn { id, name: self.name.clone(), kind: EntityKind::Player, body: self.body, head: self.head, x: self.x, y: self.y, dir: self.dir, hp: self.hp, max_hp: self.max_hp }
    }
}

pub struct Npc {
    /// Creature definition id (data/creatures.toml).
    pub def: u16,
    pub name: String,
    pub body: u16,
    pub head: u16,
    pub x: i32,
    pub y: i32,
    pub dir: Dir,
    pub hp: i32,
    pub max_hp: i32,
    pub home: (i32, i32),
    pub next_move: GameTime,
    pub next_attack: GameTime,
    pub dead_until: Option<GameTime>,
}

impl Npc {
    pub fn alive(&self) -> bool {
        self.dead_until.is_none()
    }

    pub fn spawn_msg(&self, id: EntityId) -> S2C {
        S2C::Spawn { id, name: self.name.clone(), kind: EntityKind::Creature, body: self.body, head: self.head, x: self.x, y: self.y, dir: self.dir, hp: self.hp, max_hp: self.max_hp }
    }
}

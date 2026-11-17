//! Players and creatures as the server sees them.
use crate::security::{RateLimiter, SeqGuard, Strikes, WalkLimiter};
use ao_protocol::{EntityKind, S2C};
use ao_rules::Dir;
use std::collections::HashMap;
use std::time::Instant;

pub type EntityId = u32;

pub struct Player {
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
    pub cooldowns: HashMap<u16, Instant>,
    pub strikes: Strikes,
    pub chat: RateLimiter,
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
    pub next_move: Instant,
    pub next_attack: Instant,
    pub dead_until: Option<Instant>,
}

impl Npc {
    pub fn alive(&self) -> bool {
        self.dead_until.is_none()
    }

    pub fn spawn_msg(&self, id: EntityId) -> S2C {
        S2C::Spawn { id, name: self.name.clone(), kind: EntityKind::Creature, body: self.body, head: self.head, x: self.x, y: self.y, dir: self.dir, hp: self.hp, max_hp: self.max_hp }
    }
}

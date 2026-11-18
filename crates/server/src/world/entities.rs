//! Players and creatures as the server sees them.
use super::bank::BankState;
use super::combat_state::CombatState;
use super::inventory::Inventory;
use super::spellbook::{Spellbook, StatusEffects};
use super::stats::CharStats;
use crate::security::{RateLimiter, SeqGuard, Strikes, WalkLimiter};
use ao_formats::dat::npcs::{Movement, NpcType};
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
    /// P2 pieces (docs/P2_CONTRATO.md); each system loads/saves its own in `*_on_enter/leave`.
    pub trade: super::trade::TradePiece,
    pub guild: super::guild::GuildPiece,
    pub party: super::party::PartyPiece,
    pub faction: super::faction::FactionPiece,
    pub quest: super::quest::QuestPiece,
    /// P4 pieces (docs/P4_CONTRATO.md).
    pub work: super::work::WorkPiece,
    pub craft: super::craft::CraftPiece,
    pub gm: super::gm::GmPiece,
    pub event: super::event::EventPiece,
    /// TSAO progression (systems/tsao_progress.rs).
    pub tsao: crate::systems::tsao_progress::TsaoPiece,
}

impl Player {
    pub fn spawn_msg(&self, id: EntityId) -> S2C {
        S2C::Spawn { id, name: self.name.clone(), kind: EntityKind::Player, body: self.body, head: self.head, x: self.x, y: self.y, dir: self.dir, hp: self.hp, max_hp: self.max_hp }
    }
}

/// A live NPC of a map (`t_Npc`). Static data (intervals, spells, drops, walk steps,
/// description...) stays in `World::ao.npcs[def]`; this holds the mutable state.
/// OWNER: NPC system (P1, Opus D).
pub struct Npc {
    /// Official npcs.dat number (`Numero`), key of `World::ao.npcs`.
    pub def: u16,
    pub name: String,
    pub body: u16,
    pub head: u16,
    pub x: i32,
    pub y: i32,
    pub dir: Dir,
    pub hp: i32,
    pub max_hp: i32,
    /// `Orig`: where it was created (return point, respawn point with `OrigPos`).
    pub home: (i32, i32),
    /// `Contadores.IntervaloMovimiento`: next time its AI runs.
    pub next_move: GameTime,
    /// `Contadores.IntervaloAtaque`: next melee attack allowed.
    pub next_attack: GameTime,
    /// Set by whoever kills it (combat). The NPC system then schedules the respawn.
    pub dead_until: Option<GameTime>,
    /// `npcType`.
    pub kind: NpcType,
    /// `Movement` (current; may change while it defends itself).
    pub movement: Movement,
    /// `Hostile` (current).
    pub hostile: bool,
    /// `Attackable`: combat and magic must not damage NPCs with this off.
    pub attackable: bool,
    /// `flags.OldMovement` / `flags.OldHostil`: restored by `RestoreOldMovement`.
    pub old_movement: Movement,
    pub old_hostile: bool,
    /// `TargetUser`.
    pub target: Option<EntityId>,
    /// `flags.AttackedBy`: the player who hit it (set by combat/magic through
    /// `npc_attacked_by`).
    pub attacked_by: Option<EntityId>,
    /// `pathFindingInfo.RangoVision` / `OriginalVision`.
    pub vision: i32,
    pub original_vision: i32,
    /// Cached path (next step first) and the destination it was computed for.
    pub path: Vec<(i32, i32)>,
    pub path_dest: (i32, i32),
    /// `pathFindingInfo.NextPathRecomputeAt`.
    pub next_path_at: GameTime,
    /// `Contadores.IntervaloLanzarHechizo`: next spell allowed.
    pub next_spell: GameTime,
    /// `CaminataActual`: index into the npcs.dat walk steps.
    pub walk_step: usize,
    /// `Contadores.IntervaloRespawn` in seconds, rolled at creation.
    pub respawn_secs: u32,
    /// When the NPC system will bring it back (None while alive or not yet seen dead).
    pub respawn_at: Option<GameTime>,
}

impl Npc {
    pub fn alive(&self) -> bool {
        self.dead_until.is_none()
    }

    pub fn spawn_msg(&self, id: EntityId) -> S2C {
        S2C::Spawn { id, name: self.name.clone(), kind: EntityKind::Creature, body: self.body, head: self.head, x: self.x, y: self.y, dir: self.dir, hp: self.hp, max_hp: self.max_hp }
    }
}

//! Message definitions, grouped by concern. The client never sends positions, stats or
//! results: only intentions that the server validates.
use ao_rules::spells::SpellDef;
use ao_rules::Dir;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntityKind {
    Player,
    Creature,
}

/// One character of an account, as shown on the character selection screen.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CharacterSummary {
    pub name: String,
    pub level: u32,
    /// Ids from Balance.dat (races 1-5, classes 1-12 in AO20).
    pub race: u8,
    pub class: u8,
    /// 1 = male, 2 = female.
    pub gender: u8,
    pub body: u16,
    pub head: u16,
    pub map: u16,
}

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum C2S {
    // Accounts (one account, several characters)
    CreateAccount { user: String, password: String, version: u16 },
    AccountLogin { user: String, password: String, version: u16 },
    CreateCharacter { name: String, race: u8, class: u8, gender: u8, head: u16 },
    DeleteCharacter { name: String },
    PlayCharacter { name: String },

    // Session
    /// Development shortcut: plays a throwaway character without an account. Disabled
    /// in production config once accounts are in place.
    Login { name: String, version: u16 },
    // Movement
    Walk { dir: Dir, seq: u32 },
    /// Face a direction without moving.
    Turn { dir: Dir },

    // Combat
    /// Melee attack on the tile in front (current heading).
    Melee { seq: u32 },
    /// Ranged attack (bow, throwing weapon) at a tile.
    Shoot { tx: i32, ty: i32, seq: u32 },

    // Magic
    /// `spell` is the official Hechizos.dat id; the caster must know it.
    Cast { spell: u16, tx: i32, ty: i32, seq: u32 },
    /// Toggle meditation (mana regeneration, AO style).
    Meditate,

    // Inventory (slots are 1-based like AO)
    UseItem { slot: u8 },
    EquipItem { slot: u8 },
    DropItem { slot: u8, amount: u32 },
    DropGold { amount: u32 },
    /// Pick up whatever lies on the player's tile.
    PickUp,
    SwapSlots { a: u8, b: u8 },

    // World interaction
    /// Click/double-click on a tile: talk to an NPC (banker, priest, merchant...), etc.
    Interact { tx: i32, ty: i32 },

    // Bank (vault), only while a banker is open
    BankDeposit { slot: u8, amount: u32 },
    BankWithdraw { slot: u8, amount: u32 },
    BankDepositGold { amount: u32 },
    BankWithdrawGold { amount: u32 },
    BankClose,

    // Progression
    /// Spend free skill points.
    AssignSkill { skill: u8, points: u16 },

    // Chat
    Say { text: String },

    // P2 (docs/P2_CONTRATO.md): each system owns its enum in `social.rs`.
    Trade(crate::social::TradeC2S),
    Guild(crate::social::GuildC2S),
    Party(crate::social::PartyC2S),
    Faction(crate::social::FactionC2S),
    Quest(crate::social::QuestC2S),
    Channel(crate::social::ChannelC2S),
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum S2C {
    // Accounts
    /// Result of CreateAccount / AccountLogin / CreateCharacter / DeleteCharacter.
    AccountResult { ok: bool, message: String },
    /// Sent after a successful login and after creating or deleting a character.
    CharacterList { characters: Vec<CharacterSummary> },

    // Session
    Welcome { id: u32 },
    SpellBook { spells: Vec<SpellDef> },
    Kicked { reason: String },

    // World
    /// Load this map and place yourself here (login and every map transition).
    EnterMap { map: u16, x: i32, y: i32 },
    Spawn { id: u32, name: String, kind: EntityKind, body: u16, head: u16, x: i32, y: i32, dir: Dir, hp: i32, max_hp: i32 },
    Despawn { id: u32 },
    Moved { id: u32, x: i32, y: i32, dir: Dir },
    /// Walk rejected: authoritative position for the client to snap back to.
    Correct { seq: u32, x: i32, y: i32 },

    /// Visible equipment changed (or death/resurrection: ghost body). 0 = none.
    Look { id: u32, body: u16, head: u16, weapon: u16, shield: u16, helmet: u16 },
    /// Status flags of a character (see `status` bits), for visuals.
    Status { id: u32, flags: u32 },

    // Character
    Stats { hp: i32, max_hp: i32, mana: i32, max_mana: i32, level: u32, exp: u32, next_exp: u32 },
    /// Secondary stats.
    Vitals { sta: i32, max_sta: i32, food: i32, water: i32, gold: u32 },
    Attributes { values: [u8; 5] },
    /// 24 skills (official order) and points left to assign.
    Skills { values: Vec<u8>, free_points: u16 },
    Hp { id: u32, hp: i32, max_hp: i32 },

    // Combat
    /// A melee/ranged attack happened (animation, sound, floating number).
    Attack { attacker: u32, target: u32, hit: bool, damage: i32, critical: bool },

    // Magic
    Effect { spell: u16, from: u32, tx: i32, ty: i32 },
    /// The 40 spell slots (official Hechizos.dat ids, 0 = empty).
    SpellSlots { slots: Vec<u16> },
    /// Plays an official FX (FXs.ini) on an entity.
    Fx { id: u32, fx: u16, loops: u8 },

    // Inventory (slots 1-based; obj 0 = empty slot)
    InventorySlot { slot: u8, obj: u16, amount: u32, equipped: bool },
    GroundItem { x: i32, y: i32, obj: u16, amount: u32 },
    GroundItemRemoved { x: i32, y: i32 },

    // Bank
    BankOpen { slots: Vec<(u8, u16, u32)>, gold: u32 },
    BankSlot { slot: u8, obj: u16, amount: u32 },
    BankGold { gold: u32 },
    BankClosed,

    // NPCs
    /// An NPC says something (overhead and console).
    NpcSay { id: u32, text: String },

    // Chat / console
    Console { text: String, color: Tone },

    // P2 (docs/P2_CONTRATO.md)
    Trade(crate::social::TradeS2C),
    Guild(crate::social::GuildS2C),
    Party(crate::social::PartyS2C),
    Faction(crate::social::FactionS2C),
    Quest(crate::social::QuestS2C),
    Channel(crate::social::ChannelS2C),
}

/// Bits of `S2C::Status::flags`.
pub mod status {
    pub const DEAD: u32 = 1 << 0;
    pub const PARALYZED: u32 = 1 << 1;
    pub const INVISIBLE: u32 = 1 << 2;
    pub const POISONED: u32 = 1 << 3;
    pub const MEDITATING: u32 = 1 << 4;
    pub const HIDDEN: u32 = 1 << 5;
    pub const IMMOBILIZED: u32 = 1 << 6;
    pub const BLIND: u32 = 1 << 7;
    pub const DUMB: u32 = 1 << 8;
}

/// Console message category; the client picks the colour.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Info,
    Combat,
    Warn,
    Chat,
    Good,
}

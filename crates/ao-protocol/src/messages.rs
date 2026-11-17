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
    // Magic
    Cast { spell: u16, tx: i32, ty: i32, seq: u32 },
    // Chat
    Say { text: String },
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

    // Character
    Stats { hp: i32, max_hp: i32, mana: i32, max_mana: i32, level: u32, exp: u32, next_exp: u32 },
    Hp { id: u32, hp: i32, max_hp: i32 },

    // Magic
    Effect { spell: u16, from: u32, tx: i32, ty: i32 },

    // Chat / console
    Console { text: String, color: Tone },
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

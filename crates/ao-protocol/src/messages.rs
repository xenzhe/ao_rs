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

    // P4 (docs/P4_CONTRATO.md)
    Work(crate::p4::WorkC2S),
    Craft(crate::p4::CraftC2S),
    Gm(crate::p4::GmC2S),
    Event(crate::p4::EventC2S),
    Travel(crate::p4::TravelC2S),
    Mail(crate::p4::MailC2S),
    Surgeon(crate::p4::SurgeonC2S),
    Skill(crate::p4::SkillC2S),
    Tsao(crate::p4::TsaoC2S),

    // Phase 7
    /// `WorkLeftClick` with `TargetableItem`: an `otUsableOntarget` object (trap, harpoon,
    /// hand cannon) used on a map tile.
    UseItemAt { slot: u8, tx: i32, ty: i32 },
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
    /// A fixed map object (door) now is `obj` instead of what the map file says; the
    /// client redraws it and applies its blocks (`ao_rules::doors`).
    MapObject { x: i32, y: i32, obj: u16 },
    /// A sound (`PlayWave`) at a tile of the map: instruments, horns, filling a bottle...
    Sound { wav: u16, x: i32, y: i32 },
    /// A character's walking speed changed (`SpeedingACT`): steps last `WALK_MS / speed`.
    Speed { id: u32, speed: f32 },

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

    /// World weather (ModClimas): clouds/fog intensity (0 = clear) and whether it rains
    /// (snows on snowy maps). Sent to everyone on change and to each player on entering.
    Weather { fog: u8, precipitation: bool },
    /// World clock (`WorldTime_PrepareHora`): ms into the current day and the day length;
    /// the client keeps counting from here, so everyone sees the same hour.
    WorldTime { elapsed_ms: u64, day_ms: u64 },
    /// Thunder flash and sound when the precipitation starts.
    Thunder,
    /// A GM forced a music track (`PlayMidi`, `/FORCEMIDI`): official OGG/MP3 number; 0 =
    /// back to the map's own music.
    Music { midi: u16 },

    // P2 (docs/P2_CONTRATO.md)
    Trade(crate::social::TradeS2C),
    Guild(crate::social::GuildS2C),
    Party(crate::social::PartyS2C),
    Faction(crate::social::FactionS2C),
    Quest(crate::social::QuestS2C),
    Channel(crate::social::ChannelS2C),

    // P4 (docs/P4_CONTRATO.md)
    Work(crate::p4::WorkS2C),
    Craft(crate::p4::CraftS2C),
    Gm(crate::p4::GmS2C),
    Event(crate::p4::EventS2C),
    Travel(crate::p4::TravelS2C),
    Mail(crate::p4::MailS2C),
    Surgeon(crate::p4::SurgeonS2C),
    Tsao(crate::p4::TsaoS2C),

    // Phase 7
    /// `WriteSendSkillCdUpdate`: an active buff, debuff or cooldown of the player.
    /// `icon` is the `ClientEffectTypeId` (row of `init/Effects.ini`); `uid` tells two of
    /// the same icon apart (negative = a spell cooldown, `-spell`). `remaining_ms` 0
    /// removes it. `kind`: see [`buff_kind`].
    Buff { icon: u16, uid: i32, remaining_ms: u32, total_ms: u32, kind: u8, stacks: u16 },
}

/// `e_EffectType` of `S2C::Buff::kind`.
pub mod buff_kind {
    pub const BUFF: u8 = 1;
    pub const DEBUFF: u8 = 2;
    pub const COOLDOWN: u8 = 3;
    pub const INFO_BUFF: u8 = 4;
    pub const INFO_DEBUFF: u8 = 5;
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
    /// On a boat: moves on water only.
    pub const SAILING: u32 = 1 << 9;
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

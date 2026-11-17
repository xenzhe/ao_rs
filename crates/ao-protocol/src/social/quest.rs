//! Quests (misiones). OWNER: Sonnet 1 (docs/P2_CONTRATO.md). Wrapped in `C2S::Quest` / `S2C::Quest`.
//!
//! Mirrors the packets of the official protocol (`QuestListRequest`, `QuestDetailsRequest`,
//! `QuestAccept`, `QuestAbandon`; `NpcQuestListSend`, `QuestListSend`, `QuestDetails`,
//! `UpdateNPCSimbolo`) with the texts included, so the client does not need `Quests.DAT`.
//! Talking to a quest NPC is the ordinary `C2S::Interact`: the server answers with `Offer`.
use serde::{Deserialize, Serialize};

/// Client -> server. The client only asks; every rule is checked by the server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum QuestC2S {
    /// `QuestListRequest`: my quests in progress.
    List,
    /// `QuestDetailsRequest`: one of my quests in progress (`slot` 1..=5).
    Details { slot: u8 },
    /// `QuestAccept`: take quest `quest` from the NPC I last talked to (it must be one of
    /// the quests that NPC offered in its `Offer`).
    Accept { quest: u16 },
    /// `QuestAbandon`: drop the quest in `slot` (1..=5), losing its progress.
    Abandon { slot: u8 },
}

/// The fixed facts of a quest (`WriteNpcQuestListSend` / `WriteQuestDetails`). Rewards already
/// include the server's `ExpMult` / `GoldMult`, like the official packets.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct QuestInfo {
    pub id: u16,
    pub name: String,
    pub desc: String,
    /// What the NPC says when the quest is handed in.
    pub desc_final: String,
    pub required_level: u8,
    /// 0 = no limit.
    pub limit_level: u8,
    /// Quest that has to be done first (0 = none).
    pub required_quest: u16,
    pub classes: Vec<u8>,
    /// (npc number, amount): creatures to kill.
    pub kill: Vec<(u16, u16)>,
    /// (obj number, amount): objects to hand in.
    pub objs: Vec<(u16, u16)>,
    /// Spells the player must know (taken away when handing in).
    pub spells: Vec<u16>,
    /// (skill 1..=24, minimum value); skill 0 = none.
    pub skill: (u8, u8),
    pub reward_gold: u32,
    pub reward_exp: u32,
    /// (obj number, amount).
    pub reward_objs: Vec<(u16, u16)>,
    pub reward_spells: Vec<u16>,
    /// NPC number the quest is handed in to (0 = the one that offered it) and its map.
    pub talk_to: u16,
    pub pos_map: u16,
}

/// A quest in an NPC's list, with how it looks to this player.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OfferedQuest {
    pub info: QuestInfo,
    /// 0 available, 1 in progress, 2 already done, 3 requirements not met.
    pub state: u8,
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum QuestS2C {
    /// `NpcQuestListSend`: the quests offered by the NPC `npc` (entity id). Pick one with
    /// `QuestC2S::Accept`.
    Offer { npc: u32, quests: Vec<OfferedQuest> },
    /// `QuestListSend`: my quests in progress, in slot order (slot 1 first).
    List { active: Vec<u16> },
    /// `QuestDetails` of a quest in progress: its facts plus my progress.
    Details {
        slot: u8,
        info: Box<QuestInfo>,
        /// Kills so far per `info.kill` goal.
        killed: Vec<u16>,
        /// Objects held per `info.objs` entry.
        have: Vec<u16>,
        /// Whether each `info.spells` entry is known.
        known: Vec<bool>,
        /// NPCs already visited per target goal.
        targeted: Vec<u16>,
    },
    /// A kill counted (`NOCONSOLA*n/m name`): goal `index` of the quest in `slot`.
    Progress { slot: u8, index: u8, killed: u16, required: u16 },
    /// Every quest I have completed (sent when entering the world).
    Done { quests: Vec<u16> },
    /// A quest was handed in and its rewards given (`QUESTFIN*`).
    Completed { quest: u16 },
    /// `UpdateNPCSimbolo`: symbol over the NPC `npc` for me: 1 has a quest for me, 2 nothing,
    /// 3 a quest of mine is ready to hand in, 4 a quest of mine is in progress.
    Symbol { npc: u32, symbol: u8 },
}

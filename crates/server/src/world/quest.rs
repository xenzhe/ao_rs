//! Per-player quest state (`Player.quest`). OWNER: Sonnet 1 (docs/P2_CONTRATO.md).
//!
//! `t_QuestStats` (Declares.bas): up to `MAXUSERQUESTS` quests in progress, each with its
//! kill counters (`NPCsKilled`) and visit counters (`NPCsTarget`), plus the list of quests
//! done (`QuestsDone`). Also the quest catalog (`QuestList` + the quests every NPC offers),
//! parsed once per process like `AoData`.
use ao_formats::dat::quests::{load, load_npc_offers, NpcOffers, Quest, QuestTable};
use ao_rules::quest::MAX_ACTIVE;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tracing::warn;

/// One quest in progress (`t_UserQuest`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActiveQuest {
    /// Quests.DAT id.
    pub quest: u16,
    /// `NPCsKilled(i)` per `RequiredNPC(i)`.
    pub killed: Vec<u16>,
    /// `NPCsTarget(i)` per `RequiredTargetNPC(i)`.
    pub targeted: Vec<u16>,
}

impl ActiveQuest {
    /// A fresh quest with its counters sized to the goals of `q`.
    pub fn new(id: u16, q: &Quest) -> ActiveQuest {
        ActiveQuest { quest: id, killed: vec![0; q.required_npcs.len()], targeted: vec![0; q.required_target_npcs.len()] }
    }
}

/// What is saved of a character's quests (`quest` and `quest_done` tables).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuestSave {
    /// Slots 1..=5 that hold a quest.
    pub active: Vec<(u8, ActiveQuest)>,
    pub done: Vec<u16>,
}

#[derive(Debug, Default)]
pub struct QuestPiece {
    /// `QuestStats.Quests(1..MAXUSERQUESTS)`, kept packed at the front (`ArrangeUserQuests`).
    pub slots: [Option<ActiveQuest>; MAX_ACTIVE],
    /// `QuestsDone`, without repeats.
    pub done: Vec<u16>,
    /// `flags.TargetNPC` as far as quests care: (map, NPC entity id) of the quest NPC the
    /// player last talked to. `Accept` only works against it.
    pub target: Option<(u16, u32)>,
}

impl QuestPiece {
    /// `TieneQuest`: the 0-based slot of quest `id`.
    pub fn slot_of(&self, id: u16) -> Option<usize> {
        self.slots.iter().position(|s| s.as_ref().is_some_and(|a| a.quest == id))
    }

    /// `FreeQuestSlot`: first empty 0-based slot.
    pub fn free_slot(&self) -> Option<usize> {
        self.slots.iter().position(Option::is_none)
    }

    pub fn is_active(&self, id: u16) -> bool {
        self.slot_of(id).is_some()
    }

    /// `UserDoneQuest` (quest 0 counts as done: "no previous quest needed").
    pub fn has_done(&self, id: u32) -> bool {
        id == 0 || self.done.iter().any(|d| *d as u32 == id)
    }

    /// `AddDoneQuest` (the official list repeats a repeatable quest each time; a set is
    /// enough for every question asked of it).
    pub fn add_done(&mut self, id: u16) {
        if !self.done.contains(&id) {
            self.done.push(id);
        }
    }

    /// `CleanQuestSlot` + `ArrangeUserQuests`: empty a slot and pack the rest at the front.
    pub fn remove(&mut self, slot: usize) {
        self.slots[slot] = None;
        let packed: Vec<ActiveQuest> = self.slots.iter_mut().filter_map(Option::take).collect();
        let mut packed = packed.into_iter();
        for s in self.slots.iter_mut() {
            *s = packed.next();
        }
    }

    pub fn active_ids(&self) -> Vec<u16> {
        self.slots.iter().flatten().map(|a| a.quest).collect()
    }

    pub fn to_save(&self) -> QuestSave {
        QuestSave { active: self.slots.iter().enumerate().filter_map(|(i, s)| s.clone().map(|a| (i as u8 + 1, a))).collect(), done: self.done.clone() }
    }

    /// Loads a saved state, dropping quests that no longer exist and resizing the counters to
    /// the goals the quest has now (the data file may have changed since).
    pub fn from_save(save: QuestSave, data: &QuestData) -> QuestPiece {
        let mut piece = QuestPiece::default();
        let mut active = save.active;
        active.sort_by_key(|(slot, _)| *slot);
        for (_, mut a) in active {
            let Some(q) = data.quest(a.quest as u32) else { continue };
            if piece.is_active(a.quest) {
                continue;
            }
            a.killed.resize(q.required_npcs.len(), 0);
            a.targeted.resize(q.required_target_npcs.len(), 0);
            if let Some(i) = piece.free_slot() {
                piece.slots[i] = Some(a);
            }
        }
        for d in save.done {
            if data.quest(d as u32).is_some() {
                piece.add_done(d);
            }
        }
        piece
    }
}

/// `QuestList` and the quests each NPC offers.
pub struct QuestData {
    pub quests: QuestTable,
    pub offers: NpcOffers,
}

impl QuestData {
    /// Loads (or reuses) the official quest data of a `Recursos` folder.
    pub fn shared(recursos: &Path) -> Arc<QuestData> {
        static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<QuestData>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let mut cache = cache.lock().unwrap();
        cache.entry(recursos.to_path_buf()).or_insert_with(|| Arc::new(QuestData::load(recursos))).clone()
    }

    fn load(recursos: &Path) -> QuestData {
        let quests = match load(recursos) {
            Ok(l) => {
                for w in &l.warnings {
                    warn!(?w, "dato oficial");
                }
                l.data
            }
            Err(e) => panic!("no se pudo leer Quests.DAT de {}: {e:?}", recursos.display()),
        };
        let offers = match load_npc_offers(recursos) {
            Ok(l) => l.data,
            Err(e) => panic!("no se pudo leer npcs.dat de {}: {e:?}", recursos.display()),
        };
        QuestData { quests, offers }
    }

    /// `IsQuestIndexValid` + the quest.
    pub fn quest(&self, id: u32) -> Option<&Quest> {
        if id == 0 {
            return None;
        }
        self.quests.get(id)
    }

    /// Valid quests offered by NPC number `npc` (`NpcList.QuestNumber`).
    pub fn offered_by(&self, npc: u32) -> Vec<u32> {
        self.offers.get(&npc).map(|l| l.iter().filter(|q| **q > 0 && self.quests.get(**q as u32).is_some()).map(|q| *q as u32).collect()).unwrap_or_default()
    }
}

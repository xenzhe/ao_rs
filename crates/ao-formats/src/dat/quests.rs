//! `Quests.DAT`: the missions (`[QUEST12]`), plus the missions each quest NPC offers
//! (`NumQuest` / `QuestNumberN` in `npcs.dat`).
//!
//! Mirrors `ModQuest.LoadQuests` (ModQuest.bas) and the quest lines of
//! `MODULO_NPCs.LoadNpcInfoIntoCache`. The official loader walks `1..NumQuests` and reads
//! every key of every section; a missing section (`[QUEST3]` is empty in the real file) is a
//! quest with all defaults. Sections above `NumQuests` are never read by the server, so they
//! are not loaded here either. Keys are case-insensitive (`RequiredNpc1` is read as
//! `RequiredNPC1`, `RequiredTargetNPCS` as `RequiredTargetNPCs`).
//!
//! Keys that appear in the file but that the official loader does not read (`RequiredKills`,
//! `RequiredFactionScore`, the `En_`/`Pt_`/`Fr_`/`It_` translations, audio ids, `NoWiki`)
//! are not loaded: the server never uses them.
use super::{Loaded, RawIni, Sec, Table, Warning};
use std::collections::HashMap;
use std::path::Path;

pub const FILE: &str = "Quests.DAT";
/// `RequiredNPC` id that stands for "players killed" (TSAO quests).
pub const PLAYER_KILLS: i32 = -1;
const NPCS_FILE: &str = "npcs.dat";
/// Sanity limit for the indexed lists (`RequiredNpc1..n`, `RewardObj1..n`...). The official
/// arrays are `ReDim`ed to whatever the file says; real quests have a handful.
const MAX_LIST: usize = 255;

/// `<obj>-<amount>` (`RequiredObjN`, `RewardObjN`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjAmount {
    pub obj: i32,
    pub amount: i32,
}

/// `<npc>-<amount>` (`RequiredNpcN`: kill `amount` of `npc`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NpcAmount {
    pub npc: i32,
    pub amount: i32,
}

/// `t_Quest`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quest {
    /// `Nombre`.
    pub name: String,
    /// `Desc`.
    pub desc: String,
    /// `DescFinal`.
    pub desc_final: String,
    /// `RequiredLevel`.
    pub required_level: i32,
    /// `RequiredQuest`: quest that must be done first (0 = none).
    pub required_quest: i32,
    /// `LimitLevel`: highest level allowed to start it (0 = no limit).
    pub limit_level: i32,
    /// `NextQuest` (read as text by the official loader, never used).
    pub next_quest: String,
    /// `RequiredClass1..n` (`RequiredClassesCount`), class ids 1..=12.
    pub required_classes: Vec<u8>,
    /// `RequiredOBJ1..n`.
    pub required_objs: Vec<ObjAmount>,
    /// `Trabajador=1`: only for the Trabajador class.
    pub worker_only: bool,
    /// `TalkTo`: NPC number (npcs.dat) the quest is handed in to (0 = the NPC that gave it).
    pub talk_to: i32,
    /// `PosMap`: map where that NPC is (informative).
    pub pos_map: i32,
    /// `RequiredSpell1..n`: spells the player must know, taken away at the end.
    pub required_spells: Vec<i32>,
    /// `RequiredNPC1..n`: creatures to kill.
    pub required_npcs: Vec<NpcAmount>,
    /// `RequiredSkill` / `RequiredValue`: skill id (1..=24, e_Skill) and minimum value.
    pub required_skill: i32,
    pub required_skill_value: i32,
    /// `RequiredTargetNPC1..n`: NPCs to click on once (the amount is always 1).
    pub required_target_npcs: Vec<NpcAmount>,
    /// `RewardGLD`.
    pub reward_gold: i32,
    /// `RewardEXP`.
    pub reward_exp: i32,
    /// `Repetible`: 0 = can be done once, anything else = repeatable.
    pub repeatable: i32,
    /// `PermittedFactions`: bit set (1 citizen, 2 royal army, 4 royal council, 8 criminal,
    /// 16 chaos legion, 32 chaos council); 0 = everybody.
    pub permitted_factions: i32,
    /// `GlobalQuestIndex` (ModGlobalQuests): 0 = an ordinary quest.
    pub global_quest_index: i32,
    /// `GlobalQuestThresholdNeeded`.
    pub global_quest_threshold: i32,
    /// `RewardOBJ1..n` (`RewardOBJs`).
    pub reward_objs: Vec<ObjAmount>,
    /// `RewardSkill1..n` (`RewardSkills`): spells taught as a reward.
    pub reward_spells: Vec<i32>,
}

/// Quests by id (`QuestList`). `declared` is `[INIT] NumQuests`.
pub type QuestTable = Table<Quest>;

/// Reads `Recursos/Dat/Quests.DAT`.
pub fn load(recursos: &Path) -> Result<Loaded<QuestTable>, super::Error> {
    let mut warnings = Vec::new();
    let ini = RawIni::load(recursos, FILE, &mut warnings)?;
    Ok(finish(&ini, warnings))
}

/// Parses the text of a `Quests.DAT` (already decoded).
pub fn parse_str(text: &str) -> Loaded<QuestTable> {
    let mut warnings = Vec::new();
    let ini = RawIni::parse(text, FILE, &mut warnings);
    finish(&ini, warnings)
}

fn finish(ini: &RawIni, mut w: Vec<Warning>) -> Loaded<QuestTable> {
    let declared = ini.sec(FILE, "INIT", &mut w).int("NumQuests").max(0) as u32;
    let mut table = Table { declared, ..Table::default() };
    // LoadQuests: `For i = 1 To NumQuests`, a missing section reads as all defaults.
    for id in 1..=declared {
        let name = format!("QUEST{id}");
        let mut sec = ini.sec(FILE, &name, &mut w);
        table.insert(id, read_quest(&mut sec));
    }
    Loaded { data: table, warnings: w }
}

/// `ReadField(1, text, 45)` / `ReadField(2, text, 45)`: both numbers of `a-b`.
fn pair(s: &mut Sec, key: &str) -> (i32, i32) {
    let v = s.ints(key, '-', 2);
    (v[0], v[1])
}

#[allow(clippy::field_reassign_with_default)]
fn read_quest(s: &mut Sec) -> Quest {
    let mut q = Quest::default();
    q.name = s.text("Nombre");
    q.desc = s.text("Desc");
    q.required_level = s.int("RequiredLevel");
    q.required_quest = s.int("RequiredQuest");
    q.limit_level = s.int("LimitLevel");
    q.desc_final = s.text("DescFinal");
    q.next_quest = s.text("NextQuest");

    let n = s.count("RequiredClassesCount", MAX_LIST);
    for j in 1..=n {
        let key = format!("RequiredClass{j}");
        let v = s.int(&key);
        if !(0..=255).contains(&v) {
            // `CByte` overflows in VB (error 6 -> the loader aborts); keep going with 0.
            s.warning(&key, s.raw(&key).unwrap_or(""), "class id out of byte range (read as 0)");
        }
        q.required_classes.push(v.clamp(0, 255) as u8);
    }

    let n = s.count("RequiredOBJs", MAX_LIST);
    q.worker_only = s.int("Trabajador") == 1;
    q.talk_to = s.int("TalkTo");
    q.pos_map = s.int("PosMap");
    for j in 1..=n {
        let (obj, amount) = pair(s, &format!("RequiredOBJ{j}"));
        q.required_objs.push(ObjAmount { obj, amount });
    }

    let n = s.count("RequiredSpellCount", MAX_LIST);
    for j in 1..=n {
        let v = s.int(&format!("RequiredSpell{j}"));
        q.required_spells.push(v);
    }

    let n = s.count("RequiredNPCs", MAX_LIST);
    for j in 1..=n {
        let (npc, amount) = pair(s, &format!("RequiredNPC{j}"));
        q.required_npcs.push(NpcAmount { npc, amount });
    }
    // TSAO quest type 2 ("kill N users"), converted as `RequiredUserKills=N`.
    let users = s.int("RequiredUserKills");
    if users > 0 {
        q.required_npcs.push(NpcAmount { npc: PLAYER_KILLS, amount: users });
    }
    q.required_skill = s.int("RequiredSkill");
    q.required_skill_value = s.int("RequiredValue");

    let n = s.count("RequiredTargetNPCs", MAX_LIST);
    for j in 1..=n {
        let (npc, _) = pair(s, &format!("RequiredTargetNPC{j}"));
        q.required_target_npcs.push(NpcAmount { npc, amount: 1 });
    }

    q.reward_gold = s.int("RewardGLD");
    q.reward_exp = s.int("RewardEXP");
    q.repeatable = s.int("Repetible");
    q.permitted_factions = s.int("PermittedFactions");
    q.global_quest_index = s.int("GlobalQuestIndex");
    q.global_quest_threshold = s.int("GlobalQuestThresholdNeeded");

    let n = s.count("RewardOBJs", MAX_LIST);
    for j in 1..=n {
        let (obj, amount) = pair(s, &format!("RewardOBJ{j}"));
        q.reward_objs.push(ObjAmount { obj, amount });
    }
    let n = s.count("RewardSkills", MAX_LIST);
    for j in 1..=n {
        let v = s.int(&format!("RewardSkill{j}"));
        q.reward_spells.push(v);
    }
    q
}

/// Missions each NPC offers: npc number -> quest ids (`NumQuest` / `QuestNumber1..n` of
/// `npcs.dat`, read by `LoadNpcInfoIntoCache`). Only NPCs that offer something are present.
pub type NpcOffers = HashMap<u32, Vec<i32>>;

/// Reads the quest offers of every NPC of `Recursos/Dat/npcs.dat`.
pub fn load_npc_offers(recursos: &Path) -> Result<Loaded<NpcOffers>, super::Error> {
    let mut warnings = Vec::new();
    let ini = RawIni::load(recursos, NPCS_FILE, &mut warnings)?;
    Ok(npc_offers(&ini, warnings))
}

/// Parses the quest offers from the text of an `npcs.dat` (already decoded).
pub fn parse_npc_offers_str(text: &str) -> Loaded<NpcOffers> {
    let mut warnings = Vec::new();
    let ini = RawIni::parse(text, NPCS_FILE, &mut warnings);
    npc_offers(&ini, warnings)
}

fn npc_offers(ini: &RawIni, mut w: Vec<Warning>) -> Loaded<NpcOffers> {
    // Duplicate-section / numbering warnings belong to the npcs.dat reader, not here.
    let mut scratch = Vec::new();
    let mut out = NpcOffers::new();
    for (id, name) in ini.numbered("NPC", NPCS_FILE, &mut scratch) {
        let mut sec = ini.sec(NPCS_FILE, &name, &mut w);
        let n = sec.count("NumQuest", MAX_LIST);
        if n > 0 {
            let list = (1..=n).map(|j| sec.int(&format!("QuestNumber{j}"))).collect();
            out.insert(id, list);
        }
    }
    Loaded { data: out, warnings: w }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::recursos_dir;

    #[test]
    fn synthetic_quests() {
        let text = "\
[INIT]
NumQuests=4
[QUEST1]
Nombre=Ratas
Desc=Matalas
Repetible=1
RequiredLevel=3
LimitLevel=8
RequiredNPCs=2
RequiredNpc1=642-5
RequiredNpc2=500-1
RewardEXP=600
RewardGLD=250
RewardOBJs=1
RewardObj1=1188-2
RequiredClassesCount=2
RequiredClass1=1
RequiredClass2=3
Trabajador=1
[QUEST2]
RequiredQuest=1
RequiredTargetNPCs=1
RequiredTargetNPC1=119
RequiredTargetNPCS=1
RequiredSkill=7
RequiredValue=30
RequiredSpellCount=1
RequiredSpell1=12
RewardSkills=1
RewardSkill1=14
TalkTo=1069
PosMap=44
[QUEST3]
[QUEST9]
Nombre=above NumQuests
";
        let Loaded { data: t, warnings } = parse_str(text);
        assert_eq!(t.len(), 4, "1..=NumQuests, empty sections included");
        assert!(t.get(9).is_none(), "sections above NumQuests are never loaded");
        let q = t.get(1).unwrap();
        assert_eq!((q.name.as_str(), q.desc.as_str()), ("Ratas", "Matalas"));
        assert_eq!((q.required_level, q.limit_level, q.repeatable), (3, 8, 1));
        assert_eq!(q.required_npcs, vec![NpcAmount { npc: 642, amount: 5 }, NpcAmount { npc: 500, amount: 1 }]);
        assert_eq!(q.reward_objs, vec![ObjAmount { obj: 1188, amount: 2 }]);
        assert_eq!((q.reward_exp, q.reward_gold), (600, 250));
        assert_eq!(q.required_classes, vec![1, 3]);
        assert!(q.worker_only);
        let q = t.get(2).unwrap();
        assert_eq!(q.required_quest, 1);
        assert_eq!(q.required_target_npcs, vec![NpcAmount { npc: 119, amount: 1 }]);
        assert_eq!((q.required_skill, q.required_skill_value), (7, 30));
        assert_eq!(q.required_spells, vec![12]);
        assert_eq!(q.reward_spells, vec![14]);
        assert_eq!((q.talk_to, q.pos_map), (1069, 44));
        assert_eq!(t.get(3).unwrap(), &Quest::default());
        assert!(warnings.iter().all(|w| w.message.contains("duplicate")), "{warnings:?}");
    }

    #[test]
    fn synthetic_offers() {
        let text = "[NPC407]\nNumQuest=2\nQuestNumber1=407\nQuestNumber2=408\n[NPC500]\nName=Lobo\n[NPC600]\nNumQuest=0\n";
        let o = parse_npc_offers_str(text).data;
        assert_eq!(o.len(), 1);
        assert_eq!(o[&407], vec![407, 408]);
    }

    /// The real file: every quest loads, the values the VB loader would read are there.
    #[test]
    fn official_quests_file_loads() {
        let Loaded { data: t, warnings } = load(&recursos_dir()).expect("Quests.DAT");
        assert_eq!(t.declared, 427);
        assert_eq!(t.len(), 427, "all sections 1..=NumQuests");
        // Quest 1 "Descanso Final": repeatable, level 13, three kill goals.
        let q = t.get(1).unwrap();
        assert_eq!(q.name, "Descanso Final");
        assert_eq!(q.repeatable, 1);
        assert_eq!(q.required_level, 13);
        assert_eq!(q.required_npcs, vec![NpcAmount { npc: 946, amount: 25 }, NpcAmount { npc: 1003, amount: 20 }, NpcAmount { npc: 514, amount: 30 }]);
        assert_eq!((q.reward_exp, q.reward_gold), (1625, 550));
        // Quest 2: only for workers (class 9), not repeatable, one item reward.
        let q = t.get(2).unwrap();
        assert_eq!(q.required_classes, vec![9]);
        assert_eq!(q.repeatable, 0);
        assert_eq!(q.reward_objs, vec![ObjAmount { obj: 1188, amount: 1 }]);
        // Quest 3 is an empty section.
        assert_eq!(t.get(3).unwrap(), &Quest::default());
        // "Una nueva aventura" and the one that needs it.
        assert_eq!(t.get(407).unwrap().required_npcs.len(), 4);
        assert_eq!(t.get(408).unwrap().required_quest, 407);
        // Level-limited rat quest.
        let q = t.get(427).unwrap();
        assert_eq!((q.required_level, q.limit_level), (1, 8));
        assert_eq!(q.required_npcs, vec![NpcAmount { npc: 642, amount: 5 }]);
        // The target-NPC quest (key spelled both RequiredTargetNPC1 and RequiredTargetNPCS).
        assert_eq!(t.get(5).unwrap().required_target_npcs, vec![NpcAmount { npc: 119, amount: 1 }]);
        // Some quest has a TalkTo, some a spell, some a skill, some a global event.
        assert!(t.iter().any(|(_, q)| q.talk_to > 0));
        assert!(t.iter().any(|(_, q)| !q.required_spells.is_empty()));
        assert!(t.iter().any(|(_, q)| q.required_skill > 0 && q.required_skill_value > 0));
        assert!(t.iter().any(|(_, q)| q.global_quest_index > 0));
        // Every kill goal has a positive amount.
        for (id, q) in t.iter() {
            for n in &q.required_npcs {
                assert!(n.npc > 0 && n.amount > 0, "quest {id}: bad goal {n:?}");
            }
        }
        // Official typos stay as warnings, never failures.
        for w in &warnings {
            assert_eq!(w.file, FILE);
        }
    }

    #[test]
    fn official_npc_offers() {
        let Loaded { data: o, .. } = load_npc_offers(&recursos_dir()).expect("npcs.dat");
        // Makrom (Dungeon Newbie) offers six missions, Robbie one.
        assert_eq!(o[&407], vec![407, 408, 409, 410, 411, 412]);
        assert_eq!(o[&406], vec![1]);
        assert_eq!(o[&408], vec![427]);
        // Every offered quest exists in Quests.DAT.
        let q = load(&recursos_dir()).unwrap().data;
        for (npc, list) in &o {
            for id in list {
                assert!(*id >= 0, "npc {npc}: negative quest {id}");
                if *id > 0 {
                    assert!(q.get(*id as u32).is_some(), "npc {npc} offers a missing quest {id}");
                }
            }
        }
    }
}

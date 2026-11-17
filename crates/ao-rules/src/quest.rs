//! Pure rules of quests (requirements, progress, rewards). OWNER: Sonnet 1 (docs/P2_CONTRATO.md).
//!
//! Ported from `ModQuest.bas` (`CanUserAcceptQuest`, `FinishQuestCheck`, `FinishQuest`,
//! `AllRequiredNPCsKilled`), `NPCs.OnNpcKilledUpdateQuest` (MODULO_NPCs.bas), the target
//! counting of `GameLogic.LookatTile` and the symbol choice of `MODULO_NPCs`
//! (`WriteUpdateNPCSimbolo`). Nothing here touches a world: the server system feeds it
//! facts (level, class, inventory counts...) and applies the verdict.
use ao_formats::dat::quests::Quest;

/// `MAXUSERQUESTS`: quests a character can have in progress at once.
pub const MAX_ACTIVE: usize = 5;
/// `Distancia(user, npc) > 5` in `EnviarQuest` / `HandleQuestAccept`.
pub const MAX_NPC_DISTANCE: i32 = 5;
/// `GiveGLD < 100000` goes to the purse, anything else to the bank account.
pub const WALLET_GOLD_LIMIT: u64 = 100_000;
/// `e_Class.Trabajador`.
pub const WORKER_CLASS: u8 = 9;

/// `e_QuestPermittedFactions`.
pub mod faction {
    pub const CITIZEN: i32 = 1;
    pub const ROYAL_ARMY: i32 = 2;
    pub const ROYAL_COUNCIL: i32 = 4;
    pub const CRIMINAL: i32 = 8;
    pub const CHAOS_LEGION: i32 = 16;
    pub const CHAOS_COUNCIL: i32 = 32;
}

/// What the rules need to know about the player.
#[derive(Clone, Copy, Debug)]
pub struct Candidate<'a> {
    pub level: u32,
    /// Class id 1..=12.
    pub class: u8,
    /// The 24 skills in official order (index 0 = skill 1).
    pub skills: &'a [u8],
    /// One bit of [`faction`] (0 = unknown status, refused like the VB `Case Else`).
    pub faction_bit: i32,
}

impl Candidate<'_> {
    /// `UserSkills(n)` for the 1-based e_Skill `n`; out of range reads as 0.
    pub fn skill(&self, n: i32) -> i32 {
        usize::try_from(n - 1).ok().and_then(|i| self.skills.get(i)).copied().unwrap_or(0) as i32
    }
}

/// Why `CanUserAcceptQuest` said no, in the order the VB checks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `MSG_QUEST_ONLY_FOR_WORKERS`.
    WorkersOnly,
    /// `MSG_QUEST_ALREADY_IN_PROGRESS`.
    AlreadyActive,
    /// `MSG_MUST_COMPLETE_QUEST_TO_START_MISSION`: the quest that has to be done first.
    NeedsQuest(u32),
    /// `MSG_MUST_BE_AT_LEAST_LEVEL_TO_START_MISSION`.
    LevelTooLow(i32),
    /// `MSG_LEVEL_TOO_HIGH_FOR_MISSION`.
    LevelTooHigh,
    /// `MSG_MUST_INCREASE_SKILL_TO_CONTINUE`.
    SkillTooLow(i32),
    /// `MSG_MISSION_CLASS_NOT_AVAILABLE` (class).
    WrongClass,
    /// `MSG_MISSION_CLASS_NOT_AVAILABLE` (faction) or `MSG_HAY_ERROR_OBJETO_INFORMALE_ADMINISTRADOR`.
    WrongFaction,
    /// `MSG_QUEST_ALREADY_COMPLETED`: not repeatable and done.
    AlreadyDone,
    /// Global-event quest while the event is not running (`ModGlobalQuests`).
    EventInactive,
}

/// `HasUserRequiredClassForQuest`: no required classes means any class.
pub fn class_allowed(q: &Quest, class: u8) -> bool {
    q.required_classes.is_empty() || q.required_classes.contains(&class)
}

/// `PermittedFactions` against the player's faction bit.
pub fn faction_allowed(q: &Quest, bit: i32) -> bool {
    q.permitted_factions <= 0 || (bit != 0 && q.permitted_factions & bit != 0)
}

/// `CanUserAcceptQuest` without the distance check (the system does that with the NPC).
/// `active`: the player already has this quest. `done`: has the player completed quest `n`
/// (`UserDoneQuest`). `global_open`: the global event of the quest is running (always
/// false until `ModGlobalQuests` exists, which keeps event quests unacceptable).
pub fn can_accept(q: &Quest, who: &Candidate, active: bool, done: impl Fn(u32) -> bool, this: u32, global_open: bool) -> Result<(), Refusal> {
    if q.worker_only && who.class != WORKER_CLASS {
        return Err(Refusal::WorkersOnly);
    }
    if active {
        return Err(Refusal::AlreadyActive);
    }
    if q.required_quest > 0 && !done(q.required_quest as u32) {
        return Err(Refusal::NeedsQuest(q.required_quest as u32));
    }
    if (who.level as i64) < q.required_level as i64 {
        return Err(Refusal::LevelTooLow(q.required_level));
    }
    if q.limit_level > 0 && who.level as i64 > q.limit_level as i64 {
        return Err(Refusal::LevelTooHigh);
    }
    if q.required_skill > 0 && who.skill(q.required_skill) < q.required_skill_value {
        return Err(Refusal::SkillTooLow(q.required_skill));
    }
    if !class_allowed(q, who.class) {
        return Err(Refusal::WrongClass);
    }
    if !faction_allowed(q, who.faction_bit) {
        return Err(Refusal::WrongFaction);
    }
    if q.repeatable == 0 && done(this) {
        return Err(Refusal::AlreadyDone);
    }
    if q.global_quest_index > 0 && !global_open {
        return Err(Refusal::EventInactive);
    }
    Ok(())
}

/// What is still missing to hand a quest in (`FinishQuest` messages, in its order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unmet {
    /// `MSG_NO_HAS_CONSEGUIDO_TODOS_LOS_OBJETOS_QUE_TE_HE_PEDIDO`.
    Objects,
    /// `MSG_NO_HAS_MATADO_TODAS_LAS_CRIATURAS_QUE_TE_HE_PEDIDO`.
    Kills,
    /// `MSG_NECESITAS_APRENDER_EL_HECHIZO`: the spell id.
    Spell(i32),
    /// `MSG_NO_HAS_VISITADO_AL_NPC_QUE_TE_PEDI`.
    Targets,
    /// `MsgRequiredSkill`: the skill id.
    Skill(i32),
}

/// The player's side of a quest in progress.
#[derive(Clone, Copy, Debug)]
pub struct Progress<'a> {
    /// `NPCsKilled(i)` per `RequiredNPC(i)`.
    pub killed: &'a [u16],
    /// `NPCsTarget(i)` per `RequiredTargetNPC(i)`.
    pub targeted: &'a [u16],
}

/// First unmet requirement of `FinishQuest` (class is checked separately with
/// [`class_allowed`]). `None` = everything done, the quest can be handed in.
/// `have(obj)` is `TieneObjetos` (how many of the object the inventory holds); `knows` is
/// `UserHasSpell`. A progress array shorter than the goals counts as unmet.
pub fn first_unmet(q: &Quest, who: &Candidate, p: &Progress, have: impl Fn(i32) -> u32, knows: impl Fn(i32) -> bool) -> Option<Unmet> {
    if q.required_objs.iter().any(|o| (have(o.obj) as i64) < o.amount as i64) {
        return Some(Unmet::Objects);
    }
    if !all_killed(q, p.killed) {
        return Some(Unmet::Kills);
    }
    if let Some(s) = q.required_spells.iter().find(|s| !knows(**s)) {
        return Some(Unmet::Spell(*s));
    }
    if q.required_target_npcs.iter().enumerate().any(|(i, t)| (p.targeted.get(i).copied().unwrap_or(0) as i64) < t.amount as i64) {
        return Some(Unmet::Targets);
    }
    if q.required_skill > 0 && who.skill(q.required_skill) < q.required_skill_value {
        return Some(Unmet::Skill(q.required_skill));
    }
    None
}

/// `AllRequiredNPCsKilled` (true when there are no kill goals, as the loops of
/// `FinishQuestCheck` do).
pub fn all_killed(q: &Quest, killed: &[u16]) -> bool {
    q.required_npcs.iter().enumerate().all(|(i, n)| killed.get(i).copied().unwrap_or(0) as i64 >= n.amount as i64)
}

/// One goal touched by a kill: `NPCsKilled(index)` is now `killed` of `required`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KillStep {
    pub index: usize,
    pub killed: u16,
    pub required: u16,
}

/// `OnNpcKilledUpdateQuest` for one active quest: every goal for creature `npc` that is not
/// full yet counts one more (a goal already full keeps its value but is still reported,
/// like the VB chat-over-head). `killed` grows to the goal count if it was shorter.
pub fn register_kill(q: &Quest, killed: &mut Vec<u16>, npc: i32) -> Vec<KillStep> {
    if killed.len() < q.required_npcs.len() {
        killed.resize(q.required_npcs.len(), 0);
    }
    let mut out = Vec::new();
    for (i, goal) in q.required_npcs.iter().enumerate() {
        if goal.npc != npc {
            continue;
        }
        let required = goal.amount.clamp(0, u16::MAX as i32) as u16;
        if required > killed[i] {
            killed[i] += 1;
        }
        out.push(KillStep { index: i, killed: killed[i], required });
    }
    out
}

/// Target counting of `LookatTile`: the goals for NPC `npc` count one (amount is 1). Returns
/// the indexes whose count equals the amount afterwards (the VB then calls `FinishQuest`).
pub fn register_target(q: &Quest, targeted: &mut Vec<u16>, npc: i32) -> Vec<usize> {
    if targeted.len() < q.required_target_npcs.len() {
        targeted.resize(q.required_target_npcs.len(), 0);
    }
    let mut reached = Vec::new();
    for (i, goal) in q.required_target_npcs.iter().enumerate() {
        if goal.npc != npc {
            continue;
        }
        if (goal.amount as i64) > targeted[i] as i64 {
            targeted[i] += 1;
        }
        if goal.amount as i64 == targeted[i] as i64 {
            reached.push(i);
        }
    }
    reached
}

/// Where the gold reward goes (`FinishQuest`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoldReward {
    Wallet(u32),
    /// Too much for the purse: deposited in the bank account.
    Bank(u32),
}

/// `RewardGLD * GoldMult`, in the purse below 100.000 and in the bank from there.
pub fn gold_reward(reward: i32, gold_mult: u32) -> Option<GoldReward> {
    if reward <= 0 {
        return None;
    }
    let total = (reward as u64).saturating_mul(gold_mult.max(1) as u64).min(u32::MAX as u64);
    Some(if total < WALLET_GOLD_LIMIT { GoldReward::Wallet(total as u32) } else { GoldReward::Bank(total as u32) })
}

/// `RewardEXP * ExpMult`.
pub fn exp_reward(reward: i32, exp_mult: u32) -> u32 {
    if reward <= 0 {
        return 0;
    }
    (reward as u64).saturating_mul(exp_mult.max(1) as u64).min(u32::MAX as u64) as u32
}

/// How one offered quest looks to a player (`WriteNpcQuestListSend` state byte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OfferState {
    /// 0: can be started now.
    Available = 0,
    /// 1: in progress.
    Active = 1,
    /// 2: already done.
    Done = 2,
    /// 3: requirements not met (previous quest, level, event).
    Locked = 3,
}

/// The state byte of `WriteNpcQuestListSend`.
pub fn offer_state(q: &Quest, level: u32, active: bool, done_self: bool, done_required: bool, global_open: bool) -> OfferState {
    if active {
        return OfferState::Active;
    }
    if done_self {
        return OfferState::Done;
    }
    let mut ok = true;
    if q.required_quest > 0 && !done_required {
        ok = false;
    }
    if q.required_level > 0 && (level as i64) < q.required_level as i64 {
        ok = false;
    }
    if q.limit_level > 0 && level as i64 > q.limit_level as i64 {
        ok = false;
    }
    if q.global_quest_index > 0 && !global_open {
        ok = false;
    }
    if ok {
        OfferState::Available
    } else {
        OfferState::Locked
    }
}

/// Symbol over a quest NPC's head (`WriteUpdateNPCSimbolo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Symbol {
    /// 1: has a quest for you.
    Available = 1,
    /// 2: nothing for you now.
    Unavailable = 2,
    /// 3: a quest of yours is ready to hand in.
    Ready = 3,
    /// 4: a quest of yours is still in progress.
    Pending = 4,
}

/// What one offered quest means for the symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    Available,
    Unavailable,
    Pending,
    Ready,
}

/// The symbol of an NPC from its quests (priority: ready, pending, available, else
/// unavailable); `None` when it offers nothing.
pub fn npc_symbol(standings: impl IntoIterator<Item = Standing>) -> Option<Symbol> {
    let mut any = false;
    let (mut ready, mut pending, mut available) = (false, false, false);
    for s in standings {
        any = true;
        match s {
            Standing::Ready => ready = true,
            Standing::Pending => pending = true,
            Standing::Available => available = true,
            Standing::Unavailable => {}
        }
    }
    match (any, ready, pending, available) {
        (false, ..) => None,
        (_, true, ..) => Some(Symbol::Ready),
        (_, _, true, _) => Some(Symbol::Pending),
        (_, _, _, true) => Some(Symbol::Available),
        _ => Some(Symbol::Unavailable),
    }
}

/// The standing of one offered quest for the NPC symbol (`MODULO_NPCs`, loop over `NumQuest`).
pub fn standing(q: &Quest, level: u32, class: u8, active_ready: Option<bool>, done_self: bool, done_required: bool) -> Standing {
    match active_ready {
        Some(true) => Standing::Ready,
        Some(false) => Standing::Pending,
        None => {
            let class_ok = q.required_classes.is_empty() || q.required_classes.contains(&class);
            if done_self || !done_required || (level as i64) < q.required_level as i64 || !class_ok {
                Standing::Unavailable
            } else {
                Standing::Available
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ao_formats::dat::quests::{NpcAmount, ObjAmount};

    fn skills() -> [u8; 24] {
        let mut s = [0u8; 24];
        s[6] = 40;
        s
    }

    fn who(level: u32, class: u8, s: &[u8]) -> Candidate<'_> {
        Candidate { level, class, skills: s, faction_bit: faction::CITIZEN }
    }

    fn quest() -> Quest {
        Quest { required_level: 5, limit_level: 10, required_npcs: vec![NpcAmount { npc: 500, amount: 2 }], ..Quest::default() }
    }

    #[test]
    fn accept_checks_in_order() {
        let s = skills();
        let none = |_| false;
        let q = quest();
        assert_eq!(can_accept(&q, &who(7, 3, &s), false, none, 1, false), Ok(()));
        assert_eq!(can_accept(&q, &who(7, 3, &s), true, none, 1, false), Err(Refusal::AlreadyActive));
        assert_eq!(can_accept(&q, &who(4, 3, &s), false, none, 1, false), Err(Refusal::LevelTooLow(5)));
        assert_eq!(can_accept(&q, &who(11, 3, &s), false, none, 1, false), Err(Refusal::LevelTooHigh));
        let w = Quest { worker_only: true, ..quest() };
        assert_eq!(can_accept(&w, &who(7, 3, &s), false, none, 1, false), Err(Refusal::WorkersOnly));
        assert_eq!(can_accept(&w, &who(7, WORKER_CLASS, &s), false, none, 1, false), Ok(()));
        let pre = Quest { required_quest: 4, ..quest() };
        assert_eq!(can_accept(&pre, &who(7, 3, &s), false, none, 1, false), Err(Refusal::NeedsQuest(4)));
        assert_eq!(can_accept(&pre, &who(7, 3, &s), false, |n| n == 4, 1, false), Ok(()));
        let sk = Quest { required_skill: 7, required_skill_value: 50, ..quest() };
        assert_eq!(can_accept(&sk, &who(7, 3, &s), false, none, 1, false), Err(Refusal::SkillTooLow(7)));
        let cl = Quest { required_classes: vec![1, 2], ..quest() };
        assert_eq!(can_accept(&cl, &who(7, 3, &s), false, none, 1, false), Err(Refusal::WrongClass));
        assert_eq!(can_accept(&cl, &who(7, 2, &s), false, none, 1, false), Ok(()));
        let fa = Quest { permitted_factions: faction::CRIMINAL | faction::CHAOS_LEGION, ..quest() };
        assert_eq!(can_accept(&fa, &who(7, 3, &s), false, none, 1, false), Err(Refusal::WrongFaction));
        let fa_ok = Candidate { faction_bit: faction::CRIMINAL, ..who(7, 3, &s) };
        assert_eq!(can_accept(&fa, &fa_ok, false, none, 1, false), Ok(()));
        let unknown = Candidate { faction_bit: 0, ..who(7, 3, &s) };
        assert_eq!(can_accept(&fa, &unknown, false, none, 1, false), Err(Refusal::WrongFaction));
        // Not repeatable and already done; repeatable ones can be taken again.
        assert_eq!(can_accept(&q, &who(7, 3, &s), false, |n| n == 1, 1, false), Err(Refusal::AlreadyDone));
        let rep = Quest { repeatable: 1, ..quest() };
        assert_eq!(can_accept(&rep, &who(7, 3, &s), false, |n| n == 1, 1, false), Ok(()));
        let ev = Quest { global_quest_index: 2, ..quest() };
        assert_eq!(can_accept(&ev, &who(7, 3, &s), false, none, 1, false), Err(Refusal::EventInactive));
        assert_eq!(can_accept(&ev, &who(7, 3, &s), false, none, 1, true), Ok(()));
    }

    #[test]
    fn kills_count_up_to_the_goal() {
        let q = Quest { required_npcs: vec![NpcAmount { npc: 500, amount: 2 }, NpcAmount { npc: 501, amount: 1 }], ..Quest::default() };
        let mut k = Vec::new();
        assert!(register_kill(&q, &mut k, 999).is_empty(), "an unrelated creature");
        assert_eq!(register_kill(&q, &mut k, 500), vec![KillStep { index: 0, killed: 1, required: 2 }]);
        assert!(!all_killed(&q, &k));
        register_kill(&q, &mut k, 500);
        assert_eq!(register_kill(&q, &mut k, 500), vec![KillStep { index: 0, killed: 2, required: 2 }], "full goals do not overflow");
        assert!(!all_killed(&q, &k), "the second goal is open");
        register_kill(&q, &mut k, 501);
        assert!(all_killed(&q, &k));
        assert!(all_killed(&Quest::default(), &[]), "no goals, nothing to kill");
    }

    #[test]
    fn targets_reach_their_amount_once_counted() {
        let q = Quest { required_target_npcs: vec![NpcAmount { npc: 119, amount: 1 }], ..Quest::default() };
        let mut t = Vec::new();
        assert!(register_target(&q, &mut t, 7).is_empty());
        assert_eq!(register_target(&q, &mut t, 119), vec![0]);
        assert_eq!(register_target(&q, &mut t, 119), vec![0], "still equal after another click (VB re-finishes)");
        assert_eq!(t, vec![1]);
    }

    #[test]
    fn hand_in_requirements_in_vb_order() {
        let s = skills();
        let q = Quest {
            required_objs: vec![ObjAmount { obj: 10, amount: 3 }],
            required_npcs: vec![NpcAmount { npc: 500, amount: 1 }],
            required_spells: vec![12],
            required_target_npcs: vec![NpcAmount { npc: 119, amount: 1 }],
            required_skill: 7,
            required_skill_value: 30,
            ..Quest::default()
        };
        let me = who(9, 3, &s);
        let p0 = Progress { killed: &[0], targeted: &[0] };
        let p1 = Progress { killed: &[1], targeted: &[0] };
        let p2 = Progress { killed: &[1], targeted: &[1] };
        assert_eq!(first_unmet(&q, &me, &p2, |_| 2, |_| true), Some(Unmet::Objects));
        assert_eq!(first_unmet(&q, &me, &p0, |_| 3, |_| true), Some(Unmet::Kills));
        assert_eq!(first_unmet(&q, &me, &p1, |_| 3, |_| false), Some(Unmet::Spell(12)));
        assert_eq!(first_unmet(&q, &me, &p1, |_| 3, |_| true), Some(Unmet::Targets));
        let weak = skills().map(|_| 0);
        assert_eq!(first_unmet(&q, &who(9, 3, &weak), &p2, |_| 3, |_| true), Some(Unmet::Skill(7)));
        assert_eq!(first_unmet(&q, &me, &p2, |_| 3, |_| true), None);
        assert_eq!(first_unmet(&q, &me, &Progress { killed: &[], targeted: &[] }, |_| 3, |_| true), Some(Unmet::Kills), "missing progress is unmet");
    }

    #[test]
    fn rewards_with_multipliers_and_the_bank_threshold() {
        assert_eq!(gold_reward(0, 1), None);
        assert_eq!(gold_reward(250, 1), Some(GoldReward::Wallet(250)));
        assert_eq!(gold_reward(250, 3), Some(GoldReward::Wallet(750)));
        assert_eq!(gold_reward(99_999, 1), Some(GoldReward::Wallet(99_999)));
        assert_eq!(gold_reward(100_000, 1), Some(GoldReward::Bank(100_000)));
        assert_eq!(gold_reward(60_000, 2), Some(GoldReward::Bank(120_000)));
        assert_eq!(exp_reward(600, 2), 1200);
        assert_eq!(exp_reward(-5, 2), 0);
        assert_eq!(exp_reward(600, 0), 600, "a multiplier of 0 is read as 1 like the server config");
    }

    #[test]
    fn offer_states_and_symbols() {
        let q = Quest { required_level: 5, required_quest: 3, ..Quest::default() };
        assert_eq!(offer_state(&q, 7, true, false, true, false), OfferState::Active);
        assert_eq!(offer_state(&q, 7, false, true, true, false), OfferState::Done);
        assert_eq!(offer_state(&q, 7, false, false, false, false), OfferState::Locked);
        assert_eq!(offer_state(&q, 4, false, false, true, false), OfferState::Locked);
        assert_eq!(offer_state(&q, 7, false, false, true, false), OfferState::Available);
        use Standing::*;
        assert_eq!(npc_symbol([]), None);
        assert_eq!(npc_symbol([Unavailable]), Some(Symbol::Unavailable));
        assert_eq!(npc_symbol([Unavailable, Available]), Some(Symbol::Available));
        assert_eq!(npc_symbol([Available, Pending]), Some(Symbol::Pending));
        assert_eq!(npc_symbol([Pending, Ready, Available]), Some(Symbol::Ready));
        assert_eq!(standing(&q, 7, 3, None, false, true), Available);
        assert_eq!(standing(&q, 7, 3, None, true, true), Unavailable);
        assert_eq!(standing(&q, 7, 3, Some(false), false, true), Pending);
        assert_eq!(standing(&q, 7, 3, Some(true), false, true), Ready);
    }
}

//! Quest system. OWNER: Sonnet 1 (docs/P2_CONTRATO.md).
//!
//! Ported from the official server:
//! - `ModQuest.bas`: `EnviarQuest` (talking to a quest NPC), `CanUserAcceptQuest`,
//!   `FinishQuestCheck`, `FinishQuest`, `CleanQuestSlot` / `ArrangeUserQuests`, `AddDoneQuest`;
//! - `Protocol.bas`: `HandleQuestAccept`, `HandleQuestAbandon`, `HandleQuestListRequest`,
//!   `HandleQuestDetailsRequest`; `Protocol_Writes.bas`: `WriteNpcQuestListSend`,
//!   `WriteQuestListSend`, `WriteQuestDetails`;
//! - `MODULO_NPCs.bas`: `OnNpcKilledUpdateQuest` (kill counters) and the NPC symbol;
//! - `GameLogic.LookatTile`: the "visit this NPC" counters;
//! - `CharacterPersistence.bas`: loading and saving `quest` / `quest_done`.
//!
//! Left out on purpose: global event quests (`ModGlobalQuests`: such quests are never
//! acceptable, as if their event were not running) and quests started from an item
//! (`OtQuest` objects, `QuestOpenByObj`), which need the inventory's use-item path.
use crate::world::inventory::{InvItem, Inventory};
use crate::world::quest::{ActiveQuest, QuestData, QuestPiece};
use crate::world::World;
use ao_formats::dat::balance::Skill;
use ao_formats::dat::quests::{Quest, PLAYER_KILLS};
use ao_protocol::social::quest::{OfferedQuest, QuestInfo};
use ao_protocol::{QuestC2S, QuestS2C, Tone, S2C};
use ao_rules::quest::{self, Candidate, GoldReward, Progress, Refusal, Standing, Unmet};
use std::sync::Arc;
use tracing::warn;

const TOO_FAR: &str = "Estas demasiado lejos.";
const DEAD: &str = "Estas muerto!!";

fn u16c(v: i32) -> u16 {
    v.clamp(0, u16::MAX as i32) as u16
}

/// `Distancia` of the VB server.
fn distancia(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

/// `TieneObjetos`: how many of `obj` the inventory holds.
fn count_obj(inv: &Inventory, obj: i32) -> u32 {
    inv.slots.iter().flatten().filter(|i| i.obj as i32 == obj).map(|i| i.amount).fold(0u32, u32::saturating_add)
}

impl World {
    fn quest_data(&self) -> Arc<QuestData> {
        QuestData::shared(&self.recursos)
    }

    /// `UserList.Faccion.Status` as a bit of `e_QuestPermittedFactions`. The factions system
    /// (P2, Opus C) owns the status; until it is wired every character is a citizen.
    /// INTEGRATOR: map `Player.faction` here (royal army 2, royal council 4, criminal 8,
    /// chaos legion 16, chaos council 32).
    fn quest_faction_bit(&self, id: u32) -> i32 {
        use ao_rules::faction as f;
        match self.faction_status(id) {
            f::CRIMINAL => quest::faction::CRIMINAL,
            f::CHAOS => quest::faction::CHAOS_LEGION,
            f::ROYAL => quest::faction::ROYAL_ARMY,
            f::COUNCIL_CHAOS => quest::faction::CHAOS_COUNCIL,
            f::COUNCIL_ROYAL => quest::faction::ROYAL_COUNCIL,
            _ => quest::faction::CITIZEN,
        }
    }

    fn quest_say(&self, id: u32, nid: u32, text: impl Into<String>) {
        self.send(id, &S2C::NpcSay { id: nid, text: text.into() });
    }

    /// The fixed facts of a quest for the client, with the server's reward multipliers.
    fn quest_info(&self, id: u32, q: &Quest) -> QuestInfo {
        let w = &self.cfg.world;
        let reward_gold = quest::gold_reward(q.reward_gold, w.gold_mult).map_or(0, |g| match g {
            GoldReward::Wallet(v) | GoldReward::Bank(v) => v,
        });
        QuestInfo {
            id: id as u16,
            name: q.name.clone(),
            desc: q.desc.clone(),
            desc_final: q.desc_final.clone(),
            required_level: q.required_level.clamp(0, 255) as u8,
            limit_level: q.limit_level.clamp(0, 255) as u8,
            required_quest: u16c(q.required_quest),
            classes: q.required_classes.clone(),
            kill: q.required_npcs.iter().map(|n| (if n.npc == PLAYER_KILLS { u16::MAX } else { u16c(n.npc) }, u16c(n.amount))).collect(),
            objs: q.required_objs.iter().map(|o| (u16c(o.obj), u16c(o.amount))).collect(),
            spells: q.required_spells.iter().map(|s| u16c(*s)).collect(),
            skill: (q.required_skill.clamp(0, 255) as u8, q.required_skill_value.clamp(0, 255) as u8),
            reward_gold,
            reward_exp: quest::exp_reward(q.reward_exp, w.exp_mult),
            reward_objs: q.reward_objs.iter().map(|o| (u16c(o.obj), u16c(o.amount))).collect(),
            reward_spells: q.reward_spells.iter().map(|s| u16c(*s)).collect(),
            talk_to: u16c(q.talk_to),
            pos_map: u16c(q.pos_map),
        }
    }

    /// First unmet requirement of a quest in progress (`FinishQuestCheck` / `FinishQuest`
    /// order), `None` when it can be handed in. Event quests are never ready.
    fn quest_unmet(&self, id: u32, q: &Quest, a: &ActiveQuest) -> Option<Unmet> {
        let Some(p) = self.player(id) else { return Some(Unmet::Objects) };
        let who = Candidate { level: p.level, class: p.class, skills: &p.stats.skills, faction_bit: self.quest_faction_bit(id) };
        let prog = Progress { killed: &a.killed, targeted: &a.targeted };
        quest::first_unmet(q, &who, &prog, |o| count_obj(&p.inv, o), |s| p.spells.knows(u16c(s)))
    }

    fn quest_ready(&self, id: u32, q: &Quest, a: &ActiveQuest) -> bool {
        (q.global_quest_index <= 0 || self.gquest_finish_check(id, q, false)) && self.quest_unmet(id, q, a).is_none()
    }

    /// The symbol over the NPC number `number` for this player (`MODULO_NPCs`).
    fn quest_symbol(&self, id: u32, number: u32) -> Option<quest::Symbol> {
        let p = self.player(id)?;
        let data = self.quest_data();
        let mut standings = Vec::new();
        // Quests handed in to this NPC (`TalkTo`), then the ones it offers.
        for (qid, q) in data.quests.iter() {
            if q.talk_to > 0 && q.talk_to as u32 == number {
                if let Some(slot) = p.quest.slot_of(qid as u16) {
                    let a = p.quest.slots[slot].as_ref().unwrap();
                    standings.push(if self.quest_ready(id, q, a) { Standing::Ready } else { Standing::Pending });
                }
            }
        }
        for qid in data.offered_by(number) {
            let q = data.quest(qid).unwrap();
            let active = p.quest.slot_of(qid as u16).map(|s| self.quest_ready(id, q, p.quest.slots[s].as_ref().unwrap()));
            standings.push(quest::standing(q, p.level, p.class, active, p.quest.has_done(qid), p.quest.has_done(q.required_quest.max(0) as u32)));
        }
        quest::npc_symbol(standings)
    }

    fn send_quest_symbol(&self, id: u32, nid: u32, number: u32) {
        if let Some(s) = self.quest_symbol(id, number) {
            self.send(id, &S2C::Quest(QuestS2C::Symbol { npc: nid, symbol: s as u8 }));
        }
    }

    fn send_quest_list(&self, id: u32) {
        if let Some(p) = self.player(id) {
            self.send(id, &S2C::Quest(QuestS2C::List { active: p.quest.active_ids() }));
        }
    }

    /// `WriteQuestDetails` for the quest in 0-based `slot`.
    fn send_quest_details(&self, id: u32, slot: usize) {
        let Some(p) = self.player(id) else { return };
        let Some(a) = p.quest.slots.get(slot).and_then(Option::as_ref) else { return };
        let data = self.quest_data();
        let Some(q) = data.quest(a.quest as u32) else { return };
        let have = q.required_objs.iter().map(|o| count_obj(&p.inv, o.obj).min(u16::MAX as u32) as u16).collect();
        let known = q.required_spells.iter().map(|s| p.spells.knows(u16c(*s))).collect();
        self.send(
            id,
            &S2C::Quest(QuestS2C::Details {
                slot: slot as u8 + 1,
                info: Box::new(self.quest_info(a.quest as u32, q)),
                killed: a.killed.clone(),
                have,
                known,
                targeted: a.targeted.clone(),
            }),
        );
    }

    /// Writes the character's quests to the database (accounts only).
    pub(crate) fn quest_save(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let Some(char_id) = p.char_id else { return };
        if let Err(e) = self.db.save_quests(char_id, &p.quest.to_save()) {
            warn!(?e, name = %p.name, "no se pudieron guardar las misiones");
        }
    }

    // ------------------------------------------------------------------ messages

    pub(crate) fn on_quest(&mut self, id: u32, msg: QuestC2S) {
        match msg {
            QuestC2S::List => self.send_quest_list(id),
            QuestC2S::Details { slot } => {
                // HandleQuestDetailsRequest: out of range or empty slots are ignored.
                if (1..=quest::MAX_ACTIVE as u8).contains(&slot) {
                    self.send_quest_details(id, slot as usize - 1);
                }
            }
            QuestC2S::Accept { quest } => self.quest_accept(id, quest),
            QuestC2S::Abandon { slot } => self.quest_abandon(id, slot),
        }
    }

    /// HandleQuestAbandon.
    fn quest_abandon(&mut self, id: u32, slot: u8) {
        if !(1..=quest::MAX_ACTIVE as u8).contains(&slot) {
            return self.strike(id, "abandonar mision con slot invalido");
        }
        let i = slot as usize - 1;
        let Some(qid) = self.player(id).and_then(|p| p.quest.slots[i].as_ref().map(|a| a.quest)) else { return };
        let name = self.quest_data().quest(qid as u32).map(|q| q.name.clone()).unwrap_or_default();
        self.player_mut(id).quest.remove(i);
        self.console(id, format!("Has abandonado la misión {name}."), Tone::Info);
        self.send_quest_list(id);
        self.quest_refresh_target_symbol(id);
        self.quest_save(id);
    }

    /// Updates the symbol of the quest NPC the player is talking to.
    fn quest_refresh_target_symbol(&self, id: u32) {
        let Some((map, nid)) = self.player(id).and_then(|p| p.quest.target) else { return };
        if let Some(n) = self.maps.get(&map).and_then(|m| m.npcs.get(&nid)) {
            self.send_quest_symbol(id, nid, n.def as u32);
        }
    }

    /// The quest NPC the player last talked to, if it is still there, alive and on his map.
    fn quest_target(&mut self, id: u32) -> Option<(u32, u32, (i32, i32))> {
        let p = self.player(id)?;
        let (map, nid) = p.quest.target?;
        let alive = p.map == map && self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).is_some_and(|n| n.alive());
        if !alive {
            self.player_mut(id).quest.target = None;
            return None;
        }
        let n = &self.maps[&map].npcs[&nid];
        Some((nid, n.def as u32, (n.x, n.y)))
    }

    /// HandleQuestAccept (by NPC; the item path is not ported).
    /// `UseInvItem` for `OtQuest`: the object's quest is offered (no NPC); accepting it
    /// spends the object (`HandleQuestAccept` with `QuestOpenByObj`).
    pub(crate) fn quest_from_object(&mut self, id: u32, slot: u8, qid: u16) {
        let data = self.quest_data();
        let Some(q) = data.quest(qid as u32) else { return };
        let p = self.player(id).unwrap();
        let state = quest::offer_state(q, p.level, p.quest.is_active(qid), p.quest.has_done(qid as u32), p.quest.has_done(q.required_quest.max(0) as u32), self.gquest_accept_open(q));
        let quests = vec![OfferedQuest { info: self.quest_info(qid as u32, q), state: state as u8 }];
        self.player_mut(id).quest.obj_offer = Some((slot, qid));
        self.send(id, &S2C::Quest(QuestS2C::Offer { npc: 0, quests }));
    }

    /// Accepting the quest a quest object offered.
    fn quest_accept_from_object(&mut self, id: u32, qid: u16) -> bool {
        let Some((slot, offered)) = self.player(id).and_then(|p| p.quest.obj_offer) else { return false };
        self.player_mut(id).quest.obj_offer = None;
        if offered != qid {
            return false;
        }
        let ao = self.ao.clone();
        let still_there = self.player(id).and_then(|p| p.inv.get(slot)).and_then(|it| ao.objs.get(it.obj as u32)).is_some_and(|o| o.obj_type == ao_formats::dat::obj::ObjType::Quest && o.quest_id == qid as i32);
        if !still_there {
            return true;
        }
        let data = self.quest_data();
        let Some(q) = data.quest(qid as u32) else { return true };
        let p = self.player(id).unwrap();
        if p.hp <= 0 {
            self.console(id, DEAD, Tone::Warn);
            return true;
        }
        let who = Candidate { level: p.level, class: p.class, skills: &p.stats.skills, faction_bit: self.quest_faction_bit(id) };
        if let Err(r) = quest::can_accept(q, &who, p.quest.is_active(qid), |n| p.quest.has_done(n), qid as u32, self.gquest_accept_open(q)) {
            let text = self.quest_refusal_text(&data, r);
            self.console(id, text, Tone::Warn);
            return true;
        }
        let Some(qslot) = p.quest.free_slot() else {
            self.console(id, "Debes completar las misiones en curso para poder aceptar más misiones.", Tone::Warn);
            return true;
        };
        let name = q.name.clone();
        self.player_mut(id).quest.slots[qslot] = Some(ActiveQuest::new(qid, q));
        self.remove_from_slot(id, slot, 1);
        self.console(id, format!("Has aceptado la misión {name}."), Tone::Good);
        self.send_quest_list(id);
        self.send_quest_details(id, qslot);
        self.quest_save(id);
        true
    }

    fn quest_accept(&mut self, id: u32, qid: u16) {
        if self.quest_accept_from_object(id, qid) {
            return;
        }
        let Some((nid, number, npos)) = self.quest_target(id) else { return };
        let data = self.quest_data();
        let offered = data.offered_by(number);
        // An NPC with nothing to offer showed no list: a stale packet, not a cheat.
        if offered.is_empty() {
            return;
        }
        // The quest has to be one of the NPC's: a real client only sends what it was shown.
        if !offered.contains(&(qid as u32)) {
            return self.strike(id, "acepta una mision que el NPC no ofrece");
        }
        let q = data.quest(qid as u32).expect("offered quests exist");
        let p = self.player(id).unwrap();
        if p.hp <= 0 {
            return self.console(id, DEAD, Tone::Warn);
        }
        // CanUserAcceptQuest: distance first (only when there is an NPC), then the rules.
        if distancia((p.x, p.y), npos) > quest::MAX_NPC_DISTANCE {
            return self.console(id, TOO_FAR, Tone::Info);
        }
        let who = Candidate { level: p.level, class: p.class, skills: &p.stats.skills, faction_bit: self.quest_faction_bit(id) };
        let verdict = quest::can_accept(q, &who, p.quest.is_active(qid), |n| p.quest.has_done(n), qid as u32, self.gquest_accept_open(q));
        if let Err(r) = verdict {
            let text = self.quest_refusal_text(&data, r);
            return self.console(id, text, Tone::Warn);
        }
        let Some(slot) = p.quest.free_slot() else {
            return self.quest_say(id, nid, "Debes completar las misiones en curso para poder aceptar más misiones.");
        };
        let name = q.name.clone();
        self.player_mut(id).quest.slots[slot] = Some(ActiveQuest::new(qid, q));
        self.console(id, format!("Has aceptado la misión {name}."), Tone::Good);
        self.send_quest_list(id);
        self.send_quest_details(id, slot);
        self.send_quest_symbol(id, nid, number);
        self.quest_save(id);
    }

    fn quest_refusal_text(&self, data: &QuestData, r: Refusal) -> String {
        match r {
            Refusal::WorkersOnly => "Esta misión es solo para trabajadores.".into(),
            Refusal::AlreadyActive => "Ya tienes esta misión en curso.".into(),
            Refusal::NeedsQuest(n) => {
                let name = data.quest(n).map(|q| q.name.as_str()).unwrap_or("?");
                format!("Debes completar la misión {name} para poder comenzar esta misión.")
            }
            Refusal::LevelTooLow(l) => format!("Debes ser nivel {l} o superior para comenzar esta misión."),
            Refusal::LevelTooHigh => "Tu nivel es demasiado alto para esta misión.".into(),
            Refusal::SkillTooLow(s) => format!("Debes aumentar tu skill de {} para continuar.", Skill::from_id(s).and_then(Skill::spanish_name).unwrap_or("?")),
            Refusal::WrongClass | Refusal::WrongFaction => "Esta misión no está disponible para tu clase.".into(),
            Refusal::AlreadyDone => "Ya has completado esta misión.".into(),
            Refusal::EventInactive => "El evento de esta misión ya terminó.".into(),
        }
    }

    // ------------------------------------------------------------------ hooks

    /// HOOK (interact): the player talked to a quest NPC (`NpcType::Quest`, alive, near).
    /// `EnviarQuest`: hands in what is ready, else shows the quests the NPC offers.
    pub(crate) fn quest_npc(&mut self, id: u32, npc: u32) {
        let Some(p) = self.player(id) else { return };
        let (map, pos) = (p.map, (p.x, p.y));
        let Some(n) = self.maps.get(&map).and_then(|m| m.npcs.get(&npc)).filter(|n| n.alive()) else { return };
        let (number, npos) = (n.def as u32, (n.x, n.y));
        if distancia(pos, npos) > quest::MAX_NPC_DISTANCE {
            return self.console(id, TOO_FAR, Tone::Info);
        }
        self.player_mut(id).quest.target = Some((map, npc));
        // The click is also a "visit" for the target goals (LookatTile).
        self.quest_on_npc_target(id, npc);
        let data = self.quest_data();

        // Quests handed in to this NPC, whoever gave them (`TalkTo`).
        for (qid, q) in data.quests.iter() {
            if q.talk_to > 0 && q.talk_to as u32 == number && self.quest_hand_in_if_ready(id, qid as u16, npc) {
                return;
            }
        }
        let offered = data.offered_by(number);
        if data.offers.get(&number).is_none_or(|l| l.is_empty()) {
            return self.quest_say(id, npc, "No tengo ninguna misión para ti.");
        }
        for qid in &offered {
            if self.quest_hand_in_if_ready(id, *qid as u16, npc) {
                return;
            }
        }
        // WriteNpcQuestListSend.
        let p = self.player(id).unwrap();
        let quests = offered
            .iter()
            .map(|qid| {
                let q = data.quest(*qid).unwrap();
                let state = quest::offer_state(q, p.level, p.quest.is_active(*qid as u16), p.quest.has_done(*qid), p.quest.has_done(q.required_quest.max(0) as u32), self.gquest_accept_open(q));
                OfferedQuest { info: self.quest_info(*qid, q), state: state as u8 }
            })
            .collect();
        self.send(id, &S2C::Quest(QuestS2C::Offer { npc, quests }));
        self.send_quest_symbol(id, npc, number);
    }

    /// The loop body of `EnviarQuest`: if the player has quest `qid` and `FinishQuestCheck`
    /// passes, `FinishQuest` runs and the talk ends (whether or not the rewards were given).
    fn quest_hand_in_if_ready(&mut self, id: u32, qid: u16, nid: u32) -> bool {
        let data = self.quest_data();
        let Some(q) = data.quest(qid as u32) else { return false };
        let Some(p) = self.player(id) else { return false };
        let Some(slot) = p.quest.slot_of(qid) else { return false };
        let a = p.quest.slots[slot].clone().unwrap();
        if !self.quest_ready(id, q, &a) {
            return false;
        }
        self.quest_finish(id, qid, nid);
        true
    }

    /// HOOK (integrator, optional): the player clicked on the NPC `nid` (any NPC, not only
    /// quest ones): counts the "visit" goals of his quests (`GameLogic.LookatTile`). The
    /// interact system should call it before dispatching on the NPC type; `quest_npc` calls
    /// it by itself for quest NPCs, and a repeated call is harmless (the count stops at the goal).
    pub(crate) fn quest_on_npc_target(&mut self, id: u32, nid: u32) {
        let Some(p) = self.player(id) else { return };
        let Some(number) = self.maps.get(&p.map).and_then(|m| m.npcs.get(&nid)).map(|n| n.def as i32) else { return };
        let data = self.quest_data();
        let mut reached = Vec::new();
        let p = self.player_mut(id);
        for slot in p.quest.slots.iter_mut().flatten() {
            let Some(q) = data.quest(slot.quest as u32) else { continue };
            if q.required_target_npcs.is_empty() {
                continue;
            }
            if !quest::register_target(q, &mut slot.targeted, number).is_empty() {
                reached.push(slot.quest);
            }
        }
        for qid in reached {
            // `FinishQuest` right away, even if other goals are open (it says what is missing).
            if self.quest_finish(id, qid, nid) {
                self.quest_say(id, nid, "¡Quest Finalizada!");
            }
        }
    }

    /// HOOK (combat): `killer` killed the NPC `npc_number` (npcs.dat) on `map`.
    /// `OnNpcKilledUpdateQuest`.
    pub(crate) fn quest_on_npc_killed(&mut self, killer: u32, npc_number: u16, map: u16) {
        let _ = map;
        let name = self.ao.npcs.get(npc_number as u32).map(|n| n.name.clone()).unwrap_or_default();
        self.quest_register_kill(killer, npc_number as i32, &name);
        self.quest_drop_objects(killer, npc_number);
    }

    /// HOOK (combat): `killer` killed the player `victim` (TSAO "kill N users" quests).
    /// Fixes over TSAO `RestarUser`: the same victim twice in a row, newbies and arena
    /// duels do not count (alt farming), and finishing does not depend on `=` hitting the
    /// exact number.
    pub(crate) fn quest_on_player_killed(&mut self, killer: u32, victim: u32) {
        let (Some(k), Some(v)) = (self.player(killer), self.player(victim)) else { return };
        if killer == victim || v.level <= self.data.balance.newbie_max_level || k.quest.slots.iter().all(Option::is_none) {
            return;
        }
        let m = &self.instance(v.map).map;
        if m.trigger(v.x, v.y) == 6 && m.trigger(k.x, k.y) == 6 {
            return;
        }
        let vname = v.name.clone();
        if self.quest_last_victim.insert(killer, vname.clone()).as_deref() == Some(vname.as_str()) {
            return self.console(killer, "Matar al mismo jugador otra vez no cuenta para la misión.", Tone::Info);
        }
        self.quest_register_kill(killer, PLAYER_KILLS, "jugadores");
    }

    fn quest_register_kill(&mut self, killer: u32, npc_number: i32, name: &str) {
        let Some(p) = self.player(killer) else { return };
        if p.quest.slots.iter().all(Option::is_none) {
            return;
        }
        let data = self.quest_data();
        let mut lines = Vec::new();
        let mut done = Vec::new();
        let p = self.player_mut(killer);
        for (i, slot) in p.quest.slots.iter_mut().enumerate() {
            let Some(a) = slot else { continue };
            let Some(q) = data.quest(a.quest as u32) else { continue };
            let steps = quest::register_kill(q, &mut a.killed, npc_number);
            for s in &steps {
                lines.push((i as u8 + 1, *s));
            }
            if !steps.is_empty() && quest::all_killed(q, &a.killed) {
                done.push(q.name.clone());
            }
        }
        for (slot, s) in lines {
            self.console(killer, format!("{}/{} {}", s.killed, s.required, name), Tone::Info);
            self.send(killer, &S2C::Quest(QuestS2C::Progress { slot, index: s.index as u8, killed: s.killed, required: s.required }));
        }
        for q in done {
            self.console(killer, format!("Ya has matado todas las criaturas que la misión {q} requería."), Tone::Good);
        }
    }

    /// `NpcDropQuestObj` (Modulo_InventANDobj.bas): quest objects that creatures carry
    /// (`DropQuestN=quest-obj-amount-probability` in npcs.dat) go straight to the killer's
    /// bag, 1 in `probability`, and only while his quest in progress still lacks that object
    /// (`FaltanItemsQuest`).
    fn quest_drop_objects(&mut self, killer: u32, npc_number: u16) {
        let ao = self.ao.clone();
        let Some(def) = ao.npcs.get(npc_number as u32) else { return };
        let data = self.quest_data();
        for d in &def.quest_drops {
            if d.quest <= 0 || d.obj <= 0 {
                continue;
            }
            let Some(p) = self.player(killer) else { return };
            if !p.quest.is_active(d.quest as u16) {
                continue;
            }
            let lacks = data.quest(d.quest as u32).is_some_and(|q| q.required_objs.iter().find(|o| o.obj == d.obj).is_some_and(|o| (count_obj(&p.inv, d.obj) as i64) < o.amount as i64));
            if lacks && self.rng.range(1, d.probability.max(1)) == 1 && d.amount > 0 && ao.objs.get(d.obj as u32).is_some() {
                self.add_item(killer, InvItem { obj: u16c(d.obj), amount: d.amount as u32 });
            }
        }
    }

    /// Called when the player enters the world (after everything else is sent).
    pub(crate) fn quest_on_enter(&mut self, id: u32) {
        let Some(char_id) = self.player(id).and_then(|p| p.char_id) else {
            // Development login: nothing is saved, the piece stays empty.
            self.send_quest_list(id);
            return;
        };
        let save = match self.db.load_quests(char_id) {
            Ok(s) => s,
            Err(e) => {
                warn!(?e, "no se pudieron cargar las misiones");
                Default::default()
            }
        };
        let piece = QuestPiece::from_save(save, &self.quest_data());
        self.player_mut(id).quest = piece;
        self.send_quest_list(id);
        let done = self.player(id).unwrap().quest.done.clone();
        self.send(id, &S2C::Quest(QuestS2C::Done { quests: done }));
    }

    /// Called right before the player leaves the world (logout or kick), still present.
    pub(crate) fn quest_on_leave(&mut self, id: u32) {
        self.quest_save(id);
    }

    // ------------------------------------------------------------------ hand in

    /// `FinishQuest`: checks everything, then takes what the quest asked for and gives the
    /// rewards, all or nothing. Returns true when the quest was completed.
    fn quest_finish(&mut self, id: u32, qid: u16, nid: u32) -> bool {
        let data = self.quest_data();
        let Some(q) = data.quest(qid as u32) else { return false };
        let Some(p) = self.player(id) else { return false };
        let Some(slot) = p.quest.slot_of(qid) else { return false };
        if p.hp <= 0 {
            self.console(id, DEAD, Tone::Warn);
            return false;
        }
        let a = p.quest.slots[slot].clone().unwrap();
        if !quest::class_allowed(q, p.class) {
            self.console(id, "Esta misión no está disponible para tu clase.", Tone::Warn);
            return false;
        }
        // FinishGlobalQuestCheck (ModGlobalQuests.bas).
        if q.global_quest_index > 0 && !self.gquest_finish_check(id, q, true) {
            return false;
        }
        if let Some(u) = self.quest_unmet(id, q, &a) {
            let text = match u {
                Unmet::Objects => "No has conseguido todos los objetos que te he pedido.".to_string(),
                Unmet::Kills => "No has matado todas las criaturas que te he pedido.".to_string(),
                Unmet::Spell(s) => format!("Necesitas aprender el hechizo: {}.", self.ao.spells.get(s.max(0) as u32).map_or("?", |d| d.name.as_str())),
                Unmet::Targets => "No has visitado al npc que te pedí.".to_string(),
                Unmet::Skill(s) => format!("Necesitas más skill de {}.", Skill::from_id(s).and_then(Skill::spanish_name).unwrap_or("?")),
            };
            self.quest_say(id, nid, text);
            return false;
        }
        // Room for the reward objects: one free slot per object (VB counts empty slots).
        let free = (1..=p.inv.usable_slots()).filter(|s| p.inv.get(*s).is_none()).count();
        if free < q.reward_objs.len() {
            self.quest_say(id, nid, "No tienes suficiente espacio en el inventario para recibir la recompensa. Vuelve cuando hayas hecho mas espacio.");
            return false;
        }
        // A reward made only of spells the player already knows is refused (KnownSkills).
        if !q.reward_spells.is_empty() && q.reward_spells.iter().all(|s| p.spells.knows(u16c(*s))) {
            self.quest_say(id, nid, "Ya conoces los hechizos que te ofrezco como recompensa.");
            return false;
        }

        // ---- everything is in order: take, then give.
        for o in &q.required_objs {
            // FinishGlobalQuest: every object handed in counts for the global quest.
            self.gquest_contribute(id, q, o.amount.max(0) as i64);
            self.quest_take_obj(id, o.obj, o.amount.max(0) as u32);
        }
        if !q.required_spells.is_empty() {
            let spells = &mut self.player_mut(id).spells;
            for s in &q.required_spells {
                for slot in spells.slots.iter_mut().filter(|x| **x == u16c(*s)) {
                    *slot = 0;
                }
            }
            self.send_spellbook(id);
        }
        // The quest is done now; paying comes after so nothing can be claimed twice.
        {
            let piece = &mut self.player_mut(id).quest;
            piece.remove(slot);
            piece.add_done(qid);
        }
        let exp = quest::exp_reward(q.reward_exp, self.cfg.world.exp_mult);
        if exp > 0 {
            if self.player(id).unwrap().level < self.ao.balance.max_level() {
                self.give_exp(id, exp);
                self.console(id, format!("Has ganado {exp} puntos de experiencia."), Tone::Good);
            } else {
                self.console(id, "No se te ha dado experiencia porque eres nivel máximo.", Tone::Warn);
            }
        }
        match quest::gold_reward(q.reward_gold, self.cfg.world.gold_mult) {
            Some(GoldReward::Wallet(g)) => {
                let p = self.player_mut(id);
                p.inv.gold = p.inv.gold.saturating_add(g);
                self.send_inventory_vitals(id);
                self.console(id, format!("Has ganado {g} monedas de oro como recompensa."), Tone::Good);
            }
            Some(GoldReward::Bank(g)) => {
                let p = self.player_mut(id);
                p.bank.gold = p.bank.gold.saturating_add(g);
                self.console(id, format!("Has ganado {g} monedas de oro como recompensa. La recompensa ha sido depositada en su cuenta del Banco Goliath."), Tone::Good);
            }
            None => {}
        }
        self.tsao_quest_reward(id, q);
        for r in &q.reward_objs {
            let Some(obj) = self.ao.objs.get(r.obj.max(0) as u32).map(|o| o.name.clone()) else { continue };
            if r.amount > 0 && self.add_item(id, InvItem { obj: u16c(r.obj), amount: r.amount as u32 }) {
                self.console(id, format!("Has recibido {} {} como recompensa.", r.amount, obj), Tone::Good);
            }
        }
        let mut learned = false;
        for s in &q.reward_spells {
            let spell = u16c(*s);
            if self.ao.spells.get(spell as u32).is_none() || self.player(id).unwrap().spells.knows(spell) {
                continue;
            }
            if self.player_mut(id).spells.add(spell).is_some() {
                learned = true;
            } else {
                self.console(id, "No tenes espacio para mas hechizos.", Tone::Warn);
            }
        }
        if learned {
            self.send_spellbook(id);
        }

        self.quest_say(id, nid, q.desc_final.clone());
        self.send(id, &S2C::Quest(QuestS2C::Completed { quest: qid }));
        self.send_quest_list(id);
        if let Some(number) = self.maps.get(&self.player(id).unwrap().map).and_then(|m| m.npcs.get(&nid)).map(|n| n.def as u32) {
            self.send_quest_symbol(id, nid, number);
        }
        self.quest_save(id);
        true
    }

    /// `QuitarObjetos`: removes `amount` of `obj` from the inventory, slot by slot.
    fn quest_take_obj(&mut self, id: u32, obj: i32, mut amount: u32) {
        let slots: Vec<(u8, u32)> = {
            let inv = &self.player(id).unwrap().inv;
            (1..=inv.slots.len() as u8).filter_map(|s| inv.get(s).filter(|i| i.obj as i32 == obj).map(|i| (s, i.amount))).collect()
        };
        for (slot, have) in slots {
            if amount == 0 {
                break;
            }
            let take = have.min(amount);
            self.remove_from_slot(id, slot, take);
            amount -= take;
        }
    }
}

// The whole flow against the official quests, with kills fed straight to the hook (the real
// hunt is in tests/p2_quest.rs). The world is built directly, like the vault tests.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{CharacterRecord, CharacterState};
    use crate::world::Session;
    use crate::Settings;
    use ao_protocol::{FrameReader, MAX_SERVER_FRAME};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    const ME: u32 = 1;
    const QNPC: u32 = 200_000;

    struct Rig {
        w: World,
        rx: UnboundedReceiver<Vec<u8>>,
        log: Vec<S2C>,
    }

    fn drain(rx: &mut UnboundedReceiver<Vec<u8>>) -> Vec<S2C> {
        let mut r = FrameReader::default();
        while let Ok(b) = rx.try_recv() {
            r.push(&b);
        }
        let mut out = Vec::new();
        while let Some(m) = r.next::<S2C>(MAX_SERVER_FRAME) {
            out.push(m.expect("frame"));
        }
        out
    }

    /// A level-1 mage next to the quest NPC number `npc` on map 1.
    fn rig(npc: u16) -> Rig {
        let mut w = World::new(Settings::defaults());
        w.cfg.security.kick_strikes = 2;
        assert!(w.ensure_map(1));
        let n = w.make_npc(npc, 50, 50).expect("quest npc in npcs.dat");
        w.maps.get_mut(&1).unwrap().add_npc(QNPC, n);
        let (tx, mut rx) = unbounded_channel();
        w.sessions.insert(ME, Session::new(tx, "127.0.0.1:1".parse().unwrap()));
        let rec = CharacterRecord { id: 0, account_id: 0, name: "Cliente".into(), race: 1, class: 1, gender: 1, body: 1, head: 1, level: 1, exp: 0, hp: 15, max_hp: 15, mana: 0, max_mana: 0, map: 1, x: 51, y: 50 };
        assert!(w.spawn_player(ME, &rec, &CharacterState::default(), None));
        let p = w.player_mut(ME);
        (p.x, p.y) = (51, 50);
        drain(&mut rx);
        Rig { w, rx, log: Vec::new() }
    }

    impl Rig {
        fn msgs(&mut self) -> Vec<S2C> {
            let m = drain(&mut self.rx);
            self.log.extend(m.iter().cloned());
            m
        }
        fn talk(&mut self) -> Vec<S2C> {
            self.w.quest_npc(ME, QNPC);
            self.msgs()
        }
        fn accept(&mut self, q: u16) -> Vec<S2C> {
            self.w.on_quest(ME, QuestC2S::Accept { quest: q });
            self.msgs()
        }
        fn kill(&mut self, npc: u16, times: usize) {
            for _ in 0..times {
                self.w.quest_on_npc_killed(ME, npc, 1);
            }
            self.msgs();
        }
        fn piece(&self) -> &QuestPiece {
            &self.w.player(ME).unwrap().quest
        }
        fn level(&mut self, l: u32) {
            self.w.player_mut(ME).level = l;
        }
        fn give(&mut self, slot: u8, obj: u16, amount: u32) {
            self.w.player_mut(ME).inv.slots[slot as usize - 1] = Some(InvItem { obj, amount });
        }
        fn count(&self, obj: i32) -> u32 {
            count_obj(&self.w.player(ME).unwrap().inv, obj)
        }
    }

    fn texts(m: &[S2C]) -> String {
        m.iter()
            .filter_map(|m| match m {
                S2C::Console { text, .. } | S2C::NpcSay { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn completed(m: &[S2C]) -> Option<u16> {
        m.iter().find_map(|m| if let S2C::Quest(QuestS2C::Completed { quest }) = m { Some(*quest) } else { None })
    }

    fn offer(m: &[S2C]) -> Option<Vec<(u16, u8)>> {
        m.iter().find_map(|m| if let S2C::Quest(QuestS2C::Offer { quests, .. }) = m { Some(quests.iter().map(|q| (q.info.id, q.state)).collect()) } else { None })
    }

    /// Makrom (NPC 407): quest 407 is "kill 15 snakes, 10 rats, 5 Collet snakes, 5 red
    /// ants"; reward 300 exp and 70 gold; 408 needs 407 and level 3.
    #[test]
    fn quest_407_from_offer_to_reward_paid_once() {
        let mut r = rig(407);
        let m = r.talk();
        assert_eq!(offer(&m), Some(vec![(407, 0), (408, 3), (409, 3), (410, 3), (411, 3), (412, 3)]), "only the first is open to a level 1");
        assert_eq!(r.w.player(ME).unwrap().quest.target, Some((1, QNPC)));

        // Quest 408 first: refused (needs 407 and level 3), and 407 is taken.
        let m = r.accept(408);
        assert!(texts(&m).contains("Debes completar la misión Una nueva aventura"), "{}", texts(&m));
        assert!(r.piece().active_ids().is_empty());
        let m = r.accept(407);
        assert!(texts(&m).contains("Has aceptado la misión Una nueva aventura"));
        assert_eq!(r.piece().active_ids(), vec![407]);
        assert!(m.iter().any(|m| matches!(m, S2C::Quest(QuestS2C::Details { slot: 1, killed, .. }) if killed == &vec![0, 0, 0, 0])));
        // Taken twice.
        let m = r.accept(407);
        assert!(texts(&m).contains("Ya tienes esta misión en curso"));
        assert_eq!(r.piece().active_ids(), vec![407]);

        // Kills of other creatures do not count; the right ones count up to the goal.
        r.kill(500, 3);
        assert_eq!(r.piece().slots[0].as_ref().unwrap().killed, vec![0, 0, 0, 0]);
        r.w.quest_on_npc_killed(ME, 502, 1);
        let m = r.msgs();
        assert!(m.iter().any(|m| matches!(m, S2C::Quest(QuestS2C::Progress { slot: 1, index: 0, killed: 1, required: 15 }))));
        assert!(texts(&m).contains("1/15 Serpiente"), "{}", texts(&m));
        r.kill(502, 14);
        r.kill(511, 10);
        r.kill(935, 5);
        r.kill(508, 4);
        assert_eq!(r.piece().slots[0].as_ref().unwrap().killed, vec![15, 10, 5, 4]);

        // Not done: the NPC just lists its quests again, nothing is paid.
        let m = r.talk();
        assert_eq!(completed(&m), None);
        assert!(offer(&m).is_some());
        assert_eq!((r.w.player(ME).unwrap().exp, r.w.player(ME).unwrap().inv.gold), (0, 0));

        r.kill(508, 1);
        r.kill(508, 3); // beyond the goal: counters never overflow
        assert_eq!(r.piece().slots[0].as_ref().unwrap().killed, vec![15, 10, 5, 5]);
        let m = r.talk();
        assert_eq!(completed(&m), Some(407), "{}", texts(&m));
        assert_eq!((r.w.player(ME).unwrap().exp, r.w.player(ME).unwrap().inv.gold), (300, 70));
        assert!(r.piece().active_ids().is_empty() && r.piece().has_done(407));

        // Paid once: it cannot be taken or handed in again.
        let m = r.accept(407);
        assert!(texts(&m).contains("Ya has completado esta misión"), "{}", texts(&m));
        let m = r.talk();
        assert_eq!(completed(&m), None);
        assert_eq!(offer(&m).unwrap()[0], (407, 2));
        assert_eq!((r.w.player(ME).unwrap().exp, r.w.player(ME).unwrap().inv.gold), (300, 70));

        // The next quest opens with the level.
        let m = r.accept(408);
        assert!(texts(&m).contains("Debes ser nivel 3"), "{}", texts(&m));
        r.level(3);
        let m = r.accept(408);
        assert!(texts(&m).contains("Has aceptado la misión Tierra de depredadores"), "{}", texts(&m));
    }

    #[test]
    fn five_active_quests_at_most_and_abandon_packs_the_slots() {
        let mut r = rig(407);
        r.talk();
        let q = r.w.quest_data();
        let piece = &mut r.w.player_mut(ME).quest;
        for (i, id) in [100u16, 101, 102, 103, 104].iter().enumerate() {
            piece.slots[i] = Some(ActiveQuest::new(*id, q.quest(*id as u32).unwrap()));
        }
        let m = r.accept(407);
        assert!(texts(&m).contains("Debes completar las misiones en curso"), "{}", texts(&m));
        assert_eq!(r.piece().active_ids(), vec![100, 101, 102, 103, 104]);
        // Abandon the second one: the rest move up, and there is room again.
        r.w.on_quest(ME, QuestC2S::Abandon { slot: 2 });
        let m = r.msgs();
        assert!(texts(&m).contains("Has abandonado la misión"));
        assert!(m.iter().any(|m| matches!(m, S2C::Quest(QuestS2C::List { active }) if active == &vec![100, 102, 103, 104])));
        assert_eq!(r.piece().active_ids(), vec![100, 102, 103, 104]);
        r.accept(407);
        assert_eq!(r.piece().active_ids(), vec![100, 102, 103, 104, 407]);
        // An empty slot is ignored, a slot that cannot exist is a cheat.
        r.w.on_quest(ME, QuestC2S::Abandon { slot: 5 });
        r.w.on_quest(ME, QuestC2S::Abandon { slot: 5 });
        assert_eq!(r.piece().active_ids(), vec![100, 102, 103, 104]);
        assert!(r.w.player(ME).is_some(), "an empty slot is not punished");
        r.w.on_quest(ME, QuestC2S::Abandon { slot: 9 });
        assert!(r.w.player(ME).is_some(), "the first strike only counts");
        r.w.on_quest(ME, QuestC2S::Abandon { slot: 0 });
        assert!(r.w.player(ME).is_none(), "the second one kicks (kick_strikes = 2)");
    }

    /// Quest 326 (Costhas, NPC 111): workers only, levels 1 to 14, hand in 420 + 220 + 110 +
    /// 85 + 8 + 1 of six objects.
    #[test]
    fn hand_in_takes_exactly_the_objects_and_only_with_all_of_them() {
        let mut r = rig(111);
        r.talk();
        let m = r.accept(326);
        assert!(texts(&m).contains("no está disponible para tu clase"), "{}", texts(&m));
        r.w.player_mut(ME).class = 9;
        r.accept(326);
        assert_eq!(r.piece().active_ids(), vec![326]);

        let need = [(2089u16, 420u32), (2101, 220), (2093, 110), (2155, 85), (3321, 8), (2152, 1)];
        // Missing objects (and one short of the last stack) hand in nothing.
        for (i, (obj, n)) in need.iter().enumerate() {
            r.give(i as u8 + 1, *obj, if i == 5 { n - 1 } else { *n });
        }
        r.w.player_mut(ME).inv.slots[5] = None;
        let m = r.talk();
        assert_eq!(completed(&m), None);
        assert_eq!(r.count(2089), 420, "nothing was taken");
        // All of them, with 5 spare of the first.
        r.give(1, 2089, 425);
        r.give(6, 2152, 1);
        let before = r.w.player(ME).unwrap().level;
        let m = r.talk();
        assert_eq!(completed(&m), Some(326), "{}", texts(&m));
        assert_eq!(r.count(2089), 5, "only the 420 asked for");
        assert_eq!((r.count(2101), r.count(2093), r.count(2155), r.count(3321), r.count(2152)), (0, 0, 0, 0, 0));
        assert!(r.w.player(ME).unwrap().level > before, "4950 experience");
        // Repeatable: it can be taken again, and with nothing in the bag it pays nothing.
        r.level(2);
        r.accept(326);
        assert_eq!(r.piece().active_ids(), vec![326]);
        let exp = r.w.player(ME).unwrap().exp;
        let m = r.talk();
        assert_eq!((completed(&m), r.w.player(ME).unwrap().exp), (None, exp));
    }

    /// Quest 38 (Heather, NPC 415): nothing to do, 250 gold and eight objects. A full bag
    /// refuses and loses nothing.
    #[test]
    fn a_full_inventory_does_not_get_the_reward_until_there_is_room() {
        let mut r = rig(415);
        r.level(13);
        r.talk();
        r.accept(38);
        assert_eq!(r.piece().active_ids(), vec![38]);
        for s in 1..=24u8 {
            r.give(s, 38, 1); // 24 slots in use
        }
        let m = r.talk();
        assert_eq!(completed(&m), None);
        assert!(texts(&m).contains("espacio en el inventario"), "{}", texts(&m));
        assert_eq!(r.piece().active_ids(), vec![38], "the quest is still there");
        assert_eq!(r.w.player(ME).unwrap().inv.gold, 0);
        for s in 1..=8u8 {
            r.w.player_mut(ME).inv.slots[s as usize - 1] = None;
        }
        let m = r.talk();
        assert_eq!(completed(&m), Some(38), "{}", texts(&m));
        assert_eq!(r.w.player(ME).unwrap().inv.gold, 250);
        assert_eq!(r.count(1981), 2, "reward object 1981-2");
        assert!(r.piece().has_done(38));
        // The reward is not paid twice.
        let m = r.talk();
        assert_eq!(completed(&m), None);
        assert_eq!(r.w.player(ME).unwrap().inv.gold, 250);
    }

    /// Quest 290 (Eldren, NPC 1021): know spell 5, kill 15 of creature 1361; the spell is
    /// taken away and spell 348 given.
    #[test]
    fn spells_are_required_taken_and_rewarded() {
        let mut r = rig(1021);
        r.level(12);
        r.talk();
        let m = r.accept(290);
        assert!(texts(&m).contains("Has aceptado"), "{}", texts(&m));
        r.kill(1361, 15);
        // Without the spell it is not ready.
        let m = r.talk();
        assert_eq!(completed(&m), None);
        assert!(r.w.player_mut(ME).spells.add(5).is_some());
        assert!(!r.w.player(ME).unwrap().spells.knows(348));
        let m = r.talk();
        assert_eq!(completed(&m), Some(290), "{}", texts(&m));
        let sp = &r.w.player(ME).unwrap().spells;
        assert!(!sp.knows(5) && sp.knows(348), "required spell removed, reward spell learned");
        assert!(m.iter().any(|m| matches!(m, S2C::SpellSlots { slots } if slots.contains(&348))));
    }

    /// Cheats: a quest the NPC does not offer, accepting without a quest NPC, a dead client.
    #[test]
    fn cheats_are_refused() {
        let mut r = rig(407);
        // Never talked to the NPC: nothing happens, nothing is punished (a late packet).
        let m = r.accept(407);
        assert!(m.is_empty() && r.piece().active_ids().is_empty());
        r.talk();
        // Too far from the NPC.
        r.w.player_mut(ME).x = 60;
        let m = r.accept(407);
        assert!(texts(&m).contains("demasiado lejos"));
        assert!(r.piece().active_ids().is_empty());
        r.w.player_mut(ME).x = 51;
        // Dead.
        r.w.player_mut(ME).hp = 0;
        let m = r.accept(407);
        assert!(texts(&m).contains("muerto"));
        assert!(r.piece().active_ids().is_empty());
        r.w.player_mut(ME).hp = 15;
        // The NPC is gone (killed): the target is forgotten.
        r.w.maps.get_mut(&1).unwrap().npcs.get_mut(&QNPC).unwrap().dead_until = Some(r.w.now());
        r.accept(407);
        assert!(r.piece().active_ids().is_empty() && r.piece().target.is_none());
        // Another NPC's quest (and one that does not exist) while talking to a live NPC:
        // counted as a cheat, the second one kicks (kick_strikes = 2).
        r.w.maps.get_mut(&1).unwrap().npcs.get_mut(&QNPC).unwrap().dead_until = None;
        r.talk();
        r.accept(2);
        assert!(r.w.player(ME).is_some() && r.piece().active_ids().is_empty());
        r.accept(9999);
        assert!(r.w.player(ME).is_none());
    }

    /// `LookatTile`: clicking the NPC counts as a visit; quest 5 (visit NPC 119) is handed in
    /// the moment the count is reached, with everything else already done.
    #[test]
    fn clicking_the_target_npc_counts_the_visit() {
        let mut r = rig(119);
        r.level(15);
        let q = r.w.quest_data();
        let mut a = ActiveQuest::new(5, q.quest(5).unwrap());
        a.killed = vec![100];
        r.w.player_mut(ME).quest.slots[0] = Some(a);
        r.give(1, 34, 1);
        r.give(2, 5066, 1);
        r.w.quest_on_npc_target(ME, QNPC);
        let m = r.msgs();
        assert_eq!(completed(&m), Some(5), "{}", texts(&m));
        assert_eq!(r.count(34) + r.count(5066), 0, "its two objects were handed in");
    }

    /// Quest 142 (Kleric, NPC 112): the creature Morvanna carries the quest object 2502 (1 in
    /// 50) and only while the quest in progress lacks it; nobody without the quest gets it.
    #[test]
    fn creatures_drop_the_quest_object_only_to_who_needs_it() {
        let mut r = rig(112);
        r.level(26);
        r.w.player_mut(ME).class = 2;
        // Without the quest: kills drop nothing.
        r.kill(513, 400);
        assert_eq!(r.count(2502), 0);
        r.talk();
        let m = r.accept(142);
        assert!(texts(&m).contains("Has aceptado"), "{}", texts(&m));
        let mut kills = 0;
        while r.count(2502) == 0 && kills < 5000 {
            r.kill(513, 1);
            kills += 1;
        }
        assert_eq!(r.count(2502), 1, "dropped after {kills} kills");
        // It needs only one: more kills count but drop nothing.
        r.kill(513, 600);
        assert_eq!(r.count(2502), 1);
        assert_eq!(r.piece().slots[0].as_ref().unwrap().killed, vec![125], "the counter stops at the goal");
    }

    #[test]
    fn symbol_follows_the_quest_state() {
        let mut r = rig(407);
        let sym = |m: &[S2C]| m.iter().find_map(|m| if let S2C::Quest(QuestS2C::Symbol { symbol, .. }) = m { Some(*symbol) } else { None });
        assert_eq!(sym(&r.talk()), Some(1), "a quest for me");
        assert_eq!(sym(&r.accept(407)), Some(4), "in progress");
        r.kill(502, 15);
        r.kill(511, 10);
        r.kill(935, 5);
        r.kill(508, 5);
        r.w.quest_npc(ME, QNPC);
        let m = r.msgs();
        assert_eq!(completed(&m), Some(407));
        assert_eq!(sym(&m), Some(2), "nothing for a level 1 now");
    }

    #[test]
    fn saved_quests_survive_a_relog_and_drop_what_no_longer_exists() {
        let w = World::new(Settings::defaults());
        let acc = w.db.create_account("ana", "secreta1").unwrap();
        let c = w.db.create_character(acc.id, &crate::db::NewCharacter { name: "Ana", race: 1, class: 3, gender: 1, body: 1, head: 1, hp: 100, mana: 0, map: 1, x: 50, y: 50 }).unwrap();
        let q = w.quest_data();
        let mut a = ActiveQuest::new(407, q.quest(407).unwrap());
        a.killed = vec![3, 2, 1, 0];
        let save = crate::world::quest::QuestSave { active: vec![(1, a.clone()), (2, ActiveQuest { quest: 9999, killed: vec![], targeted: vec![] })], done: vec![2, 9999] };
        w.db.save_quests(c.id, &save).unwrap();
        let loaded = QuestPiece::from_save(w.db.load_quests(c.id).unwrap(), &q);
        assert_eq!(loaded.slots[0], Some(a));
        assert_eq!(loaded.active_ids(), vec![407], "a quest missing from Quests.DAT is dropped");
        assert_eq!(loaded.done, vec![2]);
    }
}

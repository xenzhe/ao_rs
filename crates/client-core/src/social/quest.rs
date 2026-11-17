//! What the client knows about quest (filled from `S2C::Quest` only). OWNER: Sonnet 1.
use ao_protocol::social::quest::{OfferedQuest, QuestInfo};
use ao_protocol::QuestS2C;
use std::collections::HashMap;

/// A quest in progress, as `QuestDetails` last described it.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveQuest {
    pub info: QuestInfo,
    pub killed: Vec<u16>,
    pub have: Vec<u16>,
    pub known: Vec<bool>,
    pub targeted: Vec<u16>,
}

#[derive(Debug, Default)]
pub struct QuestState {
    /// The quest NPC I am talking to (entity id) and what it offers; `None` when none.
    pub offer: Option<(u32, Vec<OfferedQuest>)>,
    /// My quests in progress, slot 1 first.
    pub active: Vec<u16>,
    /// Details I asked for, by slot (1-based).
    pub details: HashMap<u8, ActiveQuest>,
    /// Quests I have completed (all of them after entering, then each new one).
    pub done: Vec<u16>,
    /// NPC entity id -> symbol over its head (1 available, 2 none, 3 ready, 4 in progress).
    pub symbols: HashMap<u32, u8>,
    /// Last `Completed` received, for the UI to celebrate.
    pub last_completed: Option<u16>,
}

impl QuestState {
    pub fn apply(&mut self, msg: &QuestS2C) {
        match msg {
            QuestS2C::Offer { npc, quests } => self.offer = Some((*npc, quests.clone())),
            QuestS2C::List { active } => {
                self.active = active.clone();
                // Slots shift when one is dropped: keep only details that still match.
                let active = &self.active;
                self.details.retain(|slot, d| active.get((*slot as usize).wrapping_sub(1)).is_some_and(|q| *q == d.info.id));
            }
            QuestS2C::Details { slot, info, killed, have, known, targeted } => {
                self.details.insert(*slot, ActiveQuest { info: (**info).clone(), killed: killed.clone(), have: have.clone(), known: known.clone(), targeted: targeted.clone() });
            }
            QuestS2C::Progress { slot, index, killed, .. } => {
                if let Some(c) = self.details.get_mut(slot).and_then(|d| d.killed.get_mut(*index as usize)) {
                    *c = *killed;
                }
            }
            QuestS2C::Done { quests } => self.done = quests.clone(),
            QuestS2C::Completed { quest } => {
                if !self.done.contains(quest) {
                    self.done.push(*quest);
                }
                self.last_completed = Some(*quest);
                self.offer = None;
            }
            QuestS2C::Symbol { npc, symbol } => {
                self.symbols.insert(*npc, *symbol);
            }
        }
    }

    /// The quest in `slot` (1-based), if its details are known.
    pub fn quest_in(&self, slot: u8) -> Option<&ActiveQuest> {
        self.details.get(&slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: u16) -> QuestInfo {
        QuestInfo { id, name: format!("Q{id}"), kill: vec![(500, 3)], ..QuestInfo::default() }
    }

    #[test]
    fn follows_the_server_messages() {
        let mut s = QuestState::default();
        s.apply(&QuestS2C::List { active: vec![7, 9] });
        s.apply(&QuestS2C::Details { slot: 2, info: Box::new(info(9)), killed: vec![0], have: vec![], known: vec![], targeted: vec![] });
        s.apply(&QuestS2C::Progress { slot: 2, index: 0, killed: 2, required: 3 });
        assert_eq!(s.quest_in(2).unwrap().killed, vec![2]);
        // Dropping slot 1 shifts slot 2 down: the old details no longer match any slot.
        s.apply(&QuestS2C::List { active: vec![9] });
        assert!(s.quest_in(2).is_none());
        s.apply(&QuestS2C::Offer { npc: 100_001, quests: vec![OfferedQuest { info: info(7), state: 0 }] });
        assert_eq!(s.offer.as_ref().unwrap().1.len(), 1);
        s.apply(&QuestS2C::Completed { quest: 9 });
        assert_eq!((s.done.clone(), s.last_completed, s.offer.is_none()), (vec![9], Some(9), true));
        s.apply(&QuestS2C::Symbol { npc: 100_001, symbol: 3 });
        assert_eq!(s.symbols[&100_001], 3);
    }
}

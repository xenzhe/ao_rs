//! P2 persistence of quests (`impl Db`). OWNER: Sonnet 1 (docs/P2_CONTRATO.md).
//!
//! Like the official `quest` / `quest_done` tables (CharacterPersistence.bas): the quests in
//! progress with one counter per goal, and the quests completed.
use super::{Db, DbResult};
use crate::world::quest::{ActiveQuest, QuestSave};
use rusqlite::params;

fn csv(v: &[u16]) -> String {
    v.iter().map(u16::to_string).collect::<Vec<_>>().join(",")
}

fn parse_csv(s: &str) -> Vec<u16> {
    s.split(',').filter(|p| !p.trim().is_empty()).map(|p| p.trim().parse().unwrap_or(0)).collect()
}

impl Db {
    /// The saved quests of a character (empty for a character that never took one).
    pub fn load_quests(&self, char_id: i64) -> DbResult<QuestSave> {
        let mut save = QuestSave::default();
        let mut st = self.conn.prepare("SELECT slot, quest, killed, targeted FROM quest_active WHERE character_id = ?1 ORDER BY slot")?;
        let rows = st.query_map(params![char_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        })?;
        for row in rows {
            let (slot, quest, killed, targeted) = row?;
            let (Ok(slot), Ok(quest)) = (u8::try_from(slot), u16::try_from(quest)) else { continue };
            save.active.push((slot, ActiveQuest { quest, killed: parse_csv(&killed), targeted: parse_csv(&targeted) }));
        }
        let mut st = self.conn.prepare("SELECT quest FROM quest_done WHERE character_id = ?1 ORDER BY quest")?;
        let rows = st.query_map(params![char_id], |r| r.get::<_, i64>(0))?;
        for row in rows {
            if let Ok(q) = u16::try_from(row?) {
                save.done.push(q);
            }
        }
        Ok(save)
    }

    /// Replaces everything saved for the character, in one transaction.
    pub fn save_quests(&self, char_id: i64, save: &QuestSave) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM quest_active WHERE character_id = ?1", params![char_id])?;
        tx.execute("DELETE FROM quest_done WHERE character_id = ?1", params![char_id])?;
        for (slot, a) in &save.active {
            tx.execute(
                "INSERT INTO quest_active (character_id, slot, quest, killed, targeted) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![char_id, *slot as i64, a.quest as i64, csv(&a.killed), csv(&a.targeted)],
            )?;
        }
        for q in &save.done {
            tx.execute("INSERT OR IGNORE INTO quest_done (character_id, quest) VALUES (?1, ?2)", params![char_id, *q as i64])?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewCharacter;

    #[test]
    fn quests_roundtrip_and_replace() {
        let db = Db::open(":memory:").unwrap();
        let acc = db.create_account("ana", "secreta1").unwrap();
        let c = db.create_character(acc.id, &NewCharacter { name: "Ana", race: 1, class: 3, gender: 1, body: 1, head: 1, hp: 100, mana: 0, map: 1, x: 50, y: 50 }).unwrap();
        assert_eq!(db.load_quests(c.id).unwrap(), QuestSave::default());
        let save = QuestSave {
            active: vec![(1, ActiveQuest { quest: 407, killed: vec![3, 0, 5, 1], targeted: vec![] }), (2, ActiveQuest { quest: 5, killed: vec![], targeted: vec![1] })],
            done: vec![1, 2, 406],
        };
        db.save_quests(c.id, &save).unwrap();
        assert_eq!(db.load_quests(c.id).unwrap(), save);
        let smaller = QuestSave { active: vec![(1, ActiveQuest { quest: 427, killed: vec![2], targeted: vec![] })], done: vec![427] };
        db.save_quests(c.id, &smaller).unwrap();
        assert_eq!(db.load_quests(c.id).unwrap(), smaller, "a save replaces the old rows");
    }
}

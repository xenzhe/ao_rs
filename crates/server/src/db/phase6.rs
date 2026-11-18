//! Phase 6 persistence (`impl Db`): global quests (`ModGlobalQuests.bas`) and the emperor
//! castles with their whitelists (`ModCastle.bas`).
use super::{Db, DbResult};
use rusqlite::{params, OptionalExtension};

/// A castle row (`t_CastleInfo` + `t_CastleCoordinates`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CastleRow {
    pub id: i64,
    pub trigger: u16,
    pub owner_account: Option<i64>,
    pub owner_name: Option<String>,
    pub spawner_obj: u16,
    pub key_obj: u16,
    pub founded: Option<i64>,
    pub active: bool,
    pub name: Option<String>,
    pub outside: Option<(u16, i32, i32)>,
    pub inside: Option<(u16, i32, i32)>,
}

fn pos(m: Option<i64>, x: Option<i64>, y: Option<i64>) -> Option<(u16, i32, i32)> {
    Some((m? as u16, x? as i32, y? as i32))
}

impl Db {
    /// `SELECT_ALL_GLOBAL_QUEST`: (is_active, summed contributions) or None if unknown.
    pub fn global_quest(&self, id: u32) -> DbResult<Option<(bool, i64)>> {
        let active: Option<i64> = self.conn.query_row("SELECT is_active FROM global_quest_desc WHERE event_id = ?1", params![id], |r| r.get(0)).optional()?;
        let Some(active) = active else { return Ok(None) };
        let sum: Option<i64> = self.conn.query_row("SELECT SUM(amount) FROM global_quest_contribution WHERE event_id = ?1", params![id], |r| r.get(0))?;
        Ok(Some((active != 0, sum.unwrap_or(0))))
    }

    /// `INSERT_NEW_GLOBAL_QUEST_DESC`.
    pub fn global_quest_insert(&self, id: u32, name: &str, obj: u16, threshold: i64, start: i64, end: i64) -> DbResult<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO global_quest_desc (event_id, name, obj_id, threshold, start_date, end_date, is_active) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![id, name, obj, threshold, start, end],
        )?;
        Ok(())
    }

    /// `UPDATE_GLOBAL_QUEST_DESC`.
    pub fn global_quest_set_active(&self, id: u32, active: bool) -> DbResult<()> {
        self.conn.execute("UPDATE global_quest_desc SET is_active = ?1 WHERE event_id = ?2", params![active as i64, id])?;
        Ok(())
    }

    /// `INSERT_GLOBAL_QUEST_USER_CONTRIBUTION`.
    pub fn global_quest_contribute(&self, id: u32, name: &str, at: i64, amount: i64) -> DbResult<()> {
        self.conn.execute("INSERT INTO global_quest_contribution (event_id, char_name, at, amount) VALUES (?1, ?2, ?3, ?4)", params![id, name, at, amount])?;
        Ok(())
    }

    /// `GetUserGuildIndexDatabase` > 0 for a live character.
    pub fn char_in_guild(&self, name: &str) -> DbResult<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM guild_member m JOIN character c ON c.id = m.character_id WHERE c.name = ?1 AND c.deleted = 0",
            params![name],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// `ChangeNameDatabase` (+ the rows keyed by name: rank, sentence, castle owner).
    pub fn rename_character(&self, old: &str, new: &str) -> DbResult<()> {
        let n = self.conn.execute("UPDATE character SET name = ?2 WHERE name = ?1 AND deleted = 0", params![old, new])?;
        if n == 0 {
            return Err(super::DbError::NotFound);
        }
        self.conn.execute("UPDATE gm_role SET name = ?2 WHERE name = ?1", params![old, new])?;
        self.conn.execute("UPDATE gm_sentence SET name = ?2 WHERE name = ?1", params![old, new])?;
        self.conn.execute("UPDATE castle SET owner_char_name = ?2 WHERE owner_char_name = ?1", params![old, new])?;
        Ok(())
    }

    pub fn castles(&self) -> DbResult<Vec<CastleRow>> {
        let mut st = self.conn.prepare(
            "SELECT id, trigger, owner_account_id, owner_char_name, spawner_obj_id, inside_key_obj_id, foundation_date, is_active, name,
                    outside_map, outside_x, outside_y, inside_map, inside_x, inside_y FROM castle ORDER BY id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(CastleRow {
                id: r.get(0)?,
                trigger: r.get::<_, i64>(1)? as u16,
                owner_account: r.get(2)?,
                owner_name: r.get(3)?,
                spawner_obj: r.get::<_, i64>(4)? as u16,
                key_obj: r.get::<_, i64>(5)? as u16,
                founded: r.get(6)?,
                active: r.get::<_, i64>(7)? != 0,
                name: r.get(8)?,
                outside: pos(r.get(9)?, r.get(10)?, r.get(11)?),
                inside: pos(r.get(12)?, r.get(13)?, r.get(14)?),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// `UPDATE_EMPEROR_CASTLE` + `UPDATE_OUTSIDE_CASTLE_LOCATION` (saved at once, the VB
    /// waited for the world save and lost a new castle on a crash).
    pub fn castle_save(&self, c: &CastleRow) -> DbResult<()> {
        let (om, ox, oy) = c.outside.map_or((None, None, None), |(m, x, y)| (Some(m), Some(x), Some(y)));
        self.conn.execute(
            "UPDATE castle SET owner_account_id = ?1, owner_char_name = ?2, foundation_date = ?3, is_active = ?4, name = ?5, outside_map = ?6, outside_x = ?7, outside_y = ?8 WHERE id = ?9",
            params![c.owner_account, c.owner_name, c.founded, c.active as i64, c.name, om, ox, oy, c.id],
        )?;
        Ok(())
    }

    pub fn castle_whitelists(&self) -> DbResult<Vec<(i64, String)>> {
        let mut st = self.conn.prepare("SELECT castle_id, character_name FROM castle_whitelist ORDER BY castle_id, character_name")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn castle_whitelist_set(&self, castle: i64, name: &str, add: bool) -> DbResult<()> {
        if add {
            self.conn.execute("INSERT OR IGNORE INTO castle_whitelist (castle_id, character_name) VALUES (?1, ?2)", params![castle, name])?;
        } else {
            self.conn.execute("DELETE FROM castle_whitelist WHERE castle_id = ?1 AND character_name = ?2", params![castle, name])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn castles_are_seeded_and_saved() {
        let db = Db::open(":memory:").unwrap();
        let mut c = db.castles().unwrap();
        assert_eq!(c.len(), 20);
        assert_eq!((c[0].trigger, c[0].spawner_obj, c[0].inside), (21, 6362, Some((758, 50, 72))));
        assert_eq!(c[19].inside, None);
        c[0].active = true;
        c[0].outside = Some((7, 61, 43));
        c[0].owner_name = Some("Rey".into());
        db.castle_save(&c[0]).unwrap();
        assert_eq!(db.castles().unwrap()[0], c[0]);
        db.castle_whitelist_set(1, "amigo", true).unwrap();
        db.castle_whitelist_set(1, "AMIGO", true).unwrap();
        assert_eq!(db.castle_whitelists().unwrap().len(), 1);
        db.castle_whitelist_set(1, "Amigo", false).unwrap();
        assert!(db.castle_whitelists().unwrap().is_empty());
    }

    #[test]
    fn global_quest_counts_contributions() {
        let db = Db::open(":memory:").unwrap();
        assert_eq!(db.global_quest(1).unwrap(), None);
        db.global_quest_insert(1, "Navidad", 6233, 500, 0, 10).unwrap();
        db.global_quest_contribute(1, "Ana", 5, 30).unwrap();
        db.global_quest_contribute(1, "Beto", 6, 12).unwrap();
        db.global_quest_set_active(1, true).unwrap();
        assert_eq!(db.global_quest(1).unwrap(), Some((true, 42)));
    }
}

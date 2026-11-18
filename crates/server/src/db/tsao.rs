//! TSAO progression (`impl Db`), see `systems/tsao_progress.rs`.
use super::{Db, DbResult};
use rusqlite::{params, OptionalExtension};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PointsRow {
    pub tournament: u32,
    pub ts: u32,
    pub credits: u32,
    pub levels_paid: Vec<u32>,
}

impl Db {
    pub fn points_load(&self, char_id: i64) -> DbResult<PointsRow> {
        let row = self
            .conn
            .query_row("SELECT tournament, ts, credits, levels_paid FROM character_points WHERE character_id = ?1", params![char_id], |r| {
                Ok(PointsRow {
                    tournament: r.get(0)?,
                    ts: r.get(1)?,
                    credits: r.get(2)?,
                    levels_paid: r.get::<_, String>(3)?.split(',').filter_map(|l| l.trim().parse().ok()).collect(),
                })
            })
            .optional()?;
        Ok(row.unwrap_or_default())
    }

    pub fn points_save(&self, char_id: i64, p: &PointsRow) -> DbResult<()> {
        let levels = p.levels_paid.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        self.conn.execute(
            "INSERT INTO character_points (character_id, tournament, ts, credits, levels_paid) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (character_id) DO UPDATE SET tournament = ?2, ts = ?3, credits = ?4, levels_paid = ?5",
            params![char_id, p.tournament, p.ts, p.credits, levels],
        )?;
        Ok(())
    }

    /// `(tier, option)` chosen, by tier.
    pub fn bonus_load(&self, char_id: i64) -> DbResult<Vec<(u8, u8)>> {
        let mut st = self.conn.prepare("SELECT tier, option FROM class_bonus WHERE character_id = ?1 ORDER BY tier")?;
        let rows = st.query_map(params![char_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Records a choice; false if that tier was already chosen (forged or repeated message).
    pub fn bonus_choose(&self, char_id: i64, tier: u8, option: u8) -> DbResult<bool> {
        Ok(self.conn.execute("INSERT OR IGNORE INTO class_bonus (character_id, tier, option) VALUES (?1, ?2, ?3)", params![char_id, tier, option])? == 1)
    }

    pub fn account_of(&self, char_id: i64) -> DbResult<Option<i64>> {
        Ok(self.conn.query_row("SELECT account_id FROM character WHERE id = ?1", params![char_id], |r| r.get(0)).optional()?)
    }

    pub fn character_id(&self, name: &str) -> DbResult<Option<(i64, String)>> {
        Ok(self.conn.query_row("SELECT id, name FROM character WHERE name = ?1 COLLATE NOCASE AND deleted = 0", params![name], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    /// The spouse's name, if married.
    pub fn spouse(&self, char_id: i64) -> DbResult<Option<(i64, String)>> {
        Ok(self
            .conn
            .query_row("SELECT c.id, c.name FROM marriage m JOIN character c ON c.id = m.spouse_id WHERE m.character_id = ?1", params![char_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    }

    /// Marries both, in one transaction; false if either is already married.
    pub fn marry(&self, a: i64, b: i64) -> DbResult<bool> {
        let tx = self.conn.unchecked_transaction()?;
        let busy: i64 = tx.query_row("SELECT COUNT(*) FROM marriage WHERE character_id IN (?1, ?2)", params![a, b], |r| r.get(0))?;
        if busy > 0 {
            return Ok(false);
        }
        tx.execute("INSERT INTO marriage (character_id, spouse_id) VALUES (?1, ?2), (?2, ?1)", params![a, b])?;
        tx.commit()?;
        Ok(true)
    }

    /// Divorces `a` and its spouse (online or not); returns the spouse id.
    pub fn divorce(&self, a: i64) -> DbResult<Option<i64>> {
        let tx = self.conn.unchecked_transaction()?;
        let spouse: Option<i64> = tx.query_row("SELECT spouse_id FROM marriage WHERE character_id = ?1", params![a], |r| r.get(0)).optional()?;
        if let Some(s) = spouse {
            tx.execute("DELETE FROM marriage WHERE character_id IN (?1, ?2)", params![a, s])?;
        }
        tx.commit()?;
        Ok(spouse)
    }

    /// One vote per account and poll; false if this account already voted.
    pub fn poll_vote(&self, poll: &str, account: i64, option: u8) -> DbResult<bool> {
        Ok(self.conn.execute("INSERT OR IGNORE INTO poll_vote (poll, account_id, option) VALUES (?1, ?2, ?3)", params![poll, account, option])? == 1)
    }

    pub fn poll_voted(&self, poll: &str, account: i64) -> DbResult<bool> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM poll_vote WHERE poll = ?1 AND account_id = ?2", params![poll, account], |r| r.get::<_, i64>(0))? > 0)
    }

    /// Votes per option (`options` long).
    pub fn poll_results(&self, poll: &str, options: usize) -> DbResult<Vec<u32>> {
        let mut out = vec![0u32; options];
        let mut st = self.conn.prepare("SELECT option, COUNT(*) FROM poll_vote WHERE poll = ?1 GROUP BY option")?;
        let rows = st.query_map(params![poll], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (o, n) = row?;
            if let Some(v) = out.get_mut(o as usize) {
                *v = n as u32;
            }
        }
        Ok(out)
    }
}

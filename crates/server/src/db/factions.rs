//! P2 persistence of factions (`impl Db`). OWNER: Opus C (docs/P2_CONTRATO.md).
use super::{Db, DbResult};
use crate::world::faction::FactionRecord;
use rusqlite::{params, OptionalExtension};

impl Db {
    /// The character's faction state; a fresh citizen when nothing was saved yet.
    pub fn load_faction(&self, char_id: i64) -> DbResult<FactionRecord> {
        let r = self
            .conn
            .query_row(
                "SELECT status, citizens_killed, criminals_killed, royal_rank, chaos_rank, royal_rewarded, chaos_rewarded, enlistments, entry_level, entry_kills, score FROM faction WHERE character_id = ?1",
                params![char_id],
                |r| {
                    Ok(FactionRecord {
                        status: r.get(0)?,
                        citizens_killed: r.get(1)?,
                        criminals_killed: r.get(2)?,
                        royal_rank: r.get(3)?,
                        chaos_rank: r.get(4)?,
                        royal_rewarded: r.get(5)?,
                        chaos_rewarded: r.get(6)?,
                        enlistments: r.get(7)?,
                        entry_level: r.get(8)?,
                        entry_kills: r.get(9)?,
                        score: r.get(10)?,
                    })
                },
            )
            .optional()?;
        Ok(r.unwrap_or_default())
    }

    pub fn save_faction(&self, char_id: i64, f: &FactionRecord) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO faction (character_id, status, citizens_killed, criminals_killed, royal_rank, chaos_rank, royal_rewarded, chaos_rewarded, enlistments, entry_level, entry_kills, score)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(character_id) DO UPDATE SET status = ?2, citizens_killed = ?3, criminals_killed = ?4, royal_rank = ?5, chaos_rank = ?6,
               royal_rewarded = ?7, chaos_rewarded = ?8, enlistments = ?9, entry_level = ?10, entry_kills = ?11, score = ?12",
            params![char_id, f.status, f.citizens_killed, f.criminals_killed, f.royal_rank, f.chaos_rank, f.royal_rewarded, f.chaos_rewarded, f.enlistments, f.entry_level, f.entry_kills, f.score],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewCharacter;

    #[test]
    fn faction_round_trip() {
        let db = Db::open(":memory:").unwrap();
        let acc = db.create_account("faccion", "secreta1").unwrap();
        let c = db.create_character(acc.id, &NewCharacter { name: "Soldado", race: 1, class: 3, gender: 1, body: 1, head: 1, hp: 15, mana: 0, map: 1, x: 50, y: 50 }).unwrap();
        assert_eq!(db.load_faction(c.id).unwrap(), FactionRecord::default());
        let f = FactionRecord { status: 3, royal_rank: 2, royal_rewarded: 2, enlistments: 1, score: 1700, citizens_killed: 0, criminals_killed: 9, ..Default::default() };
        db.save_faction(c.id, &f).unwrap();
        db.save_faction(c.id, &f).unwrap();
        assert_eq!(db.load_faction(c.id).unwrap(), f);
    }
}

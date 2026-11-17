//! P4 persistence of gm (`impl Db`). OWNER: Opus C (docs/P4_CONTRATO.md).
//! Roles, running sentences, bans, penalty history and the GM log.
use super::{now_unix, Db, DbResult};
use ao_protocol::p4::gm::{GmLogRow, GmRank};
use rusqlite::{params, OptionalExtension};

/// One row of `/PENAS` (SavePenaDatabase).
#[derive(Debug, Clone, PartialEq)]
pub struct PenaltyRow {
    pub at: i64,
    pub gm: String,
    pub kind: String,
    pub minutes: i64,
    pub reason: String,
}

impl Db {
    pub fn gm_rank(&self, name: &str) -> DbResult<GmRank> {
        let v: Option<i64> = self.conn.query_row("SELECT rank FROM gm_role WHERE name = ?1", params![name], |r| r.get(0)).optional()?;
        Ok(v.map(GmRank::from_level).unwrap_or_default())
    }

    /// Sets (or with `User`, removes) a character's rank.
    pub fn gm_set_rank(&self, name: &str, rank: GmRank, by: &str) -> DbResult<()> {
        if rank == GmRank::User {
            self.conn.execute("DELETE FROM gm_role WHERE name = ?1", params![name])?;
        } else {
            self.conn.execute(
                "INSERT INTO gm_role (name, rank, granted_by, granted_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(name) DO UPDATE SET rank = excluded.rank, granted_by = excluded.granted_by, granted_at = excluded.granted_at",
                params![name, rank.level(), by, now_unix()],
            )?;
        }
        Ok(())
    }

    pub fn gm_roles(&self) -> DbResult<Vec<(String, GmRank)>> {
        let mut st = self.conn.prepare("SELECT name, rank FROM gm_role ORDER BY rank DESC, name")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, GmRank::from_level(r.get(1)?))))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// (jail ms, silence ms) still to serve.
    pub fn gm_sentence(&self, name: &str) -> DbResult<(u64, u64)> {
        let v: Option<(i64, i64)> = self.conn.query_row("SELECT jail_ms, silence_ms FROM gm_sentence WHERE name = ?1", params![name], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        Ok(v.map(|(j, s)| (j.max(0) as u64, s.max(0) as u64)).unwrap_or((0, 0)))
    }

    pub fn gm_save_sentence(&self, name: &str, jail_ms: u64, silence_ms: u64) -> DbResult<()> {
        if jail_ms == 0 && silence_ms == 0 {
            self.conn.execute("DELETE FROM gm_sentence WHERE name = ?1", params![name])?;
        } else {
            self.conn.execute(
                "INSERT INTO gm_sentence (name, jail_ms, silence_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT(name) DO UPDATE SET jail_ms = excluded.jail_ms, silence_ms = excluded.silence_ms",
                params![name, jail_ms as i64, silence_ms as i64],
            )?;
        }
        Ok(())
    }

    /// The character exists (account character, or one that already has GM rows).
    pub fn gm_character_exists(&self, name: &str) -> DbResult<bool> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM character WHERE name = ?1 AND deleted = 0", params![name], |r| r.get(0))?;
        Ok(n > 0)
    }

    /// Account id of a (non deleted) character.
    pub fn gm_account_of(&self, name: &str) -> DbResult<Option<i64>> {
        Ok(self.conn.query_row("SELECT account_id FROM character WHERE name = ?1 AND deleted = 0", params![name], |r| r.get(0)).optional()?)
    }

    pub fn gm_set_account_banned(&self, account_id: i64, banned: bool) -> DbResult<()> {
        self.conn.execute("UPDATE account SET banned = ?1 WHERE id = ?2", params![banned as i64, account_id])?;
        Ok(())
    }

    pub fn gm_ban(&self, name: &str, reason: &str, gm: &str) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO gm_ban (name, reason, gm, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET reason = excluded.reason, gm = excluded.gm, at = excluded.at",
            params![name, reason, gm, now_unix()],
        )?;
        Ok(())
    }

    /// Returns whether there was a ban.
    pub fn gm_unban(&self, name: &str) -> DbResult<bool> {
        Ok(self.conn.execute("DELETE FROM gm_ban WHERE name = ?1", params![name])? > 0)
    }

    /// Reason of a character ban.
    pub fn gm_ban_reason(&self, name: &str) -> DbResult<Option<String>> {
        Ok(self.conn.query_row("SELECT reason FROM gm_ban WHERE name = ?1", params![name], |r| r.get(0)).optional()?)
    }

    pub fn gm_ban_ip(&self, ip: &str, reason: &str, gm: &str) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO gm_ip_ban (ip, reason, gm, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(ip) DO UPDATE SET reason = excluded.reason, gm = excluded.gm, at = excluded.at",
            params![ip, reason, gm, now_unix()],
        )?;
        Ok(())
    }

    pub fn gm_unban_ip(&self, ip: &str) -> DbResult<bool> {
        Ok(self.conn.execute("DELETE FROM gm_ip_ban WHERE ip = ?1", params![ip])? > 0)
    }

    pub fn gm_ip_ban_reason(&self, ip: &str) -> DbResult<Option<String>> {
        Ok(self.conn.query_row("SELECT reason FROM gm_ip_ban WHERE ip = ?1", params![ip], |r| r.get(0)).optional()?)
    }

    /// SavePenaDatabase.
    pub fn gm_add_penalty(&self, target: &str, gm: &str, kind: &str, minutes: i64, reason: &str) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO gm_penalty (target, gm, kind, minutes, reason, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![target, gm, kind, minutes, reason, now_unix()],
        )?;
        Ok(())
    }

    /// Penalty history of a character, oldest first.
    pub fn gm_penalties(&self, target: &str) -> DbResult<Vec<PenaltyRow>> {
        let mut st = self.conn.prepare("SELECT at, gm, kind, minutes, reason FROM gm_penalty WHERE target = ?1 ORDER BY at, id")?;
        let rows = st.query_map(params![target], |r| Ok(PenaltyRow { at: r.get(0)?, gm: r.get(1)?, kind: r.get(2)?, minutes: r.get(3)?, reason: r.get(4)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// LogGM.
    pub fn gm_log(&self, gm: &str, action: &str, target: &str, detail: &str) -> DbResult<()> {
        self.conn.execute("INSERT INTO gm_log (at, gm, action, target, detail) VALUES (?1, ?2, ?3, ?4, ?5)", params![now_unix(), gm, action, target, detail])?;
        Ok(())
    }

    /// Latest `limit` log rows, newest first; with `gm`, only that GM's actions.
    pub fn gm_recent_log(&self, limit: usize, gm: Option<&str>) -> DbResult<Vec<GmLogRow>> {
        let map = |r: &rusqlite::Row| Ok(GmLogRow { at: r.get(0)?, gm: r.get(1)?, action: r.get(2)?, target: r.get(3)?, detail: r.get(4)? });
        let rows = match gm {
            Some(g) => {
                let mut st = self.conn.prepare("SELECT at, gm, action, target, detail FROM gm_log WHERE gm = ?1 COLLATE NOCASE ORDER BY id DESC LIMIT ?2")?;
                let v = st.query_map(params![g, limit as i64], map)?.collect::<Result<Vec<_>, _>>()?;
                v
            }
            None => {
                let mut st = self.conn.prepare("SELECT at, gm, action, target, detail FROM gm_log ORDER BY id DESC LIMIT ?1")?;
                let v = st.query_map(params![limit as i64], map)?.collect::<Result<Vec<_>, _>>()?;
                v
            }
        };
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_sentences_bans_and_logs_round_trip() {
        let db = Db::open(":memory:").unwrap();
        assert_eq!(db.gm_rank("ana").unwrap(), GmRank::User);
        db.gm_set_rank("Ana", GmRank::Dios, "consola").unwrap();
        assert_eq!(db.gm_rank("ANA").unwrap(), GmRank::Dios, "names ignore case");
        db.gm_set_rank("ana", GmRank::User, "x").unwrap();
        assert!(db.gm_roles().unwrap().is_empty());

        db.gm_save_sentence("Beto", 60_000, 5_000).unwrap();
        assert_eq!(db.gm_sentence("beto").unwrap(), (60_000, 5_000));
        db.gm_save_sentence("Beto", 0, 0).unwrap();
        assert_eq!(db.gm_sentence("beto").unwrap(), (0, 0));

        db.gm_ban("Malo", "cheat", "Gm").unwrap();
        assert_eq!(db.gm_ban_reason("malo").unwrap().as_deref(), Some("cheat"));
        assert!(db.gm_unban("MALO").unwrap());
        assert!(!db.gm_unban("MALO").unwrap());

        db.gm_add_penalty("Malo", "Gm", "carcel", 10, "flood").unwrap();
        db.gm_add_penalty("Malo", "Gm", "silencio", 5, "").unwrap();
        let p = db.gm_penalties("malo").unwrap();
        assert_eq!(p.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>(), ["carcel", "silencio"]);

        db.gm_log("Gm", "carcel", "Malo", "10 min").unwrap();
        db.gm_log("Otro", "echar", "X", "").unwrap();
        assert_eq!(db.gm_recent_log(10, None).unwrap()[0].gm, "Otro", "newest first");
        assert_eq!(db.gm_recent_log(10, Some("gm")).unwrap().len(), 1);
    }
}

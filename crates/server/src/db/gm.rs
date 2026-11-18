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

/// One open help request (`Ayuda` queue entry).
#[derive(Debug, Clone, PartialEq)]
pub struct SosRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub text: String,
    pub at: i64,
}

impl Db {
    /// Rank of the character `name` whose database id is `char_id` (None = development
    /// login). A row bound to another id (the name was freed and taken again) gives
    /// nothing; an unbound row only counts for development logins when `allow_unbound`.
    pub fn gm_rank(&self, name: &str, char_id: Option<i64>, allow_unbound: bool) -> DbResult<GmRank> {
        let v: Option<(i64, Option<i64>)> = self.conn.query_row("SELECT rank, char_id FROM gm_role WHERE name = ?1", params![name], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        Ok(match v {
            Some((rank, Some(bound))) if Some(bound) == char_id => GmRank::from_level(rank),
            Some((rank, None)) if char_id.is_none() && allow_unbound => GmRank::from_level(rank),
            _ => GmRank::User,
        })
    }

    /// Id of the live (not deleted) character with that name.
    pub fn gm_char_id(&self, name: &str) -> DbResult<Option<i64>> {
        Ok(self.conn.query_row("SELECT id FROM character WHERE name = ?1 AND deleted = 0", params![name], |r| r.get(0)).optional()?)
    }

    /// Sets (or with `User`, removes) a character's rank, bound to `char_id`.
    pub fn gm_set_rank(&self, name: &str, char_id: Option<i64>, rank: GmRank, by: &str) -> DbResult<()> {
        if rank == GmRank::User {
            self.conn.execute("DELETE FROM gm_role WHERE name = ?1", params![name])?;
        } else {
            self.conn.execute(
                "INSERT INTO gm_role (name, char_id, rank, granted_by, granted_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(name) DO UPDATE SET char_id = excluded.char_id, rank = excluded.rank, granted_by = excluded.granted_by, granted_at = excluded.granted_at",
                params![name, char_id, rank.level(), by, now_unix()],
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

    /// BanPJ (`until` None, for good) or BanTemporal (`until` = unix seconds it ends).
    /// One row per character: banning again replaces the previous ban (the VB added a
    /// second `baneos.dat` entry, and the first one to expire unbanned the character).
    pub fn gm_ban(&self, name: &str, reason: &str, gm: &str, until: Option<i64>) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO gm_ban (name, reason, gm, at, until) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(name) DO UPDATE SET reason = excluded.reason, gm = excluded.gm, at = excluded.at, until = excluded.until",
            params![name, reason, gm, now_unix(), until],
        )?;
        Ok(())
    }

    /// Returns whether there was a ban.
    pub fn gm_unban(&self, name: &str) -> DbResult<bool> {
        Ok(self.conn.execute("DELETE FROM gm_ban WHERE name = ?1", params![name])? > 0)
    }

    /// Reason and end (None = permanent) of a character ban still running at `now` (unix
    /// seconds). An expired temporary ban is removed here, so it never keeps anyone out
    /// even before the periodic sweep sees it.
    pub fn gm_ban_reason(&self, name: &str, now: i64) -> DbResult<Option<(String, Option<i64>)>> {
        let v: Option<(String, Option<i64>)> = self.conn.query_row("SELECT reason, until FROM gm_ban WHERE name = ?1", params![name], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        match v {
            Some((_, Some(until))) if until <= now => {
                self.conn.execute("DELETE FROM gm_ban WHERE name = ?1", params![name])?;
                Ok(None)
            }
            v => Ok(v),
        }
    }

    /// Temporary bans ended at `now`: removed, names returned (EstadoTimer).
    pub fn gm_expire_bans(&self, now: i64) -> DbResult<Vec<String>> {
        let mut st = self.conn.prepare("SELECT name FROM gm_ban WHERE until IS NOT NULL AND until <= ?1 ORDER BY name")?;
        let names: Vec<String> = st.query_map(params![now], |r| r.get(0))?.collect::<Result<_, _>>()?;
        self.conn.execute("DELETE FROM gm_ban WHERE until IS NOT NULL AND until <= ?1", params![now])?;
        Ok(names)
    }

    /// CambiarPenaDatabase: replaces the text of penalty `n` (1-based, as `/PENAS` lists
    /// them). Returns whether it existed.
    pub fn gm_change_penalty(&self, target: &str, n: usize, text: &str) -> DbResult<bool> {
        let id: Option<i64> = self
            .conn
            .query_row("SELECT id FROM gm_penalty WHERE target = ?1 ORDER BY at, id LIMIT 1 OFFSET ?2", params![target, n.saturating_sub(1) as i64], |r| r.get(0))
            .optional()?;
        let Some(id) = id.filter(|_| n > 0) else { return Ok(false) };
        self.conn.execute("UPDATE gm_penalty SET reason = ?1 WHERE id = ?2", params![text, id])?;
        Ok(true)
    }

    // ---------------------------------------------------------------- help queue

    /// Ayuda.Push. Returns the new id.
    pub fn gm_sos_push(&self, name: &str, kind: &str, text: &str) -> DbResult<i64> {
        self.conn.execute("INSERT INTO gm_sos (name, kind, text, at) VALUES (?1, ?2, ?3, ?4)", params![name, kind, text, now_unix()])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Open requests, oldest first.
    pub fn gm_sos_open(&self) -> DbResult<Vec<SosRow>> {
        let mut st = self.conn.prepare("SELECT id, name, kind, text, at FROM gm_sos WHERE done_by IS NULL ORDER BY id")?;
        let rows = st.query_map([], |r| Ok(SosRow { id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, text: r.get(3)?, at: r.get(4)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Ayuda.Quitar: marks it attended. Returns the request if it was open.
    pub fn gm_sos_close(&self, id: i64, by: &str) -> DbResult<Option<SosRow>> {
        let row = self.gm_sos_open()?.into_iter().find(|r| r.id == id);
        if row.is_some() {
            self.conn.execute("UPDATE gm_sos SET done_by = ?1, done_at = ?2 WHERE id = ?3", params![by, now_unix(), id])?;
        }
        Ok(row)
    }

    /// Ayuda.Reset: closes every open request. Returns how many.
    pub fn gm_sos_clear(&self, by: &str) -> DbResult<usize> {
        Ok(self.conn.execute("UPDATE gm_sos SET done_by = ?1, done_at = ?2 WHERE done_by IS NULL", params![by, now_unix()])?)
    }

    // ---------------------------------------------------------------- settings

    pub fn gm_setting(&self, key: &str) -> DbResult<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM gm_setting WHERE key = ?1", params![key], |r| r.get(0)).optional()?)
    }

    pub fn gm_set_setting(&self, key: &str, value: &str) -> DbResult<()> {
        self.conn.execute("INSERT INTO gm_setting (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, value])?;
        Ok(())
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
        assert_eq!(db.gm_rank("ana", Some(7), true).unwrap(), GmRank::User);
        db.gm_set_rank("Ana", Some(7), GmRank::Dios, "consola").unwrap();
        assert_eq!(db.gm_rank("ANA", Some(7), false).unwrap(), GmRank::Dios, "names ignore case");
        // Same name, another character (deleted and created again): nothing.
        assert_eq!(db.gm_rank("Ana", Some(8), true).unwrap(), GmRank::User);
        assert_eq!(db.gm_rank("Ana", None, true).unwrap(), GmRank::User);
        // Unbound rows: development logins only, and only when allowed.
        db.gm_set_rank("Dev", None, GmRank::Admin, "x").unwrap();
        assert_eq!(db.gm_rank("Dev", None, true).unwrap(), GmRank::Admin);
        assert_eq!(db.gm_rank("Dev", None, false).unwrap(), GmRank::User);
        assert_eq!(db.gm_rank("Dev", Some(3), true).unwrap(), GmRank::User);
        db.gm_set_rank("ana", None, GmRank::User, "x").unwrap();
        db.gm_set_rank("dev", None, GmRank::User, "x").unwrap();
        assert!(db.gm_roles().unwrap().is_empty());

        db.gm_save_sentence("Beto", 60_000, 5_000).unwrap();
        assert_eq!(db.gm_sentence("beto").unwrap(), (60_000, 5_000));
        db.gm_save_sentence("Beto", 0, 0).unwrap();
        assert_eq!(db.gm_sentence("beto").unwrap(), (0, 0));

        db.gm_ban("Malo", "cheat", "Gm", None).unwrap();
        assert_eq!(db.gm_ban_reason("malo", 0).unwrap(), Some(("cheat".to_string(), None)));
        assert!(db.gm_unban("MALO").unwrap());
        assert!(!db.gm_unban("MALO").unwrap());

        // Temporary: running before its end, gone at it (lazily and by the sweep).
        db.gm_ban("Temp", "flood", "Gm", Some(1000)).unwrap();
        db.gm_ban("Otro", "flood", "Gm", Some(2000)).unwrap();
        db.gm_ban("Siempre", "x", "Gm", None).unwrap();
        assert_eq!(db.gm_ban_reason("temp", 999).unwrap(), Some(("flood".to_string(), Some(1000))));
        assert_eq!(db.gm_ban_reason("temp", 1000).unwrap(), None);
        assert_eq!(db.gm_expire_bans(5000).unwrap(), ["Otro"]);
        assert!(db.gm_ban_reason("siempre", i64::MAX).unwrap().is_some(), "permanent bans never expire");
        // Banning again replaces: a permanent ban over a temporary one stays permanent.
        db.gm_ban("Doble", "a", "Gm", Some(10)).unwrap();
        db.gm_ban("Doble", "b", "Gm", None).unwrap();
        assert!(db.gm_expire_bans(100).unwrap().is_empty());

        db.gm_add_penalty("Malo", "Gm", "carcel", 10, "flood").unwrap();
        db.gm_add_penalty("Malo", "Gm", "silencio", 5, "").unwrap();
        let p = db.gm_penalties("malo").unwrap();
        assert_eq!(p.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>(), ["carcel", "silencio"]);
        assert!(db.gm_change_penalty("Malo", 2, "Gm: <corregida>").unwrap());
        assert!(!db.gm_change_penalty("Malo", 3, "x").unwrap());
        assert!(!db.gm_change_penalty("Malo", 0, "x").unwrap());
        assert_eq!(db.gm_penalties("malo").unwrap()[1].reason, "Gm: <corregida>");

        let a = db.gm_sos_push("Pepe", "Consulta regular", "me trabé").unwrap();
        let b = db.gm_sos_push("Ana", "Denuncia a Beto", "insultos").unwrap();
        assert_eq!(db.gm_sos_open().unwrap().len(), 2);
        assert_eq!(db.gm_sos_close(a, "Gm").unwrap().map(|r| r.name), Some("Pepe".into()));
        assert!(db.gm_sos_close(a, "Gm").unwrap().is_none(), "already attended");
        assert_eq!(db.gm_sos_open().unwrap().iter().map(|r| r.id).collect::<Vec<_>>(), [b]);
        assert_eq!(db.gm_sos_clear("Gm").unwrap(), 1);

        assert_eq!(db.gm_setting("motd").unwrap(), None);
        db.gm_set_setting("motd", "hola").unwrap();
        db.gm_set_setting("motd", "chau").unwrap();
        assert_eq!(db.gm_setting("motd").unwrap().as_deref(), Some("chau"));

        db.gm_log("Gm", "carcel", "Malo", "10 min").unwrap();
        db.gm_log("Otro", "echar", "X", "").unwrap();
        assert_eq!(db.gm_recent_log(10, None).unwrap()[0].gm, "Otro", "newest first");
        assert_eq!(db.gm_recent_log(10, Some("gm")).unwrap().len(), 1);
    }
}

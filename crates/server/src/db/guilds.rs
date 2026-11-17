//! P2 persistence of guilds (`impl Db`). OWNER: Opus A (docs/P2_CONTRATO.md).
//! Written through on every change (clsClan does the same with its `UpdateGuildValue`,
//! `AceptarNuevoMiembro`, `ExpulsarMiembro`...): members and applicants may be offline.
use super::{now_unix, Db, DbResult};
use crate::world::guild::Guild;
use ao_rules::guild::{Alignment, Relation};
use rusqlite::{params, OptionalExtension};
use std::collections::{BTreeMap, HashMap};

impl Db {
    /// `LoadGuildsDB`: every guild with its members, applicants, relations and proposals.
    pub fn load_guilds(&self) -> DbResult<Vec<Guild>> {
        let mut guilds: BTreeMap<i64, Guild> = BTreeMap::new();
        let mut st = self.conn.prepare("SELECT id, name, alignment, founder_id, leader_id, description, news, level, exp FROM guild ORDER BY id")?;
        let rows = st.query_map([], |r| {
            Ok(Guild {
                id: r.get(0)?,
                name: r.get(1)?,
                alignment: Alignment::from_u8(r.get::<_, i64>(2)? as u8).unwrap_or(Alignment::Neutral),
                founder: r.get(3)?,
                leader: r.get(4)?,
                description: r.get(5)?,
                news: r.get(6)?,
                level: r.get::<_, i64>(7)?.clamp(1, 255) as u8,
                exp: r.get::<_, i64>(8)?.max(0) as u32,
                members: BTreeMap::new(),
                applicants: Vec::new(),
                relations: HashMap::new(),
                proposals: BTreeMap::new(),
            })
        })?;
        for g in rows {
            let g = g?;
            guilds.insert(g.id, g);
        }
        let mut st = self.conn.prepare("SELECT m.guild_id, m.character_id, c.name FROM guild_member m JOIN character c ON c.id = m.character_id")?;
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))? {
            let (g, c, n) = row?;
            if let Some(g) = guilds.get_mut(&g) {
                g.members.insert(c, n);
            }
        }
        let mut st = self.conn.prepare("SELECT q.guild_id, q.character_id, c.name, q.application FROM guild_request q JOIN character c ON c.id = q.character_id ORDER BY q.created_at, q.rowid")?;
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))? {
            let (g, c, n, a) = row?;
            if let Some(g) = guilds.get_mut(&g) {
                g.applicants.push((c, n, a));
            }
        }
        let mut st = self.conn.prepare("SELECT guild_a, guild_b, relation FROM guild_relation")?;
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))? {
            let (a, b, rel) = row?;
            let rel = Relation::from_i64(rel);
            if let Some(g) = guilds.get_mut(&a) {
                g.relations.insert(b, rel);
            }
            if let Some(g) = guilds.get_mut(&b) {
                g.relations.insert(a, rel);
            }
        }
        let mut st = self.conn.prepare("SELECT from_guild, to_guild, relation, text FROM guild_proposal")?;
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)))? {
            let (from, to, rel, text) = row?;
            if let Some(g) = guilds.get_mut(&to) {
                g.proposals.insert(from, (Relation::from_i64(rel), text));
            }
        }
        Ok(guilds.into_values().collect())
    }

    /// `InicializarNuevoClan` + `AceptarNuevoMiembro(founder)`. Returns the new id.
    pub fn insert_guild(&self, name: &str, alignment: Alignment, founder: i64, description: &str, news: &str) -> DbResult<i64> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO guild (name, alignment, founder_id, leader_id, description, news, level, exp, created_at) VALUES (?1, ?2, ?3, ?3, ?4, ?5, 1, 0, ?6)",
            params![name, alignment as i64, founder, description, news, now_unix()],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("DELETE FROM guild_request WHERE character_id = ?1", params![founder])?;
        tx.execute("INSERT INTO guild_member (character_id, guild_id) VALUES (?1, ?2)", params![founder, id])?;
        tx.commit()?;
        Ok(id)
    }

    /// Description, news, leader, level and exp (`UpdateGuildValue`).
    pub fn save_guild(&self, g: &Guild) -> DbResult<()> {
        self.conn.execute(
            "UPDATE guild SET description = ?2, news = ?3, leader_id = ?4, level = ?5, exp = ?6 WHERE id = ?1",
            params![g.id, g.description, g.news, g.leader, g.level as i64, g.exp as i64],
        )?;
        Ok(())
    }

    /// `AceptarNuevoMiembro`: also drops the character's application.
    pub fn add_guild_member(&self, guild: i64, char_id: i64) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM guild_request WHERE character_id = ?1", params![char_id])?;
        tx.execute("INSERT INTO guild_member (character_id, guild_id) VALUES (?1, ?2)", params![char_id, guild])?;
        tx.commit()?;
        Ok(())
    }

    /// `ExpulsarMiembro`.
    pub fn remove_guild_member(&self, char_id: i64) -> DbResult<()> {
        self.conn.execute("DELETE FROM guild_member WHERE character_id = ?1", params![char_id])?;
        Ok(())
    }

    /// `NuevoAspirante` (replaces any previous application of the character).
    pub fn set_guild_request(&self, guild: i64, char_id: i64, application: &str) -> DbResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO guild_request (character_id, guild_id, application, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![char_id, guild, application, now_unix()],
        )?;
        Ok(())
    }

    /// `RetirarAspirante`.
    pub fn remove_guild_request(&self, char_id: i64) -> DbResult<()> {
        self.conn.execute("DELETE FROM guild_request WHERE character_id = ?1", params![char_id])?;
        Ok(())
    }

    /// `SaveUserGuildRejectionReason`.
    pub fn set_guild_rejection(&self, char_id: i64, text: &str) -> DbResult<()> {
        self.conn.execute("INSERT OR REPLACE INTO guild_rejection (character_id, text) VALUES (?1, ?2)", params![char_id, text])?;
        Ok(())
    }

    /// The stored rejection reason, removed once read.
    pub fn take_guild_rejection(&self, char_id: i64) -> DbResult<Option<String>> {
        let t: Option<String> = self.conn.query_row("SELECT text FROM guild_rejection WHERE character_id = ?1", params![char_id], |r| r.get(0)).optional()?;
        if t.is_some() {
            self.conn.execute("DELETE FROM guild_rejection WHERE character_id = ?1", params![char_id])?;
        }
        Ok(t)
    }

    /// Sets the relation between two guilds (peace removes the row).
    pub fn set_guild_relation(&self, a: i64, b: i64, rel: Relation) -> DbResult<()> {
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        if rel == Relation::Peace {
            self.conn.execute("DELETE FROM guild_relation WHERE guild_a = ?1 AND guild_b = ?2", params![a, b])?;
        } else {
            self.conn.execute("INSERT OR REPLACE INTO guild_relation (guild_a, guild_b, relation) VALUES (?1, ?2, ?3)", params![a, b, rel as i64])?;
        }
        Ok(())
    }

    pub fn set_guild_proposal(&self, from: i64, to: i64, rel: Relation, text: &str) -> DbResult<()> {
        self.conn.execute("INSERT OR REPLACE INTO guild_proposal (from_guild, to_guild, relation, text) VALUES (?1, ?2, ?3, ?4)", params![from, to, rel as i64, text])?;
        Ok(())
    }

    pub fn remove_guild_proposal(&self, from: i64, to: i64) -> DbResult<()> {
        self.conn.execute("DELETE FROM guild_proposal WHERE from_guild = ?1 AND to_guild = ?2", params![from, to])?;
        Ok(())
    }

    /// `GetCharacterIdWithName`: (id, name as stored).
    pub fn character_by_name(&self, name: &str) -> DbResult<Option<(i64, String)>> {
        Ok(self.conn.query_row("SELECT id, name FROM character WHERE name = ?1 AND deleted = 0", params![name.trim()], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewCharacter;

    fn character(db: &Db, account: i64, name: &str) -> i64 {
        db.create_character(account, &NewCharacter { name, race: 1, class: 1, gender: 1, body: 1, head: 1, hp: 10, mana: 0, map: 1, x: 50, y: 50 }).unwrap().id
    }

    #[test]
    fn roundtrip() {
        let db = Db::open(":memory:").unwrap();
        let acc = db.create_account("cuenta", "secreta1").unwrap().id;
        let (a, b, c) = (character(&db, acc, "Uno"), character(&db, acc, "Dos"), character(&db, acc, "Tres"));
        let g1 = db.insert_guild("Los Pibes", Alignment::Neutral, a, "desc", "hola").unwrap();
        let g2 = db.insert_guild("Otros", Alignment::Criminal, b, "", "").unwrap();
        assert!(db.insert_guild("los pibes", Alignment::Neutral, c, "", "").is_err(), "nombre duplicado sin mayusculas");
        db.set_guild_request(g1, c, "quiero entrar").unwrap();
        db.set_guild_relation(g2, g1, Relation::War).unwrap();
        db.set_guild_proposal(g1, g2, Relation::Allied, "amigos").unwrap();
        let all = db.load_guilds().unwrap();
        assert_eq!(all.len(), 2);
        let (x, y) = (&all[0], &all[1]);
        assert_eq!((x.name.as_str(), x.leader, x.level), ("Los Pibes", a, 1));
        assert_eq!(x.members.get(&a).map(String::as_str), Some("Uno"));
        assert_eq!(x.applicants, vec![(c, "Tres".to_string(), "quiero entrar".to_string())]);
        assert_eq!(x.relation(g2), Relation::War);
        assert_eq!(y.relation(g1), Relation::War);
        assert_eq!(y.proposals.get(&g1), Some(&(Relation::Allied, "amigos".to_string())));
        db.add_guild_member(g1, c).unwrap();
        let mut g = db.load_guilds().unwrap().remove(0);
        assert!(g.applicants.is_empty() && g.members.len() == 2);
        g.news = "nuevas".into();
        g.level = 3;
        g.exp = 12;
        db.save_guild(&g).unwrap();
        db.remove_guild_member(c).unwrap();
        db.set_guild_relation(g1, g2, Relation::Peace).unwrap();
        db.remove_guild_proposal(g1, g2).unwrap();
        let g = db.load_guilds().unwrap().remove(0);
        assert_eq!((g.news.as_str(), g.level, g.exp, g.members.len()), ("nuevas", 3, 12, 1));
        assert!(g.relations.is_empty());
        db.set_guild_rejection(c, "no").unwrap();
        assert_eq!(db.take_guild_rejection(c).unwrap().as_deref(), Some("no"));
        assert_eq!(db.take_guild_rejection(c).unwrap(), None);
        assert_eq!(db.character_by_name("tres").unwrap(), Some((c, "Tres".to_string())));
    }
}

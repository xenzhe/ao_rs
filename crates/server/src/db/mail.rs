//! Player mail (`impl Db`), see `systems/mail.rs`.
use super::{Db, DbResult};
use rusqlite::{params, OptionalExtension};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailLetter {
    pub id: i64,
    pub sender: String,
    pub subject: String,
    pub body: String,
    pub gold: u32,
    pub items: Vec<(u16, u32)>,
    pub sent_at: i64,
    pub unread: bool,
}

fn items_text(items: &[(u16, u32)]) -> String {
    items.iter().map(|(o, a)| format!("{o}-{a}")).collect::<Vec<_>>().join(",")
}

fn items_of(s: &str) -> Vec<(u16, u32)> {
    s.split(',').filter_map(|p| p.split_once('-')).filter_map(|(o, a)| Some((o.parse().ok()?, a.parse().ok()?))).collect()
}

const COLS: &str = "id, sender, subject, body, gold, items, sent_at, unread";

fn row(r: &rusqlite::Row) -> rusqlite::Result<MailLetter> {
    Ok(MailLetter {
        id: r.get(0)?,
        sender: r.get(1)?,
        subject: r.get(2)?,
        body: r.get(3)?,
        gold: r.get(4)?,
        items: items_of(&r.get::<_, String>(5)?),
        sent_at: r.get(6)?,
        unread: r.get::<_, i64>(7)? != 0,
    })
}

impl Db {
    /// The name of a (non deleted) character as it was written, looked up ignoring case.
    pub fn character_name(&self, name: &str) -> DbResult<Option<String>> {
        Ok(self.conn.query_row("SELECT name FROM character WHERE name = ?1 COLLATE NOCASE AND deleted = 0", params![name], |r| r.get(0)).optional()?)
    }

    pub fn mail_count(&self, recipient: &str) -> DbResult<usize> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM mail WHERE recipient = ?1", params![recipient], |r| r.get::<_, i64>(0))? as usize)
    }

    pub fn mail_unread(&self, recipient: &str) -> DbResult<usize> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM mail WHERE recipient = ?1 AND unread = 1", params![recipient], |r| r.get::<_, i64>(0))? as usize)
    }

    pub fn mail_send(&self, recipient: &str, l: &MailLetter) -> DbResult<i64> {
        self.conn.execute(
            "INSERT INTO mail (recipient, sender, subject, body, gold, items, sent_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![recipient, l.sender, l.subject, l.body, l.gold, items_text(&l.items), l.sent_at],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Newest first.
    pub fn mail_inbox(&self, recipient: &str) -> DbResult<Vec<MailLetter>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM mail WHERE recipient = ?1 ORDER BY id DESC"))?;
        let rows = st.query_map(params![recipient], row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// A letter of this recipient (never someone else's).
    pub fn mail_get(&self, recipient: &str, id: i64) -> DbResult<Option<MailLetter>> {
        Ok(self.conn.query_row(&format!("SELECT {COLS} FROM mail WHERE recipient = ?1 AND id = ?2"), params![recipient, id], row).optional()?)
    }

    pub fn mail_mark_read(&self, recipient: &str, id: i64) -> DbResult<()> {
        self.conn.execute("UPDATE mail SET unread = 0 WHERE recipient = ?1 AND id = ?2", params![recipient, id])?;
        Ok(())
    }

    /// Empties the parcel; returns false if it was already taken (two clicks, two clients).
    pub fn mail_take_parcel(&self, recipient: &str, id: i64) -> DbResult<bool> {
        let n = self.conn.execute("UPDATE mail SET gold = 0, items = '' WHERE recipient = ?1 AND id = ?2 AND (gold > 0 OR items != '')", params![recipient, id])?;
        Ok(n == 1)
    }

    /// Deletes a letter whose parcel is empty.
    pub fn mail_delete(&self, recipient: &str, id: i64) -> DbResult<bool> {
        Ok(self.conn.execute("DELETE FROM mail WHERE recipient = ?1 AND id = ?2 AND gold = 0 AND items = ''", params![recipient, id])? == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_round_trip_and_parcel_taken_once() {
        let db = Db::open(":memory:").unwrap();
        let l = MailLetter { id: 0, sender: "Ana".into(), subject: "Hola".into(), body: "Te mando algo".into(), gold: 500, items: vec![(38, 10), (12, 3)], sent_at: 7, unread: true };
        let id = db.mail_send("Bruno", &l).unwrap();
        assert_eq!(db.mail_count("bruno").unwrap(), 1);
        assert_eq!(db.mail_unread("BRUNO").unwrap(), 1);
        let got = db.mail_get("Bruno", id).unwrap().unwrap();
        assert_eq!((got.gold, got.items.clone()), (500, vec![(38, 10), (12, 3)]));
        assert!(db.mail_get("Carla", id).unwrap().is_none(), "never someone else's letter");
        assert!(!db.mail_delete("Bruno", id).unwrap(), "a letter with a parcel is not deleted");
        assert!(db.mail_take_parcel("Bruno", id).unwrap());
        assert!(!db.mail_take_parcel("Bruno", id).unwrap(), "taken once");
        db.mail_mark_read("Bruno", id).unwrap();
        assert_eq!(db.mail_unread("Bruno").unwrap(), 0);
        assert!(db.mail_delete("Bruno", id).unwrap());
        assert_eq!(db.mail_count("Bruno").unwrap(), 0);
    }
}

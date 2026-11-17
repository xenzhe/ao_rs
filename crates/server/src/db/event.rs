//! P4 persistence of event (`impl Db`). OWNER: Opus D (docs/P4_CONTRATO.md).
//! Open auction lots and the mailbox of things owed to offline characters.
use super::{Db, DbResult};
use crate::world::event::{LotRow, MailRow};
use rusqlite::params;

impl Db {
    pub fn save_lot(&self, l: &LotRow) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO event_auction (id, seller, obj, amount, base_price, best, bidder, secs_left) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET best = ?6, bidder = ?7, secs_left = ?8",
            params![l.id, l.seller, l.obj, l.amount, l.base_price, l.best, l.bidder, l.secs_left],
        )?;
        Ok(())
    }

    pub fn delete_lot(&self, id: u32) -> DbResult<()> {
        self.conn.execute("DELETE FROM event_auction WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn load_lots(&self) -> DbResult<Vec<LotRow>> {
        let mut st = self.conn.prepare("SELECT id, seller, obj, amount, base_price, best, bidder, secs_left FROM event_auction ORDER BY id")?;
        let rows = st.query_map([], |r| {
            Ok(LotRow { id: r.get(0)?, seller: r.get(1)?, obj: r.get(2)?, amount: r.get(3)?, base_price: r.get(4)?, best: r.get(5)?, bidder: r.get(6)?, secs_left: r.get(7)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn mail(&self, name: &str, gold: u32, obj: u16, amount: u32, note: &str) -> DbResult<()> {
        self.conn.execute("INSERT INTO event_mailbox (name, gold, obj, amount, note) VALUES (?1, ?2, ?3, ?4, ?5)", params![name, gold, obj, amount, note])?;
        Ok(())
    }

    pub fn mailbox(&self, name: &str) -> DbResult<Vec<MailRow>> {
        let mut st = self.conn.prepare("SELECT id, gold, obj, amount, note FROM event_mailbox WHERE name = ?1 ORDER BY id")?;
        let rows = st.query_map(params![name], |r| Ok(MailRow { id: r.get(0)?, gold: r.get(1)?, obj: r.get(2)?, amount: r.get(3)?, note: r.get(4)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn delete_mail(&self, id: i64) -> DbResult<()> {
        self.conn.execute("DELETE FROM event_mailbox WHERE id = ?1", params![id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lots_and_mailbox_round_trip() {
        let db = Db::open(":memory:").unwrap();
        let mut l = LotRow { id: 3, seller: "Vendedor".into(), obj: 38, amount: 5, base_price: 1000, best: 0, bidder: None, secs_left: 300 };
        db.save_lot(&l).unwrap();
        l.best = 1500;
        l.bidder = Some("Comprador".into());
        l.secs_left = 120;
        db.save_lot(&l).unwrap();
        assert_eq!(db.load_lots().unwrap(), vec![l]);
        db.delete_lot(3).unwrap();
        assert!(db.load_lots().unwrap().is_empty());

        db.mail("Pepe", 700, 0, 0, "devolución").unwrap();
        db.mail("pepe", 0, 38, 2, "subasta").unwrap();
        let m = db.mailbox("PEPE").unwrap();
        assert_eq!(m.len(), 2, "names are case-insensitive");
        assert_eq!((m[0].gold, m[1].obj, m[1].amount), (700, 38, 2));
        db.delete_mail(m[0].id).unwrap();
        assert_eq!(db.mailbox("Pepe").unwrap().len(), 1);
    }
}

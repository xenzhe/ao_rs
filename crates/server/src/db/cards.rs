//! Collectible cards of an account (`impl Db`), see `systems/cards.rs`.
use super::{Db, DbResult};
use rusqlite::params;

impl Db {
    pub fn cards_of(&self, account_id: i64) -> DbResult<Vec<(u16, u8)>> {
        let mut st = self.conn.prepare("SELECT card_index, quantity FROM account_card WHERE account_id = ?1")?;
        let rows = st.query_map(params![account_id], |r| Ok((r.get::<_, i64>(0)? as u16, r.get::<_, i64>(1)? as u8)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn card_set(&self, account_id: i64, card: u16, quantity: u8) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO account_card (account_id, card_index, quantity) VALUES (?1, ?2, ?3)
             ON CONFLICT(account_id, card_index) DO UPDATE SET quantity = excluded.quantity",
            params![account_id, card as i64, quantity as i64],
        )?;
        Ok(())
    }
}

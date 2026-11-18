//! Clan vault persistence (`impl Db`), see `systems/bank.rs`.
use super::{Db, DbResult};
use rusqlite::{params, OptionalExtension};

/// `(slot 1-based, obj, amount)`.
pub type VaultRow = (u8, u16, u32);

impl Db {
    /// The stacks and the gold.
    pub fn clan_vault(&self, guild: i64) -> DbResult<(Vec<VaultRow>, u32)> {
        let mut st = self.conn.prepare("SELECT slot, obj, amount FROM guild_vault WHERE guild_id = ?1 ORDER BY slot")?;
        let slots = st.query_map(params![guild], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
        let gold = self.conn.query_row("SELECT gold FROM guild_vault_gold WHERE guild_id = ?1", params![guild], |r| r.get(0)).optional()?.unwrap_or(0);
        Ok((slots, gold))
    }

    /// One slot changed (`None` = emptied).
    pub fn clan_vault_set_slot(&self, guild: i64, slot: u8, item: Option<(u16, u32)>) -> DbResult<()> {
        match item {
            Some((obj, amount)) => self.conn.execute(
                "INSERT INTO guild_vault (guild_id, slot, obj, amount) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(guild_id, slot) DO UPDATE SET obj = ?3, amount = ?4",
                params![guild, slot, obj, amount],
            )?,
            None => self.conn.execute("DELETE FROM guild_vault WHERE guild_id = ?1 AND slot = ?2", params![guild, slot])?,
        };
        Ok(())
    }

    pub fn clan_vault_set_gold(&self, guild: i64, gold: u32) -> DbResult<()> {
        self.conn.execute("INSERT INTO guild_vault_gold (guild_id, gold) VALUES (?1, ?2) ON CONFLICT(guild_id) DO UPDATE SET gold = ?2", params![guild, gold])?;
        Ok(())
    }
}

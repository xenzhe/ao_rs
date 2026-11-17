//! Persistence in SQLite (same engine as the official AO20 server).
//!
//! - Versioned migrations embedded in the binary (`crates/server/migrations/*.sql`),
//!   applied in order on open; the applied version is stored in `PRAGMA user_version`.
//! - WAL journal for safe concurrent readers and crash safety.
//! - `":memory:"` opens a private in-memory database (tests).
mod accounts;

pub use accounts::{Account, CharacterRecord, NewCharacter};

use rusqlite::Connection;
use std::path::Path;

/// Migrations in order. Never edit an applied one: add a new file.
const MIGRATIONS: &[&str] = &[include_str!("../../migrations/0001_accounts.sql")];

#[derive(Debug, PartialEq)]
pub enum DbError {
    /// Name already in use (account or character).
    Taken,
    NotFound,
    WrongPassword,
    Banned,
    TooManyCharacters,
    Sql(String),
}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => DbError::NotFound,
            rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => DbError::Taken,
            other => DbError::Sql(other.to_string()),
        }
    }
}

pub type DbResult<T> = Result<T, DbError>;

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &str) -> DbResult<Db> {
        let conn = if path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            if let Some(dir) = Path::new(path).parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let c = Connection::open(path)?;
            c.pragma_update(None, "journal_mode", "WAL")?;
            c
        };
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> DbResult<()> {
        let current: usize = self.conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? as usize;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> usize {
        self.conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)).unwrap_or(0) as usize
    }
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

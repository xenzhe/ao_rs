//! Hot backups of the SQLite database, rotation and restore.
//!
//! A backup is `VACUUM INTO` run on a second connection: SQLite reads a consistent snapshot
//! (the database is in WAL mode, so the world keeps writing meanwhile) and writes a compact,
//! standalone `.db` file. It is written under a temporary name and renamed when complete, so
//! a backup folder never holds a half-written copy.
use crate::db::now_unix;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// `ao-YYYYMMDD-HHMMSS.db` (a `_NN` suffix before `.db` when two fall in the same second).
const PREFIX: &str = "ao-";
const EXT: &str = ".db";

/// Civil date (year, month, day) of a day count since 1970-01-01 (proleptic Gregorian).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// `YYYYMMDD-HHMMSS` (UTC) of a unix time.
pub fn stamp(unix: i64) -> String {
    let (y, m, d) = civil(unix.div_euclid(86_400));
    let s = unix.rem_euclid(86_400);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", s / 3600, s % 3600 / 60, s % 60)
}

#[derive(Debug)]
pub enum BackupError {
    /// In-memory database: there is no file to copy.
    NoFile,
    Missing(PathBuf),
    /// The server (or another process) has the database busy.
    InUse,
    Invalid(String),
    Io(String),
}

impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupError::NoFile => write!(f, "la base esta en memoria: no hay archivo que copiar"),
            BackupError::Missing(p) => write!(f, "no existe {}", p.display()),
            BackupError::InUse => write!(f, "la base esta en uso: para el servidor antes de restaurar"),
            BackupError::Invalid(e) => write!(f, "copia invalida: {e}"),
            BackupError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<rusqlite::Error> for BackupError {
    fn from(e: rusqlite::Error) -> Self {
        BackupError::Io(e.to_string())
    }
}

impl From<std::io::Error> for BackupError {
    fn from(e: std::io::Error) -> Self {
        BackupError::Io(e.to_string())
    }
}

/// SQL string literal for a path (`VACUUM INTO` takes no bound parameter on every build).
fn quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "''"))
}

/// Writes a consistent copy of `db` to exactly `dest` (must not exist).
pub fn copy_to(db: &Path, dest: &Path) -> Result<(), BackupError> {
    if !db.exists() {
        return Err(BackupError::Missing(db.to_path_buf()));
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = dest.with_extension("part");
    let _ = std::fs::remove_file(&tmp);
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.execute_batch(&format!("VACUUM INTO {}", quote(&tmp)))?;
    drop(conn);
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

/// Takes a backup of `db` into `dir` named after `unix`; returns its path.
pub fn backup_into(db: &str, dir: &Path, unix: i64) -> Result<PathBuf, BackupError> {
    if db == ":memory:" || db.is_empty() {
        return Err(BackupError::NoFile);
    }
    std::fs::create_dir_all(dir)?;
    let base = format!("{PREFIX}{}", stamp(unix));
    let mut dest = dir.join(format!("{base}{EXT}"));
    let mut n = 1;
    while dest.exists() {
        dest = dir.join(format!("{base}_{n:02}{EXT}"));
        n += 1;
    }
    copy_to(Path::new(db), &dest)?;
    Ok(dest)
}

/// Backups in `dir`, oldest first (names sort by time).
pub fn list(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.len() >= PREFIX.len() + 15 + EXT.len() && n.starts_with(PREFIX) && n.ends_with(EXT))
        })
        .collect();
    v.sort();
    v
}

/// Day part (`YYYYMMDD`) of a backup name.
fn day_of(p: &Path) -> String {
    p.file_name().and_then(|n| n.to_str()).map(|n| n[PREFIX.len()..PREFIX.len() + 8].to_string()).unwrap_or_default()
}

/// Rotation: keeps the newest backup of each of the newest `keep_days` days that have
/// backups, plus every backup of the newest day (today's, taken more often). Returns the
/// deleted files.
pub fn rotate(dir: &Path, keep_days: usize) -> Vec<PathBuf> {
    let files = list(dir);
    let keep_days = keep_days.max(1);
    let mut days: Vec<String> = files.iter().map(|p| day_of(p)).collect();
    days.dedup();
    let newest_day = days.last().cloned().unwrap_or_default();
    let kept_days: Vec<&String> = days.iter().rev().take(keep_days).collect();
    let mut deleted = Vec::new();
    for (i, p) in files.iter().enumerate() {
        let day = day_of(p);
        let last_of_day = files.get(i + 1).is_none_or(|n| day_of(n) != day);
        let keep = day == newest_day || (kept_days.contains(&&day) && last_of_day);
        if !keep && std::fs::remove_file(p).is_ok() {
            deleted.push(p.clone());
        }
    }
    deleted
}

/// Opens a backup read-only and checks that SQLite finds it healthy. Returns its schema version.
pub fn verify(file: &Path) -> Result<i64, BackupError> {
    if !file.exists() {
        return Err(BackupError::Missing(file.to_path_buf()));
    }
    let conn = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| BackupError::Invalid(e.to_string()))?;
    let res: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).map_err(|e| BackupError::Invalid(e.to_string()))?;
    if res != "ok" {
        return Err(BackupError::Invalid(res));
    }
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)).unwrap_or(0))
}

/// Replaces `db` with the backup `from`. The server must be stopped (an exclusive lock is
/// tried first). The current database is kept next to it as `<db>.antes-<fecha>` unless it
/// does not exist. Returns that safety copy, if any.
pub fn restore(from: &Path, db: &Path) -> Result<Option<PathBuf>, BackupError> {
    verify(from)?;
    let mut safety = None;
    if db.exists() {
        {
            let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
            conn.busy_timeout(std::time::Duration::from_millis(500))?;
            // Leaving WAL mode needs the database to ourselves (and folds in any WAL left by a
            // crash): it fails while a running server has it open, even when that is idle.
            let mode = conn.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get::<_, String>(0)).map_err(|_| BackupError::InUse)?;
            if !mode.eq_ignore_ascii_case("delete") {
                return Err(BackupError::InUse);
            }
        }
        let name = format!("{}.antes-{}", db.file_name().and_then(|n| n.to_str()).unwrap_or("ao.db"), stamp(now_unix()));
        let dest = db.with_file_name(name);
        let _ = std::fs::remove_file(&dest);
        copy_to(db, &dest)?;
        safety = Some(dest);
    } else if let Some(dir) = db.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = db.with_extension("restoring");
    std::fs::copy(from, &tmp)?;
    for suffix in ["-wal", "-shm"] {
        let mut s = db.as_os_str().to_owned();
        s.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(s));
    }
    std::fs::rename(&tmp, db)?;
    Ok(safety)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_utc_dates() {
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(1_000_000_000), "20010909-014640");
        assert_eq!(stamp(1_709_164_800 - 1), "20240228-235959");
        assert_eq!(stamp(1_709_164_800), "20240229-000000"); // leap day
        assert_eq!(stamp(1_709_164_800 + 86_400), "20240301-000000");
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ao_ops_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"x").unwrap();
    }

    #[test]
    fn rotation_keeps_one_per_day_and_all_of_today() {
        let d = scratch("rot");
        for n in [
            "ao-20260101-060000.db",
            "ao-20260101-120000.db",
            "ao-20260102-060000.db",
            "ao-20260103-060000.db",
            "ao-20260103-120000.db",
            "ao-20260104-010000.db",
            "ao-20260104-020000_01.db",
            "notes.txt",
        ] {
            touch(&d, n);
        }
        let deleted = rotate(&d, 3);
        let left: Vec<String> = list(&d).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(deleted.len(), 3);
        assert_eq!(left, ["ao-20260102-060000.db", "ao-20260103-120000.db", "ao-20260104-010000.db", "ao-20260104-020000_01.db"]);
        assert!(d.join("notes.txt").exists(), "foreign files are never touched");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn memory_database_has_no_file() {
        assert!(matches!(backup_into(":memory:", Path::new("x"), 0), Err(BackupError::NoFile)));
    }
}

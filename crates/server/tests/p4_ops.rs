//! P4 operation: hot backup and restore of a database with data, the statistics report, and
//! the orderly shutdown of a real-clock server (players saved, final backup, farewell).
use ao_protocol::{encode, FrameReader, C2S, S2C, PROTOCOL_VERSION};
use server::config::ServerConfig;
use server::data::GameData;
use server::db::{Db, NewCharacter};
use server::ops::backup::{self, BackupError};
use server::{ops, Settings};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ao_p4ops_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn count(db: &Path, table: &str) -> i64 {
    let c = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

fn make_character(db: &Db, user: &str, name: &str) {
    let acc = db.create_account(user, "secreta1").unwrap();
    let nc = NewCharacter { name, race: 1, class: 1, gender: 1, body: 1, head: 1, hp: 20, mana: 0, map: 1, x: 50, y: 50 };
    db.create_character(acc.id, &nc).unwrap();
}

#[test]
fn hot_backup_restores_every_row() {
    let d = scratch("hot");
    let dbfile = d.join("ao.db");
    let db = Db::open(&p(&dbfile)).unwrap();
    make_character(&db, "ana", "Ana");
    make_character(&db, "beto", "Beto");

    // The server keeps the database open (WAL) while the copy is taken.
    let file = backup::backup_into(&p(&dbfile), &d.join("backups"), 1_700_000_000).unwrap();
    assert!(file.file_name().unwrap().to_string_lossy().starts_with("ao-20231114-"));
    assert_eq!(backup::verify(&file).unwrap(), db.schema_version() as i64);
    assert_eq!((count(&file, "account"), count(&file, "character")), (2, 2));

    // A second copy in the same second does not overwrite the first.
    let again = backup::backup_into(&p(&dbfile), &d.join("backups"), 1_700_000_000).unwrap();
    assert_ne!(file, again);
    assert_eq!(backup::list(&d.join("backups")).len(), 2);

    // More data after the copy, then restore: the restored database is the backup's.
    make_character(&db, "carla", "Carla");
    assert_eq!(count(&dbfile, "character"), 3);
    assert!(matches!(backup::restore(&file, &dbfile), Err(BackupError::InUse)), "refuses while the server holds the database");
    drop(db);
    let safety = backup::restore(&file, &dbfile).unwrap().expect("the previous database is kept");
    assert_eq!(count(&dbfile, "character"), 2);
    assert_eq!(count(&safety, "character"), 3, "nothing is lost: the pre-restore copy has Carla");
    let reopened = Db::open(&p(&dbfile)).unwrap();
    assert!(reopened.verify_account("ana", "secreta1").is_ok());
    assert!(reopened.verify_account("carla", "secreta1").is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn corrupt_or_missing_backups_are_refused() {
    let d = scratch("bad");
    let junk = d.join("ao-20260101-000000.db");
    std::fs::write(&junk, b"this is not a database at all, just text").unwrap();
    let target = d.join("ao.db");
    assert!(matches!(backup::restore(&junk, &target), Err(BackupError::Invalid(_))));
    assert!(!target.exists(), "a bad backup never touches the database");
    assert!(matches!(backup::restore(&d.join("missing.db"), &target), Err(BackupError::Missing(_))));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn cli_backup_then_restore_latest() {
    let d = scratch("cli");
    let dbfile = d.join("ao.db");
    let cfg = d.join("server.toml");
    std::fs::write(&cfg, format!("database = \"{}\"\n[ops]\nbackup_dir = \"{}\"\nbackup_keep_days = 2\n", p(&dbfile), p(&d.join("bk")))).unwrap();
    {
        let db = Db::open(&p(&dbfile)).unwrap();
        make_character(&db, "ana", "Ana");
    }
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let cfg_s = p(&cfg);
    assert_eq!(ops::cli(&args(&["backup", &cfg_s])), Some(0));
    assert_eq!(ops::cli(&args(&["backups", &cfg_s])), Some(0));
    assert_eq!(ops::cli(&args(&["--no-subcommand"])), None, "other arguments start the server");
    {
        let db = Db::open(&p(&dbfile)).unwrap();
        make_character(&db, "beto", "Beto");
    }
    assert_eq!(ops::cli(&args(&["restore", "latest", &cfg_s])), Some(0));
    assert_eq!(count(&dbfile, "character"), 1);
    assert_eq!(ops::cli(&args(&["restore", "nope.db", &cfg_s])), Some(1));
    let _ = std::fs::remove_dir_all(&d);
}

/// Reads frames until `pred` holds for one, or the stream ends/times out.
async fn read_until(sock: &mut TcpStream, rd: &mut FrameReader, secs: u64, pred: impl Fn(&S2C) -> bool) -> Option<S2C> {
    let end = Instant::now() + Duration::from_secs(secs);
    let mut buf = [0u8; 4096];
    loop {
        while let Some(m) = rd.next::<S2C>(1 << 20) {
            let m = m.ok()?;
            if pred(&m) {
                return Some(m);
            }
        }
        let left = end.checked_duration_since(Instant::now())?;
        match tokio::time::timeout(left, sock.read(&mut buf)).await {
            Ok(Ok(n)) if n > 0 => rd.push(&buf[..n]),
            _ => return None,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn graceful_shutdown_saves_backs_up_and_reports() {
    let d = scratch("run");
    let dbfile = d.join("ao.db");
    let stats = d.join("stats.json");
    let backups = d.join("bk");
    let mut config = ServerConfig { database: p(&dbfile), ..ServerConfig::default() };
    config.dev_login = false;
    config.ops.stats_file = p(&stats);
    config.ops.stats_file_secs = 1;
    config.ops.stats_interval_min = 0;
    config.ops.backup_interval_hours = 0;
    config.ops.backup_dir = p(&backups);
    let recursos = config.recursos_path();
    let settings = Settings::new(config, GameData::defaults(), recursos);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(ops::run_until(listener, settings, async {
        let _ = stop_rx.await;
    }));

    let mut sock = TcpStream::connect(addr).await.unwrap();
    for m in [
        C2S::CreateAccount { user: "xenzhe".into(), password: "secreta1".into(), version: PROTOCOL_VERSION },
        C2S::CreateCharacter { name: "Gandalf".into(), race: 1, class: 1, gender: 1, head: 1 },
        C2S::PlayCharacter { name: "Gandalf".into() },
    ] {
        sock.write_all(&encode(&m)).await.unwrap();
    }
    let mut rd = FrameReader::default();

    // The JSON a web page would read shows the player once it has been refreshed.
    let end = Instant::now() + Duration::from_secs(20);
    let json = loop {
        if let Ok(t) = std::fs::read_to_string(&stats) {
            if t.contains("\"players\": 1") {
                break t;
            }
        }
        assert!(Instant::now() < end, "stats.json never showed the player");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    for k in ["uptime_secs", "players_peak", "accounts", "characters", "maps_loaded", "npcs", "tick_avg_ms", "tick_max_total_ms", "msgs_per_sec", "memory_mb"] {
        assert!(json.contains(&format!("\"{k}\"")), "{k} missing in {json}");
    }
    assert!(json.contains("\"accounts\": 1") && json.contains("\"characters\": 1") && json.contains("\"players_peak\": 1"), "{json}");
    assert!(!json.contains("\"maps_loaded\": 0") && !json.contains("\"msgs_total\": 0"), "{json}");
    assert!(backup::list(&backups).is_empty(), "no backup before the stop (periodic ones are off)");

    // Orderly stop: the server finishes, the client is told, and the final copy exists.
    stop_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(20), server).await.expect("shutdown in time").unwrap();
    let farewell = read_until(&mut sock, &mut rd, 5, |m| matches!(m, S2C::Kicked { .. })).await;
    assert!(farewell.is_some(), "the player is told the server is stopping");
    let copies = backup::list(&backups);
    assert_eq!(copies.len(), 1, "one backup taken on shutdown");
    assert_eq!((count(&copies[0], "account"), count(&copies[0], "character")), (1, 1));
    assert!(backup::verify(&copies[0]).is_ok());
    let last = std::fs::read_to_string(&stats).unwrap();
    assert!(last.contains("\"uptime_secs\""), "final statistics written");
    let _ = std::fs::remove_dir_all(&d);
}

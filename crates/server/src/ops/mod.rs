//! Server operation: statistics, database backups and orderly shutdown (docs/OPERACION.md).
//!
//! - [`stats`]: counters, snapshot, log report and `data/stats.json`.
//! - [`backup`]: hot copy (`VACUUM INTO`), rotation, verify and restore.
//! - [`run`]: the production run loop (same as `world::run` plus the hooks above and a
//!   graceful stop on Ctrl+C / SIGTERM). `main.rs` calls it instead of `server::run`.
//! - [`cli`]: `server backup | backups | restore <file|latest> | stats` subcommands.
//!
//! Nothing here changes gameplay: it only reads the world and calls `save_player`.
pub mod backup;
mod mem;
pub mod stats;

use crate::clock::Clock;
use crate::config::{OpsConfig, ServerConfig};
use crate::net::{self, Event};
use crate::world::World;
use crate::Settings;
use ao_protocol::S2C;
use stats::{DbCounts, Metrics, Stats};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{error, info, warn};

/// Runtime state of the operation features.
pub struct Ops {
    cfg: OpsConfig,
    db: String,
    pub metrics: Metrics,
    counts: Option<DbCounts>,
    next_file: Instant,
    next_log: Option<Instant>,
    next_backup: Option<Instant>,
    backing_up: Arc<AtomicBool>,
    last: Option<Stats>,
}

impl Ops {
    pub fn new(cfg: &ServerConfig) -> Ops {
        let now = Instant::now();
        let o = cfg.ops.clone();
        let file_every = Duration::from_secs(o.stats_file_secs.max(1));
        Ops {
            counts: DbCounts::open(&cfg.database),
            db: cfg.database.clone(),
            metrics: Metrics::default(),
            next_file: now + file_every,
            next_log: (o.stats_interval_min > 0).then(|| now + Duration::from_secs(o.stats_interval_min * 60)),
            next_backup: (o.backup_interval_hours > 0).then(|| now + Duration::from_secs(o.backup_interval_hours * 3600)),
            backing_up: Arc::new(AtomicBool::new(false)),
            last: None,
            cfg: o,
        }
    }

    /// A client message reached the world.
    pub fn on_frame(&mut self) {
        self.metrics.frame();
    }

    /// `World::tick` took `d` (see [`run`]; wrap the call with `Instant::now()` if you drive
    /// the world yourself).
    pub fn on_tick(&mut self, d: Duration) {
        self.metrics.tick(d);
    }

    /// Fresh statistics on demand (closes the rate window like the periodic one does).
    pub fn stats(&mut self, world: &World) -> Stats {
        if self.counts.is_none() {
            self.counts = DbCounts::open(&self.db);
        }
        let s = self.metrics.snapshot(world, self.counts.as_ref());
        self.last = Some(s.clone());
        s
    }

    /// The last statistics taken (periodic or on demand).
    pub fn last_stats(&self) -> Option<&Stats> {
        self.last.as_ref()
    }

    /// Due work: JSON file, log report, periodic backup. Cheap when nothing is due; call it
    /// after every world tick.
    pub fn housekeeping(&mut self, world: &World) {
        let now = Instant::now();
        if now >= self.next_file {
            self.next_file = now + Duration::from_secs(self.cfg.stats_file_secs.max(1));
            let s = self.stats(world);
            if !self.cfg.stats_file.is_empty() {
                if let Err(e) = s.write_json(Path::new(&self.cfg.stats_file)) {
                    warn!("no se pudo escribir {}: {e}", self.cfg.stats_file);
                }
            }
            if self.next_log.is_some_and(|t| now >= t) {
                self.next_log = Some(now + Duration::from_secs(self.cfg.stats_interval_min * 60));
                info!("{}", s.line());
                self.metrics.period_done();
            }
        }
        if self.next_backup.is_some_and(|t| now >= t) {
            self.next_backup = Some(now + Duration::from_secs(self.cfg.backup_interval_hours * 3600));
            save_all(world);
            self.backup_in_background();
        }
    }

    /// Logs the current statistics now (SIGUSR1, a GM command, tests).
    pub fn report_now(&mut self, world: &World) -> Stats {
        let s = self.stats(world);
        info!("{}", s.line());
        if !self.cfg.stats_file.is_empty() {
            let _ = s.write_json(Path::new(&self.cfg.stats_file));
        }
        s
    }

    /// Backup on a helper thread (the world keeps ticking); one at a time.
    fn backup_in_background(&self) {
        if self.backing_up.swap(true, Ordering::SeqCst) {
            return;
        }
        let (db, dir, keep, flag) = (self.db.clone(), PathBuf::from(&self.cfg.backup_dir), self.cfg.backup_keep_days, self.backing_up.clone());
        std::thread::spawn(move || {
            report_backup(do_backup(&db, &dir, keep));
            flag.store(false, Ordering::SeqCst);
        });
    }

    /// Saves everyone, then takes a backup right now and waits for it.
    pub fn backup_now(&self, world: &World) -> Result<PathBuf, backup::BackupError> {
        save_all(world);
        do_backup(&self.db, Path::new(&self.cfg.backup_dir), self.cfg.backup_keep_days)
    }

    /// Orderly stop: tells everyone, saves every character, backs up the database.
    /// The world must not process more events afterwards.
    pub fn shutdown(&mut self, world: &World) {
        info!("apagando: avisando y guardando jugadores");
        for id in world.player_ids() {
            world.send(id, &S2C::Kicked { reason: "El servidor se esta apagando. Tu personaje esta guardado.".into() });
        }
        let saved = save_all(world);
        for (id, s) in &world.sessions {
            if s.player.is_some() {
                world.close(*id);
            }
        }
        info!(saved, "personajes guardados");
        let s = self.stats(world);
        info!("{}", s.line());
        if !self.cfg.stats_file.is_empty() {
            let _ = s.write_json(Path::new(&self.cfg.stats_file));
        }
        if self.cfg.backup_on_shutdown {
            // Wait for a periodic one that may be running, so the final copy is the newest.
            let t = Instant::now();
            while self.backing_up.load(Ordering::SeqCst) && t.elapsed() < Duration::from_secs(30) {
                std::thread::sleep(Duration::from_millis(50));
            }
            report_backup(self.backup_now(world));
        }
    }
}

/// Writes every logged-in account character to the database; returns how many.
pub fn save_all(world: &World) -> usize {
    let mut n = 0;
    for s in world.sessions.values() {
        if let Some(p) = &s.player {
            world.save_player(p);
            n += 1;
        }
    }
    n
}

fn do_backup(db: &str, dir: &Path, keep_days: usize) -> Result<PathBuf, backup::BackupError> {
    let file = backup::backup_into(db, dir, crate::db::now_unix())?;
    backup::verify(&file)?;
    for old in backup::rotate(dir, keep_days) {
        info!("copia antigua eliminada: {}", old.display());
    }
    Ok(file)
}

fn report_backup(r: Result<PathBuf, backup::BackupError>) {
    match r {
        Ok(p) => {
            let kb = std::fs::metadata(&p).map(|m| m.len() / 1024).unwrap_or(0);
            info!("copia de seguridad: {} ({kb} KB)", p.display());
        }
        Err(backup::BackupError::NoFile) => {}
        Err(e) => error!("copia de seguridad fallida: {e}"),
    }
}

/// Resolves on Ctrl+C, SIGTERM (Unix) or console close/shutdown (Windows).
pub async fn os_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(windows)]
    {
        use tokio::signal::windows::{ctrl_break, ctrl_close, ctrl_shutdown};
        let (mut brk, mut close, mut shutdown) = (ctrl_break().expect("ctrl_break"), ctrl_close().expect("ctrl_close"), ctrl_shutdown().expect("ctrl_shutdown"));
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = brk.recv() => {}
            _ = close.recv() => {}
            _ = shutdown.recv() => {}
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// SIGUSR1 (Unix): log the statistics now. Never fires elsewhere.
struct ReportSignal {
    #[cfg(unix)]
    sig: Option<tokio::signal::unix::Signal>,
}

impl ReportSignal {
    fn new() -> ReportSignal {
        ReportSignal {
            #[cfg(unix)]
            sig: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1()).ok(),
        }
    }

    async fn recv(&mut self) {
        #[cfg(unix)]
        if let Some(s) = &mut self.sig {
            s.recv().await;
            return;
        }
        std::future::pending::<()>().await
    }
}

/// Production entry point: `server::spawn` + `world::run` with statistics, periodic backups
/// and a graceful stop when the OS asks (Ctrl+C, SIGTERM, console close).
pub async fn run(listener: TcpListener, settings: Settings) {
    run_until(listener, settings, os_shutdown_signal()).await
}

/// [`run`] with the stop request supplied by the caller (tests use a channel). Returns once the
/// players are saved and the final backup is written. A virtual clock has no real-time loop, so
/// it falls back to the plain server and never returns.
pub async fn run_until(listener: TcpListener, settings: Settings, stop: impl Future<Output = ()>) {
    if matches!(settings.clock, Clock::Virtual(_)) {
        return crate::run(listener, settings).await;
    }
    let (ev_tx, mut rx) = unbounded_channel::<Event>();
    let security = settings.config.security.clone();
    let clock = settings.clock.clone();
    let mut world = World::new(settings);
    let mut ops = Ops::new(&world.cfg);
    let accept = tokio::spawn(net::accept_loop(listener, ev_tx, security, clock));
    let start_map = world.start.0;
    world.ensure_map(start_map);
    let mut tick = tokio::time::interval(Duration::from_millis(world.cfg.world.tick_ms.max(1)));
    let mut usr1 = ReportSignal::new();
    tokio::pin!(stop);
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Some(ev) => {
                    if matches!(ev, Event::Frame { .. }) {
                        ops.on_frame();
                    }
                    world.on_event(ev);
                }
                None => break,
            },
            _ = tick.tick() => {
                let now = world.now();
                let t = Instant::now();
                world.tick(now);
                ops.on_tick(t.elapsed());
                ops.housekeeping(&world);
                // `/APAGAR` (GM) finished its countdown: same orderly shutdown as a signal.
                if world.gms.shut_down {
                    accept.abort();
                    ops.shutdown(&world);
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    break;
                }
            }
            _ = usr1.recv() => {
                ops.report_now(&world);
            }
            _ = &mut stop => {
                accept.abort();
                // Frames already queued still count: drain them before saving.
                while let Ok(ev) = rx.try_recv() {
                    world.on_event(ev);
                }
                ops.shutdown(&world);
                // Let the writer tasks flush the farewell before the process ends.
                tokio::time::sleep(Duration::from_millis(300)).await;
                break;
            }
        }
    }
}

// ---------------------------------------------------------------- command line

/// Subcommands that never start the server. `args` is `std::env::args().skip(1)`; returns the
/// exit code when one ran, `None` to start the server as usual.
///
/// ```text
/// server backup [config]                 copy the database now (safe while the server runs)
/// server backups [config]                list the copies
/// server restore <file|latest> [config]  replace the database (server stopped)
/// server stats [config]                  print data/stats.json
/// ```
pub fn cli(args: &[String]) -> Option<i32> {
    let cmd = args.first()?.as_str();
    if !matches!(cmd, "backup" | "backups" | "restore" | "stats") {
        return None;
    }
    let rest: Vec<&String> = args[1..].iter().collect();
    let (target, cfg_arg) = if cmd == "restore" { (rest.first().map(|s| s.as_str()), rest.get(1)) } else { (None, rest.first()) };
    let cfg_path = cfg_arg.map(|s| PathBuf::from(s.as_str())).unwrap_or_else(|| PathBuf::from("config/server.toml"));
    let mut config = match ServerConfig::load(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("configuracion invalida: {e}");
            return Some(2);
        }
    };
    if let Ok(d) = std::env::var("AO_DB") {
        config.database = d;
    }
    let dir = PathBuf::from(&config.ops.backup_dir);
    Some(match cmd {
        "backup" => match backup::backup_into(&config.database, &dir, crate::db::now_unix()).and_then(|p| backup::verify(&p).map(|_| p)) {
            Ok(p) => {
                let removed = backup::rotate(&dir, config.ops.backup_keep_days).len();
                println!("copia creada: {} (copias antiguas eliminadas: {removed})", p.display());
                0
            }
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        "backups" => {
            let files = backup::list(&dir);
            if files.is_empty() {
                println!("no hay copias en {}", dir.display());
            }
            for f in files {
                println!("{}  {} KB", f.display(), std::fs::metadata(&f).map(|m| m.len() / 1024).unwrap_or(0));
            }
            0
        }
        "restore" => {
            let Some(target) = target else {
                eprintln!("uso: server restore <archivo|latest> [config]");
                return Some(2);
            };
            let from = if target == "latest" {
                match backup::list(&dir).pop() {
                    Some(p) => p,
                    None => {
                        eprintln!("no hay copias en {}", dir.display());
                        return Some(1);
                    }
                }
            } else {
                PathBuf::from(target)
            };
            match backup::restore(&from, Path::new(&config.database)) {
                Ok(safety) => {
                    println!("restaurada {} en {}", from.display(), config.database);
                    if let Some(s) = safety {
                        println!("la base anterior queda en {}", s.display());
                    }
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        _ => match std::fs::read_to_string(&config.ops.stats_file) {
            Ok(t) => {
                print!("{t}");
                0
            }
            Err(e) => {
                eprintln!("no se pudo leer {}: {e} (el servidor esta en marcha?)", config.ops.stats_file);
                1
            }
        },
    })
}

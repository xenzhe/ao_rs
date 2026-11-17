//! Server statistics: counters fed by the run loop, a snapshot of the world, a one-line log
//! report and a JSON file for a web page.
use crate::world::World;
use std::path::Path;
use std::time::{Duration, Instant};

/// Counters since the last snapshot plus all-time extremes.
pub struct Metrics {
    started: Instant,
    last: Instant,
    frames: u64,
    ticks: u64,
    tick_sum: Duration,
    tick_max: Duration,
    /// Worst tick since the server started.
    tick_max_total: Duration,
    /// Worst tick since the last log report.
    tick_max_period: Duration,
    pub players_peak: usize,
    ticks_total: u64,
    frames_total: u64,
}

impl Default for Metrics {
    fn default() -> Self {
        let now = Instant::now();
        Metrics {
            started: now,
            last: now,
            frames: 0,
            ticks: 0,
            tick_sum: Duration::ZERO,
            tick_max: Duration::ZERO,
            tick_max_total: Duration::ZERO,
            tick_max_period: Duration::ZERO,
            players_peak: 0,
            ticks_total: 0,
            frames_total: 0,
        }
    }
}

impl Metrics {
    /// One client message reached the world.
    pub fn frame(&mut self) {
        self.frames += 1;
        self.frames_total += 1;
    }

    /// One `World::tick` took `d`.
    pub fn tick(&mut self, d: Duration) {
        self.ticks += 1;
        self.ticks_total += 1;
        self.tick_sum += d;
        self.tick_max = self.tick_max.max(d);
        self.tick_max_total = self.tick_max_total.max(d);
        self.tick_max_period = self.tick_max_period.max(d);
    }

    /// Reads the world and closes the measurement window (rates are per window).
    pub fn snapshot(&mut self, world: &World, db: Option<&DbCounts>) -> Stats {
        let now = Instant::now();
        let window = now.duration_since(self.last).as_secs_f64().max(0.001);
        let players = world.player_ids().len();
        self.players_peak = self.players_peak.max(players);
        let npcs = world.maps.values().map(|m| m.npcs.values().filter(|n| n.dead_until.is_none()).count()).sum();
        let (accounts, characters) = db.map(|d| d.read()).unwrap_or((0, 0));
        let s = Stats {
            unix: crate::db::now_unix(),
            uptime_secs: now.duration_since(self.started).as_secs(),
            players,
            players_peak: self.players_peak,
            accounts,
            characters,
            maps_loaded: world.maps.len(),
            npcs,
            tick_avg_ms: if self.ticks == 0 { 0.0 } else { self.tick_sum.as_secs_f64() * 1000.0 / self.ticks as f64 },
            tick_max_ms: self.tick_max.as_secs_f64() * 1000.0,
            tick_max_total_ms: self.tick_max_total.as_secs_f64() * 1000.0,
            tick_max_period_ms: self.tick_max_period.as_secs_f64() * 1000.0,
            ticks_total: self.ticks_total,
            msgs_per_sec: self.frames as f64 / window,
            msgs_total: self.frames_total,
            memory_mb: super::mem::resident_bytes().map(|b| b as f64 / 1_048_576.0),
        };
        self.last = now;
        self.frames = 0;
        self.ticks = 0;
        self.tick_sum = Duration::ZERO;
        self.tick_max = Duration::ZERO;
        s
    }

    /// Call after logging a report: the next one measures its own period.
    pub fn period_done(&mut self) {
        self.tick_max_period = Duration::ZERO;
    }
}

/// Read-only connection used to count accounts and characters without touching the world.
pub struct DbCounts(std::sync::Mutex<rusqlite::Connection>);

impl DbCounts {
    pub fn open(path: &str) -> Option<DbCounts> {
        if path == ":memory:" || !Path::new(path).exists() {
            return None;
        }
        let c = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        let _ = c.busy_timeout(Duration::from_secs(1));
        Some(DbCounts(std::sync::Mutex::new(c)))
    }

    fn read(&self) -> (u64, u64) {
        let Ok(c) = self.0.lock() else { return (0, 0) };
        let n = |sql: &str| c.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0).max(0) as u64;
        (n("SELECT COUNT(*) FROM account"), n("SELECT COUNT(*) FROM character"))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    pub unix: i64,
    pub uptime_secs: u64,
    pub players: usize,
    pub players_peak: usize,
    pub accounts: u64,
    pub characters: u64,
    pub maps_loaded: usize,
    pub npcs: usize,
    /// Average world tick (ms) in the last measurement window.
    pub tick_avg_ms: f64,
    pub tick_max_ms: f64,
    pub tick_max_total_ms: f64,
    pub tick_max_period_ms: f64,
    pub ticks_total: u64,
    /// Client messages per second in the last window.
    pub msgs_per_sec: f64,
    pub msgs_total: u64,
    /// Resident memory of the process (`None` when the platform gives no figure).
    pub memory_mb: Option<f64>,
}

impl Stats {
    /// One-line report for the log.
    pub fn line(&self) -> String {
        let up = self.uptime_secs;
        format!(
            "estadisticas: jugadores {} (pico {}), cuentas {}, personajes {}, mapas {}, npcs {}, tick medio {:.2} ms, peor {:.2} ms (total {:.2} ms), {:.1} msg/s, memoria {}, activo {}d {:02}h {:02}m",
            self.players,
            self.players_peak,
            self.accounts,
            self.characters,
            self.maps_loaded,
            self.npcs,
            self.tick_avg_ms,
            self.tick_max_period_ms,
            self.tick_max_total_ms,
            self.msgs_per_sec,
            self.memory_mb.map_or("n/d".to_string(), |m| format!("{m:.0} MB")),
            up / 86_400,
            up % 86_400 / 3600,
            up % 3600 / 60,
        )
    }

    /// JSON object (flat, numbers only; `memory_mb` is `null` when unknown).
    pub fn json(&self) -> String {
        format!(
            "{{\n  \"unix\": {},\n  \"uptime_secs\": {},\n  \"players\": {},\n  \"players_peak\": {},\n  \"accounts\": {},\n  \"characters\": {},\n  \"maps_loaded\": {},\n  \"npcs\": {},\n  \"tick_avg_ms\": {:.3},\n  \"tick_max_ms\": {:.3},\n  \"tick_max_total_ms\": {:.3},\n  \"ticks_total\": {},\n  \"msgs_per_sec\": {:.2},\n  \"msgs_total\": {},\n  \"memory_mb\": {}\n}}\n",
            self.unix,
            self.uptime_secs,
            self.players,
            self.players_peak,
            self.accounts,
            self.characters,
            self.maps_loaded,
            self.npcs,
            self.tick_avg_ms,
            self.tick_max_ms,
            self.tick_max_total_ms,
            self.ticks_total,
            self.msgs_per_sec,
            self.msgs_total,
            self.memory_mb.map_or("null".to_string(), |m| format!("{m:.1}")),
        )
    }

    /// Writes the JSON atomically (temporary file, then rename): a reader never sees half of it.
    pub fn write_json(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.json())?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Stats {
        Stats {
            unix: 1,
            uptime_secs: 90_061,
            players: 3,
            players_peak: 7,
            accounts: 10,
            characters: 12,
            maps_loaded: 2,
            npcs: 55,
            tick_avg_ms: 0.25,
            tick_max_ms: 1.5,
            tick_max_total_ms: 4.0,
            tick_max_period_ms: 2.0,
            ticks_total: 100,
            msgs_per_sec: 12.5,
            msgs_total: 99,
            memory_mb: None,
        }
    }

    #[test]
    fn json_has_every_field_and_null_memory() {
        let j = sample().json();
        for k in ["uptime_secs", "players_peak", "accounts", "characters", "maps_loaded", "npcs", "tick_avg_ms", "tick_max_total_ms", "msgs_per_sec"] {
            assert!(j.contains(&format!("\"{k}\"")), "{k}");
        }
        assert!(j.contains("\"memory_mb\": null"));
        assert!(j.contains("\"players\": 3"));
    }

    #[test]
    fn line_reads_well() {
        let l = sample().line();
        assert!(l.contains("jugadores 3 (pico 7)") && l.contains("activo 1d 01h 01m"), "{l}");
    }

    #[test]
    fn metrics_track_window_and_extremes() {
        let mut m = Metrics::default();
        m.tick(Duration::from_millis(2));
        m.tick(Duration::from_millis(6));
        assert_eq!(m.ticks, 2);
        assert_eq!(m.tick_max_total, Duration::from_millis(6));
        m.frame();
        assert_eq!(m.frames_total, 1);
    }
}

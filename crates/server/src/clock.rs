//! Game time. All gameplay reads time from the world's [`Clock`], never from the OS, so a
//! test can run the world on a [`VirtualClock`] and make seconds pass instantly and
//! deterministically (AI, regeneration, respawns, cooldowns, walk pace).
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Milliseconds since the world started. Monotonic.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct GameTime(pub u64);

impl GameTime {
    pub const ZERO: GameTime = GameTime(0);

    pub fn ms(self) -> u64 {
        self.0
    }

    pub fn secs_f64(self) -> f64 {
        self.0 as f64 / 1000.0
    }

    /// `self + ms`.
    pub fn after_ms(self, ms: u64) -> GameTime {
        GameTime(self.0.saturating_add(ms))
    }

    pub fn after_secs(self, secs: u64) -> GameTime {
        self.after_ms(secs.saturating_mul(1000))
    }

    /// Milliseconds from `earlier` to `self` (0 if `earlier` is later).
    pub fn since(self, earlier: GameTime) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

/// Manually advanced time shared between the world and a test.
#[derive(Clone, Default, Debug)]
pub struct VirtualClock(Arc<AtomicU64>);

impl VirtualClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn now(&self) -> GameTime {
        GameTime(self.0.load(Ordering::Acquire))
    }

    /// Moves time forward to `t` (never backwards).
    pub fn set(&self, t: GameTime) {
        self.0.fetch_max(t.0, Ordering::AcqRel);
    }
}

/// Where the world (and the network flood counter) read the time from.
#[derive(Clone, Debug)]
pub enum Clock {
    /// Wall clock: production.
    Real(Instant),
    /// Only moves when the test says so.
    Virtual(VirtualClock),
}

impl Clock {
    pub fn real() -> Clock {
        Clock::Real(Instant::now())
    }

    pub fn now(&self) -> GameTime {
        match self {
            Clock::Real(origin) => GameTime(origin.elapsed().as_millis() as u64),
            Clock::Virtual(v) => v.now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_clock_is_shared_and_monotonic() {
        let v = VirtualClock::new();
        let c = Clock::Virtual(v.clone());
        v.set(GameTime(500));
        assert_eq!(c.now(), GameTime(500));
        v.set(GameTime(100));
        assert_eq!(c.now(), GameTime(500), "never goes back");
        assert_eq!(GameTime(500).since(GameTime(800)), 0);
        assert_eq!(GameTime(500).after_secs(2), GameTime(2500));
    }
}

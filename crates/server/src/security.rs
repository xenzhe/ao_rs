//! Anti-cheat building blocks. Each one is a small state machine with its own tests; the
//! systems combine them. None of them trusts anything the client says about itself.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Token bucket for walking: one token per `WALK_MS`, max 2. Absorbs network jitter
/// (two packets arriving together) but not a speed-hack.
pub struct WalkLimiter {
    tokens: f32,
    last: Instant,
    step_ms: f32,
    tolerance: f32,
}

impl WalkLimiter {
    pub fn new(step_ms: u64, tolerance: f32, now: Instant) -> Self {
        WalkLimiter { tokens: 2.0, last: now, step_ms: step_ms as f32, tolerance }
    }

    pub fn try_step(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f32() * 1000.0;
        self.last = now;
        self.tokens = (self.tokens + elapsed / self.step_ms).min(2.0);
        if self.tokens >= 1.0 - self.tolerance {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Sequence numbers must strictly increase: a captured packet sent again is refused.
#[derive(Default)]
pub struct SeqGuard {
    last: u32,
}

impl SeqGuard {
    pub fn accept(&mut self, seq: u32) -> bool {
        if seq > self.last {
            self.last = seq;
            true
        } else {
            false
        }
    }
}

/// Counts cheat attempts with slow decay, so lag spikes never get anyone kicked.
pub struct Strikes {
    count: u32,
    last_decay: Instant,
    decay: Duration,
}

impl Strikes {
    pub fn new(decay: Duration, now: Instant) -> Self {
        Strikes { count: 0, last_decay: now, decay }
    }

    /// Adds one and returns the current total.
    pub fn add(&mut self, now: Instant) -> u32 {
        if now.saturating_duration_since(self.last_decay) > self.decay {
            self.count = self.count.saturating_sub(1);
            self.last_decay = now;
        }
        self.count += 1;
        self.count
    }
}

/// At most `max` messages in a sliding `window`.
pub struct RateLimiter {
    times: VecDeque<Instant>,
    max: usize,
    window: Duration,
}

impl RateLimiter {
    pub fn new(max: usize, window: Duration) -> Self {
        RateLimiter { times: VecDeque::new(), max, window }
    }

    pub fn allow(&mut self, now: Instant) -> bool {
        while self.times.front().is_some_and(|t| now.saturating_duration_since(*t) > self.window) {
            self.times.pop_front();
        }
        if self.times.len() >= self.max {
            return false;
        }
        self.times.push_back(now);
        true
    }
}

/// Removes control characters and caps the length of a chat line.
pub fn sanitize_chat(text: &str, max: usize) -> Option<String> {
    let clean: String = text.chars().filter(|c| !c.is_control()).take(max).collect();
    (!clean.trim().is_empty()).then_some(clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_pace() {
        let t0 = Instant::now();
        let mut w = WalkLimiter::new(200, 0.08, t0);
        // Two buffered steps are allowed (jitter), a third immediately is not.
        assert!(w.try_step(t0));
        assert!(w.try_step(t0));
        assert!(!w.try_step(t0));
        // After a full step time, one more.
        assert!(w.try_step(t0 + Duration::from_millis(200)));
    }

    #[test]
    fn replay() {
        let mut s = SeqGuard::default();
        assert!(s.accept(1) && s.accept(5));
        assert!(!s.accept(5) && !s.accept(2));
    }

    #[test]
    fn strikes_decay() {
        let t0 = Instant::now();
        let mut s = Strikes::new(Duration::from_secs(10), t0);
        assert_eq!(s.add(t0), 1);
        assert_eq!(s.add(t0), 2);
        assert_eq!(s.add(t0 + Duration::from_secs(11)), 2, "one forgotten, one added");
    }

    #[test]
    fn rate_and_chat() {
        let t0 = Instant::now();
        let mut r = RateLimiter::new(2, Duration::from_secs(3));
        assert!(r.allow(t0) && r.allow(t0) && !r.allow(t0));
        assert!(r.allow(t0 + Duration::from_secs(4)));
        assert_eq!(sanitize_chat("ho\u{7}la\u{1b}", 10).as_deref(), Some("hola"));
        assert_eq!(sanitize_chat(" \u{7} ", 10), None);
    }
}

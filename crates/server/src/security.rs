//! Anti-cheat building blocks. Each one is a small state machine with its own tests; the
//! systems combine them. None of them trusts anything the client says about itself, and
//! all of them run on game time (see `clock`), so tests drive them without waiting.
use crate::clock::GameTime;
use std::collections::VecDeque;

/// Token bucket for walking: one token per `WALK_MS`, max 2. Absorbs network jitter
/// (two packets arriving together) but not a speed-hack.
pub struct WalkLimiter {
    tokens: f32,
    last: GameTime,
    step_ms: f32,
    tolerance: f32,
}

impl WalkLimiter {
    pub fn new(step_ms: u64, tolerance: f32, now: GameTime) -> Self {
        WalkLimiter { tokens: 2.0, last: now, step_ms: step_ms as f32, tolerance }
    }

    /// After a teleport the client starts walking afresh (its step timer restarts with the
    /// new map): the budget restarts too, or legit steps would count as a speed-hack.
    pub fn reset(&mut self, now: GameTime) {
        self.tokens = 2.0;
        self.last = now;
    }

    pub fn try_step(&mut self, now: GameTime) -> bool {
        // Frames arrive in order, but `now` may be older than `last` after a reset.
        let now = if now.0 < self.last.0 { self.last } else { now };
        let elapsed = now.since(self.last) as f32;
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
    last_decay: GameTime,
    decay_ms: u64,
}

impl Strikes {
    pub fn new(decay_ms: u64, now: GameTime) -> Self {
        Strikes { count: 0, last_decay: now, decay_ms }
    }

    /// Adds one and returns the current total.
    pub fn add(&mut self, now: GameTime) -> u32 {
        if now.since(self.last_decay) > self.decay_ms {
            self.count = self.count.saturating_sub(1);
            self.last_decay = now;
        }
        self.count += 1;
        self.count
    }
}

/// At most `max` messages in a sliding window of `window_ms`.
pub struct RateLimiter {
    times: VecDeque<GameTime>,
    max: usize,
    window_ms: u64,
}

impl RateLimiter {
    pub fn new(max: usize, window_ms: u64) -> Self {
        RateLimiter { times: VecDeque::new(), max, window_ms }
    }

    pub fn allow(&mut self, now: GameTime) -> bool {
        while self.times.front().is_some_and(|t| now.since(*t) > self.window_ms) {
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
        let t0 = GameTime(1000);
        let mut w = WalkLimiter::new(200, 0.08, t0);
        // Two buffered steps are allowed (jitter), a third immediately is not.
        assert!(w.try_step(t0));
        assert!(w.try_step(t0));
        assert!(!w.try_step(t0));
        // After a full step time, one more.
        assert!(w.try_step(t0.after_ms(200)));
    }

    #[test]
    fn replay() {
        let mut s = SeqGuard::default();
        assert!(s.accept(1) && s.accept(5));
        assert!(!s.accept(5) && !s.accept(2));
    }

    #[test]
    fn strikes_decay() {
        let t0 = GameTime(1000);
        let mut s = Strikes::new(10_000, t0);
        assert_eq!(s.add(t0), 1);
        assert_eq!(s.add(t0), 2);
        assert_eq!(s.add(t0.after_secs(11)), 2, "one forgotten, one added");
    }

    #[test]
    fn rate_and_chat() {
        let t0 = GameTime(1000);
        let mut r = RateLimiter::new(2, 3000);
        assert!(r.allow(t0) && r.allow(t0) && !r.allow(t0));
        assert!(r.allow(t0.after_secs(4)));
        assert_eq!(sanitize_chat("ho\u{7}la\u{1b}", 10).as_deref(), Some("hola"));
        assert_eq!(sanitize_chat(" \u{7} ", 10), None);
    }
}

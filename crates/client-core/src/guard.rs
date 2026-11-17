//! Obfuscated integers for client-side stats.
//!
//! Cheat Engine style tools find values by scanning memory for "100", then "93" after taking
//! damage, and so on. Here the value is never stored in clear: it is XORed with a key that
//! changes on every write, plus a second encoded copy used as an integrity check. Editing
//! one copy without the other is detected by `verify()`.
//!
//! This only raises the bar: the real protection is that the server owns every stat, so a
//! modified client value changes nothing in the game.
use std::sync::atomic::{AtomicU32, Ordering};

static ENTROPY: AtomicU32 = AtomicU32::new(0);

fn next_key() -> u32 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(7);
    let mut x = ENTROPY.fetch_add(0x9E37_79B9, Ordering::Relaxed) ^ t ^ (&ENTROPY as *const _ as usize as u32);
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x | 1
}

fn mirror(v: u32, key: u32) -> u32 {
    v.rotate_left(13) ^ !key.wrapping_mul(0x85EB_CA6B)
}

pub struct Guarded {
    a: u32,
    b: u32,
    key: u32,
}

impl Guarded {
    pub fn new(v: i32) -> Self {
        let mut g = Guarded { a: 0, b: 0, key: 0 };
        g.set(v);
        g
    }

    pub fn set(&mut self, v: i32) {
        self.key = next_key();
        self.a = (v as u32) ^ self.key;
        self.b = mirror(v as u32, self.key);
    }

    pub fn get(&self) -> i32 {
        (self.a ^ self.key) as i32
    }

    pub fn verify(&self) -> bool {
        self.b == mirror(self.a ^ self.key, self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tamper() {
        let mut g = Guarded::new(100);
        assert_eq!(g.get(), 100);
        assert!(g.verify());
        g.set(93);
        assert_eq!(g.get(), 93);
        for v in [-5, 0, 1, 99, 100, 188, 12345, i32::MAX, i32::MIN] {
            for _ in 0..50 {
                g.set(v);
                assert_eq!(g.get(), v);
                assert!(g.verify());
            }
        }
        g.set(93);
        // Simulate a memory editor overwriting the stored word.
        g.a ^= 0x55;
        assert!(!g.verify());
    }
}

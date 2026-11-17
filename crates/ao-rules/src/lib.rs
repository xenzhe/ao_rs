//! Pure game rules shared by client and server, so both always agree (the client mirrors
//! them for prediction, the server enforces them). No I/O, no networking: everything here
//! is testable in isolation.
//!
//! - [`walk`]: who can step where (official AO20 LegalWalk), line of sight
//! - [`spells`], [`creatures`], [`balance`]: data definitions loaded from `data/*.toml`
//! - [`progression`]: experience and levels
pub mod balance;
pub mod character;
pub mod combat;
pub mod creatures;
pub mod items;
pub mod magic;
pub mod pathfinding;
pub mod progression;
pub mod spells;
pub mod walk;

use serde::{Deserialize, Serialize};

/// Time to walk one tile (classic AO pace).
pub const WALK_MS: u64 = 200;
pub const MAX_CHAT: usize = 120;
/// Visible window in tiles, same as the official AO20 client (XWindow x YWindow).
pub const VIEW_TILES_W: i32 = 23;
pub const VIEW_TILES_H: i32 = 18;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir::Up => (0, -1),
            Dir::Down => (0, 1),
            Dir::Left => (-1, 0),
            Dir::Right => (1, 0),
        }
    }

    /// AO heading index: NORTH=1, EAST=2, SOUTH=3, WEST=4.
    pub fn heading(self) -> usize {
        match self {
            Dir::Up => 1,
            Dir::Right => 2,
            Dir::Down => 3,
            Dir::Left => 4,
        }
    }

    /// Blocked-side bit checked on the destination tile (2^(heading-1)).
    pub fn block_bit(self) -> u8 {
        1 << (self.heading() - 1)
    }

    /// Direction that reduces the larger axis distance towards (dx, dy).
    pub fn towards(dx: i32, dy: i32) -> Dir {
        if dx.abs() > dy.abs() {
            if dx > 0 { Dir::Right } else { Dir::Left }
        } else if dy > 0 {
            Dir::Down
        } else {
            Dir::Up
        }
    }
}

/// 3-16 letters, digits or underscores.
pub fn valid_name(name: &str) -> bool {
    (3..=16).contains(&name.chars().count()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Chebyshev distance (AO ranges are squares).
pub fn tile_distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_bits_match_ao() {
        assert_eq!(Dir::Up.block_bit(), 1);
        assert_eq!(Dir::Right.block_bit(), 2);
        assert_eq!(Dir::Down.block_bit(), 4);
        assert_eq!(Dir::Left.block_bit(), 8);
    }

    #[test]
    fn names() {
        assert!(valid_name("Gandalf_2"));
        assert!(!valid_name("ab"));
        assert!(!valid_name("con espacio"));
    }
}

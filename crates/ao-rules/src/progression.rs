//! Experience and levels.
use crate::balance::Balance;

pub fn exp_for_level(b: &Balance, level: u32) -> u32 {
    b.exp_factor * level * level
}

/// Result of gaining experience.
#[derive(Debug, PartialEq)]
pub struct LevelState {
    pub level: u32,
    pub exp: u32,
    pub levels_gained: u32,
}

/// Adds `gained` experience, carrying over every level reached.
pub fn add_exp(b: &Balance, level: u32, exp: u32, gained: u32) -> LevelState {
    let (mut level, mut exp, mut levels_gained) = (level, exp + gained, 0);
    while exp >= exp_for_level(b, level) {
        exp -= exp_for_level(b, level);
        level += 1;
        levels_gained += 1;
    }
    LevelState { level, exp, levels_gained }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_over_levels() {
        let b = Balance::default();
        assert_eq!(add_exp(&b, 1, 0, 30), LevelState { level: 1, exp: 30, levels_gained: 0 });
        // Level 1 needs 40, level 2 needs 160: 210 exp = two levels and 10 left.
        assert_eq!(add_exp(&b, 1, 0, 210), LevelState { level: 3, exp: 10, levels_gained: 2 });
    }
}

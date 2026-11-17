//! Walking rules from the official server (GameLogic.bas `LegalWalk`, `LegalPos`).
use crate::Dir;
use ao_formats::csm::{AoMap, BLOCK_ALL, FLAG_WATER};

/// Walkable border, derived from the official 23x18 tile window (XWindow/YWindow).
pub const MIN_X: i32 = 12;
pub const MAX_X: i32 = 89;
pub const MIN_Y: i32 = 10;
pub const MAX_Y: i32 = 91;

pub fn in_bounds(x: i32, y: i32) -> bool {
    (MIN_X..=MAX_X).contains(&x) && (MIN_Y..=MAX_Y).contains(&y)
}

/// Inside the border, not water, and the destination tile is not blocked on the side we
/// enter from. Occupancy by other characters is checked by the caller.
pub fn can_enter(map: &AoMap, x: i32, y: i32, dir: Dir) -> bool {
    if !in_bounds(x, y) {
        return false;
    }
    let b = map.blocked(x, y);
    b & FLAG_WATER == 0 && b & dir.block_bit() == 0
}

/// A tile a character can be placed on (spawns, respawns, teleports).
pub fn standable(map: &AoMap, x: i32, y: i32) -> bool {
    in_bounds(x, y) && map.blocked(x, y) & (FLAG_WATER | BLOCK_ALL) == 0 && map.exit(x, y).is_none()
}

/// Fully blocked tiles (walls, trees, rocks) block line of sight; water does not.
pub fn line_of_sight(map: &AoMap, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let (mut x, mut y, mut err) = (x0, y0, dx + dy);
    loop {
        if (x, y) != (x0, y0) && (x, y) != (x1, y1) && map.blocked(x, y) & BLOCK_ALL == BLOCK_ALL {
            return false;
        }
        if x == x1 && y == y1 {
            return true;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ao_formats::recursos_dir;

    #[test]
    fn ullathorpe_rules() {
        let Ok(m) = AoMap::load(&recursos_dir(), 1) else { return };
        assert!(standable(&m, 50, 50), "the square is standable");
        assert!(!in_bounds(MIN_X - 1, 50) && !in_bounds(50, MAX_Y + 1));
        // Somewhere in the city a tile is entered from one side only.
        let one_way = (MIN_Y..=MAX_Y).any(|y| (MIN_X..=MAX_X).any(|x| {
            let b = m.blocked(x, y) & BLOCK_ALL;
            b != 0 && b != BLOCK_ALL
        }));
        assert!(one_way, "Ullathorpe has directional blocks");
    }
}

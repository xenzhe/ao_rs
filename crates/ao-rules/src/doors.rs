//! Doors (`General.bas::BloquearPuerta`, `WorldActions.bas::HandleDoorAction`).
//!
//! A door is a map object (`OBJType=6`) whose `Subtipo` says which tile sides it blocks
//! when closed. Opening swaps the object for its `IndexAbierta` and clears those sides;
//! closing swaps it for `IndexCerrada` and sets them again. Server and client apply the
//! same rule so walking prediction stays right.
use ao_formats::csm::{tile, BLOCK_ALL, BLOCK_NORTH, BLOCK_SOUTH, MAP_SIZE};

/// Clicks reach a door up to this distance (`Distance(...) > 2`).
pub const DOOR_REACH: i32 = 2;

/// The (dx, dy, bits) a door of this `Subtipo` toggles.
pub fn door_sides(subtype: i32) -> &'static [(i32, i32, u8)] {
    const N: u8 = BLOCK_NORTH;
    const S: u8 = BLOCK_SOUTH;
    const A: u8 = BLOCK_ALL;
    match subtype {
        // Two tiles wide.
        0 => &[(0, 0, N), (-1, 0, N), (0, 1, S), (-1, 1, S)],
        // Lever: no blocks of its own (it works another door).
        1 => &[],
        // Three tiles, one row up.
        2 => &[(0, -1, N), (-1, -1, N), (1, -1, N), (0, 0, S), (-1, 0, S), (1, 0, S)],
        // Three tiles wide.
        3 => &[(0, 0, N), (-1, 0, N), (1, 0, N), (0, 1, S), (-1, 1, S), (1, 1, S)],
        // One tile.
        4 => &[(0, 0, N), (0, 1, S)],
        // Vertical doors (the official case 6 writes x+1,y+2 from x+1,y-2: a typo, the
        // pair is y-2 like case 5).
        5 => &[(1, 0, A), (1, -1, A), (0, -2, A), (-1, -2, A)],
        6 => &[(-1, 0, A), (-1, -1, A), (0, -2, A), (1, -2, A)],
        _ => &[],
    }
}

/// Sets (`closed`) or clears the sides a door at (x, y) blocks.
pub fn apply_door_blocks(blocked: &mut [u8], x: i32, y: i32, subtype: i32, closed: bool) {
    for &(dx, dy, bits) in door_sides(subtype) {
        let (tx, ty) = (x + dx, y + dy);
        if !(1..=MAP_SIZE).contains(&tx) || !(1..=MAP_SIZE).contains(&ty) {
            continue;
        }
        let b = &mut blocked[tile(tx, ty)];
        if closed {
            *b |= bits;
        } else {
            *b &= !bits;
        }
    }
}

/// Where a click on (x, y) finds a door: the tile itself or, for doors that span more
/// than one tile, (x+1, y), (x+1, y+1) and (x, y+1) (`TryHandleMultiTileObjectWorldAction`).
pub const DOOR_CLICK_OFFSETS: [(i32, i32); 4] = [(0, 0), (1, 0), (1, 1), (0, 1)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_clears_exactly_what_closing_sets() {
        let n = (MAP_SIZE * MAP_SIZE) as usize;
        for sub in 0..=6 {
            let mut b = vec![0u8; n];
            b[tile(10, 10)] = 0x20; // water flag must survive
            apply_door_blocks(&mut b, 10, 10, sub, true);
            let closed = b.clone();
            apply_door_blocks(&mut b, 10, 10, sub, false);
            assert_eq!(b[tile(10, 10)], 0x20, "subtype {sub}");
            assert_eq!(b.iter().filter(|v| **v != 0).count(), 1, "subtype {sub}");
            apply_door_blocks(&mut b, 10, 10, sub, true);
            assert_eq!(b, closed);
        }
        let mut b = vec![0u8; n];
        apply_door_blocks(&mut b, 10, 10, 0, true);
        assert_eq!((b[tile(10, 10)], b[tile(9, 10)], b[tile(10, 11)], b[tile(9, 11)]), (BLOCK_NORTH, BLOCK_NORTH, BLOCK_SOUTH, BLOCK_SOUTH));
    }

    #[test]
    fn edges_are_ignored() {
        let mut b = vec![0u8; (MAP_SIZE * MAP_SIZE) as usize];
        apply_door_blocks(&mut b, 1, MAP_SIZE, 0, true);
        assert_eq!(b[tile(1, MAP_SIZE)], BLOCK_NORTH);
    }
}

//! Client actions and state for neighbour maps and the continuous world (graphics-free and
//! testable). OWNER: Opus C (docs/P3_CONTRATO.md).
//!
//! How AO20 knows which map borders which:
//! - `Recursos/init/mapsworlddata.dat` (client `Recursos.bas::Cargarmapsworlddata`): per world
//!   a grid `ANCHO` x `ALTO` of map numbers, cell `i` (1-based) at column `(i-1) mod ANCHO`,
//!   row `(i-1) \ ANCHO` (`ModMinimap.bas`). Filler maps (300, 400...) repeat in many cells.
//! - The edge exits of every `.csm`: a whole border line of tile exits towards the same map
//!   with a constant axial delta (Ullathorpe: west -> map 8 with +74 columns, north -> 5 with
//!   +80 rows). That delta is what lets us draw the neighbour at the right place, so the
//!   exits are the source of truth and the grid is only a cross-check (tests).
//!
//! Neighbouring maps overlap (26 columns / 20 rows are the same ground on both maps), so a
//! tile is drawn from the current map while it is on the map and from the neighbour beyond.
use ao_formats::csm::{AoMap, MAP_SIZE};
use ao_formats::ini::Ini;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    North,
    South,
    West,
    East,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::North, Side::South, Side::West, Side::East];
}

/// A bordering map: its tile = our tile + `delta`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Neighbour {
    pub map: u16,
    pub delta: (i32, i32),
}

/// Minimum exits on a border line to count it as a map crossing (not a door).
const MIN_EDGE_EXITS: usize = 5;

/// Border neighbours of `map` from its edge exits.
pub fn neighbours_from_exits(map: &AoMap) -> HashMap<Side, Neighbour> {
    let mut votes: HashMap<(Side, u16, i32, i32), usize> = HashMap::new();
    let mid = MAP_SIZE / 2;
    for e in &map.exits {
        let (dx, dy) = (e.dest_x - e.x, e.dest_y - e.y);
        // Edge crossings are axial and long (more than half a map).
        let side = if dy == 0 && dx.abs() > mid {
            if dx > 0 { Side::West } else { Side::East }
        } else if dx == 0 && dy.abs() > mid {
            if dy > 0 { Side::North } else { Side::South }
        } else {
            continue;
        };
        *votes.entry((side, e.map, dx, dy)).or_default() += 1;
    }
    let mut out: HashMap<Side, (Neighbour, usize)> = HashMap::new();
    for ((side, m, dx, dy), n) in votes {
        if n < MIN_EDGE_EXITS || m == map.number {
            continue;
        }
        let better = out.get(&side).is_none_or(|(_, best)| n > *best);
        if better {
            out.insert(side, (Neighbour { map: m, delta: (dx, dy) }, n));
        }
    }
    out.into_iter().map(|(s, (nb, _))| (s, nb)).collect()
}

/// `mapsworlddata.dat`: the world grids of the official world map.
pub struct WorldGrid {
    worlds: Vec<(i32, i32, Vec<u16>)>,
}

impl WorldGrid {
    pub fn load(recursos: &Path) -> WorldGrid {
        WorldGrid::parse(&std::fs::read(recursos.join("init").join("mapsworlddata.dat")).unwrap_or_default())
    }

    /// The INIT count is stale (says 3, there are 4 sections): read sections until one is missing.
    pub fn parse(bytes: &[u8]) -> WorldGrid {
        let ini = Ini::parse(bytes);
        let mut worlds = Vec::new();
        for j in 1.. {
            let Some(s) = ini.section(&format!("worldmap{j}")) else { break };
            let get = |k: &str| s.get(k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
            let (w, h) = (get("ancho"), get("alto"));
            let cells = (1..=w * h).map(|i| get(&i.to_string()) as u16).collect();
            worlds.push((w, h, cells));
        }
        WorldGrid { worlds }
    }

    /// (world, column, row) of a map, first match like the official client.
    pub fn locate(&self, map: u16) -> Option<(usize, i32, i32)> {
        self.worlds.iter().enumerate().find_map(|(j, (w, _, c))| c.iter().position(|m| *m == map).map(|i| (j, i as i32 % w, i as i32 / w)))
    }

    pub fn at(&self, world: usize, col: i32, row: i32) -> u16 {
        let Some((w, h, c)) = self.worlds.get(world) else { return 0 };
        if col < 0 || row < 0 || col >= *w || row >= *h {
            return 0;
        }
        c[(row * w + col) as usize]
    }

    /// Map on that side in the grid (0 = none).
    pub fn neighbour(&self, map: u16, side: Side) -> u16 {
        let Some((j, x, y)) = self.locate(map) else { return 0 };
        match side {
            Side::North => self.at(j, x, y - 1),
            Side::South => self.at(j, x, y + 1),
            Side::West => self.at(j, x - 1, y),
            Side::East => self.at(j, x + 1, y),
        }
    }
}

fn on_map(x: i32, y: i32) -> bool {
    (1..=MAP_SIZE).contains(&x) && (1..=MAP_SIZE).contains(&y)
}

/// The current map plus its loaded neighbours (and theirs, for the corners). Owned by the
/// renderer; maps are parsed on demand and kept, so entering a neighbour is instant.
pub struct ContinuousWorld {
    recursos: PathBuf,
    maps: HashMap<u16, AoMap>,
    links: HashMap<u16, HashMap<Side, Neighbour>>,
    current: u16,
    /// Maps parsed since the last call to `take_new`, for texture preloading.
    fresh: Vec<u16>,
    /// Last map change, if it was a crossing to a bordering map: the delta used.
    pub last_crossing: Option<(i32, i32)>,
}

impl ContinuousWorld {
    pub fn new(recursos: PathBuf) -> ContinuousWorld {
        ContinuousWorld { recursos, maps: HashMap::new(), links: HashMap::new(), current: 0, fresh: Vec::new(), last_crossing: None }
    }

    pub fn current(&self) -> u16 {
        self.current
    }

    fn ensure(&mut self, n: u16) -> bool {
        if self.maps.contains_key(&n) {
            return true;
        }
        match AoMap::load(&self.recursos, n) {
            Ok(m) => {
                self.links.insert(n, neighbours_from_exits(&m));
                self.maps.insert(n, m);
                self.fresh.push(n);
                true
            }
            Err(_) => false,
        }
    }

    /// Call once per frame with the current map. Parses at most `budget` new maps per call
    /// (neighbours first, then the corners), and forgets maps that are no longer near.
    pub fn update(&mut self, current: u16, budget: usize) {
        if current != self.current {
            self.last_crossing = self.links.get(&self.current).and_then(|l| l.values().find(|n| n.map == current)).map(|n| n.delta);
            self.current = current;
        }
        let mut left = budget;
        if !self.maps.contains_key(&current) {
            // The game already has this map parsed; we still need its exits here.
            if !self.ensure(current) {
                return;
            }
            left = left.saturating_sub(1);
        }
        let wanted = self.wanted();
        for n in &wanted {
            if left == 0 {
                break;
            }
            if !self.maps.contains_key(n) {
                self.ensure(*n);
                left -= 1;
            }
        }
        // Keep a ring of two around the current map (going back is free too).
        if self.maps.len() > 16 {
            let keep: Vec<u16> = self.ring(2);
            self.maps.retain(|k, _| keep.contains(k));
        }
    }

    /// Neighbours, then the neighbours' neighbours on the perpendicular axis (corners).
    fn wanted(&self) -> Vec<u16> {
        let mut out = Vec::new();
        let Some(l) = self.links.get(&self.current) else { return out };
        for s in Side::ALL {
            if let Some(n) = l.get(&s) {
                out.push(n.map);
            }
        }
        for s in [Side::North, Side::South] {
            if let Some(n) = l.get(&s) {
                if let Some(l2) = self.links.get(&n.map) {
                    for s2 in [Side::West, Side::East] {
                        if let Some(c) = l2.get(&s2) {
                            out.push(c.map);
                        }
                    }
                }
            }
        }
        out
    }

    fn ring(&self, depth: usize) -> Vec<u16> {
        let mut seen = vec![self.current];
        let mut frontier = vec![self.current];
        for _ in 0..depth {
            let mut next = Vec::new();
            for m in frontier {
                for n in self.links.get(&m).into_iter().flat_map(|l| l.values()) {
                    if !seen.contains(&n.map) {
                        seen.push(n.map);
                        next.push(n.map);
                    }
                }
            }
            frontier = next;
        }
        seen
    }

    /// Maps parsed since the last call (to preload their textures).
    pub fn take_new(&mut self) -> Vec<u16> {
        std::mem::take(&mut self.fresh)
    }

    pub fn map(&self, n: u16) -> Option<&AoMap> {
        self.maps.get(&n)
    }

    pub fn neighbour(&self, side: Side) -> Option<Neighbour> {
        self.links.get(&self.current).and_then(|l| l.get(&side)).copied()
    }

    /// Which loaded map draws tile (x, y) of the current map's coordinates, and that tile in
    /// its own coordinates. `None` for the current map itself or nothing known there.
    pub fn resolve(&self, x: i32, y: i32) -> Option<(u16, i32, i32)> {
        if on_map(x, y) {
            return None;
        }
        // Horizontal first, then vertical from there (corners through either neighbour).
        let mut at = (self.current, x, y);
        for _ in 0..2 {
            let (m, tx, ty) = at;
            if on_map(tx, ty) {
                break;
            }
            let side = if tx < 1 {
                Side::West
            } else if tx > MAP_SIZE {
                Side::East
            } else if ty < 1 {
                Side::North
            } else {
                Side::South
            };
            let n = self.links.get(&m).and_then(|l| l.get(&side))?;
            at = (n.map, tx + n.delta.0, ty + n.delta.1);
        }
        let (m, tx, ty) = at;
        (on_map(tx, ty) && self.maps.contains_key(&m)).then_some(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ao_formats::recursos_dir;

    fn root() -> Option<PathBuf> {
        let r = recursos_dir();
        r.join("Mapas").join("mapa1.csm").exists().then_some(r)
    }

    #[test]
    fn grid_parses_the_official_layout() {
        let g = WorldGrid::parse(b"[INIT]\nTOTALWORLDS=1\n[WORLDMAP1]\n1=4\n2=5\n3=6\n4=7\nALTO=2\nANCHO=2\n");
        assert_eq!(g.locate(6), Some((0, 0, 1)));
        assert_eq!(g.neighbour(6, Side::North), 4);
        assert_eq!(g.neighbour(6, Side::East), 7);
        assert_eq!(g.neighbour(6, Side::West), 0);
    }

    #[test]
    fn ullathorpe_neighbours_from_exits_and_grid_agree() {
        let Some(r) = root() else { return };
        let m = AoMap::load(&r, 1).unwrap();
        let n = neighbours_from_exits(&m);
        assert_eq!(n[&Side::North], Neighbour { map: 5, delta: (0, 80) });
        assert_eq!(n[&Side::South], Neighbour { map: 2, delta: (0, -80) });
        assert_eq!(n[&Side::West], Neighbour { map: 8, delta: (74, 0) });
        assert_eq!(n[&Side::East], Neighbour { map: 11, delta: (-74, 0) });
        let g = WorldGrid::load(&r);
        for s in Side::ALL {
            assert_eq!(g.neighbour(1, s), n[&s].map, "{s:?}");
        }
    }

    /// Every edge link found in the exits matches the official grid, for the big cities.
    #[test]
    fn real_cities_match_the_grid() {
        let Some(r) = root() else { return };
        let g = WorldGrid::load(&r);
        let (mut checked, mut wrong) = (0, Vec::new());
        for num in [1u16, 34, 59, 20, 62, 112] {
            let Ok(m) = AoMap::load(&r, num) else { continue };
            for (s, nb) in neighbours_from_exits(&m) {
                checked += 1;
                if g.neighbour(num, s) != nb.map {
                    wrong.push((num, s, nb.map, g.neighbour(num, s)));
                }
            }
        }
        assert!(checked >= 12, "{checked}");
        assert!(wrong.is_empty(), "{wrong:?}");
    }

    /// The overlap strip is the same ground on both maps: crossing is invisible.
    #[test]
    fn overlap_strip_matches_the_neighbour() {
        let Some(r) = root() else { return };
        let a = AoMap::load(&r, 1).unwrap();
        for s in Side::ALL {
            let nb = neighbours_from_exits(&a)[&s];
            let b = AoMap::load(&r, nb.map).unwrap();
            let (mut same, mut total) = (0, 0);
            for y in 1..=MAP_SIZE {
                for x in 1..=MAP_SIZE {
                    let (bx, by) = (x + nb.delta.0, y + nb.delta.1);
                    if on_map(bx, by) {
                        total += 1;
                        same += (a.layer(0, x, y) == b.layer(0, bx, by)) as i32;
                    }
                }
            }
            assert!(total > 0);
            println!("{s:?} -> {}: {same}/{total} floor tiles equal", nb.map);
            assert!(same * 10 >= total * 7, "{s:?}: {same}/{total}");
        }
    }

    #[test]
    fn resolve_beyond_the_border_and_crossing() {
        let Some(r) = root() else { return };
        let mut w = ContinuousWorld::new(r);
        for _ in 0..10 {
            w.update(1, 2);
        }
        assert!(w.take_new().len() >= 5, "current + 4 neighbours + corners");
        assert_eq!(w.resolve(50, 50), None);
        assert_eq!(w.resolve(0, 50), Some((8, 74, 50)));
        assert_eq!(w.resolve(101, 40), Some((11, 27, 40)));
        assert_eq!(w.resolve(50, 0), Some((5, 50, 80)));
        assert_eq!(w.resolve(50, 105), Some((2, 50, 25)));
        // Corner: through the west neighbour, then its north.
        let corner = w.resolve(-2, -3);
        assert!(corner.is_some_and(|(m, x, y)| m != 1 && on_map(x, y)), "{corner:?}");
        // Crossing west: the delta is known, so the camera can stay put.
        w.update(8, 2);
        assert_eq!(w.last_crossing, Some((74, 0)));
        assert!(w.map(1).is_some(), "the previous map stays loaded");
    }
}

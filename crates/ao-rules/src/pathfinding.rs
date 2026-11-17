//! NPC path finding: port of `PathFinding.bas` (`SeekPath`, `ProcessAdjacent`, `MakePath`).
//!
//! A* over 4-connected tiles with an euclidean heuristic plus optional random noise, a
//! search radius bounded by the NPC's "intelligence" (`RangoVision`) and a step budget.
//! When the target cannot be reached and `closest` is set, the path to the explored tile
//! nearest to the target is returned (the VB behaviour that makes creatures hug walls
//! instead of freezing). Walkability is a closure so the server decides occupancy, water,
//! triggers and blocked sides (`IsWalkable`), and randomness is injected so the caller
//! keeps it deterministic (world seed).
use crate::Dir;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// `MAX_PATH_LENGTH` (Declares.bas), also the default `NPC_PATHFINDING_MAX_STEPS`.
pub const MAX_PATH_LENGTH: i32 = 512;
/// `NPC_PATHFINDING_NOISE` default (ServerConfig.cls, `NPCPathfindingNoise=0.75`).
pub const DEFAULT_NOISE: f32 = 0.75;
/// `PATH_VISION_DELTA` (Declares.bas): vision gained after a failed search.
pub const PATH_VISION_DELTA: i32 = 5;
/// `DEFAULT_NPC_MAX_VISION_RANGE` (Consts.bas).
pub const MAX_VISION_RANGE: i32 = 100;
/// `DEFAULT_PATH_RECOMPUTE_COOLDOWN_MS` (Consts.bas).
pub const RECOMPUTE_COOLDOWN_MS: u64 = 250;

/// Map side (tiles 1..=100 like the VB `XMinMapSize..XMaxMapSize`).
const SIDE: i32 = 101;

pub struct Query {
    pub from: (i32, i32),
    pub to: (i32, i32),
    /// `pathFindingInfo.RangoVision`.
    pub vision: i32,
    /// `NPC_PATHFINDING_MAX_STEPS`.
    pub max_steps: i32,
    /// `NPC_PATHFINDING_NOISE` (0 disables it).
    pub noise: f32,
    /// Return a partial path to the closest explored tile when the target is unreachable.
    pub closest: bool,
}

#[derive(Clone, Copy)]
struct Open {
    f: f32,
    order: u32,
    x: i32,
    y: i32,
}

impl PartialEq for Open {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Open {}
impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Open {
    // Min-heap on the estimated total, first opened wins ties (stable, like the VB scan
    // that keeps the first minimum it finds).
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal).then(o.order.cmp(&self.order))
    }
}

fn idx(x: i32, y: i32) -> usize {
    (y * SIDE + x) as usize
}

fn inside(x: i32, y: i32) -> bool {
    (1..SIDE).contains(&x) && (1..SIDE).contains(&y)
}

fn euclid(a: (i32, i32), b: (i32, i32)) -> f32 {
    let (dx, dy) = ((a.0 - b.0) as f32, (a.1 - b.1) as f32);
    (dx * dx + dy * dy).sqrt()
}

/// `SeekPath`. `walkable(x, y, dir)` says whether the NPC may enter (x, y) moving `dir`
/// (`IsWalkable`). `rand()` returns uniform u32s for `ShuffleHeadings` and the noise.
/// Returns the steps to take in order (first element = next tile), or `None`.
pub fn seek_path(q: &Query, mut walkable: impl FnMut(i32, i32, Dir) -> bool, mut rand: impl FnMut() -> u32) -> Option<Vec<(i32, i32)>> {
    if q.from == q.to || !inside(q.from.0, q.from.1) {
        return None;
    }
    let n = (SIDE * SIDE) as usize;
    let mut dist = vec![i32::MAX; n];
    let mut closed = vec![false; n];
    let mut prev = vec![(0i32, 0i32); n];
    let mut heap = BinaryHeap::new();
    let mut order = 0u32;

    // MaxDistance = Min(MAX_PATH_LENGTH, TileDistance(npc, target) + RangoVision)
    let manhattan = (q.from.0 - q.to.0).abs() + (q.from.1 - q.to.1).abs();
    let max_distance = MAX_PATH_LENGTH.min(manhattan + q.vision);
    dist[idx(q.from.0, q.from.1)] = 0;
    let start_h = euclid(q.from, q.to);
    heap.push(Open { f: start_h, order, x: q.from.0, y: q.from.1 });
    let mut closest = (start_h, q.from);
    let mut headings = [Dir::Up, Dir::Right, Dir::Down, Dir::Left];
    let mut steps = 0;

    while let Some(v) = heap.pop() {
        if steps >= q.max_steps {
            break;
        }
        let vi = idx(v.x, v.y);
        if closed[vi] {
            continue; // stale entry (a shorter way was found later)
        }
        steps += 1;
        if (v.x, v.y) == q.to {
            return Some(make_path(&prev, q.from, q.to));
        }
        closed[vi] = true;
        if dist[vi] >= max_distance {
            continue;
        }
        // ShuffleHeadings: Fisher-Yates with the injected RNG.
        for i in (1..headings.len()).rev() {
            let j = (rand() as usize) % (i + 1);
            headings.swap(i, j);
        }
        for dir in headings {
            let (dx, dy) = dir.delta();
            let (x, y) = (v.x + dx, v.y + dy);
            if !inside(x, y) {
                continue;
            }
            let ni = idx(x, y);
            if closed[ni] || !walkable(x, y, dir) {
                continue;
            }
            let d = dist[vi] + 1;
            if d >= dist[ni] {
                continue;
            }
            dist[ni] = d;
            prev[ni] = (v.x, v.y);
            let h = euclid((x, y), q.to);
            let noise = if q.noise > 0.0 { (rand() % 10_000) as f32 / 10_000.0 * q.noise } else { 0.0 };
            order += 1;
            heap.push(Open { f: d as f32 + h + noise, order, x, y });
            if h < closest.0 {
                closest = (h, (x, y));
            }
        }
    }
    if q.closest && closest.1 != q.from {
        return Some(make_path(&prev, q.from, closest.1));
    }
    None
}

fn make_path(prev: &[(i32, i32)], from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    let mut path = vec![to];
    let mut cur = to;
    while prev[idx(cur.0, cur.1)] != from {
        cur = prev[idx(cur.0, cur.1)];
        path.push(cur);
    }
    path.reverse();
    path
}

/// `GetHeadingFromWorldPos`: heading from one tile to an adjacent (or any) tile.
pub fn heading_to(from: (i32, i32), to: (i32, i32)) -> Dir {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if dx.abs() > dy.abs() {
        if dx > 0 { Dir::Right } else { Dir::Left }
    } else if dy > 0 {
        Dir::Down
    } else {
        Dir::Up
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg() -> impl FnMut() -> u32 {
        let mut s = 12345u32;
        move || {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
            s >> 8
        }
    }

    fn q(from: (i32, i32), to: (i32, i32), closest: bool) -> Query {
        Query { from, to, vision: 15, max_steps: MAX_PATH_LENGTH, noise: DEFAULT_NOISE, closest }
    }

    #[test]
    fn goes_around_a_wall() {
        // Vertical wall at x = 50 from y = 40 to 60.
        let wall = |x: i32, y: i32| x == 50 && (40..=60).contains(&y);
        // Too dumb (vision 15: search bounded by 10 + 15 steps) for a 32-step detour...
        assert!(seek_path(&q((45, 50), (55, 50), false), |x, y, _| !wall(x, y), lcg()).is_none());
        // ...but a wider vision (what failed searches grant) finds it.
        let wide = Query { vision: 30, ..q((45, 50), (55, 50), false) };
        let p = seek_path(&wide, |x, y, _| !wall(x, y), lcg()).expect("path");
        assert_eq!(*p.last().unwrap(), (55, 50));
        assert!(p.iter().all(|&(x, y)| !wall(x, y)));
        // Contiguous 4-connected steps.
        let mut cur = (45, 50);
        for &s in &p {
            assert_eq!((s.0 - cur.0).abs() + (s.1 - cur.1).abs(), 1);
            cur = s;
        }
        assert!(p.len() >= 10 + 2 * 10, "must go round the 21-tile wall, got {}", p.len());
    }

    #[test]
    fn unreachable_gives_closest_or_none() {
        // Target enclosed in a box.
        let boxed = |x: i32, y: i32| (x - 60).abs().max((y - 50).abs()) == 2;
        assert!(seek_path(&q((40, 50), (60, 50), false), |x, y, _| !boxed(x, y), lcg()).is_none());
        let p = seek_path(&q((40, 50), (60, 50), true), |x, y, _| !boxed(x, y), lcg()).expect("partial");
        let end = *p.last().unwrap();
        assert!(euclid(end, (60, 50)) <= 3.0, "ends next to the box, got {end:?}");
    }

    #[test]
    fn deterministic_with_same_rng() {
        let a = seek_path(&q((10, 10), (30, 25), false), |_, _, _| true, lcg());
        let b = seek_path(&q((10, 10), (30, 25), false), |_, _, _| true, lcg());
        assert_eq!(a, b);
    }
}

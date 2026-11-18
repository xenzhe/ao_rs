//! Pure parts of the ranged creature AI (`AI_NPC.bas`: `AI_RangeAttack`,
//! `ComputeNpcRangedRetreatDestination`, `EnsureNpcOrbitDirection`) with the official
//! `[AI]` defaults of `Server.ini`.

/// `NPC_ORBIT_REEVALUATE_MS`: how long an orbit side is kept.
pub const ORBIT_REEVALUATE_MS: u64 = 1800;
/// `NPC_ORBIT_TANGENT_WEIGHT`: share of sideways motion while backing off.
pub const ORBIT_TANGENT_WEIGHT: f64 = 0.35;
/// `NPC_RETREAT_DISTANCE_BUFFER`: closer than `PreferedRange - buffer` = back off.
pub const RETREAT_DISTANCE_BUFFER: f64 = 0.75;
/// `NPC_ORBIT_STEP_DEGREES`: rotation per orbit step at the right distance.
pub const ORBIT_STEP_DEGREES: f64 = 55.0;

/// `Distance`: Euclidean.
pub fn distance(a: (i32, i32), b: (i32, i32)) -> f64 {
    (((a.0 - b.0) as f64).powi(2) + ((a.1 - b.1) as f64).powi(2)).sqrt()
}

/// VB `CInt`: banker's rounding.
fn cint(v: f64) -> i32 {
    crate::combat::vb_round(v) as i32
}

/// One try of `ComputeNpcRangedRetreatDestination`: the tile at `preferred` tiles from the
/// target, away from it and turned to the `orbit` side (+1 / -1). Too close, it backs off
/// with a bit of tangent; otherwise it rotates `ORBIT_STEP_DEGREES` around the target.
pub fn retreat_candidate(npc: (i32, i32), target: (i32, i32), preferred: f64, orbit: i32) -> (i32, i32) {
    let (mut bx, by) = ((npc.0 - target.0) as f64, (npc.1 - target.1) as f64);
    if bx == 0.0 && by == 0.0 {
        bx = 1.0;
    }
    let len = (bx * bx + by * by).sqrt();
    let (nx, ny) = (bx / len, by / len);
    let o = orbit.signum() as f64;
    let (mut ox, mut oy) = if distance(npc, target) <= preferred - RETREAT_DISTANCE_BUFFER {
        let (tx, ty) = (-ny, nx);
        (nx * (1.0 - ORBIT_TANGENT_WEIGHT) + tx * o * ORBIT_TANGENT_WEIGHT, ny * (1.0 - ORBIT_TANGENT_WEIGHT) + ty * o * ORBIT_TANGENT_WEIGHT)
    } else {
        let a = (o * ORBIT_STEP_DEGREES).to_radians();
        (nx * a.cos() - ny * a.sin(), nx * a.sin() + ny * a.cos())
    };
    let l = (ox * ox + oy * oy).sqrt();
    if l > 0.0 {
        ox /= l;
        oy /= l;
    }
    (target.0 + cint(ox * preferred), target.1 + cint(oy * preferred))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retreat_backs_off_to_the_preferred_range() {
        // Target 2 tiles west, wants 5: ends about 5 tiles east of the target, a bit to one side.
        let c = retreat_candidate((12, 10), (10, 10), 5.0, 1);
        assert!((distance(c, (10, 10)) - 5.0).abs() <= 1.0, "{c:?}");
        assert!(c.0 > 12);
        let other = retreat_candidate((12, 10), (10, 10), 5.0, -1);
        assert_ne!(c, other, "the two orbit sides differ");
        // At range: it circles (rotates around the target).
        let r = retreat_candidate((15, 10), (10, 10), 5.0, 1);
        assert!((distance(r, (10, 10)) - 5.0).abs() <= 1.0 && r != (15, 10));
    }
}

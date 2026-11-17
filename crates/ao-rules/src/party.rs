//! Pure rules of parties (experience sharing). OWNER: Opus C (docs/P2_CONTRATO.md).
//!
//! Sources: ModGrupos.bas (`InvitarMiembro`), Declares.bas (`Tgrupo.Miembros(1 To 6)`),
//! SistemaCombate.bas (`CalcularDarExpGrupal`), Example.Configuracion.ini (`PartyELV`,
//! `PartyELVwLeadership`, `LeadershipExpPartyBonus`).

/// `Tgrupo.Miembros(1 To 6)`: leader included.
pub const MAX_MEMBERS: usize = 6;
/// `PartyELV`: maximum level difference between leader and guest.
pub const PARTY_ELV: u32 = 4;
/// `PartyELVwLeadership`: the same when the leader has enough leadership.
pub const PARTY_ELV_LEADERSHIP: u32 = 5;
/// `LeadershipExpPartyBonus`.
pub const LEADERSHIP_EXP_BONUS: f64 = 1.01;
/// `CalcularDarExpGrupal`: members get experience when `Distancia(...) < 20`.
pub const EXP_SHARE_DISTANCE: i32 = 20;
/// `WorkLeftClick` (`e_Skill.Grupo`): the guest must be within 8 tiles (Manhattan).
pub const INVITE_DISTANCE: i32 = 8;
/// 0-based index of `e_Skill.liderazgo` (11) in the skills array.
pub const SKILL_LEADERSHIP: usize = 10;
/// 0-based index of `e_Atributos.Carisma` (5).
pub const ATTR_CHARISMA: usize = 4;

/// `UserSkills(liderazgo) >= 15 - Carisma / 2` (VB `/` is a float division).
pub fn has_leadership(leadership: u8, charisma: u8) -> bool {
    leadership as f64 >= 15.0 - charisma as f64 / 2.0
}

/// `InvitarMiembro`: the faction pairs (by `e_Facciones` status) that cannot share a party.
/// Ported literally: criminal(0) / citizen(1) / chaos(2) / royal army(3); the councils
/// (4, 5) are not in the original matrix.
pub fn factions_compatible(leader: u8, guest: u8) -> bool {
    !matches!((leader, guest), (0, 1) | (1, 0) | (1, 2) | (2, 1) | (3, 0) | (0, 3) | (3, 2) | (2, 3))
}

/// `InvitarMiembro`: level gap allowed between leader and guest.
pub fn level_gap_ok(leader_level: u32, guest_level: u32, leader_has_leadership: bool) -> bool {
    let max = if leader_has_leadership { PARTY_ELV_LEADERSHIP } else { PARTY_ELV };
    leader_level.abs_diff(guest_level) <= max
}

/// `Distancia` (Matematicas.bas): Manhattan plus 100 per map of difference.
pub fn distancia(a: (u16, i32, i32), b: (u16, i32, i32)) -> i32 {
    (a.1 - b.1).abs() + (a.2 - b.2).abs() + (a.0 as i32 - b.0 as i32).abs() * 100
}

/// VB6 assigns a Double to a Long with banker's rounding (half to even).
pub fn vb_round(v: f64) -> i64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 && r % 2.0 != 0.0 {
        (r - v.signum()) as i64
    } else {
        r as i64
    }
}

/// One member as `CalcularDarExpGrupal` sees it.
#[derive(Clone, Copy, Debug)]
pub struct ShareMember {
    pub alive: bool,
    pub pos: (u16, i32, i32),
    pub leadership: bool,
}

/// `CalcularDarExpGrupal`: experience each member gets (same order as `members`) when
/// the member at `earner` (map, x, y) earns `amount` (already multiplied by `ExpMult`).
///
/// - The divisor counts the live members on the earner's map within 20 tiles, plus one
///   when the leader is on another map (that share is lost).
/// - Every live member within `Distancia < 20` gets `amount / divisor` (rounded), times
///   `LeadershipExpPartyBonus` if it has leadership.
pub fn share_exp(amount: u32, earner: (u16, i32, i32), leader_map: u16, members: &[ShareMember]) -> Vec<u32> {
    let near = |m: &ShareMember| m.alive && distancia(earner, m.pos) < EXP_SHARE_DISTANCE;
    let mut count = members.iter().filter(|m| m.alive && m.pos.0 == earner.0 && near(m)).count();
    if leader_map != earner.0 {
        count += 1;
    }
    if count == 0 || amount == 0 {
        return vec![0; members.len()];
    }
    let each = vb_round(amount as f64 / count as f64);
    members
        .iter()
        .map(|m| {
            if !near(m) || each <= 0 {
                return 0;
            }
            let e = if m.leadership { vb_round(each as f64 * LEADERSHIP_EXP_BONUS) } else { each };
            e.clamp(0, u32::MAX as i64) as u32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(alive: bool, pos: (u16, i32, i32)) -> ShareMember {
        ShareMember { alive, pos, leadership: false }
    }

    #[test]
    fn rounding_is_bankers() {
        assert_eq!(vb_round(2.5), 2);
        assert_eq!(vb_round(3.5), 4);
        assert_eq!(vb_round(1.4), 1);
        assert_eq!(vb_round(1.6), 2);
    }

    #[test]
    fn splits_among_near_live_members() {
        let at = (2, 50, 50);
        let ms = [m(true, at), m(true, (2, 55, 50)), m(false, (2, 51, 50)), m(true, (2, 90, 90)), m(true, (3, 50, 50))];
        // Valid: the earner and the one 5 tiles away; dead, far and other map out.
        assert_eq!(share_exp(10, at, 2, &ms), vec![5, 5, 0, 0, 0]);
    }

    #[test]
    fn leader_on_another_map_takes_a_lost_share() {
        let at = (2, 50, 50);
        let ms = [m(true, (1, 50, 50)), m(true, at), m(true, (2, 51, 50))];
        // 2 valid + the absent leader = 3: 9 / 3 = 3 each, the leader gets nothing.
        assert_eq!(share_exp(9, at, 1, &ms), vec![0, 3, 3]);
    }

    #[test]
    fn leadership_bonus() {
        let at = (2, 50, 50);
        let ms = [ShareMember { alive: true, pos: at, leadership: true }, m(true, at)];
        assert_eq!(share_exp(200, at, 2, &ms), vec![101, 100]);
        assert!(has_leadership(5, 20));
        assert!(!has_leadership(4, 20));
    }

    #[test]
    fn faction_matrix_and_levels() {
        assert!(factions_compatible(1, 1));
        assert!(factions_compatible(1, 3));
        assert!(!factions_compatible(1, 0));
        assert!(!factions_compatible(3, 2));
        assert!(factions_compatible(0, 2));
        assert!(level_gap_ok(10, 14, false));
        assert!(!level_gap_ok(10, 15, false));
        assert!(level_gap_ok(10, 15, true));
    }
}

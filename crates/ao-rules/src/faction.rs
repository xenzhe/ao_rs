//! Pure rules of factions (ranks, criminal status, who may attack whom). OWNER: Opus C (docs/P2_CONTRATO.md).
//!
//! Sources: Declares.bas (`e_Facciones`), GameLogic.bas (`esCiudadano`, `esCriminal`,
//! `esArmada`, `esCaos`), SistemaCombate.bas (`PuedeAtacar`), Modulo_UsUaRiOs.bas
//! (`ContarMuerte`, `HandleFactionScoreForKill`, `CalculateBaseFactionScore`...),
//! FileIO.bas (`LoadRangosFaccion`, `LoadRecompensasFaccion`), ModFacciones.bas.
use ao_formats::dat::obj::ObjData;

/// `e_Facciones`.
pub const CRIMINAL: u8 = 0;
pub const CITIZEN: u8 = 1;
pub const CHAOS: u8 = 2;
pub const ROYAL: u8 = 3;
/// Concilio de las Sombras (chaos leaders).
pub const COUNCIL_CHAOS: u8 = 4;
/// Consejo de Banderbill (royal leaders).
pub const COUNCIL_ROYAL: u8 = 5;

/// `GameLogic.FactionCanHelpFaction`: citizens, Armada and Consejo help neither the chaos
/// side nor criminals; Caos and Concilio do not help the royal side; criminals help anyone.
pub fn faction_can_help(source: u8, target: u8) -> bool {
    match source {
        CITIZEN | ROYAL | COUNCIL_ROYAL => !matches!(target, CHAOS | COUNCIL_CHAOS | CRIMINAL),
        CHAOS | COUNCIL_CHAOS => !matches!(target, ROYAL | COUNCIL_ROYAL | CITIZEN),
        _ => true,
    }
}

#[cfg(test)]
mod help_tests {
    use super::*;

    #[test]
    fn who_helps_whom() {
        assert!(faction_can_help(CITIZEN, ROYAL) && !faction_can_help(CITIZEN, CRIMINAL) && !faction_can_help(ROYAL, CHAOS));
        assert!(faction_can_help(CHAOS, CRIMINAL) && !faction_can_help(CHAOS, CITIZEN));
        assert!(faction_can_help(CRIMINAL, CITIZEN) && faction_can_help(CRIMINAL, CHAOS));
    }
}

/// `MAX_FACTION_ENLISTMENTS` (ModFacciones.bas): once enlisted, never again.
pub const MAX_FACTION_ENLISTMENTS: u32 = 0;
/// `MAXUSERMATADOS` (Declares.bas).
pub const MAX_USER_KILLS: u32 = 65000;
/// `ContarMuerte`: no kill counted with more than 10 levels of difference.
pub const MAX_KILL_LEVEL_GAP: i32 = 10;
/// `HandleFactionScoreForKill`: cap per kill.
pub const MAX_SCORE_PER_KILL: i32 = 20;
/// `HandleFactionRecruiterNpcInteraction`: `Distancia > 3` is too far.
pub const RECRUITER_DISTANCE: i32 = 3;

pub fn is_citizen(s: u8) -> bool {
    s == CITIZEN
}
pub fn is_criminal(s: u8) -> bool {
    s == CRIMINAL
}
/// `esArmada`: royal army or its council.
pub fn is_royal(s: u8) -> bool {
    s == ROYAL || s == COUNCIL_ROYAL
}
/// `esCaos`: chaos legion or its council.
pub fn is_chaos(s: u8) -> bool {
    s == CHAOS || s == COUNCIL_CHAOS
}
/// Citizen side (citizen, royal army, its council): victims that make a citizen criminal.
pub fn is_lawful(s: u8) -> bool {
    is_citizen(s) || is_royal(s)
}

pub fn status_name(s: u8) -> &'static str {
    match s {
        CRIMINAL => "Criminal",
        CITIZEN => "Ciudadano",
        CHAOS => "Legión Oscura",
        ROYAL => "Ejército Real",
        COUNCIL_CHAOS => "Concilio de las Sombras",
        COUNCIL_ROYAL => "Consejo de Banderbill",
        _ => "?",
    }
}

/// One faction rank (`t_RangoFaccion`).
#[derive(Clone, Debug, PartialEq)]
pub struct Rank {
    pub rank: u8,
    pub level: u32,
    pub score: i32,
    pub title: String,
}

/// A reward line of recompensas_faccion.dat (`t_RecompensaFaccion`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reward {
    pub rank: u8,
    pub obj: u16,
}

/// rangos_faccion.dat + recompensas_faccion.dat.
#[derive(Clone, Debug, Default)]
pub struct FactionTables {
    /// `[ArmadaReal]`, index 0 = rank 1 (`RangosFaccion(2r - 1)`).
    pub royal: Vec<Rank>,
    /// `[LegionCaos]`, index 0 = rank 1 (`RangosFaccion(2r)`).
    pub chaos: Vec<Rank>,
    /// Both factions mixed: the object's `Real`/`Caos` says whose it is (`CanUseObject`).
    pub rewards: Vec<Reward>,
}

impl FactionTables {
    /// `MaxRangoFaccion` (`[INIT] NumRangos`).
    pub fn max_rank(&self) -> u8 {
        self.royal.len().min(self.chaos.len()) as u8
    }

    fn ranks(&self, royal: bool) -> &[Rank] {
        if royal { &self.royal } else { &self.chaos }
    }

    /// Rank `r` (1-based) of a faction.
    pub fn rank(&self, royal: bool, r: u8) -> Option<&Rank> {
        self.ranks(royal).get((r as usize).wrapping_sub(1))
    }

    /// `ProximoRango`: the rank after `current`.
    pub fn next_rank(&self, royal: bool, current: u8) -> Option<&Rank> {
        if current >= self.max_rank() {
            return None;
        }
        self.rank(royal, current + 1)
    }
}

fn ini_lines(text: &str) -> impl Iterator<Item = (String, &str)> {
    let mut section = String::new();
    text.lines().filter_map(move |l| {
        let l = l.trim();
        if l.is_empty() || l.starts_with('\'') || l.starts_with(';') {
            return None;
        }
        if l.starts_with('[') && l.ends_with(']') {
            section = l[1..l.len() - 1].to_ascii_lowercase();
            return None;
        }
        Some((section.clone(), l))
    })
}

/// `LoadRangosFaccion`: `<N>Rango=<NivelRequerido>-<PuntosFaccionarios>-<Título>` in
/// `[ArmadaReal]` and `[LegionCaos]`, `NumRangos` of each.
pub fn parse_ranks(text: &str) -> (Vec<Rank>, Vec<Rank>) {
    let mut num = 0usize;
    let (mut royal, mut chaos) = (Vec::new(), Vec::new());
    for (sec, line) in ini_lines(text) {
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim();
        if sec == "init" && k.eq_ignore_ascii_case("NumRangos") {
            num = v.trim().parse().unwrap_or(0);
            continue;
        }
        let Some(n) = k.to_ascii_lowercase().strip_suffix("rango").and_then(|n| n.parse::<u8>().ok()) else { continue };
        let mut parts = v.trim().splitn(3, '-');
        let level = parts.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        let score = parts.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        let title = parts.next().unwrap_or("").trim().to_string();
        let r = Rank { rank: n, level, score, title };
        match sec.as_str() {
            "armadareal" => royal.push(r),
            "legioncaos" => chaos.push(r),
            _ => {}
        }
    }
    for v in [&mut royal, &mut chaos] {
        v.sort_by_key(|r| r.rank);
        v.truncate(num);
    }
    (royal, chaos)
}

/// `LoadRecompensasFaccion`: `Recompensa<N>=<rango>-<objIndex>`.
pub fn parse_rewards(text: &str) -> Vec<Reward> {
    let mut out = Vec::new();
    for (sec, line) in ini_lines(text) {
        let Some((k, v)) = line.split_once('=') else { continue };
        if sec != "recompensas" || !k.trim().to_ascii_lowercase().starts_with("recompensa") {
            continue;
        }
        let Some((r, o)) = v.trim().split_once('-') else { continue };
        if let (Ok(rank), Ok(obj)) = (r.trim().parse(), o.trim().parse()) {
            out.push(Reward { rank, obj });
        }
    }
    out
}

/// What `PuedeAtacar` needs to know of each side (faction part).
#[derive(Clone, Copy, Debug)]
pub struct Fighter {
    pub status: u8,
    /// `flags.Seguro` (citizens only).
    pub safe: bool,
    /// `flags.LegionarySecure`.
    pub legion_safe: bool,
    /// `Grupo.Id` (0 = none).
    pub party: u64,
}

/// `PuedeAtacar` from "same group" on, the faction checks (the dead, arenas and safe
/// zones are checked by combat before). `victim_map_safe` is `MapInfo.Seguro`.
pub fn attack_refusal(a: Fighter, v: Fighter, victim_map_safe: bool) -> Option<&'static str> {
    if a.party != 0 && a.party == v.party {
        return Some("No podés atacar a un miembro de tu grupo.");
    }
    if is_royal(a.status) {
        if is_royal(v.status) {
            return Some("Los miembros del Ejercito Real tienen prohibido atacarse entre sí.");
        }
        if is_citizen(v.status) {
            return Some("Los miembros del Ejercito Real tienen prohibido atacar ciudadanos.");
        }
    } else if is_citizen(a.status) {
        if a.safe {
            if is_citizen(v.status) {
                return Some("No podés atacar ciudadanos, para hacerlo debes desactivar el seguro.");
            }
            if is_royal(v.status) {
                return Some("No podés atacar miembros del Ejercito Real, para hacerlo debes desactivar el seguro.");
            }
        }
    } else if is_chaos(a.status) && is_chaos(v.status) && a.legion_safe && !victim_map_safe {
        return Some("Los miembros de la Legión Oscura no se pueden atacar entre sí.");
    }
    None
}

/// `UsuarioAtacadoPorUsuario`: a citizen hitting a citizen or the army turns criminal.
pub fn attack_turns_criminal(attacker: u8, victim: u8) -> bool {
    is_citizen(attacker) && is_lawful(victim)
}

/// `CalculateBaseFactionScore`.
pub fn base_score(attacker_level: u32, target_level: u32) -> i32 {
    let (a, t) = (attacker_level as i32, target_level as i32);
    if a < t {
        10 + t - a.max(0)
    } else {
        10 - (a - t).max(0)
    }
}

/// `ShouldApplyFactionBonus`: army vs legion (either way, councils included).
fn faction_bonus(a: u8, t: u8) -> bool {
    (is_chaos(a) && is_royal(t)) || (is_royal(a) && is_chaos(t))
}

/// `AreLegionsOrCouncils`.
pub fn both_chaos(a: u8, t: u8) -> bool {
    is_chaos(a) && is_chaos(t)
}

/// Result of `ContarMuerte` for the killer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KillCount {
    pub criminals: u32,
    pub citizens: u32,
    /// Change of faction score (may be negative; the result is never below 0).
    pub score: i32,
}

/// `ContarMuerte` + `HandleFactionScoreForKill` + `PenalizeFactionScoreLegionAndCouncil`
/// (assists are not counted: they need the last-helper/last-attacker tracking).
/// `rekill` = the victim was already killed by this killer within `FactionReKillTime`.
pub fn count_kill(killer: u8, killer_level: u32, victim: u8, victim_level: u32, victim_newbie: bool, rekill: bool) -> Option<KillCount> {
    if victim_newbie || killer_level as i32 - victim_level as i32 > MAX_KILL_LEVEL_GAP {
        return None;
    }
    let mut k = KillCount::default();
    let base = base_score(killer_level, victim_level);
    let penalty = || {
        let mut s = base;
        if faction_bonus(killer, victim) {
            s = (s as f64 * 1.5) as i32;
        }
        -(s.min(MAX_SCORE_PER_KILL).abs())
    };
    let gain = || {
        let mut s = base;
        if faction_bonus(killer, victim) {
            s = (s as f64 * 1.5) as i32;
        } else if is_criminal(victim) && (is_criminal(killer) || is_chaos(killer)) {
            s = (s as f64 * 0.5) as i32;
        }
        s.clamp(0, MAX_SCORE_PER_KILL)
    };
    if is_criminal(victim) || is_chaos(victim) {
        if both_chaos(killer, victim) {
            k.score += penalty();
        }
        if rekill {
            return Some(k);
        }
        k.criminals = 1;
        if is_lawful(killer) || (is_criminal(victim) && (is_criminal(killer) || is_chaos(killer))) {
            // HandleFactionScoreForKill penalizes again between legions instead of adding.
            k.score += if both_chaos(killer, victim) { penalty() } else { gain() };
        }
    } else if is_lawful(victim) {
        if rekill {
            return Some(k);
        }
        k.citizens = 1;
        if is_criminal(killer) || is_chaos(killer) {
            k.score += gain();
        }
    }
    Some(k)
}

/// `CanUseObject` as `DarRecompensas` uses it: faction (`FaccionPuedeUsarItem`), hierarchy
/// (`JerarquiaPuedeUsarItem`), level, class, gender and race.
pub fn reward_usable(o: &ObjData, status: u8, rank: u8, level: u32, class: u8, gender: u8, race: u8) -> bool {
    let faction_ok = if o.royal == 1 {
        if o.leaders_only { status == COUNCIL_ROYAL } else { is_royal(status) }
    } else if o.chaos == 1 {
        if o.leaders_only { status == COUNCIL_CHAOS } else { is_chaos(status) }
    } else {
        true
    };
    faction_ok
        && rank as i32 >= o.rank
        && level as i32 >= o.min_level
        && (o.max_level <= 0 || level as i32 <= o.max_level)
        && crate::items::class_can_use(o, class)
        && crate::items::gender_can_use(o, gender)
        && crate::items::race_can_use(o, race, gender)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RANKS: &str = "'''\n' comment\n[INIT]\nNumRangos=2\n\n[ArmadaReal]\n1Rango=23-500-Recluta Real\n2Rango=26-1500-Soldado Real\n3Rango=1-1-Extra\n\n[LegionCaos]\n1Rango=23-500-Iniciado Oscuro\n2Rango=26-1500-Acólito Impío\n";

    #[test]
    fn parses_ranks_and_rewards() {
        let (r, c) = parse_ranks(RANKS);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], Rank { rank: 1, level: 23, score: 500, title: "Recluta Real".into() });
        assert_eq!(c[1].title, "Acólito Impío");
        let w = parse_rewards("[INIT]\nNumRecompensas=2\n[Recompensas]\n' x\nRecompensa1=1-1493\nRecompensa2=2-1524\n");
        assert_eq!(w, vec![Reward { rank: 1, obj: 1493 }, Reward { rank: 2, obj: 1524 }]);
        let t = FactionTables { royal: r, chaos: c, rewards: w };
        assert_eq!(t.max_rank(), 2);
        assert_eq!(t.next_rank(true, 1).unwrap().score, 1500);
        assert!(t.next_rank(true, 2).is_none());
    }

    fn f(status: u8) -> Fighter {
        Fighter { status, safe: true, legion_safe: true, party: 0 }
    }

    #[test]
    fn who_may_attack_whom() {
        assert!(attack_refusal(f(ROYAL), f(CITIZEN), false).is_some());
        assert!(attack_refusal(f(ROYAL), f(ROYAL), false).is_some());
        assert!(attack_refusal(f(ROYAL), f(CRIMINAL), false).is_none());
        assert!(attack_refusal(f(CITIZEN), f(CITIZEN), false).is_some());
        assert!(attack_refusal(Fighter { safe: false, ..f(CITIZEN) }, f(CITIZEN), false).is_none());
        assert!(attack_refusal(f(CITIZEN), f(CRIMINAL), false).is_none());
        assert!(attack_refusal(f(CHAOS), f(CHAOS), false).is_some());
        assert!(attack_refusal(Fighter { legion_safe: false, ..f(CHAOS) }, f(CHAOS), false).is_none());
        let p = Fighter { party: 7, safe: false, ..f(CRIMINAL) };
        assert!(attack_refusal(p, p, false).is_some());
        assert!(attack_turns_criminal(CITIZEN, ROYAL));
        assert!(!attack_turns_criminal(CITIZEN, CRIMINAL));
    }

    #[test]
    fn kill_scores() {
        assert_eq!(base_score(30, 30), 10);
        assert_eq!(base_score(25, 30), 15);
        assert_eq!(base_score(30, 25), 5);
        // Citizen kills a criminal of its level: +10.
        assert_eq!(count_kill(CITIZEN, 30, CRIMINAL, 30, false, false), Some(KillCount { criminals: 1, citizens: 0, score: 10 }));
        // Army kills legion: x1.5.
        assert_eq!(count_kill(ROYAL, 30, CHAOS, 30, false, false).unwrap().score, 15);
        // Criminal kills criminal: x0.5.
        assert_eq!(count_kill(CRIMINAL, 30, CRIMINAL, 30, false, false).unwrap().score, 5);
        // Criminal kills a citizen: counter and points.
        assert_eq!(count_kill(CRIMINAL, 30, CITIZEN, 30, false, false), Some(KillCount { criminals: 0, citizens: 1, score: 10 }));
        // Citizen kills a citizen: counted, no points.
        assert_eq!(count_kill(CITIZEN, 30, CITIZEN, 30, false, false), Some(KillCount { criminals: 0, citizens: 1, score: 0 }));
        // Legion kills legion: penalized.
        assert_eq!(count_kill(CHAOS, 30, CHAOS, 30, false, false).unwrap().score, -10);
        // Newbies, big gaps and re-kills.
        assert_eq!(count_kill(CITIZEN, 30, CRIMINAL, 12, true, false), None);
        assert_eq!(count_kill(CITIZEN, 41, CRIMINAL, 30, false, false), None);
        assert_eq!(count_kill(CITIZEN, 30, CRIMINAL, 30, false, true), Some(KillCount::default()));
        // Cap of 20.
        assert_eq!(count_kill(CITIZEN, 20, CRIMINAL, 35, false, false).unwrap().score, 20);
    }
}

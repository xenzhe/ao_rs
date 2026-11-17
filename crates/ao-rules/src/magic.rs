//! Official AO20 spell, status and meditation formulas (pure, no I/O).
//!
//! Sources (server VB6, `argentum-online-server-classic/Codigo`):
//! - `modHechizos.bas`: `PuedeLanzar`, `HechizoPropUsuario`, `HechizoPropNPC`,
//!   `HechizoEstadoUsuario`, `GetSpellStaminaCost`.
//! - `Trabajo.bas::DoMeditar`, `Protocol.bas::HandleMeditate`, `Meditaciones.dat`.
//! - `General.bas::EfectoVeneno`, `PasarSegundo`; `modGameEvents.bas` (40 ms loop).
//! - `intervalos.ini` (server config shipped with the VB server).

/// `intervalos.ini` `IntervaloLanzaHechizo`: minimum time between two casts.
pub const CAST_INTERVAL_MS: u64 = 1230;
/// `modGameEvents.bas` `GAME_EVENTS_INTERVAL_MS`: poison and meditation run on this loop.
pub const GAME_EVENTS_MS: u64 = 40;
/// `DoMeditar`: no mana until `TiempoInicioMeditar > 20` loops (21 * 40 ms).
pub const MEDITATE_WARMUP_MS: u64 = 21 * GAME_EVENTS_MS;
/// `intervalos.ini` `IntervaloMeditar=10` loops between two mana pulses.
pub const MEDITATE_PULSE_MS: u64 = 10 * GAME_EVENTS_MS;
/// `intervalos.ini` `IntervaloVeneno=90`: `EfectoVeneno` counts 0..=90 loops, then hurts.
pub const POISON_INTERVAL_TICKS: u64 = 90;
pub const POISON_PERIOD_MS: u64 = (POISON_INTERVAL_TICKS + 1) * GAME_EVENTS_MS;

/// `GameLogic.bas::InRangoVision`: `MinXBorder = 1 + XWindow \ 2` (23 -> 12), strict `<`.
pub const VIEW_RANGE_X: i32 = 11;
/// `MinYBorder = 1 + YWindow \ 2` (18 -> 10), strict `<`.
pub const VIEW_RANGE_Y: i32 = 9;
/// `HandleWorkLeftClick`: beyond `RANGO_VISION_X/Y` (15/13) the click is logged as an
/// attack out of range (a client never sends it).
pub const CHEAT_RANGE_X: i32 = 15;
pub const CHEAT_RANGE_Y: i32 = 13;

/// `e_Class` ids used by the magic rules.
pub mod class {
    pub const MAGE: u8 = 1;
    pub const CLERIC: u8 = 2;
    pub const WARRIOR: u8 = 3;
    pub const HUNTER: u8 = 8;
    pub const WORKER: u8 = 9;
    pub const PIRATE: u8 = 10;
    pub const THIEF: u8 = 11;
}

/// VB6 assigns a Double to an integer with banker's rounding (round half to even).
pub fn vb_round(v: f64) -> i64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 && r as i64 % 2 != 0 {
        (r - v.signum()) as i64
    } else {
        r as i64
    }
}

/// `Matematicas.bas::Porcentaje`.
pub fn porcentaje(total: f64, porc: f64) -> f64 {
    total * porc / 100.0
}

/// Bonuses of one equipped item that adds magic damage (weapon, amulet, ring): obj.dat
/// `MagicDamageBonus` (%), `MagicAbsoluteBonus`, `MagicPenetration`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MagicItem {
    pub damage_bonus: i32,
    pub absolute_bonus: i32,
    pub penetration: i32,
}

/// Damage before the target's resistances: the `HechizoPropUsuario`/`HechizoPropNPC`
/// steps (each line assigns to a Long, so every step rounds like VB).
/// `roll` is `RandomNumber(MinHP, MaxHP)`. Returns (damage, total magic penetration).
pub fn spell_damage(roll: i32, caster_level: u32, items: &[MagicItem]) -> (i32, i32) {
    let mut d = roll as i64;
    d += vb_round(porcentaje(d as f64, 3.0 * caster_level as f64));
    let mut pen = 0;
    for it in items {
        d += vb_round(porcentaje(d as f64, it.damage_bonus as f64));
        d += it.absolute_bonus as i64;
        pen += it.penetration;
    }
    (d as i32, pen)
}

/// Against a user (`HechizoPropUsuario`): `PorcentajeRM = max(0, GetUserMR(target) -
/// penetration)` unless the spell has `AntiRm`. Never below zero.
pub fn damage_vs_user(damage: i32, target_mr_percent: i32, penetration: i32, anti_rm: bool) -> i32 {
    let mut d = damage as i64;
    if !anti_rm {
        let rm = (target_mr_percent - penetration).max(0);
        d -= vb_round(porcentaje(d as f64, rm as f64));
    }
    d.max(0) as i32
}

/// Magic stats of a creature (npcs.dat `MagicResistance`, `MagicDef`, `DefM`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NpcMagicDefense {
    pub magic_resistance: i32,
    pub magic_def: i32,
    pub def_m: i32,
}

/// Against a creature (`HechizoPropNPC`).
pub fn damage_vs_npc(damage: i32, npc: NpcMagicDefense, caster_magic_skill: i32, penetration: i32, anti_rm: bool) -> i32 {
    let mut d = damage as i64;
    if npc.magic_resistance > 0 {
        let diff = npc.magic_resistance - caster_magic_skill;
        let pct = if diff > 0 { (npc.magic_def + diff * 2 - penetration).max(0) } else { (npc.magic_def - penetration).max(0) };
        d -= vb_round(porcentaje(d as f64, pct as f64));
    }
    if !anti_rm {
        d -= npc.def_m as i64;
    }
    d.max(0) as i32
}

/// `GetSpellStaminaCost`.
pub fn stamina_cost(sta_required: i32, sta_percent_required: i32, max_sta: i32) -> i32 {
    let mut cost = sta_required;
    if sta_percent_required > 0 {
        let pct = (porcentaje(max_sta as f64, sta_percent_required as f64).ceil() as i32).max(1);
        cost = cost.max(pct);
    }
    cost
}

/// `HechizoEstadoUsuario`: warriors and hunters stay paralysed/immobilised 70% of
/// `Duration` (assigned to an Integer counter: VB rounding). Seconds.
pub fn paralysis_secs(duration: i32, target_class: u8) -> i64 {
    if target_class == class::WARRIOR || target_class == class::HUNTER {
        vb_round(duration as f64 * 0.7)
    } else {
        duration as i64
    }
}

/// `DoMeditar`: mana per pulse, at least 1.
/// `Porcentaje(MaxMAN, Porcentaje(PorcentajeRecuperoMana, RecoveryMana + skill * Mult))`.
pub fn meditation_mana(max_mana: i32, meditate_skill: i32, recovery_percent: i32, recovery_mana: i32, per_skill: f64) -> i32 {
    let inner = porcentaje(recovery_percent as f64, recovery_mana as f64 + meditate_skill as f64 * per_skill);
    (vb_round(porcentaje(max_mana as f64, inner)) as i32).max(1)
}

/// `HandleMeditate`: classes that cannot meditate.
pub fn can_meditate(class_id: u8) -> bool {
    !matches!(class_id, class::HUNTER | class::WORKER | class::WARRIOR | class::PIRATE | class::THIEF)
}

/// `HandleMeditate` FX by level (`Meditaciones.dat`, `[FXsMEDITATIONSBYLEVEL]` and the
/// criminal table).
pub fn meditation_fx(level: u32, criminal: bool) -> u16 {
    let (normal, crim) = match level {
        0..=12 => (153, 154),
        13..=17 => (155, 156),
        18..=24 => (157, 158),
        25..=28 => (159, 160),
        29..=32 => (161, 162),
        33..=36 => (163, 164),
        37..=39 => (165, 166),
        40..=42 => (167, 168),
        43..=44 => (169, 170),
        45..=46 => (171, 172),
        _ => (120, 120),
    };
    if criminal { crim } else { normal }
}

/// `EfectoVeneno`: `roll` is `RandomNumber(3, 5)` percent of max HP, rounded up.
pub fn poison_damage(roll: i32, max_hp: i32) -> i32 {
    1 + roll * max_hp / 100
}

/// `HandleHechizoUsuario`: resurrecting outside a safe map costs the caster
/// `target_level * 1.5 + caster_hp * 0.45` HP.
pub fn resurrect_hp_cost(target_level: u32, caster_hp: i32) -> i32 {
    vb_round(target_level as f64 * 1.5 + caster_hp as f64 * 0.45) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vb_rounding_is_bankers() {
        assert_eq!(vb_round(2.5), 2);
        assert_eq!(vb_round(3.5), 4);
        assert_eq!(vb_round(-2.5), -2);
        assert_eq!(vb_round(2.4), 2);
        assert_eq!(vb_round(2.6), 3);
    }

    #[test]
    fn damage_adds_three_percent_per_level_and_items() {
        // Level 10: +30%. 10 -> 13.
        assert_eq!(spell_damage(10, 10, &[]), (13, 0));
        // Staff +10% and +2 absolute, ring with penetration 5.
        let items = [MagicItem { damage_bonus: 10, absolute_bonus: 2, penetration: 0 }, MagicItem { damage_bonus: 0, absolute_bonus: 0, penetration: 5 }];
        // 20 + 30% = 26; +10% = 28.6 -> 29; +2 = 31.
        assert_eq!(spell_damage(20, 10, &items), (31, 5));
    }

    #[test]
    fn resistances() {
        assert_eq!(damage_vs_user(100, 20, 5, false), 85);
        assert_eq!(damage_vs_user(100, 20, 5, true), 100);
        assert_eq!(damage_vs_user(100, 3, 10, false), 100, "penetration above MR does not add damage");
        let npc = NpcMagicDefense { magic_resistance: 50, magic_def: 10, def_m: 3 };
        // skill 30: diff 20 -> 10 + 40 = 50% -> 50, minus DefM 3.
        assert_eq!(damage_vs_npc(100, npc, 30, 0, false), 47);
        // skill 80: diff < 0 -> 10%.
        assert_eq!(damage_vs_npc(100, npc, 80, 0, true), 90);
        assert_eq!(damage_vs_npc(2, npc, 80, 0, false), 0, "never negative");
    }

    #[test]
    fn stamina_cost_takes_the_percent_when_higher() {
        assert_eq!(stamina_cost(5, 0, 400), 5);
        assert_eq!(stamina_cost(5, 10, 400), 40);
        assert_eq!(stamina_cost(50, 1, 40), 50);
        assert_eq!(stamina_cost(0, 1, 10), 1);
    }

    #[test]
    fn paralysis_is_shorter_for_warriors_and_hunters() {
        assert_eq!(paralysis_secs(13, class::MAGE), 13);
        assert_eq!(paralysis_secs(13, class::WARRIOR), 9);
        assert_eq!(paralysis_secs(15, class::HUNTER), 10, "10.5 rounds to even");
    }

    #[test]
    fn meditation_with_official_balance() {
        // Balance.dat: PorcentajeRecuperoMana=7, RecoveryMana=50, MultiplierManaxSkills=0.5.
        // Skill 0: 3.5% of max mana per pulse; skill 100: 7%.
        assert_eq!(meditation_mana(1000, 0, 7, 50, 0.5), 35);
        assert_eq!(meditation_mana(1000, 100, 7, 50, 0.5), 70);
        assert_eq!(meditation_mana(10, 0, 7, 50, 0.5), 1, "at least one point");
        assert!(!can_meditate(class::WARRIOR) && can_meditate(class::MAGE) && can_meditate(class::CLERIC));
        assert_eq!(meditation_fx(1, false), 153);
        assert_eq!(meditation_fx(20, true), 158);
        assert_eq!(meditation_fx(50, false), 120);
    }

    #[test]
    fn poison_and_resurrect() {
        assert_eq!(poison_damage(3, 100), 4);
        assert_eq!(poison_damage(5, 250), 13);
        assert_eq!(resurrect_hp_cost(10, 100), 60);
        assert_eq!(POISON_PERIOD_MS, 3640);
    }
}

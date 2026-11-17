//! Experience, levels and growth rules (official AO20 values).
//!
//! Ported from the VB6 server: `CheckUserLevel` (Modulo_UsUaRiOs.bas), `UserMod.GetMaxMana /
//! GetHitModifier / GetMaxStamina / GetMaxHp`, `CalcularPromedioVida` and `RandomIntBiased`
//! (General.bas), `SubirSkill` (skill cap by level) and `HandleModifySkills` (Protocol.bas).
//! Everything here is pure: randomness comes in through a closure.
use crate::balance::Balance;
use ao_formats::dat::balance::{Balance as Official, Class};

/// `STAT_MAXELV` (Declares.bas).
pub const MAX_LEVEL: u32 = 47;
/// `MAXSKILLPOINTS`: no skill goes above this.
pub const MAX_SKILL: u8 = 100;

/// `[EXP]` of Balance.dat (`ExpLevelUp`): experience needed to leave level `n` is entry
/// `n - 1`. Level 47 is the cap and needs nothing. A test compares it with the data file.
pub const EXP_TABLE: [u32; MAX_LEVEL as usize] = [
    500, 750, 1125, 1687, 2531, 3796, 5695, 8542, 12814, 19221, 28832, 43248, 71360, 99904, 139866, 195813, 274138, 383793, 537311, 752235, 1053130, 1474382,
    2064135, 2889789, 4450275, 5562844, 6953555, 8691944, 10864930, 13581163, 16976454, 21220567, 29178280, 35743394, 43785657, 53637430, 65705852, 80489669,
    98599845, 132863291, 159435949, 191323139, 229587767, 275505320, 330606385, 396727662, 0,
];

/// Experience needed to go from `level` to the next one (0 at the level cap). The balance
/// argument is kept so callers do not change; the official curve is fixed.
pub fn exp_for_level(_b: &Balance, level: u32) -> u32 {
    exp_to_next(level)
}

pub fn exp_to_next(level: u32) -> u32 {
    if level == 0 || level >= MAX_LEVEL {
        return 0;
    }
    EXP_TABLE[level as usize - 1]
}

/// Result of gaining experience.
#[derive(Debug, PartialEq)]
pub struct LevelState {
    pub level: u32,
    pub exp: u32,
    pub levels_gained: u32,
}

/// Adds `gained` experience, carrying over every level reached (`CheckUserLevel` loop:
/// `Do While Exp >= ExpLevelUp(ELV) And ELV < STAT_MAXELV`). At the cap the experience is 0.
pub fn add_exp(_b: &Balance, level: u32, exp: u32, gained: u32) -> LevelState {
    let (mut level, mut exp, mut levels_gained) = (level.max(1), exp as u64 + gained as u64, 0);
    while level < MAX_LEVEL && exp >= exp_to_next(level) as u64 {
        exp -= exp_to_next(level) as u64;
        level += 1;
        levels_gained += 1;
    }
    if level >= MAX_LEVEL {
        exp = 0;
    }
    LevelState { level, exp: exp.min(u32::MAX as u64) as u32, levels_gained }
}

/// VB6 assigns doubles to integers with banker's rounding (half to even).
pub fn vb_round(x: f64) -> i32 {
    x.round_ties_even() as i32
}

/// `UserMod.GetMaxMana`: `Int * ManaInicial` rounded, plus `MultMana * Int` per level above 1.
pub fn max_mana(bal: &Official, class: Class, intelligence: u8, level: u32) -> i32 {
    let m = bal.class(class);
    let int = intelligence as f64;
    let base = vb_round(int * m.initial_mana);
    vb_round(base as f64 + (m.mana_mult * int) * (level.saturating_sub(1)) as f64)
}

/// `UserMod.GetMaxStamina`.
pub fn max_stamina(bal: &Official, class: Class, level: u32) -> i32 {
    60 + level.saturating_sub(1) as i32 * bal.class(class).stamina_gain
}

/// `UserMod.GetHitModifier`: bonus that grows per level (3 per level before 36 for fighters).
pub fn hit_modifier(bal: &Official, class: Class, level: u32) -> i32 {
    let m = bal.class(class);
    let level = level as i32;
    if level <= 36 {
        (level - 1) * m.hit_pre36
    } else {
        35 * m.hit_pre36 + (level - 36) * m.hit_post36
    }
}

/// `MinHIT` / `MaxHit` of a character of this class and level (`GetHitModifier + 1 / + 2`).
pub fn hit_range(bal: &Official, class: Class, level: u32) -> (i32, i32) {
    let h = hit_modifier(bal, class, level);
    (h + 1, h + 2)
}

/// `ModClase.Vida - (21 - Constitucion) * 0.5`: average hit points gained per level.
pub fn average_hp_per_level(bal: &Official, class: Class, constitution: u8) -> f64 {
    bal.class(class).life - (21.0 - constitution as f64) * 0.5
}

/// `UserMod.GetMaxHp`: hit points of a character with exactly average luck.
pub fn average_max_hp(bal: &Official, class: Class, constitution: u8, level: u32) -> i32 {
    vb_round(average_hp_per_level(bal, class, constitution) * level.saturating_sub(1) as f64 + constitution as f64)
}

/// Hit points gained on reaching `level` (`CheckUserLevel`, Shugar 15/07/2024): a random
/// integer around the class/constitution average, biased so the character's own average
/// drifts back to it, then capped to `CapVidaMin..CapVidaMax` of the average life curve.
/// `rnd` must return uniform values in `[0, 1)` (VB `Rnd`, called twice).
pub fn hp_gain(bal: &Official, class: Class, constitution: u8, level: u32, current_max_hp: i32, rnd: &mut impl FnMut() -> f64) -> i32 {
    let e = &bal.extra;
    let prom_class = average_hp_per_level(bal, class, constitution);
    // CalcularPromedioVida: always on average at level 1; the VB runs it after ELV was raised.
    let prom_char = if level <= 1 { prom_class } else { (current_max_hp - constitution as i32) as f64 / (level - 1) as f64 };
    let bias = prom_class + (prom_class - prom_char) * e.life_average_imbalance as f64;
    // RandomIntBiased(min, max, bias, influence)
    let (min, max) = (prom_class - e.life_range as f64, prom_class + e.life_range as f64);
    let random_range = rnd() * (max - min) + min;
    let mix = rnd() * e.life_average_influence as f64;
    let mut gain = vb_round(random_range * (1.0 - mix) + bias * mix);
    let average = average_max_hp(bal, class, constitution, level);
    if current_max_hp + gain > vb_round(average as f64 + e.life_cap_max as f64) {
        gain = vb_round(average as f64 + e.life_cap_max as f64) - current_max_hp;
    }
    if current_max_hp + gain < vb_round(average as f64 + e.life_cap_min as f64) {
        gain = vb_round(average as f64 + e.life_cap_min as f64) - current_max_hp;
    }
    gain
}

/// Free skill points per level (`ModClase.LevelSkillPoints`, `MODSKILLPOINTS`).
pub fn skill_points_per_level(bal: &Official, class: Class) -> u16 {
    bal.class(class).skill_points_per_level.max(0) as u16
}

/// Mana recovered per meditation step (`Trabajo.DoMeditar`, without lutes): a percentage
/// of the maximum that grows with the Meditate skill. At least 1.
pub fn meditation_mana(bal: &Official, max_mana: i32, meditate_skill: u8) -> i32 {
    let e = &bal.extra;
    let pct = e.mana_recovery_percent as f64 * (e.recovery_mana as f64 + meditate_skill as f64 * e.mana_per_skill_multiplier) / 100.0;
    vb_round(max_mana as f64 * pct / 100.0).max(1)
}

/// `SubirSkill` limit: the most a skill may reach by *use* at this level (5 points every two
/// levels). Spending free points (`assign_skill`) is only limited by `MAX_SKILL`.
pub fn skill_cap_for_level(level: u32) -> u32 {
    if level.is_multiple_of(2) {
        (level / 2) * 5
    } else {
        (level / 2) * 5 + 3 - ((((level - 1) / 2) * 5) % 10) / 5
    }
}

/// Why a skill assignment was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum AssignError {
    /// More points than the character has: a forged packet (`HandleModifySkills`).
    OverBudget,
}

/// Spends `requested` free points on a skill that has `current`. Points above `MAX_SKILL`
/// are not spent (the VB clamps and refunds them). Returns `(new value, points spent)`.
pub fn assign_skill(current: u8, free: u16, requested: u16) -> Result<(u8, u16), AssignError> {
    if requested > free {
        return Err(AssignError::OverBudget);
    }
    let new = (current as u32 + requested as u32).min(MAX_SKILL as u32) as u8;
    Ok((new, (new as u16).saturating_sub(current as u16)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ao_formats::dat::balance;
    use ao_formats::paths::recursos_dir;

    fn official() -> Option<Official> {
        let root = recursos_dir();
        root.join("Dat").join(balance::FILE).is_file().then(|| balance::load(&root).expect("Balance.dat").data)
    }

    #[test]
    fn carries_over_levels() {
        let b = Balance::default();
        assert_eq!(add_exp(&b, 1, 0, 400), LevelState { level: 1, exp: 400, levels_gained: 0 });
        // Level 1 needs 500, level 2 needs 750: 1300 exp = two levels and 50 left.
        assert_eq!(add_exp(&b, 1, 0, 1300), LevelState { level: 3, exp: 50, levels_gained: 2 });
        assert_eq!(add_exp(&b, 1, 499, 1), LevelState { level: 2, exp: 0, levels_gained: 1 });
    }

    #[test]
    fn level_cap_is_47_and_never_loops() {
        let b = Balance::default();
        let r = add_exp(&b, 46, 0, 396_727_662 + 12_345);
        assert_eq!(r, LevelState { level: 47, exp: 0, levels_gained: 1 });
        assert_eq!(add_exp(&b, 47, 0, u32::MAX), LevelState { level: 47, exp: 0, levels_gained: 0 });
        assert_eq!(exp_for_level(&b, 47), 0);
        assert_eq!(exp_for_level(&b, 0), 0);
        assert_eq!(exp_for_level(&b, 1), 500);
    }

    #[test]
    fn exp_table_matches_balance_dat() {
        let Some(bal) = official() else { return };
        for level in 1..=MAX_LEVEL {
            assert_eq!(exp_to_next(level) as i32, bal.exp_for_level(level).unwrap(), "level {level}");
        }
    }

    #[test]
    fn skill_cap_follows_five_every_two_levels() {
        assert_eq!(skill_cap_for_level(1), 3);
        assert_eq!(skill_cap_for_level(2), 5);
        assert_eq!(skill_cap_for_level(3), 7);
        assert_eq!(skill_cap_for_level(7), 17);
        assert_eq!(skill_cap_for_level(4), 10);
        assert_eq!(skill_cap_for_level(5), 13);
        assert_eq!(skill_cap_for_level(10), 25);
    }

    #[test]
    fn assignment_clamps_and_refunds() {
        assert_eq!(assign_skill(0, 10, 4), Ok((4, 4)));
        assert_eq!(assign_skill(98, 10, 5), Ok((100, 2)), "3 points are refunded");
        assert_eq!(assign_skill(100, 10, 1), Ok((100, 0)));
        assert_eq!(assign_skill(0, 3, 4), Err(AssignError::OverBudget));
    }

    #[test]
    fn rounding_is_bankers() {
        assert_eq!(vb_round(0.5), 0);
        assert_eq!(vb_round(1.5), 2);
        assert_eq!(vb_round(2.5), 2);
        assert_eq!(vb_round(149.94), 150);
    }

    #[test]
    fn growth_by_class_matches_hand_calculation() {
        let Some(bal) = official() else { return };
        // Human mage: Int 18 (18+0), Con 20 (18+2).
        // Level 2 mana: round(18*8.33)=150, + 2.65*18*1 = 47.7 -> 197.7 -> 198.
        assert_eq!(max_mana(&bal, Class::Mage, 18, 1), 150);
        assert_eq!(max_mana(&bal, Class::Mage, 18, 2), 198);
        // Stamina is 60 + 15 per level above 1.
        assert_eq!(max_stamina(&bal, Class::Mage, 2), 75);
        assert_eq!(max_stamina(&bal, Class::Warrior, 10), 60 + 9 * 15);
        // Hit: warrior 3 per level up to 36 (min = 3*(L-1)+1), then 2.
        assert_eq!(hit_range(&bal, Class::Warrior, 1), (1, 2));
        assert_eq!(hit_range(&bal, Class::Warrior, 10), (28, 29));
        assert_eq!(hit_range(&bal, Class::Warrior, 36), (106, 107));
        assert_eq!(hit_range(&bal, Class::Warrior, 40), (35 * 3 + 4 * 2 + 1, 35 * 3 + 4 * 2 + 2));
        assert_eq!(hit_range(&bal, Class::Mage, 5), (5, 6));
        // Warrior with Con 20: 10.5 - 0.5 = 10 per level; level 5 average = 10*4 + 20 = 60.
        assert_eq!(average_max_hp(&bal, Class::Warrior, 20, 5), 60);
        assert_eq!(skill_points_per_level(&bal, Class::Worker), 6);
        assert_eq!(skill_points_per_level(&bal, Class::Warrior), 5);
    }

    #[test]
    fn hp_gain_stays_inside_the_official_window() {
        let Some(bal) = official() else { return };
        // RangoVidas = 2, no bias: warrior Con 20 gains 10 +- 2 per level.
        let mut seed = 12345u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f64 / 4_294_967_296.0
        };
        let mut hp = 20;
        for level in 2..=47 {
            let g = hp_gain(&bal, Class::Warrior, 20, level, hp, &mut rnd);
            assert!((8..=12).contains(&g), "level {level} gained {g}");
            hp += g;
            let avg = average_max_hp(&bal, Class::Warrior, 20, level);
            assert!((avg - 10..=avg + 10).contains(&hp), "capped to the average curve");
        }
        // The cap kicks in: a character 30 points above average cannot gain.
        let g = hp_gain(&bal, Class::Warrior, 20, 5, 90, &mut || 0.99);
        assert_eq!(g, 60 + 10 - 90, "capped at average + CapVidaMax");
    }

    #[test]
    fn meditation_recovers_a_share_of_the_maximum() {
        let Some(bal) = official() else { return };
        // 7% of (50 + 0*0.5)% of 150 = 0.035 * 150 = 5.25 -> 5.
        assert_eq!(meditation_mana(&bal, 150, 0), 5);
        // skill 100: 7% * (50 + 50)% = 7% of 150 = 10.5 -> 10 (half to even).
        assert_eq!(meditation_mana(&bal, 150, 100), 10);
        assert_eq!(meditation_mana(&bal, 0, 0), 1, "at least one point");
    }
}

//! Official AO20 combat formulas (SistemaCombate.bas, Modulo_UsUaRiOs.bas), as pure
//! functions. Random rolls are made by the caller and passed in, so every formula can be
//! checked by hand. VB6 assigns `Single`/`Double` results to `Long`/`Integer` with
//! banker's rounding (`CLng`), at every step: [`vb_round`] reproduces that.
use ao_formats::dat::balance::{Balance, ClassMods, Skill, WeaponType, MAX_LEVEL};
use ao_formats::dat::obj::ObjData;

/// `IntervaloUserPuedeAtacar` (intervalos.ini): melee attack interval.
pub const ATTACK_INTERVAL_MS: u64 = 1165;
/// `IntervaloFlechasCazadores` (intervalos.ini): bows and throwables.
pub const BOW_INTERVAL_MS: u64 = 1200;
/// `IntervaloGolpeMagia` (intervalos.ini): spell after a melee hit (checked by magic).
pub const HIT_THEN_SPELL_MS: u64 = 800;
/// `MAXDISTANCIAARCO` (SistemaCombate.bas).
pub const MAX_BOW_DISTANCE: i32 = 18;
/// `MIN_APUÑALAR` (Declares.bas).
pub const MIN_STAB_SKILL: u8 = 10;
/// `iCuerpoMuerto` (Declares.bas): ghost body.
pub const DEAD_BODY: u16 = 829;
/// `e_Trigger.ZonaSegura` (Declares.bas).
pub const TRIGGER_SAFE_ZONE: u16 = 4;
/// `e_Trigger.ZONAPELEA` (Declares.bas).
pub const TRIGGER_FIGHT_ZONE: u16 = 6;
/// Stamina needed to swing, and the random cost (UsuarioAtaca).
pub const MIN_ATTACK_STAMINA: i32 = 10;
pub const ATTACK_STAMINA_COST: (i32, i32) = (1, 10);
/// Critical hit extra damage against NPCs (UserDamageNpc: `DamageBase * 0.33`).
pub const NPC_CRITICAL_FACTOR: f64 = 0.33;
/// `e_PartesCuerpo.bCabeza` / `bTorso` (Declares.bas).
pub const BODY_HEAD: i32 = 1;
pub const BODY_LEFT_LEG: i32 = 2;
pub const BODY_TORSO: i32 = 6;

/// Official server feature toggles that touch combat (Example.feature_toggle.ini).
pub const FEATURE_IMPROVED_HIT_CHANCE: bool = false;
pub const FEATURE_CLASS_WEAPON_BONUS: bool = false;
pub const FEATURE_ARMOR_PENETRATION: bool = true;
pub const FEATURE_HEALERS_AND_TANKS: bool = true;

/// VB6 `CLng`/implicit conversion: round half to even.
pub fn vb_round(v: f64) -> i64 {
    v.round_ties_even() as i64
}

/// `ClampChance` (GameLogic.bas): 0..100, truncated.
pub fn clamp_chance(v: f64) -> i32 {
    v.clamp(0.0, 100.0).trunc() as i32
}

/// `GetHitModifier` (Modulo_UsUaRiOs.bas).
pub fn hit_modifier(class: &ClassMods, level: u32) -> i32 {
    let level = level.max(1) as i32;
    if level <= 36 {
        (level - 1) * class.hit_pre36
    } else {
        35 * class.hit_pre36 + (level - 36) * class.hit_post36
    }
}

/// `Stats.MinHIT` / `Stats.MaxHit` of a user: `GetHitModifier + 1` / `+ 2`
/// (CheckUserLevel; 1 and 2 at creation, GameLogic.bas).
pub fn user_hit_range(class: &ClassMods, level: u32) -> (i32, i32) {
    let m = hit_modifier(class, level);
    (m + 1, m + 2)
}

/// `AttackPower` (SistemaCombate.bas). `hit_bonus` = `UserMod.GetHitBonus`.
pub fn attack_power(skill: u8, agility: u8, level: u32, skill_modifier: f64, hit_bonus: i32) -> i64 {
    let s = skill as f64;
    let temp = vb_round((s + (3.0 * s / 100.0) * agility as f64) * skill_modifier);
    vb_round(temp as f64 + 2.5 * (level as f64 - 12.0).max(0.0)) + hit_bonus as i64
}

/// `PoderEvasion` (SistemaCombate.bas).
pub fn evasion_power(tactics: u8, agility: u8, level: u32, class_evasion: f64, evasion_bonus: i32) -> i64 {
    let t = tactics as f64;
    let base = vb_round((t + (3.0 * t / 100.0) * agility as f64) * class_evasion);
    vb_round(base as f64 + 2.5 * (level as f64 - 12.0).max(0.0)) + evasion_bonus as i64
}

/// `PoderEvasionEscudo` (SistemaCombate.bas): `shield_percent` = shield `Porcentaje`.
pub fn shield_evasion(defense_skill: u8, class_shield: f64, shield_percent: i32) -> i64 {
    vb_round(((defense_skill as f64 * class_shield) / 2.0) * (shield_percent as f64 / 100.0))
}

/// User attacking (NPC or user): `ProbExito` of UserImpactoNpc / UsuarioImpacto, 5..95.
pub fn user_hit_chance(attack: i64, evasion: i64, weapon_hit_modifier: i32) -> i64 {
    vb_round((50.0 + (attack - evasion) as f64 * 0.4 + weapon_hit_modifier as f64).clamp(5.0, 95.0))
}

/// NPC attacking a user: `ProbExito` of NpcImpacto, 10..90.
pub fn npc_hit_chance(npc_attack: i64, user_evasion: i64) -> i64 {
    vb_round((50.0 + (npc_attack - user_evasion) as f64 * 0.4).clamp(10.0, 90.0))
}

/// Missed NPC hit blocked by a shield (NpcImpacto `ProbRechazo`); `None` without skills.
pub fn npc_block_chance(defense: u8, tactics: u8) -> Option<i64> {
    let (d, t) = (defense as f64, tactics as f64);
    (d + t > 0.0).then(|| vb_round((100.0 * (d / (d + t))).clamp(10.0, 90.0)))
}

/// Missed user hit blocked by the victim's shield (UsuarioImpacto `ProbRechazo`).
pub fn user_block_chance(shield_percent: i32, defense: u8, tactics: u8) -> i64 {
    if shield_percent <= 0 {
        return 0;
    }
    if defense == 0 {
        return 10;
    }
    let (d, t) = (defense as f64, tactics as f64);
    vb_round((shield_percent as f64 * (d / (d + t).max(1.0))).clamp(10.0, 90.0))
}

/// `UsuarioImpacto`: meditating victims evade 25% less.
pub fn meditating_hit_chance(chance: i64) -> i64 {
    let evade = vb_round((100 - chance) as f64 * 0.75);
    (100 - evade).min(90)
}

/// `GetSkillRequiredForWeapon`.
pub fn weapon_skill(weapon: Option<&ObjData>) -> Skill {
    match weapon.and_then(|w| w.weapon_type) {
        None if weapon.is_none() => Skill::Wrestling,
        Some(WeaponType::Knuckle) => Skill::Wrestling,
        Some(WeaponType::Bow) | Some(WeaponType::GunPowder) => Skill::Projectiles,
        Some(WeaponType::Dagger) => Skill::Stab,
        _ => Skill::Weapons,
    }
}

/// Class multiplier of the attack skill: `ModificadorPoderAtaqueArmas` /
/// `...Proyectiles` (PoderAtaqueArma, PoderAtaqueProyectil, AttackPowerDaggers and
/// PoderAtaqueWrestling all use the melee one except projectiles).
pub fn attack_skill_modifier(class: &ClassMods, skill: Skill) -> f64 {
    if skill == Skill::Projectiles {
        class.ranged_attack
    } else {
        class.melee_attack
    }
}

/// `GetWeaponHitBonus` (class_weapon_bonus feature, off in the official config).
pub fn weapon_hit_bonus(class: &ClassMods, weapon: Option<&ObjData>) -> i32 {
    match weapon.and_then(|w| w.weapon_type) {
        Some(t) if FEATURE_CLASS_WEAPON_BONUS => class.weapon_hit_bonus[t as usize],
        _ => 0,
    }
}

/// `GetClassAttackModifier`, or `DañoWrestling` without a weapon.
pub fn class_damage_modifier(class: &ClassMods, weapon: Option<&ObjData>) -> f64 {
    match weapon {
        None => class.wrestling_damage,
        Some(w) if w.projectile > 0 => class.ranged_damage,
        Some(w) if w.weapon_type == Some(WeaponType::Knuckle) => class.wrestling_damage,
        Some(_) => class.melee_damage,
    }
}

/// `GetHitRangeValues`: the NPC-specific range when the object has one.
pub fn hit_range(obj: &ObjData, vs_npc: bool) -> (i32, i32) {
    let (mut min, mut max) = (obj.min_hit, obj.max_hit);
    if vs_npc && (obj.min_hit_npc > 0 || obj.max_hit_npc > 0) {
        if obj.min_hit_npc > 0 {
            min = obj.min_hit_npc;
        }
        if obj.max_hit_npc > 0 {
            max = obj.max_hit_npc;
        }
    }
    (min, max.max(min))
}

/// `GetUserDamageWithItem` base: `(3 * weapon + maxWeapon * 0.2 * max(0, STR - 15) +
/// userHit) * classModifier`. `weapon_roll`/`max_weapon` include the ammunition.
pub fn user_damage(weapon_roll: i32, max_weapon: i32, strength: u8, user_roll: i32, class_modifier: f64) -> i64 {
    let str_bonus = (strength as f64 - 15.0).max(0.0);
    vb_round((3.0 * weapon_roll as f64 + max_weapon as f64 * 0.2 * str_bonus + user_roll as f64) * class_modifier)
}

/// Percent part of `GetArmorPenetration`: `ArmorPenetrationPercent` (as 0..1, or 0..100
/// when above 1) of the target's armor, assigned to an Integer.
pub fn armor_penetration_share(percent: i32, target_armor: i32) -> i32 {
    if percent <= 0 {
        return 0;
    }
    let mut pct = percent as f64;
    if pct > 1.0 {
        pct /= 100.0;
    }
    vb_round(target_armor as f64 * pct.min(1.0)) as i32
}

/// `PuedeApuñalar`.
pub fn can_stab(is_assassin: bool, stab_skill: u8, weapon: Option<&ObjData>) -> bool {
    weapon.is_some_and(|w| w.stabs == 1) && (is_assassin || stab_skill >= MIN_STAB_SKILL)
}

/// `PuedeGolpeCritico`: bandits with knuckles.
pub fn can_critical(is_bandit: bool, weapon: Option<&ObjData>) -> bool {
    is_bandit && weapon.is_some_and(|w| w.weapon_type == Some(WeaponType::Knuckle))
}

/// `GetStabbingChanceBase`: `per_skill` is the class's `*StabbingChance` of Balance.dat.
pub fn stab_chance(stab_skill: u8, per_skill: f32, weapon_extra: i32) -> i32 {
    clamp_chance(stab_skill as f64 * per_skill as f64 + weapon_extra as f64)
}

/// `GetCriticalHitChanceBase`.
pub fn critical_chance(wrestling: u8, bandit_chance: f32, weapon_extra: i32) -> i32 {
    clamp_chance(wrestling as f64 * bandit_chance as f64 + weapon_extra as f64)
}

/// Experience for taking an NPC from `hp_before` to `hp_after` (GetExpForUser: damage *
/// GiveEXP / MaxHp). Written as a difference of floors so that the hits of a whole kill
/// add up to exactly `give_exp` (the official server keeps `ExpCount` for the same).
pub fn exp_for_damage(give_exp: u32, max_hp: i32, hp_before: i32, hp_after: i32) -> u32 {
    if max_hp <= 0 {
        return 0;
    }
    let m = max_hp as i64;
    let lost = |hp: i32| (give_exp as i64 * (m - (hp as i64).clamp(0, m))) / m;
    (lost(hp_after) - lost(hp_before)).max(0) as u32
}

/// `ExpLevelUp(level)` from Balance.dat `[EXP]`; `None` at `STAT_MAXELV`.
pub fn exp_to_level_up(b: &Balance, level: u32) -> Option<u32> {
    if level == 0 || level as usize >= MAX_LEVEL {
        return None;
    }
    Some(b.exp_level_up[level as usize - 1].max(1) as u32)
}

/// Result of gaining experience with the official curve.
#[derive(Debug, PartialEq, Eq)]
pub struct ExpGain {
    pub level: u32,
    pub exp: u32,
    pub levels_gained: u32,
}

/// `CheckUserLevel` loop: levels up while the experience reaches the official curve.
pub fn add_exp(b: &Balance, level: u32, exp: u32, gained: u32) -> ExpGain {
    let (mut level, mut exp, mut levels_gained) = (level.max(1), exp.saturating_add(gained), 0);
    while let Some(need) = exp_to_level_up(b, level) {
        if exp < need {
            break;
        }
        exp -= need;
        level += 1;
        levels_gained += 1;
    }
    ExpGain { level, exp, levels_gained }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class() -> ClassMods {
        ClassMods { evasion: 0.9, melee_attack: 1.1, melee_damage: 1.1, wrestling_damage: 0.95, shield: 1.0, hit_pre36: 3, hit_post36: 2, ..Default::default() }
    }

    #[test]
    fn vb_rounding_is_bankers() {
        assert_eq!(vb_round(2.5), 2);
        assert_eq!(vb_round(3.5), 4);
        assert_eq!(vb_round(1.9), 2);
        assert_eq!(clamp_chance(37.9), 37);
    }

    #[test]
    fn hit_range_grows_with_level() {
        // GetHitModifier: (ELV - 1) * GOLPE_PRE_36, then 35 * pre + (ELV - 36) * post.
        assert_eq!(user_hit_range(&class(), 1), (1, 2));
        assert_eq!(user_hit_range(&class(), 10), (28, 29));
        assert_eq!(user_hit_range(&class(), 40), (114, 115));
    }

    #[test]
    fn attack_and_evasion_by_hand() {
        // AttackPower: skill 50, AGI 20, ELV 15, ModAtaqueArmas 1.1:
        // round((50 + 1.5 * 20) * 1.1) = 88; round(88 + 2.5 * 3) = round(95.5) = 96.
        assert_eq!(attack_power(50, 20, 15, 1.1, 0), 96);
        // PoderEvasion: Tacticas 30, AGI 18, ELV 1, ModEvasion 0.9:
        // round((30 + 0.9 * 18) * 0.9) = round(41.58) = 42.
        assert_eq!(evasion_power(30, 18, 1, 0.9, 0), 42);
        // ProbExito: 50 + (96 - 42) * 0.4 = 71.6 -> 72; capped at 95 / 5.
        assert_eq!(user_hit_chance(96, 42, 0), 72);
        assert_eq!(user_hit_chance(1000, 0, 0), 95);
        assert_eq!(user_hit_chance(0, 1000, 0), 5);
        assert_eq!(npc_hit_chance(0, 1000), 10);
        assert_eq!(npc_hit_chance(1000, 0), 90);
    }

    #[test]
    fn user_damage_by_hand() {
        // Sword roll 5 of max 8, STR 18, user hit 2, ModDañoArmas 1.1:
        // (15 + 8 * 0.2 * 3 + 2) * 1.1 = 23.98 -> 24.
        assert_eq!(user_damage(5, 8, 18, 2, 1.1), 24);
        // Bare hands of a level 1 warrior: (0 + 0 + 1..2) * 0.95 -> 1..2.
        assert_eq!(user_damage(0, 0, 18, 1, 0.95), 1);
        assert_eq!(user_damage(0, 0, 18, 2, 0.95), 2);
        // A level 1 mage (0.20) hits for 0 with bare hands, as in the original.
        assert_eq!(user_damage(0, 0, 18, 2, 0.20), 0);
        // Ties go to even: 2 * 1.25 = 2.5 -> 2.
        assert_eq!(user_damage(0, 0, 15, 2, 1.25), 2);
    }

    #[test]
    fn weapon_classification() {
        let mut dagger = ObjData { weapon_type: Some(WeaponType::Dagger), stabs: 1, ..Default::default() };
        assert_eq!(weapon_skill(None), Skill::Wrestling);
        assert_eq!(weapon_skill(Some(&dagger)), Skill::Stab);
        assert!(can_stab(false, 10, Some(&dagger)));
        assert!(!can_stab(false, 9, Some(&dagger)));
        assert!(can_stab(true, 0, Some(&dagger)));
        dagger.projectile = 1;
        assert_eq!(class_damage_modifier(&ClassMods { ranged_damage: 0.7, ..class() }, Some(&dagger)), 0.7);
        let npc_range = ObjData { min_hit: 2, max_hit: 4, max_hit_npc: 9, ..Default::default() };
        assert_eq!(hit_range(&npc_range, true), (2, 9));
        assert_eq!(hit_range(&npc_range, false), (2, 4));
    }

    #[test]
    fn exp_of_a_whole_kill_adds_up() {
        let (e, m) = (37, 55);
        let hits = [7, 13, 1, 20, 14];
        let mut hp = m;
        let mut total = 0;
        for h in hits {
            let after = (hp - h).max(0);
            total += exp_for_damage(e, m, hp, after);
            hp = after;
        }
        assert_eq!(hp, 0);
        assert_eq!(total, e);
    }

    #[test]
    fn shield_blocks() {
        assert_eq!(npc_block_chance(0, 0), None);
        assert_eq!(npc_block_chance(30, 10), Some(75));
        assert_eq!(user_block_chance(0, 50, 50), 0);
        assert_eq!(user_block_chance(40, 0, 50), 10);
        assert_eq!(user_block_chance(40, 50, 50), 20);
        assert_eq!(shield_evasion(60, 1.0, 50), 15);
        assert_eq!(meditating_hit_chance(60), 70);
    }
}

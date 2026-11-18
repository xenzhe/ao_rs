//! Pure rules of effects over time (`EffectsOverTime.bas`, `ReferenceUtils.UpdateIncreaseModifier`,
//! the `Get*Bonus`/`Get*Reduction` getters of `UserMod` and `NPCs`) and of area spells
//! (`modHechizos.AreaSpellTileDistance`, `ApplyAreaSpellFalloff`).
//!
//! AO20 keeps a running sum per character that every effect adds on start and subtracts
//! on removal. Here the sum is rebuilt from the active effects whenever it is needed, so a
//! missed or repeated removal can never leave a permanent bonus behind.
use ao_formats::dat::eots::{EotDef, EotKind, EotLimit, EotModifiers};

/// `Declares.AREA_SPELL_FALLOFF_PER_TILE`: % less per tile away from the centre.
pub const AREA_SPELL_FALLOFF_PER_TILE: i32 = 20;

/// `t_Modifiers` of a user or creature: the sum of its active buffs and debuffs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Modifiers {
    pub magic_bonus: f32,
    pub magic_reduction: f32,
    pub physical_bonus: f32,
    /// `PhysicalDamageBonusPve`: only against creatures.
    pub physical_bonus_pve: f32,
    pub physical_reduction: f32,
    pub speed: f32,
    pub hit: i32,
    pub evasion: i32,
    pub self_healing: f32,
    pub magic_healing: f32,
    pub linear: i32,
    pub linear_pve: i32,
    pub defense: i32,
}

/// VB `IncreaseInteger` with a Single: `CInt` rounds half to even.
fn int(v: f32) -> i32 {
    crate::combat::vb_round(v as f64) as i32
}

impl Modifiers {
    /// `ApplyEotModifier(target, stats, modifier)`: each value grows by `value * scale`
    /// (`scale` = the caster's weapon `MagicDamageBonus` / 100 for `AffectedByMagicBonus`).
    pub fn add(&mut self, m: &EotModifiers, scale: f32, pve_only: bool) {
        let s = |v: f32| v + v * scale;
        self.magic_bonus += s(m.magic_done);
        if pve_only {
            self.physical_bonus_pve += s(m.physical_done);
            self.linear_pve += int(s(m.physical_linear));
        } else {
            self.physical_bonus += s(m.physical_done);
            self.linear += int(s(m.physical_linear));
        }
        self.magic_reduction += s(m.magic_reduction);
        self.physical_reduction += s(m.physical_reduction);
        self.speed += s(m.speed);
        self.hit += int(s(m.hit));
        self.evasion += int(s(m.evasion));
        self.self_healing += s(m.self_healing);
        self.magic_healing += s(m.magic_healing);
        self.defense += int(s(m.defense));
    }

    /// `GetPhysicalDamageModifier`: max(1 + bonus, 0), the PvE share against creatures.
    pub fn physical_factor(&self, vs_npc: bool) -> f64 {
        let b = self.physical_bonus + if vs_npc { self.physical_bonus_pve } else { 0.0 };
        (1.0 + b as f64).max(0.0)
    }

    /// `GetPhysicDamageReduction`: max(1 - reduction, 0).
    pub fn physical_taken(&self) -> f64 {
        (1.0 - self.physical_reduction as f64).max(0.0)
    }

    /// `GetMagicDamageModifier`.
    pub fn magic_factor(&self) -> f64 {
        (1.0 + self.magic_bonus as f64).max(0.0)
    }

    /// `GetMagicDamageReduction`.
    pub fn magic_taken(&self) -> f64 {
        (1.0 - self.magic_reduction as f64).max(0.0)
    }

    /// `GetSelfHealingBonus` (healing received).
    pub fn self_healing_factor(&self) -> f64 {
        (1.0 + self.self_healing as f64).max(0.0)
    }

    /// `GetMagicHealingBonus` (healing cast on others).
    pub fn magic_healing_factor(&self) -> f64 {
        (1.0 + self.magic_healing as f64).max(0.0)
    }

    /// `GetLinearDamageBonus`.
    pub fn linear_bonus(&self, vs_npc: bool) -> i32 {
        self.linear + if vs_npc { self.linear_pve } else { 0 }
    }
}

/// `Damage = Damage * factor` on a VB Integer (banker's rounding on assignment).
pub fn scale(damage: i64, factor: f64) -> i64 {
    crate::combat::vb_round(damage as f64 * factor)
}

/// What an effect on a target is, for `FindEffectOnTarget`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectKey {
    pub eot: u16,
    pub kind: EotKind,
    pub shared_type_id: i32,
    /// Who cast it (an opaque id: user connection or creature).
    pub caster: u64,
}

/// `FindEffectOnTarget`: does `existing` count as the same effect as a new `def` (id
/// `eot`) cast by `caster`? `eAny` never matches.
pub fn same_effect(def: &EotDef, eot: u16, caster: u64, existing: &EffectKey) -> bool {
    match def.limit {
        EotLimit::Any => false,
        EotLimit::Single => existing.eot == eot,
        EotLimit::SingleByCaster => existing.eot == eot && existing.caster == caster,
        EotLimit::SingleByType => existing.kind == def.kind,
        EotLimit::SingleBySharedType => existing.shared_type_id == def.shared_type_id,
    }
}

/// `AreaSpellTileDistance`: Manhattan distance to the centre tile.
pub fn area_tile_distance(center: (i32, i32), tile: (i32, i32)) -> i32 {
    (tile.0 - center.0).abs() + (tile.1 - center.1).abs()
}

/// `ApplyAreaSpellFalloff`: 20 % less per tile, never below 0.
/// `BaseAmount - CInt(BaseAmount / 100 * reduction)`.
pub fn area_falloff(base: i32, tiles: i32) -> i32 {
    let pct = (tiles.max(0) * AREA_SPELL_FALLOFF_PER_TILE).min(100);
    base - crate::combat::vb_round(base as f64 / 100.0 * pct as f64) as i32
}

/// Tiles of an area spell: a square of side `AreaRadio` centred on the target tile
/// (`HechizoSobreArea`: `mitad = (AreaRadio - 1) \ 2`).
pub fn area_tiles(center: (i32, i32), side: i32) -> impl Iterator<Item = (i32, i32)> {
    let side = side.max(1);
    let half = (side - 1) / 2;
    (0..side).flat_map(move |x| (0..side).map(move |y| (center.0 + x - half, center.1 + y - half)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_sum_and_factors() {
        let mut m = Modifiers::default();
        let afilar = EotModifiers { physical_linear: 15.0, ..Default::default() };
        m.add(&afilar, 0.0, true);
        assert_eq!((m.linear_bonus(true), m.linear_bonus(false)), (15, 0), "PvE only");
        let penitencia = EotModifiers { physical_reduction: -0.3, magic_reduction: -0.3, ..Default::default() };
        m.add(&penitencia, 0.0, false);
        assert!((m.physical_taken() - 1.3).abs() < 1e-6);
        let quick = EotModifiers { physical_done: -0.65, ..Default::default() };
        m.add(&quick, 0.0, false);
        assert!((m.physical_factor(false) - 0.35).abs() < 1e-6);
        // Scaled by the caster's weapon (AffectedByMagicBonus): +20 % of 20.
        let mut p = Modifiers::default();
        p.add(&EotModifiers { physical_linear: 20.0, ..Default::default() }, 0.2, false);
        assert_eq!(p.linear, 24);
        assert_eq!(scale(100, 0.35), 35);
    }

    #[test]
    fn limits_follow_find_effect_on_target() {
        let def = |limit, shared| EotDef { limit, shared_type_id: shared, kind: EotKind::Shield, ..Default::default() };
        let have = EffectKey { eot: 43, kind: EotKind::Shield, shared_type_id: 6, caster: 1 };
        assert!(same_effect(&def(EotLimit::SingleBySharedType, 6), 29, 2, &have), "Protección Divina 1 and 3 share id 6");
        assert!(!same_effect(&def(EotLimit::Single, 6), 29, 1, &have));
        assert!(same_effect(&def(EotLimit::SingleByType, 0), 1, 1, &have));
        assert!(!same_effect(&def(EotLimit::SingleByCaster, 0), 43, 2, &have));
        assert!(!same_effect(&def(EotLimit::Any, 6), 43, 1, &have));
    }

    #[test]
    fn area_falloff_and_square() {
        assert_eq!(area_falloff(100, 0), 100);
        assert_eq!(area_falloff(100, 2), 60);
        assert_eq!(area_falloff(100, 7), 0);
        assert_eq!(area_falloff(37, 1), 37 - 7);
        assert_eq!(area_tile_distance((10, 10), (12, 9)), 3);
        let t: Vec<_> = area_tiles((10, 10), 3).collect();
        assert_eq!(t.len(), 9);
        assert!(t.contains(&(9, 9)) && t.contains(&(11, 11)));
        assert_eq!(area_tiles((10, 10), 4).next(), Some((9, 9)), "even sides lean right/down");
    }
}

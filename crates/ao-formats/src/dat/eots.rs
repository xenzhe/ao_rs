//! `Dat/EffectsOverTime.dat` (`FileIO.bas::LoadEffectsOverTime`): the definitions behind
//! every effect over time (`EOTn`) a spell, a potion or an item applies.
//!
//! Values are read with VB `Val` semantics: the number at the start of the value, so the
//! shipped `Ticks=5           'Cantidad de repeticiones` is 5 and an empty `CCInmunity=`
//! is 0.
use std::collections::HashMap;

/// `e_EffectOverTimeType` (the `Type` key).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EotKind {
    /// 1 `UpdateHpOverTime`.
    ModifyHealth,
    /// 2 `StatModifier`.
    ApplyModifiers,
    /// 3 `EffectProvoke`.
    Taunt,
    /// 4 `EffectProvoked`.
    Taunted,
    /// 5 `clsTrap`.
    Trap,
    /// 6 `DrunkEffect`.
    Drunk,
    /// 7 `TranslationEffect`.
    Translation,
    /// 8 `ApplyEffectOnHit`.
    ApplyEffectOnHit,
    /// 9 `UpdateManaOverTime`.
    ModifyMana,
    /// 10 `ApplyEffectToParty`.
    ApplyEffectToParty,
    /// 11 `AttrackEffect`.
    PullTarget,
    /// 12 `DelayedBlast`.
    DelayedBlast,
    /// 13 `UnequipItem`.
    Unequip,
    /// 14 `MultipleAttacks`.
    MultipleAttacks,
    /// 15 `ProtectEffect`.
    Shield,
    /// 16 `TransformEffect`.
    Transform,
    /// 17 `BonusDamageEffect`.
    DamageBonus,
    #[default]
    Unknown,
}

impl EotKind {
    pub fn from_id(id: i32) -> EotKind {
        use EotKind::*;
        match id {
            1 => ModifyHealth,
            2 => ApplyModifiers,
            3 => Taunt,
            4 => Taunted,
            5 => Trap,
            6 => Drunk,
            7 => Translation,
            8 => ApplyEffectOnHit,
            9 => ModifyMana,
            10 => ApplyEffectToParty,
            11 => PullTarget,
            12 => DelayedBlast,
            13 => Unequip,
            14 => MultipleAttacks,
            15 => Shield,
            16 => Transform,
            17 => DamageBonus,
            _ => Unknown,
        }
    }
}

/// `e_EOTTargetLimit` (the `Limit` key): what counts as "already affected".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EotLimit {
    /// 1: one effect with this id on the target.
    #[default]
    Single,
    /// 2: one per caster.
    SingleByCaster,
    /// 3: no limit.
    Any,
    /// 4: one of this `Type`.
    SingleByType,
    /// 5: one of this `SharedTypeId` (levels of the same buff).
    SingleBySharedType,
}

impl EotLimit {
    pub fn from_id(id: i32) -> EotLimit {
        match id {
            2 => EotLimit::SingleByCaster,
            3 => EotLimit::Any,
            4 => EotLimit::SingleByType,
            5 => EotLimit::SingleBySharedType,
            _ => EotLimit::Single,
        }
    }
}

/// The modifiers a buff or debuff adds while it lasts (`ApplyEotModifier`). Fractions:
/// 0.4 = 40 %; `hit`, `evasion`, `physical_linear` and `defense` are flat points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EotModifiers {
    pub physical_reduction: f32,
    pub magic_reduction: f32,
    pub physical_done: f32,
    pub magic_done: f32,
    pub speed: f32,
    pub hit: f32,
    pub evasion: f32,
    pub self_healing: f32,
    pub magic_healing: f32,
    pub physical_linear: f32,
    pub defense: f32,
}

/// One `[EOTn]` section.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EotDef {
    pub kind: EotKind,
    pub raw_kind: i32,
    pub limit: EotLimit,
    pub shared_type_id: i32,
    pub power_min: i32,
    pub power_max: i32,
    pub ticks: u32,
    pub tick_ms: u64,
    pub tick_fx: u16,
    pub override_: bool,
    /// `BuffType`: 1 buff, 2 debuff, 3 cooldown, 4/5 informative.
    pub buff_type: i32,
    pub tick_mana: i32,
    pub tick_stamina: i32,
    /// `Area` (tiles) of party songs and taunts.
    pub area: i32,
    pub apply_effect_id: u16,
    pub secondary_effect_id: u16,
    pub npc_id: u16,
    /// `SubType`: blows per hit of `MultipleAttacks`, trap kind.
    pub subtype: i32,
    pub on_hit_fx: u16,
    pub require_transform: u16,
    pub require_weapon: bool,
    pub require_shield: bool,
    pub require_armor: bool,
    pub require_helm: bool,
    pub physical_bonus_pve_only: bool,
    pub cast_only_on_self: bool,
    pub transform: bool,
    pub cc_immunity: bool,
    /// `ApplyStatusMask=8` of the bear and spirit forms (`eCastOnlyOnSelf`... the raw bits).
    pub status_mask: i32,
    pub affected_by_magic_bonus: bool,
    pub affected_by_magic_reduction: bool,
    pub affected_by_physical_bonus: bool,
    pub affected_by_physical_reduction: bool,
    pub affected_by_magic_healing: bool,
    pub affected_by_self_healing: bool,
    pub secondary_target_modifier: f32,
    pub mods: EotModifiers,
    /// `ClientEffectTypeId`: icon row of the client's `init/Effects.ini` (0 = not shown).
    pub client_effect_type_id: u16,
    /// `OnHitWAV`: sound of a trap or blast going off.
    pub on_hit_wav: u16,
}

/// VB `Val`: the leading number of a value (`5   'comment` = 5, `` = 0).
fn val(v: &str) -> f64 {
    let v = v.trim();
    let end = v.char_indices().find(|(i, c)| !(c.is_ascii_digit() || *c == '.' || (*i == 0 && (*c == '-' || *c == '+')))).map_or(v.len(), |(i, _)| i);
    v[..end].parse().unwrap_or(0.0)
}

/// Reads the whole file. Unknown keys are ignored.
pub fn parse(text: &str) -> HashMap<u16, EotDef> {
    let mut out = HashMap::new();
    let mut cur: Option<(u16, EotDef)> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('[') {
            if let Some((id, def)) = cur.take() {
                out.insert(id, def);
            }
            let name = rest.split(']').next().unwrap_or("");
            cur = name.strip_prefix("EOT").and_then(|n| n.parse().ok()).map(|id| (id, EotDef::default()));
            continue;
        }
        if line.starts_with('\'') {
            continue;
        }
        let (Some((_, d)), Some((k, v))) = (cur.as_mut(), line.split_once('=')) else { continue };
        let n = val(v);
        let b = n != 0.0;
        let u16v = n.clamp(0.0, u16::MAX as f64) as u16;
        let f = n as f32;
        let m = &mut d.mods;
        match k.trim().to_ascii_lowercase().as_str() {
            "type" => {
                d.raw_kind = n as i32;
                d.kind = EotKind::from_id(n as i32);
            }
            "limit" | "limits" => d.limit = EotLimit::from_id(n as i32),
            "sharedtypeid" => d.shared_type_id = n as i32,
            "tickpowermin" => d.power_min = n as i32,
            "tickpowermax" => d.power_max = n as i32,
            "ticks" => d.ticks = n.max(0.0) as u32,
            "ticktime" => d.tick_ms = n.max(0.0) as u64,
            "tickfx" => d.tick_fx = u16v,
            "override" => d.override_ = b,
            "bufftype" => d.buff_type = n as i32,
            "tickmanaconsumption" => d.tick_mana = n as i32,
            "tickstaminaconsumption" => d.tick_stamina = n as i32,
            "area" => d.area = n as i32,
            "applyeffectid" => d.apply_effect_id = u16v,
            "secondaryeffectid" => d.secondary_effect_id = u16v,
            "npcid" => d.npc_id = u16v,
            "subtype" => d.subtype = n as i32,
            "onhitfx" => d.on_hit_fx = u16v,
            "requiretransform" => d.require_transform = u16v,
            "requireweapon" => d.require_weapon = b,
            "requireshield" => d.require_shield = b,
            "requirearmor" => d.require_armor = b,
            "requirehelm" => d.require_helm = b,
            "physicalbonuspveonly" => d.physical_bonus_pve_only = b,
            "limitcastonlyonself" => d.cast_only_on_self = b,
            "transform" => d.transform = b,
            "ccinmunity" => d.cc_immunity = b,
            "applystatusmask" => d.status_mask = n as i32,
            "affectedbymagicbonus" => d.affected_by_magic_bonus = b,
            "affectedbymagicreduction" => d.affected_by_magic_reduction = b,
            "affectedbyphysicalbonus" => d.affected_by_physical_bonus = b,
            "affectedbyphysicalreduction" => d.affected_by_physical_reduction = b,
            "affectedbymagichealing" => d.affected_by_magic_healing = b,
            "affectedbyselfhealing" => d.affected_by_self_healing = b,
            "secondarytargetmodifier" => d.secondary_target_modifier = f,
            "physicaldamagereduction" => m.physical_reduction = f,
            "magicdamagereduction" => m.magic_reduction = f,
            "physicaldamagedone" => m.physical_done = f,
            "magicdamagedone" => m.magic_done = f,
            "speedmodifier" => m.speed = f,
            "hitmodifier" => m.hit = f,
            "evasionmodifier" => m.evasion = f,
            "selfhealingbonus" => m.self_healing = f,
            "magichealingbonus" => m.magic_healing = f,
            "physicallinearbonus" => m.physical_linear = f,
            "defensebonus" => m.defense = f,
            "clienteffecttypeid" => d.client_effect_type_id = u16v,
            "onhitwav" => d.on_hit_wav = u16v,
            _ => {}
        }
    }
    if let Some((id, def)) = cur {
        out.insert(id, def);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_types_limits_modifiers_and_vb_val() {
        let t = parse("'Types\n[INIT]\nEffectCount=2\n[EOT1] 'x\nType=1\nTickPowerMin=8\nTickPowerMax=12\nTicks=5           'Cantidad\nTickTime=3000\nTickFX=144\nOverride=1\n[EOT30] 'wolf\nType=16\nLimit=4\nNpcId=900\nPhysicalLinearBonus=35\nHitModifier=50\nCCInmunity=\nMagicDamageReduction=0.1\n");
        assert_eq!((t[&1].kind, t[&1].ticks, t[&1].tick_ms, t[&1].tick_fx, t[&1].override_), (EotKind::ModifyHealth, 5, 3000, 144, true));
        let w = &t[&30];
        assert_eq!((w.kind, w.limit, w.npc_id, w.cc_immunity), (EotKind::Transform, EotLimit::SingleByType, 900, false));
        assert_eq!((w.mods.physical_linear, w.mods.hit, w.mods.magic_reduction), (35.0, 50.0, 0.1));
    }

    #[test]
    fn shipped_file_has_every_spell_effect() {
        let Ok(bytes) = std::fs::read(crate::recursos_dir().join("Dat").join("EffectsOverTime.dat")) else { return };
        let t = parse(&String::from_utf8_lossy(&bytes));
        assert_eq!(t[&29].kind, EotKind::Shield);
        assert_eq!((t[&29].shared_type_id, t[&29].limit), (6, EotLimit::SingleBySharedType));
        assert_eq!(t[&24].subtype, 2, "quick blows: 2 extra blows per hit");
        assert_eq!(t[&49].mods.physical_linear, 15.0);
        assert!(t[&49].require_weapon && t[&49].physical_bonus_pve_only);
        assert_eq!(t[&21].apply_effect_id, 22);
        assert_eq!((t[&5].kind, t[&5].client_effect_type_id, t[&5].area), (EotKind::Taunt, 4, 20));
        assert_eq!((t[&7].kind, t[&7].subtype, t[&7].on_hit_wav), (EotKind::Trap, 2, 162));
        assert_eq!((t[&20].kind, t[&20].area, t[&20].on_hit_fx), (EotKind::DelayedBlast, 5, 121));
        assert_eq!((t[&11].kind, t[&11].ticks, t[&11].apply_effect_id), (EotKind::Translation, 5, 12));
    }
}

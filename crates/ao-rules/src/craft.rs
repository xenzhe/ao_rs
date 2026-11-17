//! Crafting rules of the four trades (blacksmith, carpenter, alchemist, tailor), pure and
//! without I/O. OWNER: Opus B (docs/P4_CONTRATO.md).
//!
//! Ported from `Trabajo.bas` (`*TieneMateriales`, `*QuitarMateriales`, `PuedeConstruir*`,
//! `*ConstruirItem`, `Mod*` class modifiers, `Trabajar`), the list filters of
//! `Protocol_Writes.bas` (`WriteBlacksmithWeapons/Armors/ElementalRunes`,
//! `WriteCarpenterObjects`, `WriteAlquimistaObjects`, `WriteSastreObjects`) and
//! `Modulo_UsUaRiOs.SubirSkill` (raising a skill by use).
//!
//! The four trades never fail: there is no success roll in `*ConstruirItem` (the
//! probability of `modCrafteos.CalculateCraftProb` belongs to the NPC combination
//! crafting, which no official NPC offers).
use ao_formats::dat::balance::Class;
use ao_formats::dat::craft::CraftLists;
use ao_formats::dat::obj::{ObjData, ObjType};
use serde::{Deserialize, Serialize};

/// `IntervaloTrabajarConstruir` (intervalos.ini): one build step every 500 ms.
pub const BUILD_INTERVAL_MS: u64 = 500;
/// Most units one build order may ask for (the inventory stack limit).
pub const MAX_ORDER: u32 = 10_000;
/// `MAXSKILLPOINTS`.
pub const MAX_SKILL: u8 = 100;
/// `Distancia(pos, user) > 2` in `HandleAnvilAction`.
pub const ANVIL_RANGE: i32 = 2;
/// `NONEXPERT_SKILL_CUTOFF` (Declares.bas): the raise roll must be below it.
pub const SKILL_CUTOFF: i32 = 10;
/// `e_AnvilType.BlodiumAnvil` (anvil `Subtipo`).
pub const BLODIUM_ANVIL: i32 = 2;
/// `SubirSkill`: `BonusExp = 5 * ExpMult` (ExpMult 1 here).
pub const SKILL_RAISE_EXP: u32 = 5;

/// The four trades. The number is what travels in the protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Trade {
    Blacksmith = 1,
    Carpenter = 2,
    Alchemy = 3,
    Tailor = 4,
}

impl Trade {
    pub const ALL: [Trade; 4] = [Trade::Blacksmith, Trade::Carpenter, Trade::Alchemy, Trade::Tailor];

    pub fn from_id(id: u8) -> Option<Trade> {
        Trade::ALL.into_iter().find(|t| *t as u8 == id)
    }

    /// Name for the window tab.
    pub fn name(self) -> &'static str {
        match self {
            Trade::Blacksmith => "Herrería",
            Trade::Carpenter => "Carpintería",
            Trade::Alchemy => "Alquimia",
            Trade::Tailor => "Sastrería",
        }
    }

    /// Index into the 24 skills (`e_Skill` - 1): Herreria 20, Carpinteria 21, Alquimia 22,
    /// Sastreria 23.
    pub fn skill(self) -> usize {
        match self {
            Trade::Blacksmith => 19,
            Trade::Carpenter => 20,
            Trade::Alchemy => 21,
            Trade::Tailor => 22,
        }
    }

    /// `SkillsNames` (General.bas), for the raise message.
    pub fn skill_name(self) -> &'static str {
        self.name()
    }

    /// `Subtipo` of the working tool it needs (`e_WorkingToolSubType`): SmithHammer 7,
    /// CarpentryHacksaw 5, the alchemy pot 4 (`AlquimistaConstruirItem`), TailorSewingbox 9.
    pub fn tool_subtype(self) -> i32 {
        match self {
            Trade::Blacksmith => 7,
            Trade::Carpenter => 5,
            Trade::Alchemy => 4,
            Trade::Tailor => 9,
        }
    }

    /// Stamina: (it must be above this to work, it costs this). Blacksmith: `MinSta - 2`
    /// with no check (we clamp at 0 and require something left). Carpenter: `MinSta > 2`,
    /// `QuitarSta 2`. Alchemy: `MinSta > 0`, `- 1`. Tailor: `MinSta > 0`, `- 2`.
    pub fn stamina(self) -> (i32, i32) {
        match self {
            Trade::Blacksmith => (0, 2),
            Trade::Carpenter => (2, 2),
            Trade::Alchemy => (0, 1),
            Trade::Tailor => (0, 2),
        }
    }

    /// Wave played around the crafter: MARTILLOHERRERO 41, LABUROCARPINTERO 42, 1152
    /// (alchemy), 63 (tailor).
    pub fn sound(self) -> u16 {
        match self {
            Trade::Blacksmith => 41,
            Trade::Carpenter => 42,
            Trade::Alchemy => 1152,
            Trade::Tailor => 63,
        }
    }

    /// The skill an object needs in this trade (`SkHerreria`, `SkCarpinteria`,
    /// `SkPociones`, `SKSastreria`).
    pub fn required_skill(self, o: &ObjData) -> i32 {
        match self {
            Trade::Blacksmith => o.blacksmith_skill,
            Trade::Carpenter => o.carpentry_skill,
            Trade::Alchemy => o.potion_skill,
            Trade::Tailor => o.tailoring_skill,
        }
    }
}

// ---- material object ids (Declares.bas) ----
pub const IRON_INGOT: u16 = 386;
pub const SILVER_INGOT: u16 = 387;
pub const GOLD_INGOT: u16 = 388;
pub const COAL: u16 = 3391;
pub const BLODIUM: u16 = 3787;
pub const FIRE_ESSENCE: u16 = 5179;
pub const WATER_ESSENCE: u16 = 5180;
pub const EARTH_ESSENCE: u16 = 5181;
pub const WIND_ESSENCE: u16 = 5182;
pub const WOOD: u16 = 58;
pub const ELVEN_WOOD: u16 = 2781;
pub const PINE_WOOD: u16 = 3788;
pub const MORTAR: u16 = 4997;
pub const ALCHEMY_FLASK: u16 = 3075;
pub const LIGHT_MUSHROOM: u16 = 4310;
pub const SPORES: u16 = 4311;
pub const PRICKLY_PEAR: u16 = 4312;
pub const CALLA_LILY: u16 = 4313;
pub const FOXTAIL: u16 = 4314;
pub const OCEAN_FLOWER: u16 = 4315;
pub const RED_FLOWER: u16 = 4316;
pub const PROSPEROUS_SEEDS: u16 = 4320;
pub const PELT_WOLF: u16 = 414;
pub const PELT_BROWN_BEAR: u16 = 415;
pub const PELT_POLAR_BEAR: u16 = 416;
pub const PELT_BLACK_WOLF: u16 = 1146;
pub const PELT_TIGER: u16 = 4339;
pub const PELT_BENGAL_TIGER: u16 = 1145;

/// One material of a recipe: object, units, and the official message when missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Material {
    pub obj: u16,
    pub amount: u32,
    pub missing: &'static str,
}

const ESSENCE: &str = "Te falta la esencia requerida.";

/// What building `amount` units of `o` takes, in the order the VB checks them
/// (`HerreroTieneMateriales`, `CarpinteroTieneMateriales`, `AlquimistaTieneMateriales`,
/// `SastreTieneMateriales`). Only the carpenter multiplies by the amount: the other trades
/// always build one unit per step.
pub fn materials(trade: Trade, o: &ObjData, amount: u32) -> Vec<Material> {
    let per: &[(i32, u16, &'static str)] = match trade {
        Trade::Blacksmith => &[
            (o.iron_ingots, IRON_INGOT, "No tenés suficientes lingotes de hierro."),
            (o.silver_ingots, SILVER_INGOT, "No tenés suficientes lingotes de plata."),
            (o.gold_ingots, GOLD_INGOT, "No tenés suficientes lingotes de oro."),
            (o.coal, COAL, "No tenés suficientes carbón."),
            (o.blodium, BLODIUM, "No tenés suficiente blodium."),
            (o.fire_essence, FIRE_ESSENCE, ESSENCE),
            (o.water_essence, WATER_ESSENCE, ESSENCE),
            (o.earth_essence, EARTH_ESSENCE, ESSENCE),
            (o.wind_essence, WIND_ESSENCE, ESSENCE),
        ],
        Trade::Carpenter => &[
            (o.wood, WOOD, "No tenés suficiente madera."),
            (o.elven_wood, ELVEN_WOOD, "No tenés suficiente madera élfica."),
            (o.pine_wood, PINE_WOOD, "No tenés suficiente madera de pino nudoso."),
        ],
        Trade::Alchemy => &[
            (o.mortar, MORTAR, "No tenés suficientes morteros."),
            (o.alchemy_flask, ALCHEMY_FLASK, "No tenés suficientes frascos de alquimistas."),
            (o.light_mushroom, LIGHT_MUSHROOM, "No tenés suficientes Hongos de Luz."),
            (o.spores, SPORES, "No tenés suficientes Esporas Silvestres."),
            (o.prickly_pear, PRICKLY_PEAR, "No tenés suficientes Tunas Silvestres."),
            (o.calla_lily, CALLA_LILY, "No tenés suficientes Calas Venenosas."),
            (o.foxtail, FOXTAIL, "No tenés suficientes Colas de Zorro."),
            (o.ocean_flower, OCEAN_FLOWER, "No tenés suficientes Flores del óceano."),
            (o.red_flower, RED_FLOWER, "No tenés suficientes Flores Roja."),
            (o.prosperous_seeds, PROSPEROUS_SEEDS, "No tenés suficientes Semillas Prosperas."),
        ],
        Trade::Tailor => &[
            (o.pelt_wolf, PELT_WOLF, "No tenés suficientes pieles de lobo."),
            (o.pelt_brown_bear, PELT_BROWN_BEAR, "No tenés suficientes pieles de oso pardo."),
            (o.pelt_polar_bear, PELT_POLAR_BEAR, "No tenés suficientes pieles de oso polar."),
            (o.pelt_black_wolf, PELT_BLACK_WOLF, "No tenés suficientes pieles de lobo negro."),
            (o.pelt_tiger, PELT_TIGER, "No tenés suficientes pieles de tigre."),
            (o.pelt_bengal_tiger, PELT_BENGAL_TIGER, "No tenés suficientes pieles de tigre de bengala."),
        ],
    };
    let mult = if trade == Trade::Carpenter { amount.max(1) } else { 1 };
    per.iter().filter(|(n, _, _)| *n > 0).map(|&(n, obj, missing)| Material { obj, amount: (n as u32).saturating_mul(mult), missing }).collect()
}

/// `ModHerreria`, `ModCarpinteria`, `ModAlquimia` (druids too), `ModSastre`: 1 for the
/// worker, 3 for the rest.
pub fn class_mod(trade: Trade, class: Option<Class>) -> i32 {
    match (trade, class) {
        (_, Some(Class::Worker)) => 1,
        (Trade::Alchemy, Some(Class::Druid)) => 1,
        _ => 3,
    }
}

/// VB `Round(x, 0)`: banker's rounding.
fn vb_round(x: f64) -> i32 {
    let f = x.floor();
    let d = x - f;
    let r = if d > 0.5 || (d == 0.5 && (f as i64) % 2 != 0) { f + 1.0 } else { f };
    r as i32
}

/// The skill level the official list compares against. The class modifier only applies to
/// the blacksmith's armors (`Round(skill / ModHerreria)`) and to alchemy
/// (`skill \ ModAlquimia`); weapons, runes, carpentry and tailoring use the raw skill.
pub fn effective_skill(trade: Trade, o: &ObjData, skill: u8, class: Option<Class>) -> i32 {
    let s = skill as i32;
    match trade {
        Trade::Blacksmith if smith_group(o) == Some(SmithGroup::Armor) => vb_round(s as f64 / class_mod(trade, class) as f64),
        Trade::Alchemy => s / class_mod(trade, class),
        _ => s,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmithGroup {
    Weapon,
    Armor,
    Rune,
}

/// `PuedeConstruirHerreria`: which blacksmith list an object type is looked up in.
fn smith_group(o: &ObjData) -> Option<SmithGroup> {
    match o.obj_type {
        ObjType::Weapon | ObjType::Arrows => Some(SmithGroup::Weapon),
        ObjType::Armor | ObjType::Helmet | ObjType::Shield | ObjType::Amulets | ObjType::RingAccessory => Some(SmithGroup::Armor),
        ObjType::ElementalRune => Some(SmithGroup::Rune),
        _ => None,
    }
}

/// `PuedeConstruirHerreria` / `PuedeConstruirCarpintero` / `PuedeConstruirAlquimista` /
/// `PuedeConstruirSastre`: the object is in the trade's official list. For the blacksmith
/// the list depends on the object type, so an item in the wrong list (or of another type)
/// cannot be built even if it is listed.
pub fn in_trade_list(trade: Trade, lists: &CraftLists, id: u16, o: &ObjData) -> bool {
    match trade {
        Trade::Blacksmith => match smith_group(o) {
            Some(SmithGroup::Weapon) => lists.smith_weapons.contains(&id),
            Some(SmithGroup::Armor) => lists.smith_armors.contains(&id),
            Some(SmithGroup::Rune) => lists.smith_runes.contains(&id),
            None => false,
        },
        Trade::Carpenter => lists.carpenter.contains(&id),
        Trade::Alchemy => lists.alchemy.contains(&id),
        Trade::Tailor => lists.tailor.contains(&id),
    }
}

/// Every object of a trade's lists, in the official order (blacksmith: weapons, armors,
/// runes, the order the anvil sends them in `HandleAnvilAction`).
pub fn trade_list(trade: Trade, lists: &CraftLists) -> Vec<u16> {
    match trade {
        Trade::Blacksmith => lists.smith_weapons.iter().chain(&lists.smith_armors).chain(&lists.smith_runes).copied().collect(),
        Trade::Carpenter => lists.carpenter.clone(),
        Trade::Alchemy => lists.alchemy.clone(),
        Trade::Tailor => lists.tailor.clone(),
    }
}

/// Whether the player may build `o` with this skill and class: it is in the trade list
/// and the requirement is within the (class-adjusted) skill. The same predicate decides
/// the window list and the build, so what is not listed cannot be built. (VB lists with the
/// class modifier but builds with the raw skill; we use the stricter one for both.)
pub fn can_build(trade: Trade, lists: &CraftLists, id: u16, o: &ObjData, skill: u8, class: Option<Class>) -> bool {
    in_trade_list(trade, lists, id, o) && trade.required_skill(o) <= effective_skill(trade, o, skill, class)
}

/// `Trabajar` (carpentry): a worker builds up to `skill / 10` units per step (VB assigns a
/// Double to a Long: banker's rounding; at least 1); everyone else one by one. The other
/// trades always build one unit per step.
pub fn units_per_step(trade: Trade, class: Option<Class>, skill: u8) -> u32 {
    if trade == Trade::Carpenter && class == Some(Class::Worker) {
        vb_round(skill as f64 / 10.0).max(1) as u32
    } else {
        1
    }
}

/// `SubirSkill`: the highest value a skill may reach at a level (5 points every two
/// levels).
pub fn skill_cap(level: u32) -> i32 {
    let l = level as i32;
    if l % 2 == 0 {
        (l / 2) * 5
    } else {
        (l / 2) * 5 + 3 - ((((l - 1) / 2) * 5) % 10) / 5
    }
}

/// `SubirSkill`: the raise succeeds when `RandomNumber(1, roll_max) < SKILL_CUTOFF`.
/// `Prob = Int(0.1 * lvl^2 + 15)`, `roll_max = Prob * DificultadSubirSkill`.
pub fn raise_roll_max(level: u32, difficulty: i32) -> i32 {
    let prob = (0.1 * (level as f64).powi(2) + 15.0).floor() as i32;
    (prob * difficulty.max(1)).max(1)
}

/// `SubirSkill` preconditions: below 100, below the level cap and not starving.
pub fn may_raise(skill: u8, level: u32, food: i32, water: i32) -> bool {
    skill < MAX_SKILL && (skill as i32) < skill_cap(level) && food > 0 && water > 0
}

/// `Distancia` (Manhattan), as used by `HandleAnvilAction`.
pub fn distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(t: ObjType) -> ObjData {
        ObjData { obj_type: t, ..Default::default() }
    }

    #[test]
    fn class_modifiers() {
        assert_eq!(class_mod(Trade::Blacksmith, Some(Class::Worker)), 1);
        assert_eq!(class_mod(Trade::Blacksmith, Some(Class::Mage)), 3);
        assert_eq!(class_mod(Trade::Alchemy, Some(Class::Druid)), 1);
        assert_eq!(class_mod(Trade::Tailor, Some(Class::Druid)), 3);
    }

    #[test]
    fn armor_list_uses_the_class_modifier_and_weapons_do_not() {
        let lists = CraftLists { smith_weapons: vec![10], smith_armors: vec![20], ..Default::default() };
        let mut sword = obj(ObjType::Weapon);
        sword.blacksmith_skill = 30;
        let mut helm = obj(ObjType::Helmet);
        helm.blacksmith_skill = 30;
        // Warrior with 60: weapons need 30 raw (ok); armor needs 30 <= round(60/3) = 20 (no).
        let w = Some(Class::Warrior);
        assert!(can_build(Trade::Blacksmith, &lists, 10, &sword, 60, w));
        assert!(!can_build(Trade::Blacksmith, &lists, 20, &helm, 60, w));
        assert!(can_build(Trade::Blacksmith, &lists, 20, &helm, 90, w));
        assert!(can_build(Trade::Blacksmith, &lists, 20, &helm, 30, Some(Class::Worker)));
        // A helmet is never looked up in the weapon list.
        assert!(!can_build(Trade::Blacksmith, &CraftLists { smith_weapons: vec![20], ..Default::default() }, 20, &helm, 100, w));
    }

    #[test]
    fn alchemy_divides_by_the_class_modifier() {
        let lists = CraftLists { alchemy: vec![5], ..Default::default() };
        let mut p = obj(ObjType::Potions);
        p.potion_skill = 20;
        assert!(!can_build(Trade::Alchemy, &lists, 5, &p, 59, Some(Class::Mage)));
        assert!(can_build(Trade::Alchemy, &lists, 5, &p, 60, Some(Class::Mage)));
        assert!(can_build(Trade::Alchemy, &lists, 5, &p, 20, Some(Class::Druid)));
    }

    #[test]
    fn carpenter_materials_scale_with_amount_others_do_not() {
        let mut bow = obj(ObjType::Weapon);
        bow.wood = 200;
        bow.elven_wood = 3;
        let m = materials(Trade::Carpenter, &bow, 4);
        assert_eq!(m, vec![
            Material { obj: WOOD, amount: 800, missing: "No tenés suficiente madera." },
            Material { obj: ELVEN_WOOD, amount: 12, missing: "No tenés suficiente madera élfica." }
        ]);
        let mut armor = obj(ObjType::Armor);
        armor.pelt_wolf = 75;
        assert_eq!(materials(Trade::Tailor, &armor, 9)[0].amount, 75);
        // The carpenter's wood is not a blacksmith material.
        assert!(materials(Trade::Blacksmith, &bow, 1).is_empty());
    }

    #[test]
    fn worker_batch_rounds_like_vb() {
        let w = Some(Class::Worker);
        assert_eq!(units_per_step(Trade::Carpenter, w, 0), 1);
        assert_eq!(units_per_step(Trade::Carpenter, w, 15), 2);
        assert_eq!(units_per_step(Trade::Carpenter, w, 25), 2);
        assert_eq!(units_per_step(Trade::Carpenter, w, 100), 10);
        assert_eq!(units_per_step(Trade::Carpenter, Some(Class::Mage), 100), 1);
        assert_eq!(units_per_step(Trade::Tailor, w, 100), 1);
    }

    #[test]
    fn skill_cap_follows_subirskill() {
        assert_eq!(skill_cap(1), 3);
        assert_eq!(skill_cap(2), 5);
        assert_eq!(skill_cap(3), 7);
        assert_eq!(skill_cap(4), 10);
        assert_eq!(skill_cap(5), 13);
        assert_eq!(skill_cap(40), 100);
        assert!(!may_raise(3, 1, 100, 100));
        assert!(may_raise(2, 1, 100, 100));
        assert!(!may_raise(2, 1, 0, 100));
        assert_eq!(raise_roll_max(1, 4), 60);
    }

    #[test]
    fn trade_ids_roundtrip() {
        for t in Trade::ALL {
            assert_eq!(Trade::from_id(t as u8), Some(t));
        }
        assert_eq!(Trade::from_id(0), None);
        assert_eq!(Trade::from_id(9), None);
    }
}

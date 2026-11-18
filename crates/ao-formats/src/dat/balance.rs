//! `Balance.dat`: class and race modifiers, backstab/stun/extra tuning, experience per
//! level, `/hogar` cost tiers and the NPC elemental matrix.
//!
//! Mirrors `LoadBalance` (`FileIO.bas`). Also defines the official id enums for races,
//! classes, skills and weapon types (`Declares.bas`: `e_Raza`, `e_Class`, `e_Skill`,
//! `e_WeaponType`), which the other readers share.
use super::{tilde, Loaded, RawIni, Warning};
use std::path::Path;

pub const FILE: &str = "Balance.dat";
/// `NUMCLASES`.
pub const NUM_CLASSES: usize = 12;
/// `NUMRAZAS`.
pub const NUM_RACES: usize = 6;
/// `NUMSKILLS`.
pub const NUM_SKILLS: usize = 24;
/// `STAT_MAXELV`: the experience table has one entry per level up to this one.
pub const MAX_LEVEL: usize = 47;
/// Longest experience table read (sanity limit).
pub const MAX_LEVEL_LIMIT: usize = 255;
/// `HOME_COST_TIER_COUNT`.
pub const HOME_COST_TIERS: usize = 6;
/// `MAX_ELEMENT_TAGS`: side of the elemental matrix.
pub const ELEMENT_TAGS: usize = 4;
/// `eWeaponTypeCount - 1`: weapon types are 1-based.
pub const NUM_WEAPON_TYPES: usize = 11;

/// `e_Raza`, with the official ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Race {
    Human = 1,
    Elf = 2,
    /// "Elfo Oscuro".
    Drow = 3,
    Gnome = 4,
    Dwarf = 5,
    Orc = 6,
}

impl Race {
    pub const ALL: [Race; NUM_RACES] = [Race::Human, Race::Elf, Race::Drow, Race::Gnome, Race::Dwarf, Race::Orc];

    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: i32) -> Option<Race> {
        Race::ALL.get(usize::try_from(id).ok()?.checked_sub(1)?).copied()
    }

    /// Spanish name as the official server lists it (`ListaRazas`).
    pub fn spanish_name(self) -> &'static str {
        match self {
            Race::Human => "Humano",
            Race::Elf => "Elfo",
            Race::Drow => "Elfo Oscuro",
            Race::Gnome => "Gnomo",
            Race::Dwarf => "Enano",
            Race::Orc => "Orco",
        }
    }

    /// Accent- and case-insensitive; accepts `RaceToString` names (`DROW`) and the list
    /// name (`Elfo Oscuro`).
    pub fn from_name(name: &str) -> Option<Race> {
        let t = tilde(name);
        Race::ALL.into_iter().find(|r| tilde(r.spanish_name()) == t).or(if t == "DROW" { Some(Race::Drow) } else { None })
    }

    /// Prefix of its keys in `[MODRAZA]` (`Tilde(name)` without spaces).
    fn balance_key(self) -> String {
        tilde(self.spanish_name()).replace(' ', "")
    }
}

/// `e_Class`, with the official ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    Mage = 1,
    Cleric = 2,
    Warrior = 3,
    Assassin = 4,
    Bard = 5,
    Druid = 6,
    Paladin = 7,
    Hunter = 8,
    /// "Trabajador".
    Worker = 9,
    Pirate = 10,
    Thief = 11,
    Bandit = 12,
}

impl Class {
    pub const ALL: [Class; NUM_CLASSES] = [
        Class::Mage,
        Class::Cleric,
        Class::Warrior,
        Class::Assassin,
        Class::Bard,
        Class::Druid,
        Class::Paladin,
        Class::Hunter,
        Class::Worker,
        Class::Pirate,
        Class::Thief,
        Class::Bandit,
    ];

    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: i32) -> Option<Class> {
        Class::ALL.get(usize::try_from(id).ok()?.checked_sub(1)?).copied()
    }

    /// Spanish name as the official server lists it (`ListaClases`).
    pub fn spanish_name(self) -> &'static str {
        match self {
            Class::Mage => "Mago",
            Class::Cleric => "Clérigo",
            Class::Warrior => "Guerrero",
            Class::Assassin => "Asesino",
            Class::Bard => "Bardo",
            Class::Druid => "Druida",
            Class::Paladin => "Paladín",
            Class::Hunter => "Cazador",
            Class::Worker => "Trabajador",
            Class::Pirate => "Pirata",
            Class::Thief => "Ladrón",
            Class::Bandit => "Bandido",
        }
    }

    /// Accent- and case-insensitive (`CLERIGO`, `Ladrón`...).
    pub fn from_name(name: &str) -> Option<Class> {
        let t = tilde(name);
        Class::ALL.into_iter().find(|c| tilde(c.spanish_name()) == t)
    }

    /// Key used in the `[MOD*]` sections (`Tilde(name)` without spaces).
    fn balance_key(self) -> String {
        tilde(self.spanish_name()).replace(' ', "")
    }
}

/// `e_Skill` (the 24 trainable skills plus the two special ids the server also uses).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Skill {
    Magic = 1,
    Steal = 2,
    Tactics = 3,
    Weapons = 4,
    Meditate = 5,
    Stab = 6,
    Hide = 7,
    Survival = 8,
    Trade = 9,
    ShieldDefense = 10,
    Leadership = 11,
    Projectiles = 12,
    Wrestling = 13,
    Sailing = 14,
    Riding = 15,
    MagicResistance = 16,
    Woodcutting = 17,
    Fishing = 18,
    Mining = 19,
    Blacksmithing = 20,
    Carpentry = 21,
    Alchemy = 22,
    Tailoring = 23,
    Taming = 24,
    TargetableItem = 25,
    Smelting = 26,
}

impl Skill {
    pub const ALL: [Skill; 26] = [
        Skill::Magic,
        Skill::Steal,
        Skill::Tactics,
        Skill::Weapons,
        Skill::Meditate,
        Skill::Stab,
        Skill::Hide,
        Skill::Survival,
        Skill::Trade,
        Skill::ShieldDefense,
        Skill::Leadership,
        Skill::Projectiles,
        Skill::Wrestling,
        Skill::Sailing,
        Skill::Riding,
        Skill::MagicResistance,
        Skill::Woodcutting,
        Skill::Fishing,
        Skill::Mining,
        Skill::Blacksmithing,
        Skill::Carpentry,
        Skill::Alchemy,
        Skill::Tailoring,
        Skill::Taming,
        Skill::TargetableItem,
        Skill::Smelting,
    ];

    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: i32) -> Option<Skill> {
        Skill::ALL.get(usize::try_from(id).ok()?.checked_sub(1)?).copied()
    }

    /// Official in-game name (`SkillsNames`); `None` for the two ids above `NUMSKILLS`.
    pub fn spanish_name(self) -> Option<&'static str> {
        Some(match self {
            Skill::Magic => "Magia",
            Skill::Steal => "Robar",
            Skill::Tactics => "Destreza en combate",
            Skill::Weapons => "Combate con armas",
            Skill::Meditate => "Meditar",
            Skill::Stab => "Apuñalar",
            Skill::Hide => "Ocultarse",
            Skill::Survival => "Supervivencia",
            Skill::Trade => "Comercio",
            Skill::ShieldDefense => "Defensa con escudo",
            Skill::Leadership => "Liderazgo",
            Skill::Projectiles => "Armas a distancia",
            Skill::Wrestling => "Combate sin armas",
            Skill::Sailing => "Navegación",
            Skill::Riding => "Equitación",
            Skill::MagicResistance => "Resistencia mágica",
            Skill::Woodcutting => "Tala",
            Skill::Fishing => "Pesca",
            Skill::Mining => "Minería",
            Skill::Blacksmithing => "Herrería",
            Skill::Carpentry => "Carpintería",
            Skill::Alchemy => "Alquimia",
            Skill::Tailoring => "Sastrería",
            Skill::Taming => "Domar",
            Skill::TargetableItem | Skill::Smelting => return None,
        })
    }

    pub fn from_name(name: &str) -> Option<Skill> {
        let t = tilde(name);
        Skill::ALL.into_iter().find(|s| s.spanish_name().is_some_and(|n| tilde(n) == t))
    }
}

/// `e_WeaponType` (1-based).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WeaponType {
    Sword = 1,
    Dagger = 2,
    Bow = 3,
    Staff = 4,
    Mace = 5,
    ThrowableAxe = 6,
    Axe = 7,
    Knuckle = 8,
    Fist = 9,
    Spear = 10,
    GunPowder = 11,
}

impl WeaponType {
    pub const ALL: [WeaponType; NUM_WEAPON_TYPES] = [
        WeaponType::Sword,
        WeaponType::Dagger,
        WeaponType::Bow,
        WeaponType::Staff,
        WeaponType::Mace,
        WeaponType::ThrowableAxe,
        WeaponType::Axe,
        WeaponType::Knuckle,
        WeaponType::Fist,
        WeaponType::Spear,
        WeaponType::GunPowder,
    ];

    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: i32) -> Option<WeaponType> {
        WeaponType::ALL.get(usize::try_from(id).ok()?.checked_sub(1)?).copied()
    }

    /// `WeaponTypeNames`: the key used under the class sections of `Balance.dat`.
    pub fn name(self) -> &'static str {
        match self {
            WeaponType::Sword => "Sword",
            WeaponType::Dagger => "Dagger",
            WeaponType::Bow => "Bow",
            WeaponType::Staff => "Staff",
            WeaponType::Mace => "Mace",
            WeaponType::ThrowableAxe => "ThrowableAxe",
            WeaponType::Axe => "Axe",
            WeaponType::Knuckle => "Knuckle",
            WeaponType::Fist => "Fist",
            WeaponType::Spear => "Spear",
            WeaponType::GunPowder => "GunPowder",
        }
    }
}

/// `ModClase`: combat and growth modifiers of one class.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClassMods {
    /// `MODEVASION`.
    pub evasion: f64,
    /// `MODATAQUEARMAS`: melee attack skill multiplier.
    pub melee_attack: f64,
    /// `MODATAQUEPROYECTILES`.
    pub ranged_attack: f64,
    /// `MODDANOARMAS`.
    pub melee_damage: f64,
    /// `MODDANOPROYECTILES`.
    pub ranged_damage: f64,
    /// `MODDANOWRESTLING`.
    pub wrestling_damage: f64,
    /// `MODESCUDO`.
    pub shield: f64,
    /// `MODAPUNALAR` (default 1 when missing).
    pub stab: f64,
    /// `MODAPUNALARNPCMIN` (default 1).
    pub stab_npc_min: f64,
    /// `MODAPUNALARNPCMAX` (default 1).
    pub stab_npc_max: f64,
    /// `MODVIDA`: average hit points per level.
    pub life: f64,
    /// `MANA_INICIAL`.
    pub initial_mana: f64,
    /// `MULT_MANA`.
    pub mana_mult: f64,
    /// `AUMENTO_STA`: stamina gained per level.
    pub stamina_gain: i32,
    /// `GOLPE_PRE_36`: hit bonus before level 36.
    pub hit_pre36: i32,
    /// `GOLPE_POST_36`.
    pub hit_post36: i32,
    /// `MODRESISTENCIAMAGICA`.
    pub magic_resistance: f64,
    /// `MODSKILLPOINTS`: free skill points per level.
    pub skill_points_per_level: i32,
    /// Under `[<CLASS>]`, key = [`WeaponType::name`]; index by `WeaponType::id()` (slot 0
    /// is unused).
    pub weapon_hit_bonus: [i32; NUM_WEAPON_TYPES + 1],
}

/// `ModRaza`: attribute modifiers of one race (`[MODRAZA]`, e.g. `ElfoOscuroFuerza=+2`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RaceMods {
    pub strength: i32,
    pub agility: i32,
    pub intelligence: i32,
    pub charisma: i32,
    pub constitution: i32,
}

/// `[BACKSTAB]`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Backstab {
    pub critical_hit_dmg_modifier: f32,
    pub ignore_armor_chance: f32,
    pub extra_backstab_chance: f32,
    pub assassin_stabbing_chance: f32,
    pub hunter_stabbing_chance: f32,
    pub bard_stabbing_chance: f32,
    pub generic_stabbing_chance: f32,
    pub bandit_critical_hit_chance: f32,
}

/// `[EXTRA]`: miscellaneous gameplay tuning. Times are in milliseconds unless noted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extra {
    pub mana_recovery_percent: i32,
    pub recovery_mana: i32,
    pub mana_per_skill_multiplier: f64,
    pub mana_common_lute: i32,
    pub mana_magic_lute: i32,
    pub mana_elven_lute: i32,
    pub skill_up_difficulty: i32,
    pub life_average_influence: f32,
    pub life_average_imbalance: f32,
    pub life_range: f32,
    pub life_cap_max: f32,
    pub life_cap_min: f32,
    pub required_spell_display_time: i32,
    pub max_invisible_spell_display_time: i32,
    pub multi_shot_reduction: f32,
    /// `/hogar` cooldown in seconds, by player rank.
    pub home_timer: i32,
    pub home_timer_adventurer: i32,
    pub home_timer_hero: i32,
    pub home_timer_legend: i32,
    pub magic_skill_bonus_damage_modifier: f32,
    pub magic_resistance_skill_protection_modifier: f32,
    pub magic_resistance_skill_protection_modifier_npc: f32,
    pub assist_damage_valid_time: i32,
    pub assist_help_valid_time: i32,
    pub hide_after_hit_time: i32,
    pub faction_rekill_time: i32,
    pub air_hit_reduce_paralysis_time: i32,
    pub safe_fishing_percent: i32,
    pub divine_blood_healing_multiplier_bonus: f32,
    pub divine_blood_mana_cost_multiplier: f32,
    pub warrior_life_steal_on_hit_multiplier: f32,
}

/// `[STUN]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stun {
    pub player_stun_time: i32,
    pub npc_stun_time: i32,
    pub player_immune_time: i32,
}

/// `[HOME_COST_TIERn]`: `/hogar` price for a level range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HomeCostTier {
    pub min_level: i32,
    pub max_level: i32,
    pub floor_gold: i32,
    pub cap_gold: i32,
}

#[derive(Clone, Debug)]
pub struct Balance {
    classes: [ClassMods; NUM_CLASSES],
    races: [RaceMods; NUM_RACES],
    pub backstab: Backstab,
    pub extra: Extra,
    pub stun: Stun,
    /// `[GUILD] GuildCallCooldown`. The shipped file keeps it under `[EXTRA]`; when
    /// `[GUILD]` is missing the `[EXTRA]` value is used (with a warning).
    pub guild_call_cooldown: i32,
    /// `[EXP]`: experience needed at level `n` is `exp_level_up[n - 1]`.
    pub exp_level_up: Vec<i32>,
    pub home_cost_tiers: [HomeCostTier; HOME_COST_TIERS],
    /// `[ElementalMatrixForNpcs]`: `elemental_matrix[row][column]`.
    pub elemental_matrix: [[f32; ELEMENT_TAGS]; ELEMENT_TAGS],
}

impl Balance {
    pub fn class(&self, class: Class) -> &ClassMods {
        &self.classes[class as usize - 1]
    }

    pub fn race(&self, race: Race) -> &RaceMods {
        &self.races[race as usize - 1]
    }

    /// Experience needed to go from `level` to the next one (`None` outside the table).
    pub fn exp_for_level(&self, level: u32) -> Option<i32> {
        self.exp_level_up.get((level as usize).checked_sub(1)?).copied()
    }

    /// Highest level (`STAT_MAXELV`): one entry per level of `[EXP]`, the last one is the
    /// cap (AO20 47, TSAO 70).
    pub fn max_level(&self) -> u32 {
        self.exp_level_up.len().max(1) as u32
    }
}

/// Reads `Balance.dat`.
pub fn load(recursos: &Path) -> Result<Loaded<Balance>, super::Error> {
    let mut warnings: Vec<Warning> = Vec::new();
    let ini = RawIni::load(recursos, FILE, &mut warnings)?;
    let data = parse(&ini, &mut warnings);
    Ok(Loaded { data, warnings })
}

fn parse(ini: &RawIni, w: &mut Vec<Warning>) -> Balance {
    let mut classes: [ClassMods; NUM_CLASSES] = Default::default();
    for class in Class::ALL {
        let key = class.balance_key();
        let m = &mut classes[class as usize - 1];
        let mut get = |section: &str, name: &str, default: f64| -> f64 { ini.sec(FILE, section, w).num_or(name, default) };
        m.evasion = get("MODEVASION", &key, 0.0);
        m.melee_attack = get("MODATAQUEARMAS", &key, 0.0);
        m.ranged_attack = get("MODATAQUEPROYECTILES", &key, 0.0);
        m.melee_damage = get("MODDANOARMAS", &key, 0.0);
        m.ranged_damage = get("MODDANOPROYECTILES", &key, 0.0);
        m.wrestling_damage = get("MODDANOWRESTLING", &key, 0.0);
        m.shield = get("MODESCUDO", &key, 0.0);
        m.stab = get("MODAPUNALAR", &key, 1.0);
        m.stab_npc_min = get("MODAPUNALARNPCMIN", &key, 1.0);
        m.stab_npc_max = get("MODAPUNALARNPCMAX", &key, 1.0);
        m.life = get("MODVIDA", &key, 0.0);
        m.initial_mana = get("MANA_INICIAL", &key, 0.0);
        m.mana_mult = get("MULT_MANA", &key, 0.0);
        m.stamina_gain = get("AUMENTO_STA", &key, 0.0).round() as i32;
        m.hit_pre36 = get("GOLPE_PRE_36", &key, 0.0).round() as i32;
        m.hit_post36 = get("GOLPE_POST_36", &key, 0.0).round() as i32;
        m.magic_resistance = get("MODRESISTENCIAMAGICA", &key, 0.0);
        m.skill_points_per_level = get("MODSKILLPOINTS", &key, 0.0).round() as i32;
        for wt in WeaponType::ALL {
            m.weapon_hit_bonus[wt as usize] = get(&key, wt.name(), 0.0).round() as i32;
        }
    }

    let mut races: [RaceMods; NUM_RACES] = Default::default();
    {
        let mut s = ini.sec(FILE, "MODRAZA", w);
        for race in Race::ALL {
            let k = race.balance_key();
            let r = &mut races[race as usize - 1];
            r.strength = s.int(&format!("{k}Fuerza"));
            r.agility = s.int(&format!("{k}Agilidad"));
            r.intelligence = s.int(&format!("{k}Inteligencia"));
            r.charisma = s.int(&format!("{k}Carisma"));
            r.constitution = s.int(&format!("{k}Constitucion"));
        }
    }

    let backstab = {
        let mut s = ini.sec(FILE, "BACKSTAB", w);
        Backstab {
            critical_hit_dmg_modifier: s.float("CriticalHitDmgModifier"),
            ignore_armor_chance: s.float("IgnoreArmorChance"),
            extra_backstab_chance: s.float("ExtraBackstabChance"),
            assassin_stabbing_chance: s.float("AssasinStabbingChance"),
            hunter_stabbing_chance: s.float("HunterStabbingChance"),
            bard_stabbing_chance: s.float("BardStabbingChance"),
            generic_stabbing_chance: s.float("GenericStabbingChance"),
            bandit_critical_hit_chance: s.float("BanditCriticalHitChance"),
        }
    };

    let extra = {
        let mut s = ini.sec(FILE, "EXTRA", w);
        Extra {
            mana_recovery_percent: s.int("PorcentajeRecuperoMana"),
            recovery_mana: s.int("RecoveryMana"),
            mana_per_skill_multiplier: s.num("MultiplierManaxSkills"),
            mana_common_lute: s.int("ManaCommonLute"),
            mana_magic_lute: s.int("ManaMagicLute"),
            mana_elven_lute: s.int("ManaElvenLute"),
            skill_up_difficulty: s.int("DificultadSubirSkill"),
            life_average_influence: s.float("InfluenciaPromedioVidas"),
            life_average_imbalance: s.float("DesbalancePromedioVidas"),
            life_range: s.float("RangoVidas"),
            life_cap_max: s.float("CapVidaMax"),
            life_cap_min: s.float("CapVidaMin"),
            required_spell_display_time: s.int("RequiredSpellDisplayTime"),
            max_invisible_spell_display_time: s.int("MaxInvisibleSpellDisplayTime"),
            multi_shot_reduction: s.float("MultiShotReduction"),
            home_timer: s.int("HomeTimer"),
            home_timer_adventurer: s.int("HomeTimerAdventurer"),
            home_timer_hero: s.int("HomeTimerHero"),
            home_timer_legend: s.int("HomeTimerLegend"),
            magic_skill_bonus_damage_modifier: s.float("MagicSkillBonusDamageModifier"),
            magic_resistance_skill_protection_modifier: s.float("MagicResistanceSkillProtectionModifier"),
            magic_resistance_skill_protection_modifier_npc: s.float("MagicResistanceSkillProtectionModifierNpc"),
            assist_damage_valid_time: s.int("AssistDamageValidTime"),
            assist_help_valid_time: s.int("AssistHelpValidTime"),
            hide_after_hit_time: s.int("HideAfterHitTime"),
            faction_rekill_time: s.int("FactionReKillTime"),
            air_hit_reduce_paralysis_time: s.int("AirHitReductParalisisTime"),
            safe_fishing_percent: s.int("PorcentajePescaSegura"),
            divine_blood_healing_multiplier_bonus: s.float("DivineBloodHealingMultiplierBonus"),
            divine_blood_mana_cost_multiplier: s.float("DivineBloodManaCostMultiplier"),
            warrior_life_steal_on_hit_multiplier: s.float("WarriorLifeStealOnHitMultiplier"),
        }
    };

    let stun = {
        let mut s = ini.sec(FILE, "STUN", w);
        Stun { player_stun_time: s.int("PlayerStunTime"), npc_stun_time: s.int("NpcStunTime"), player_immune_time: s.int("PlayerInmuneTime") }
    };

    let guild_call_cooldown = if ini.has("GUILD") {
        ini.sec(FILE, "GUILD", w).int("GuildCallCooldown")
    } else {
        let mut s = ini.sec(FILE, "EXTRA", w);
        if s.raw("GuildCallCooldown").is_some() {
            s.warning("GuildCallCooldown", s.raw("GuildCallCooldown").unwrap_or(""), "the server reads it from [GUILD]; the file keeps it under [EXTRA] (used as fallback)");
        }
        s.int("GuildCallCooldown")
    };

    // `[EXP]` 1..n: the table decides the level cap (AO20 47; TSAO 70). Read until the
    // first missing or empty level (AO20 leaves `32=` and up empty... and they were read
    // as 0, see `max_level`).
    let mut exp_level_up = Vec::new();
    {
        let mut s = ini.sec(FILE, "EXP", w);
        for i in 1..=MAX_LEVEL_LIMIT {
            if s.raw(&i.to_string()).is_none() {
                break;
            }
            exp_level_up.push(s.int(&i.to_string()));
        }
        if exp_level_up.len() < MAX_LEVEL {
            exp_level_up.resize(MAX_LEVEL, 0);
        }
    }

    let mut home_cost_tiers: [HomeCostTier; HOME_COST_TIERS] = Default::default();
    for (i, tier) in home_cost_tiers.iter_mut().enumerate() {
        let name = format!("HOME_COST_TIER{}", i + 1);
        let mut s = ini.sec(FILE, &name, w);
        *tier = HomeCostTier {
            min_level: s.int("MinLevel"),
            max_level: s.int("MaxLevel"),
            floor_gold: s.int("FloorGLD"),
            cap_gold: s.int("CapGLD"),
        };
    }

    let mut elemental_matrix = [[1.0f32; ELEMENT_TAGS]; ELEMENT_TAGS];
    {
        let mut s = ini.sec(FILE, "ElementalMatrixForNpcs", w);
        for (i, row) in elemental_matrix.iter_mut().enumerate() {
            let key = format!("Row{}", i + 1);
            let Some(raw) = s.raw(&key) else {
                s.warning(&key, "", "missing matrix row (read as 1.0)");
                continue;
            };
            let parts: Vec<&str> = raw.split_whitespace().collect();
            if parts.len() < ELEMENT_TAGS {
                s.warning(&key, raw, format!("row has {} values, expected {ELEMENT_TAGS}", parts.len()));
            }
            for (cell, part) in row.iter_mut().zip(parts) {
                match part.replace(',', ".").parse::<f32>() {
                    Ok(v) => *cell = v,
                    Err(_) => s.warning(&key, raw, format!("value {part:?} is not a number (read as 1.0)")),
                }
            }
        }
    }

    Balance { classes, races, backstab, extra, stun, guild_call_cooldown, exp_level_up, home_cost_tiers, elemental_matrix }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::recursos_dir;

    #[test]
    fn id_enums_round_trip() {
        for c in Class::ALL {
            assert_eq!(Class::from_id(c.id() as i32), Some(c));
            assert_eq!(Class::from_name(c.spanish_name()), Some(c));
        }
        for r in Race::ALL {
            assert_eq!(Race::from_id(r.id() as i32), Some(r));
            assert_eq!(Race::from_name(r.spanish_name()), Some(r));
        }
        assert_eq!(Class::from_name("LADRON"), Some(Class::Thief));
        assert_eq!(Class::from_name("clerigo"), Some(Class::Cleric));
        assert_eq!(Race::from_name("DROW"), Some(Race::Drow));
        assert_eq!(Race::from_name("ELFO OSCURO"), Some(Race::Drow));
        assert_eq!(Skill::from_name("Combate con armas"), Some(Skill::Weapons));
        assert_eq!(Class::from_id(0), None);
        assert_eq!(Class::from_id(13), None);
        assert_eq!(Race::from_id(-1), None);
    }

    #[test]
    fn official_balance_loads() {
        let root = recursos_dir();
        if !root.join("Dat").join(FILE).is_file() {
            eprintln!("AO resources not found, skipping");
            return;
        }
        let Loaded { data: b, warnings } = load(&root).expect("Balance.dat");

        // [MODRAZA]: ElfoOscuro (Drow) comes before Enano/Gnomo in the file but its id is 3.
        assert_eq!(*b.race(Race::Human), RaceMods { strength: 1, agility: 1, intelligence: 0, charisma: 0, constitution: 2 });
        assert_eq!(*b.race(Race::Drow), RaceMods { strength: 2, agility: 1, intelligence: 1, charisma: -1, constitution: 1 });
        assert_eq!(*b.race(Race::Gnome), RaceMods { strength: -2, agility: 3, intelligence: 4, charisma: 2, constitution: 0 });
        assert_eq!(*b.race(Race::Dwarf), RaceMods { strength: 3, agility: 0, intelligence: -3, charisma: -1, constitution: 3 });
        assert_eq!(*b.race(Race::Orc), RaceMods { strength: 3, agility: 1, intelligence: -2, charisma: -1, constitution: 2 });

        // Class tables: Guerrero evasion 0.9 / life 10.5; Mago life 7.5 / mana 8.33; Asesino stab 1.25.
        let warrior = b.class(Class::Warrior);
        assert_eq!(warrior.evasion, 0.9);
        assert_eq!(warrior.life, 10.5);
        assert_eq!(warrior.melee_attack, 1.1);
        assert_eq!(warrior.skill_points_per_level, 5);
        assert_eq!(warrior.hit_pre36, 3);
        assert_eq!(warrior.hit_post36, 2);
        let mage = b.class(Class::Mage);
        assert_eq!(mage.life, 7.5);
        assert_eq!(mage.initial_mana, 8.33);
        assert_eq!(mage.evasion, 0.15);
        assert_eq!(mage.hit_pre36, 1);
        assert_eq!(b.class(Class::Assassin).stab, 1.25);
        assert_eq!(b.class(Class::Assassin).stab_npc_max, 2.0);
        assert_eq!(b.class(Class::Worker).skill_points_per_level, 6);

        // Experience table: 47 levels.
        assert_eq!(b.exp_for_level(1), Some(500));
        assert_eq!(b.exp_for_level(2), Some(750));
        assert_eq!(b.exp_for_level(14), Some(99904));
        // The last level (47) needs no experience: it is the cap.
        assert!(b.exp_level_up[..MAX_LEVEL - 1].iter().all(|&e| e > 0), "every level below the cap has experience");
        assert_eq!(b.exp_for_level(47), Some(0));
        assert_eq!(b.exp_for_level(46), Some(396_727_662));
        assert_eq!(b.exp_for_level(48), None);

        assert_eq!(b.backstab.assassin_stabbing_chance, 0.25);
        assert_eq!(b.backstab.critical_hit_dmg_modifier, 0.40);
        assert_eq!(b.stun, Stun { player_stun_time: 1200, npc_stun_time: 10000, player_immune_time: 1000 });
        assert_eq!(b.extra.home_timer, 150);
        assert_eq!(b.extra.mana_recovery_percent, 7);
        assert_eq!(b.extra.multi_shot_reduction, 0.7);
        assert_eq!(b.home_cost_tiers[0], HomeCostTier { min_level: 1, max_level: 12, floor_gold: 10, cap_gold: 100 });
        assert_eq!(b.home_cost_tiers[5].max_level, MAX_LEVEL as i32);
        assert_eq!(b.elemental_matrix[0], [0.5, 0.5, 2.0, 1.0]);
        assert_eq!(b.elemental_matrix[3], [1.0, 2.0, 0.5, 0.5]);
        assert_eq!(b.guild_call_cooldown, 3000);
        // The only expected oddity: GuildCallCooldown lives under [EXTRA].
        assert!(warnings.iter().all(|w| w.key == "GuildCallCooldown"), "unexpected warnings: {warnings:#?}");
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(load(Path::new("definitely/not/here")).is_err());
    }
}

//! `Hechizos.dat`: every spell (`[HECHIZO123]`).
//!
//! Mirrors `CargarHechizos` (`FileIO.bas`), whose result is used by `modHechizos.bas`.
//! Field comments give the original key. Display strings and the icon (`HechizeroMsg`,
//! `PropioMsg`, `TargetMsg`, `IconoIndex`) are not read by the server but the client needs
//! them, so they are included.
use super::balance::WeaponType;
use super::{Loaded, RawIni, Sec, Table, Warning};
use std::path::Path;

pub const FILE: &str = "Hechizos.dat";

/// `e_TipoHechizo`: what the spell acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpellType {
    /// 1 `uPropiedades`: HP, mana, stamina, hunger, thirst and attributes.
    Properties,
    /// 2 `uEstado`: status effects (paralysis, invisibility, curse...).
    Status,
    /// 3 `uMaterializa`: creates an object.
    Materialize,
    /// 4 `uInvocacion`: summons creatures.
    Summon,
    /// 5 `uArea`.
    Area,
    /// 6 `uPortal`.
    Portal,
    /// 8 `uCombinados`.
    Combined,
    /// 9 `uMultiShoot`.
    MultiShoot,
    /// 10 `uPhysicalSkill`.
    PhysicalSkill,
    /// Any other id (0 when `Tipo` is missing).
    Unknown(i32),
}

impl SpellType {
    pub fn from_id(id: i32) -> SpellType {
        match id {
            1 => SpellType::Properties,
            2 => SpellType::Status,
            3 => SpellType::Materialize,
            4 => SpellType::Summon,
            5 => SpellType::Area,
            6 => SpellType::Portal,
            8 => SpellType::Combined,
            9 => SpellType::MultiShoot,
            10 => SpellType::PhysicalSkill,
            n => SpellType::Unknown(n),
        }
    }

    pub fn id(self) -> i32 {
        match self {
            SpellType::Properties => 1,
            SpellType::Status => 2,
            SpellType::Materialize => 3,
            SpellType::Summon => 4,
            SpellType::Area => 5,
            SpellType::Portal => 6,
            SpellType::Combined => 8,
            SpellType::MultiShoot => 9,
            SpellType::PhysicalSkill => 10,
            SpellType::Unknown(n) => n,
        }
    }
}

/// `e_TargetType`: who the spell can be cast on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpellTarget {
    /// 1 `uUsuarios`.
    Users,
    /// 2 `uNPC`.
    Npcs,
    /// 3 `uUsuariosYnpc`.
    UsersAndNpcs,
    /// 4 `uTerreno`: a map tile.
    Terrain,
    /// 5 `uPets`.
    Pets,
    Unknown(i32),
}

impl SpellTarget {
    pub fn from_id(id: i32) -> SpellTarget {
        match id {
            1 => SpellTarget::Users,
            2 => SpellTarget::Npcs,
            3 => SpellTarget::UsersAndNpcs,
            4 => SpellTarget::Terrain,
            5 => SpellTarget::Pets,
            n => SpellTarget::Unknown(n),
        }
    }

    pub fn id(self) -> i32 {
        match self {
            SpellTarget::Users => 1,
            SpellTarget::Npcs => 2,
            SpellTarget::UsersAndNpcs => 3,
            SpellTarget::Terrain => 4,
            SpellTarget::Pets => 5,
            SpellTarget::Unknown(n) => n,
        }
    }
}

/// `e_TargetEffectType`: is the spell good or bad for its target (decides who may cast it
/// on whom).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetEffect {
    Positive,
    Negative,
    /// 0 when `TargetEffectType` is missing, or any other id.
    Unknown(i32),
}

impl TargetEffect {
    pub fn from_id(id: i32) -> TargetEffect {
        match id {
            1 => TargetEffect::Positive,
            2 => TargetEffect::Negative,
            n => TargetEffect::Unknown(n),
        }
    }
}

impl Default for SpellType {
    fn default() -> Self {
        SpellType::Unknown(0)
    }
}

impl Default for SpellTarget {
    fn default() -> Self {
        SpellTarget::Unknown(0)
    }
}

impl Default for TargetEffect {
    fn default() -> Self {
        TargetEffect::Unknown(0)
    }
}

/// `e_SpellEffects` bit mask (the server packs the boolean keys of the spell into it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SpellEffects(pub u32);

impl SpellEffects {
    pub const INVISIBILITY: u32 = 1;
    pub const PARALYZE: u32 = 2;
    pub const IMMOBILIZE: u32 = 4;
    pub const REMOVE_PARALYSIS: u32 = 8;
    pub const REMOVE_DUMB: u32 = 16;
    pub const CURE_POISON: u32 = 32;
    pub const INCINERATE: u32 = 64;
    pub const CURSE: u32 = 128;
    pub const REMOVE_CURSE: u32 = 256;
    pub const PRECISE_HIT: u32 = 512;
    /// `SubeHP=1`.
    pub const DO_HEAL: u32 = 1024;
    pub const DUMB: u32 = 2048;
    pub const BLINDNESS: u32 = 4096;
    pub const RESURRECT: u32 = 8192;
    /// Declared by the server but never set by the loader.
    pub const MORPH: u32 = 16384;
    pub const REMOVE_INVISIBILITY: u32 = 32768;
    pub const TOGGLE_CLEAVE: u32 = 65536;
    pub const REMOVE_DEBUFF: u32 = 131072;
    pub const STEAL_BUFF: u32 = 262144;
    /// `SubeHP=2`.
    pub const DO_DAMAGE: u32 = 524288;
    pub const ADJUST_STATS_WITH_CASTER: u32 = 1048576;
    pub const TOGGLE_DIVINE_BLOOD: u32 = 2097152;
    pub const CANCEL_ACTIVE_EFFECT: u32 = 4194304;

    pub fn contains(self, flag: u32) -> bool {
        self.0 & flag == flag
    }
}

/// `e_SpellRequirementMask` bit mask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SpellRequirements(pub u32);

impl SpellRequirements {
    pub const WEAPON: u32 = 1;
    pub const SHIELD: u32 = 2;
    pub const ARMOR: u32 = 4;
    pub const HELM: u32 = 8;
    pub const MAGIC_ITEM: u32 = 16;
    pub const PROJECTILE: u32 = 32;
    pub const SHIP: u32 = 64;
    /// Declared by the server but never set by the loader.
    pub const TOOL: u32 = 128;
    pub const KNUCKLE: u32 = 256;
    pub const TARGET_ON_LAND: u32 = 512;
    pub const TARGET_ON_WATER: u32 = 1024;
    pub const WORKS_ON_DEAD: u32 = 2048;
    pub const IS_SKILL: u32 = 4096;
    pub const IS_BINDABLE: u32 = 8192;

    pub fn contains(self, flag: u32) -> bool {
        self.0 & flag == flag
    }
}

/// One stat a spell modifies (`Sube*`, `Min*`, `Max*`). `mode` is the raw `Sube*` value
/// (the server: 1 adds, 2 subtracts), `min`/`max` the rolled range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatChange {
    pub mode: i32,
    pub min: i32,
    pub max: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spell {
    // ---- texts (client) ----
    /// `Nombre`.
    pub name: String,
    /// `Desc`.
    pub description: String,
    /// `PalabrasMagicas`: words shown when casting.
    pub magic_words: String,
    /// `HechizeroMsg` (client): message to the caster.
    pub caster_msg: String,
    /// `PropioMsg` (client): message when cast on oneself.
    pub own_msg: String,
    /// `TargetMsg` (client): message to the target.
    pub target_msg: String,
    /// `IconoIndex` (client): spell icon graphic.
    pub icon: i32,

    // ---- classification ----
    /// `Tipo`.
    pub spell_type: SpellType,
    /// `Target`.
    pub target: SpellTarget,
    /// `TargetEffectType`.
    pub target_effect: TargetEffect,
    /// `SkillType` (`e_SkillType`: 1 push arrow, 2 cannon).
    pub skill_type: i32,
    /// Boolean keys (`Paraliza`, `Invisibilidad`, `Incinera`, `Revivir`, `Maldicion`...)
    /// packed as the server does.
    pub effects: SpellEffects,
    /// `RequireWeapon`, `RequireShield`, `IsBindable`, `WorkOnDead`... packed as the
    /// server does.
    pub requirements: SpellRequirements,

    // ---- casting cost and limits ----
    /// `ManaRequerido`.
    pub mana_required: i32,
    /// `StaRequerido`.
    pub stamina_required: i32,
    /// `StaPercentRequired`.
    pub stamina_percent_required: i32,
    /// `RequiredHP`.
    pub required_hp: i32,
    /// `MinSkill`: Magic skill needed.
    pub min_skill: i32,
    /// `MagicPowerNeeded`: staff power needed.
    pub magic_power_needed: i32,
    /// `MaxLevelCasteable`.
    pub max_level_castable: i32,
    /// `RequireTransform`.
    pub require_transform: i32,
    /// TSAO `ExclusivoClase` / `ExclusivoClase2`: only these classes may learn and cast it
    /// (empty = everyone). Unknown names are dropped.
    pub exclusive_classes: Vec<super::balance::Class>,
    /// TSAO `StaffAffected`: its damage depends on the caster's staff (mages, druids).
    pub staff_affected: bool,
    /// `RequireWeaponType`.
    pub require_weapon_type: Option<WeaponType>,
    /// `IsElementalTagsOnly`.
    pub elemental_tags_only: bool,
    /// `NecesitaObj`: object consumed/required.
    pub needs_obj: i32,
    /// `NecesitaObj2`.
    pub needs_obj2: i32,
    /// `CoolDown`.
    pub cooldown: i32,
    /// `CdEffectId`.
    pub cd_effect_id: i32,
    /// `AutoLanzar`.
    pub auto_cast: i32,

    // ---- effects on stats ----
    /// `SubeHP` (1 heal, 2 damage; also set in `effects`), `MinHP`, `MaxHP`.
    pub hp: StatChange,
    /// `SubeMana`, `MinMana`, `MaxMana`.
    pub mana: StatChange,
    /// `SubeSta`, `MinSta`, `MaxSta`.
    pub stamina: StatChange,
    /// `SubeHam`, `MinHam`, `MaxHam`.
    pub hunger: StatChange,
    /// `SubeSed`, `MinSed`, `MaxSed`.
    pub thirst: StatChange,
    /// `SubeAG`, `MinAG`, `MaxAG`.
    pub agility: StatChange,
    /// `SubeFU`, `MinFU`, `MaxFU`.
    pub strength: StatChange,
    /// `SubeCA`, `MinCA`, `MaxCA`.
    pub charisma: StatChange,
    /// `Duration`: duration of timed effects.
    pub duration: i32,
    /// `EOTID`: effect-over-time id (`EffectsOverTime.dat`).
    pub eot_id: i32,
    /// `Envenena`: poison strength.
    pub poison: i32,
    /// `desencantar`.
    pub dispel: i32,
    /// `Sanacion`.
    pub sanation: i32,
    /// `AntiRm`.
    pub anti_magic_resistance: i32,
    /// `Mimetiza`.
    pub mimic: i32,
    /// `velocidad`: projectile speed.
    pub speed: f32,

    // ---- summon / materialize / teleport ----
    /// `Invoca`.
    pub summons: i32,
    /// `NumNpc`: NPC summoned.
    pub npc_number: i32,
    /// `Cant`: how many.
    pub summon_count: i32,
    /// `MaterializaObj`.
    pub materialize_obj: i32,
    /// `MaterializaCant`.
    pub materialize_amount: i32,
    /// `Teleport`.
    pub teleport: i32,
    /// `TeleportMap`.
    pub teleport_map: i32,
    /// `TeleportX`.
    pub teleport_x: i32,
    /// `TeleportY`.
    pub teleport_y: i32,
    /// `AreaRadio`.
    pub area_radius: i32,
    /// `AreaAfecta` (`e_AreaAfecta`: 1 users, 2 NPCs, 3 both).
    pub area_affects: i32,

    // ---- presentation ----
    /// `WAV`: cast sound.
    pub wav: i32,
    /// `Fxgrh`: FX graphic on the target.
    pub fx: i32,
    /// `Loops`.
    pub loops: i32,
    /// `Particle`.
    pub particle: i32,
    /// `ParticleViaje`.
    pub travel_particle: i32,
    /// `TimeParticula`.
    pub particle_time: i32,
    /// `ScreenColor`.
    pub screen_color: i32,
    /// `TimeEfect`.
    pub screen_effect_time: i32,
}

/// Reads `Hechizos.dat`.
pub fn load(recursos: &Path) -> Result<Loaded<Table<Spell>>, super::Error> {
    let mut warnings = Vec::new();
    let ini = RawIni::load(recursos, FILE, &mut warnings)?;
    Ok(finish(&ini, warnings))
}

/// Parses the text of a `Hechizos.dat` (already decoded).
pub fn parse_str(text: &str) -> Loaded<Table<Spell>> {
    let mut warnings = Vec::new();
    let ini = RawIni::parse(text, FILE, &mut warnings);
    finish(&ini, warnings)
}

fn finish(ini: &RawIni, mut w: Vec<Warning>) -> Loaded<Table<Spell>> {
    let declared = ini.sec(FILE, "INIT", &mut w).int("NumeroHechizos").max(0) as u32;
    let mut table = Table { declared, ..Table::default() };
    for (id, name) in ini.numbered("Hechizo", FILE, &mut w) {
        if id > declared {
            w.push(Warning {
                file: FILE,
                section: name.clone(),
                key: String::new(),
                value: String::new(),
                message: format!("id above NumeroHechizos={declared}: the official server never loads it"),
            });
        }
        let mut sec = ini.sec(FILE, &name, &mut w);
        let spell = read_spell(&mut sec);
        table.insert(id, spell);
    }
    Loaded { data: table, warnings: w }
}

fn stat(s: &mut Sec, sube: &str, min: &str, max: &str) -> StatChange {
    StatChange { mode: s.int(sube), min: s.int(min), max: s.int(max) }
}

fn read_spell(s: &mut Sec) -> Spell {
    fn set(on: bool, flag: u32, mask: &mut u32) {
        if on {
            *mask |= flag;
        }
    }
    let mut p = Spell::default();
    let mut effects = 0u32;
    let mut req = 0u32;

    p.speed = s.float("Velocidad");
    p.materialize_obj = s.int("MaterializaObj");
    p.materialize_amount = s.int("MaterializaCant");
    p.screen_color = s.int("ScreenColor");
    p.screen_effect_time = s.int("TimeEfect");
    p.teleport = s.int("Teleport");
    p.teleport_map = s.int("TeleportMap");
    p.teleport_x = s.int("TeleportX");
    p.teleport_y = s.int("TeleportY");
    p.name = s.text("Nombre");
    p.description = s.text("Desc");
    p.magic_words = s.text("PalabrasMagicas");
    p.caster_msg = s.text("HechizeroMsg");
    p.own_msg = s.text("PropioMsg");
    p.target_msg = s.text("TargetMsg");
    p.icon = s.int("IconoIndex");
    p.needs_obj = s.int("NecesitaObj");
    p.needs_obj2 = s.int("NecesitaObj2");
    p.spell_type = SpellType::from_id(s.int("Tipo"));
    p.skill_type = s.int("SkillType");
    p.wav = s.int("WAV");
    p.fx = s.int("Fxgrh");
    p.particle = s.int("Particle");
    p.travel_particle = s.int("ParticleViaje");
    p.particle_time = s.int("TimeParticula");
    p.dispel = s.int("desencantar");
    p.sanation = s.int("Sanacion");
    p.anti_magic_resistance = s.int("AntiRm");
    p.area_radius = s.int("AreaRadio");
    p.area_affects = s.int("AreaAfecta");
    set(s.flag("Incinera"), SpellEffects::INCINERATE, &mut effects);
    set(s.flag("RemoveDebuff"), SpellEffects::REMOVE_DEBUFF, &mut effects);
    set(s.flag("StealBuff"), SpellEffects::STEAL_BUFF, &mut effects);
    p.auto_cast = s.int("AutoLanzar");
    p.target_effect = TargetEffect::from_id(s.int("TargetEffectType"));
    p.cooldown = s.int("CoolDown");
    p.cd_effect_id = s.int("CdEffectId");
    p.loops = s.int("Loops");
    p.hp = stat(s, "SubeHP", "MinHP", "MaxHP");
    p.mana = stat(s, "SubeMana", "MinMana", "MaxMana");
    p.stamina = stat(s, "SubeSta", "MinSta", "MaxSta");
    p.hunger = stat(s, "SubeHam", "MinHam", "MaxHam");
    p.thirst = stat(s, "SubeSed", "MinSed", "MaxSed");
    p.agility = stat(s, "SubeAG", "MinAG", "MaxAG");
    p.strength = stat(s, "SubeFU", "MinFU", "MaxFU");
    p.charisma = stat(s, "SubeCA", "MinCA", "MaxCA");
    set(s.flag("Invisibilidad"), SpellEffects::INVISIBILITY, &mut effects);
    set(s.flag("Paraliza"), SpellEffects::PARALYZE, &mut effects);
    set(s.flag("Inmoviliza"), SpellEffects::IMMOBILIZE, &mut effects);
    set(s.flag("RemoverParalisis"), SpellEffects::REMOVE_PARALYSIS, &mut effects);
    set(s.flag("RemoverEstupidez"), SpellEffects::REMOVE_DUMB, &mut effects);
    set(s.flag("RemueveInvisibilidadParcial"), SpellEffects::REMOVE_INVISIBILITY, &mut effects);
    set(s.flag("CuraVeneno"), SpellEffects::CURE_POISON, &mut effects);
    p.poison = s.int("Envenena");
    set(s.flag("Maldicion"), SpellEffects::CURSE, &mut effects);
    set(s.flag("RemoverMaldicion"), SpellEffects::REMOVE_CURSE, &mut effects);
    set(s.flag("Revivir"), SpellEffects::RESURRECT, &mut effects);
    set(s.flag("Ceguera"), SpellEffects::BLINDNESS, &mut effects);
    set(s.flag("Estupidez"), SpellEffects::DUMB, &mut effects);
    set(s.flag("ToggleCleave"), SpellEffects::TOGGLE_CLEAVE, &mut effects);
    set(s.flag("ToggleDivineBlood"), SpellEffects::TOGGLE_DIVINE_BLOOD, &mut effects);
    set(s.flag("AdjustStatsWithCaster"), SpellEffects::ADJUST_STATS_WITH_CASTER, &mut effects);
    set(s.flag("CancelActiveEffect"), SpellEffects::CANCEL_ACTIVE_EFFECT, &mut effects);
    p.summons = s.int("Invoca");
    p.npc_number = s.int("NumNpc");
    p.summon_count = s.int("Cant");
    p.mimic = s.int("Mimetiza");
    set(s.flag("GolpeCertero"), SpellEffects::PRECISE_HIT, &mut effects);
    p.min_skill = s.int("MinSkill");
    p.mana_required = s.int("ManaRequerido");
    p.required_hp = s.int("RequiredHP");
    p.duration = s.int("Duration");
    p.stamina_required = s.int("StaRequerido");
    p.stamina_percent_required = s.int("StaPercentRequired");
    p.target = SpellTarget::from_id(s.int("Target"));
    p.require_transform = s.int("RequireTransform");
    p.magic_power_needed = s.int("MagicPowerNeeded");
    // TSAO staves: `NeedStaff` is the same staff power requirement.
    if p.magic_power_needed == 0 {
        p.magic_power_needed = s.int("NeedStaff");
    }
    p.staff_affected = s.int("StaffAffected") != 0;
    p.exclusive_classes = ["ExclusivoClase", "ExclusivoClase2"].iter().filter_map(|k| super::balance::Class::from_name(&s.text(k))).collect();
    p.eot_id = s.int("EOTID");
    p.max_level_castable = s.int("MaxLevelCasteable");
    set(s.flag("RequireArmor"), SpellRequirements::ARMOR, &mut req);
    set(s.flag("RequireShip"), SpellRequirements::SHIP, &mut req);
    set(s.flag("RequireHelm"), SpellRequirements::HELM, &mut req);
    set(s.flag("RequireKnucle"), SpellRequirements::KNUCKLE, &mut req);
    set(s.flag("RequireMagicItem"), SpellRequirements::MAGIC_ITEM, &mut req);
    set(s.flag("RequireProjectile"), SpellRequirements::PROJECTILE, &mut req);
    set(s.flag("RequireShield"), SpellRequirements::SHIELD, &mut req);
    set(s.flag("RequireWeapon"), SpellRequirements::WEAPON, &mut req);
    set(s.flag("RequireTargetOnLand"), SpellRequirements::TARGET_ON_LAND, &mut req);
    set(s.flag("RequireTargetOnWater"), SpellRequirements::TARGET_ON_WATER, &mut req);
    set(s.flag("WorkOnDead"), SpellRequirements::WORKS_ON_DEAD, &mut req);
    set(s.flag("IsSkill"), SpellRequirements::IS_SKILL, &mut req);
    set(s.flag("IsBindable"), SpellRequirements::IS_BINDABLE, &mut req);
    let wt = s.int("RequireWeaponType");
    p.require_weapon_type = WeaponType::from_id(wt);
    if wt != 0 && p.require_weapon_type.is_none() {
        s.warning("RequireWeaponType", &wt.to_string(), "unknown weapon type id (ignored)");
    }
    p.elemental_tags_only = s.flag("IsElementalTagsOnly");
    match p.hp.mode {
        1 => effects |= SpellEffects::DO_HEAL,
        2 => effects |= SpellEffects::DO_DAMAGE,
        _ => {}
    }
    p.effects = SpellEffects(effects);
    p.requirements = SpellRequirements(req);
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::recursos_dir;

    #[test]
    fn synthetic_spell() {
        let text = "\
[INIT]
NumeroHechizos=10
[HECHIZO1]
Nombre=Dardo
PalabrasMagicas=IrX' ÔA'l
Tipo=1
Target=3
SubeHP=2
MinHP=5
MaxHP=xx
Paraliza=1
RequireShield=1
[HECHIZO99]
Nombre=Lejano
";
        let Loaded { data: t, warnings } = parse_str(text);
        let s = t.get(1).unwrap();
        assert_eq!(s.magic_words, "IrX' ÔA'l");
        assert_eq!(s.spell_type, SpellType::Properties);
        assert_eq!(s.target, SpellTarget::UsersAndNpcs);
        assert_eq!((s.hp.mode, s.hp.min, s.hp.max), (2, 5, 0));
        assert!(s.effects.contains(SpellEffects::PARALYZE | SpellEffects::DO_DAMAGE));
        assert!(!s.effects.contains(SpellEffects::DO_HEAL));
        assert!(s.requirements.contains(SpellRequirements::SHIELD));
        assert!(warnings.iter().any(|w| w.key == "MaxHP"));
        assert!(warnings.iter().any(|w| w.section == "HECHIZO99"));
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn official_spells_load() {
        let root = recursos_dir();
        if !root.join("Dat").join(FILE).is_file() {
            eprintln!("AO resources not found, skipping");
            return;
        }
        let Loaded { data: t, warnings } = load(&root).expect("Hechizos.dat");
        assert_eq!(t.len(), 213, "spells defined in Hechizos.dat");
        assert_eq!(t.declared, 352, "NumeroHechizos in [INIT]");

        // [HECHIZO1] Dardo Mágico: 5-10 damage, 10 mana, 5 stamina, target users+NPCs.
        let dart = t.get(1).unwrap();
        assert_eq!(dart.name, "Dardo Mágico");
        assert_eq!(dart.magic_words, "OHL VOR PEK");
        assert_eq!(dart.spell_type, SpellType::Properties);
        assert_eq!(dart.target, SpellTarget::UsersAndNpcs);
        assert_eq!(dart.target_effect, TargetEffect::Negative);
        assert_eq!((dart.hp.min, dart.hp.max, dart.hp.mode), (5, 10, 2));
        assert_eq!((dart.mana_required, dart.stamina_required), (10, 5));
        assert_eq!((dart.wav, dart.fx, dart.travel_particle, dart.icon), (16, 15, 186, 74714));
        assert!(dart.effects.contains(SpellEffects::DO_DAMAGE));
        assert!(dart.requirements.contains(SpellRequirements::IS_BINDABLE));

        // [HECHIZO12] Curar Heridas Leves: heals 5-20 for 5 mana; [HECHIZO14] Resucitar.
        let heal = t.get(12).unwrap();
        assert_eq!(heal.name, "Curar Heridas Leves");
        assert_eq!((heal.hp.min, heal.hp.max, heal.mana_required, heal.target), (5, 20, 5, SpellTarget::UsersAndNpcs));
        assert!(heal.effects.contains(SpellEffects::DO_HEAL) && !heal.effects.contains(SpellEffects::DO_DAMAGE));
        let revive = t.get(14).unwrap();
        assert_eq!((revive.spell_type, revive.mana_required, revive.target), (SpellType::Status, 400, SpellTarget::Users));
        assert!(revive.effects.contains(SpellEffects::RESURRECT));
        // The only comments inside values in the shipped file (HECHIZO15 area keys).
        let detect = t.get(15).unwrap();
        assert_eq!((detect.spell_type, detect.mana_required, detect.target), (SpellType::Unknown(12), 2000, SpellTarget::Terrain));
        assert_eq!((detect.area_radius, detect.area_affects), (22, 1));

        // Class of spells exist: paralysis, summon, resurrect.
        assert!(t.iter().any(|(_, s)| s.effects.contains(SpellEffects::PARALYZE)));
        assert!(t.iter().any(|(_, s)| s.effects.contains(SpellEffects::RESURRECT)));
        assert!(t.iter().any(|(_, s)| s.spell_type == SpellType::Summon && s.npc_number > 0 && s.summon_count > 0));
        assert!(t.iter().any(|(_, s)| s.effects.contains(SpellEffects::INVISIBILITY)));
        assert!(warnings.iter().all(|w| w.file == FILE));
    }
}

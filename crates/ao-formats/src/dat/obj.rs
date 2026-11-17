//! `obj.dat`: every item, weapon, armor, door, key, potion, chest... of the game.
//!
//! Mirrors `LoadOBJData` (`FileIO.bas`) field by field, including which keys the official
//! server only reads for certain object types (weapon damage for weapons, destination map
//! for passage tickets...). Field comments give the original key name. Keys the server does
//! not read but the file carries (`Peso`, `Info`, `ExtraStr`) are included as extras.
use super::balance::{Class, Race, Skill, WeaponType, NUM_CLASSES, NUM_RACES};
use super::{Loaded, RawIni, Sec, Table, Warning};
use std::path::Path;

pub const FILE: &str = "obj.dat";
/// `MAX_SLOTS_CRAFTEO`: ingredients in a crafting recipe.
pub const MAX_CRAFT_SLOTS: usize = 5;

/// `e_OBJType` (`Declares.bas`). Ids 12, 15 and 25 are free in the server enum but a few
/// objects still carry them (and 49, 90), so unlisted ids are kept as [`ObjType::Unknown`]
/// exactly like the server, which gives them no type-specific behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjType {
    UseOnce,
    Weapon,
    Armor,
    Trees,
    GoldCoin,
    Doors,
    Backpack,
    SignBoards,
    Keys,
    CastleSpawner,
    Potions,
    Drinks,
    Wood,
    Shield,
    Helmet,
    WorkingTools,
    Teleport,
    Decorations,
    Amulets,
    OreDeposit,
    Minerals,
    Parchment,
    MusicalInstruments,
    Anvil,
    Forge,
    BlacksmithMaterial,
    MagicalInstrument,
    Ships,
    Arrows,
    EmptyBottle,
    FullBottle,
    RingAccessory,
    PassageTicket,
    Map,
    SkinsWings,
    SkinsArmours,
    SkinsShields,
    SkinsHelmets,
    SkinsWeapons,
    SkinsBoats,
    Saddles,
    RecallStones,
    SkinsSpells,
    Mail,
    Chest,
    Donator,
    Quest,
    FishingPool,
    UsableOnTarget,
    Plants,
    ElementalRune,
    FactionForgiveness,
    CollectibleCard,
    /// `otElse` (100).
    Else,
    /// An id the server enum does not name (including 0 for a missing `ObjType`).
    Unknown(i32),
}

impl ObjType {
    pub fn from_id(id: i32) -> ObjType {
        use ObjType::*;
        match id {
            1 => UseOnce,
            2 => Weapon,
            3 => Armor,
            4 => Trees,
            5 => GoldCoin,
            6 => Doors,
            7 => Backpack,
            8 => SignBoards,
            9 => Keys,
            10 => CastleSpawner,
            11 => Potions,
            13 => Drinks,
            14 => Wood,
            16 => Shield,
            17 => Helmet,
            18 => WorkingTools,
            19 => Teleport,
            20 => Decorations,
            21 => Amulets,
            22 => OreDeposit,
            23 => Minerals,
            24 => Parchment,
            26 => MusicalInstruments,
            27 => Anvil,
            28 => Forge,
            29 => BlacksmithMaterial,
            30 => MagicalInstrument,
            31 => Ships,
            32 => Arrows,
            33 => EmptyBottle,
            34 => FullBottle,
            35 => RingAccessory,
            36 => PassageTicket,
            37 => Map,
            38 => SkinsWings,
            39 => SkinsArmours,
            40 => SkinsShields,
            41 => SkinsHelmets,
            42 => SkinsWeapons,
            43 => SkinsBoats,
            44 => Saddles,
            45 => RecallStones,
            46 => SkinsSpells,
            47 => Mail,
            48 => Chest,
            50 => Donator,
            51 => Quest,
            52 => FishingPool,
            53 => UsableOnTarget,
            54 => Plants,
            55 => ElementalRune,
            56 => FactionForgiveness,
            57 => CollectibleCard,
            100 => Else,
            other => Unknown(other),
        }
    }

    /// The numeric id as written in `obj.dat`.
    pub fn id(self) -> i32 {
        use ObjType::*;
        match self {
            UseOnce => 1,
            Weapon => 2,
            Armor => 3,
            Trees => 4,
            GoldCoin => 5,
            Doors => 6,
            Backpack => 7,
            SignBoards => 8,
            Keys => 9,
            CastleSpawner => 10,
            Potions => 11,
            Drinks => 13,
            Wood => 14,
            Shield => 16,
            Helmet => 17,
            WorkingTools => 18,
            Teleport => 19,
            Decorations => 20,
            Amulets => 21,
            OreDeposit => 22,
            Minerals => 23,
            Parchment => 24,
            MusicalInstruments => 26,
            Anvil => 27,
            Forge => 28,
            BlacksmithMaterial => 29,
            MagicalInstrument => 30,
            Ships => 31,
            Arrows => 32,
            EmptyBottle => 33,
            FullBottle => 34,
            RingAccessory => 35,
            PassageTicket => 36,
            Map => 37,
            SkinsWings => 38,
            SkinsArmours => 39,
            SkinsShields => 40,
            SkinsHelmets => 41,
            SkinsWeapons => 42,
            SkinsBoats => 43,
            Saddles => 44,
            RecallStones => 45,
            SkinsSpells => 46,
            Mail => 47,
            Chest => 48,
            Donator => 50,
            Quest => 51,
            FishingPool => 52,
            UsableOnTarget => 53,
            Plants => 54,
            ElementalRune => 55,
            FactionForgiveness => 56,
            CollectibleCard => 57,
            Else => 100,
            Unknown(n) => n,
        }
    }
}

impl Default for ObjType {
    fn default() -> Self {
        ObjType::Unknown(0)
    }
}

/// An object stack: chest contents (`ItemN` / `CantidadN`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjStack {
    /// Object id.
    pub obj: i32,
    pub amount: i32,
    /// Chest subtype 3 only: `101 - DropN`, the server's drop roll threshold.
    pub drop_roll: i32,
}

/// A gem an ore deposit can yield (`GemaN=obj-oneInX`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gem {
    pub obj: i32,
    /// Drops with probability 1 in `one_in`.
    pub one_in: i32,
}

/// `SkillRequerido=Skill name-level`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillRequirement {
    pub skill: Skill,
    pub level: i32,
}

/// Crafting recipe of an object (`Crafteable=1`, `Materiales=a-b-c-d-e`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CraftRecipe {
    /// `Materiales`, sorted ascending like the server does; object ids above the declared
    /// object count are replaced by 0.
    pub ingredients: Vec<i32>,
    /// `TipoCrafteo`.
    pub kind: i32,
    /// `ProbCrafteo`, clamped to 0..=100.
    pub chance: i32,
    /// `CostoCrafteo`.
    pub price: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjData {
    // ---- identity ----
    /// `Name`.
    pub name: String,
    /// `Texto`: description / sign text / door text.
    pub text: String,
    /// `GrhIndex`: inventory graphic.
    pub grh: i32,
    /// `ObjType`.
    pub obj_type: ObjType,
    /// `Subtipo`.
    pub subtype: i32,
    /// `Valor`: base price.
    pub value: i32,
    /// `Peso` (not read by the official server).
    pub weight: f32,
    /// `Info` (not read by the server; client/wiki text).
    pub info: String,
    /// `ExtraStr` (not read by the server).
    pub extra_text: String,
    /// `VGrande`: large graphic (`GrhSecundario`).
    pub grh_large: i32,
    /// `ID`: forum / external id.
    pub forum_id: String,

    // ---- behaviour flags ----
    /// `Log`.
    pub log: i32,
    /// `NoLog`.
    pub no_log: i32,
    /// `Newbie`: newbie-only item (not dropped on death).
    pub newbie: i32,
    /// `Instransferible`.
    pub non_transferable: i32,
    /// `Destruye`.
    pub destroys: i32,
    /// `Intirable`: cannot be thrown to the floor.
    pub non_droppable: i32,
    /// `Crucial`.
    pub crucial: i32,
    /// `Agarrable`: can be picked up from the floor.
    pub grabbable: i32,
    /// `NoSeCae`: not lost on death.
    pub never_falls: i32,
    /// `NoSeLimpia`: not cleaned from the floor.
    pub never_cleaned: i32,
    /// `Subastable`.
    pub auctionable: i32,
    /// `NFT` (donor shop item, `ObjDonador`).
    pub shop_item: bool,
    /// `Bindable` flag.
    pub bindable: bool,
    /// `UseOnSafeAreaOnly` flag.
    pub use_on_safe_area_only: bool,
    /// `JailObject` flag.
    pub jail_object: bool,
    /// `NoTradeableToNPC` flag.
    pub no_tradeable_to_npc: bool,

    // ---- requirements ----
    /// `MinELV`.
    pub min_level: i32,
    /// `MaxLEV`.
    pub max_level: i32,
    /// `MinSkill`.
    pub min_skill: i32,
    /// `SkillRequerido`.
    pub skill_requirement: Option<SkillRequirement>,
    /// `CP1..CP12`: classes that cannot use it.
    pub forbidden_classes: Vec<Class>,
    /// `RP1..RP6`: races that cannot use it.
    pub forbidden_races: Vec<Race>,
    /// `Hombre`: male only.
    pub male_only: bool,
    /// `Mujer`: female only.
    pub female_only: bool,
    /// `ClaseTipo`.
    pub class_type: i32,
    /// `RazaHumana`, `RazaElfa`, `RazaDrow`, `RazaGnoma`, `RazaEnana`, `RazaOrca`: per-race
    /// flags, indexed by `Race::id() - 1`.
    pub race_flags: [i32; NUM_RACES],
    /// `RequiereObjeto` (armor, shield, helmet, backpack, weapon, ship, parchment).
    pub required_object: i32,
    /// `Real` (royal army only: armor, shield, helmet, weapon, instrument, saddle).
    pub royal: i32,
    /// `Caos` (chaos legion only).
    pub chaos: i32,
    /// `LeadersOnly`.
    pub leaders_only: bool,
    /// `JineteLevel`.
    pub rider_level: i32,
    /// `Jerarquia`.
    pub rank: i32,
    /// `FactionScore`.
    pub faction_score: i32,
    /// `CantidadSkill`.
    pub skill_amount: i32,
    /// `QueSkill`: skill raised on use (see [`Skill::from_id`]).
    pub skill_id: i32,
    /// `queatributo`.
    pub attribute_id: i32,
    /// `cuantoaumento`.
    pub attribute_increase: i32,

    // ---- weapons, armor, shields, helmets ----
    /// `MinHIT` (weapon, ship, saddle, arrow, usable-on-target).
    pub min_hit: i32,
    /// `MaxHIT`.
    pub max_hit: i32,
    /// `MinHitToNPC` (weapon, arrow).
    pub min_hit_npc: i32,
    /// `MaxHitToNPC`.
    pub max_hit_npc: i32,
    /// `MINDEF`.
    pub min_def: i32,
    /// `MAXDEF`.
    pub max_def: i32,
    /// `Anim` of a weapon or working tool.
    pub weapon_anim: i32,
    /// `Anim` of a shield.
    pub shield_anim: i32,
    /// `Anim` of a helmet.
    pub helmet_anim: i32,
    /// `NumRopaje`: body graphic of armor (any race).
    pub clothing: i32,
    /// `RopajeHumano`, `RopajeElfo`, `RopajeElfoOscuro`, `RopajeEnano`, `RopajeGnomo`,
    /// `RopajeOrco`: male body per race, indexed by `Race::id() - 1`.
    pub clothing_male: [i32; NUM_RACES],
    /// `RopajeHumana`, `RopajeElfa`, `RopajeElfaOscura`, `RopajeEnana`, `RopajeGnoma`,
    /// `RopajeOrca`: female body per race.
    pub clothing_female: [i32; NUM_RACES],
    /// `RazaAltos`: body for tall races.
    pub clothing_tall: i32,
    /// `RazaBajos`: body for short races.
    pub clothing_short: i32,
    /// `ResistenciaMagica`.
    pub magic_resistance: i32,
    /// `velocidad` (armor: 0 reads as 1; ship, saddle).
    pub speed: f32,
    /// `Invernal` (armor).
    pub winter: bool,
    /// `Camouflage` (armor).
    pub camouflage: bool,
    /// `Porcentaje` (shield, weapon, potion).
    pub percentage: i32,
    /// `WeaponType` (see [`WeaponType::from_id`]).
    pub weapon_type: Option<WeaponType>,
    /// `DosManos`.
    pub two_handed: i32,
    /// `Proyectil`: ranged weapon / throwable.
    pub projectile: i32,
    /// `Municiones`.
    pub ammo: i32,
    /// `ProjectileType`.
    pub projectile_type: i32,
    /// `BowCategory`.
    pub bow_category: i32,
    /// `ArrowCategory`.
    pub arrow_category: i32,
    /// `Apuñala`: stabs.
    pub stabs: i32,
    /// `Envenena`: poisons.
    pub poisons: i32,
    /// `Paraliza`.
    pub paralyzes: i32,
    /// `Estupidiza`.
    pub dumbs: i32,
    /// `Incinera`.
    pub burns: i32,
    /// `MinArmorPenetrationFlat`.
    pub min_armor_penetration_flat: i32,
    /// `MaxArmorPenetrationFlat`.
    pub max_armor_penetration_flat: i32,
    /// `ArmorPenetrationPercent`.
    pub armor_penetration_percent: i32,
    /// `ExtraCritAndStabChance`.
    pub extra_crit_and_stab_chance: i32,
    /// `ImprovedRHit`.
    pub improved_ranged_hit_chance: i32,
    /// `ImprovedMHit`.
    pub improved_melee_hit_chance: i32,
    /// `RepairTo`.
    pub repair_to: i32,
    /// `VidaUtil`: durability.
    pub durability: i32,
    /// `TiempoRegenerar`.
    pub regen_time: i32,

    // ---- magic and effects ----
    /// `MagicPower` (weapon, magical instrument, ring) or `Power` (working tool).
    pub power: i32,
    /// `EfectoMagico`.
    pub magic_effect: i32,
    /// `Revive` (weapon, amulet, magical instrument).
    pub revive: bool,
    /// `MagicDamageBonus`.
    pub magic_damage_bonus: i32,
    /// `MagicAbsoluteBonus`.
    pub magic_absolute_bonus: i32,
    /// `MagicPenetration`.
    pub magic_penetration: i32,
    /// `HechizoIndex`.
    pub spell_index: i32,
    /// `Hechizo` (potion, elemental rune).
    pub spell: i32,
    /// `ElementalTags`.
    pub elemental_tags: i32,
    /// `ApplyEffectId`.
    pub apply_effect_id: i32,
    /// `CD`: cooldown (recall stones: `Cooldown`).
    pub cooldown: i32,
    /// `CDType`.
    pub cooldown_type: i32,

    // ---- consumables ----
    /// `MinHam`: hunger restored.
    pub min_hunger: i32,
    /// `MinAgu`: thirst restored (`MinSed`).
    pub min_thirst: i32,
    /// `MinST`: stamina restored.
    pub min_stamina: i32,
    /// `MinHP`.
    pub min_hp: i32,
    /// `MaxHP`.
    pub max_hp: i32,
    /// `TipoPocion`.
    pub potion_type: i32,
    /// `MinModificador`.
    pub min_modifier: i32,
    /// `MaxModificador`.
    pub max_modifier: i32,
    /// `DuracionEfecto`.
    pub effect_duration: i32,

    // ---- doors, keys ----
    /// `Cerrada`.
    pub locked: i32,
    /// `Llave` (only read when `Cerrada=1`).
    pub key_obj: i32,
    /// `Clave`.
    pub key_code: i32,
    /// `IndexAbierta` (doors, bottles).
    pub open_index: i32,
    /// `IndexCerrada`.
    pub closed_index: i32,
    /// `IndexCerradaLlave`.
    pub locked_index: i32,

    // ---- travel ----
    /// `DesdeMap` (passage ticket, recall stone).
    pub from_map: i32,
    /// `Map` (passage ticket, donator) or `HastaMap` (recall stone).
    pub to_map: i32,
    /// `X` or `HastaX`.
    pub to_x: i32,
    /// `Y` or `HastaY`.
    pub to_y: i32,
    /// `NecesitaNave`.
    pub needs_ship: i32,
    /// `Runetype`.
    pub rune_type: i32,
    /// `Radio` (teleport).
    pub radius: i32,
    /// `AssignedCastleIndex`.
    pub castle_index: i32,
    /// `QuestID`.
    pub quest_id: i32,
    /// `Peces` (amulet with `EfectoMagico=12`): `MaxItems`.
    pub max_items: i32,

    // ---- chests, deposits, cards ----
    /// `CantItem`.
    pub item_count: i32,
    /// `CantEntrega` (chest subtype 2).
    pub deliver_count: i32,
    /// `Item1..n` / `Cantidad1..n` (chest subtypes 1-3).
    pub contents: Vec<ObjStack>,
    /// `MineralIndex`.
    pub mineral_index: i32,
    /// `Gemas` / `Gema1..n` (ore deposit).
    pub gems: Vec<Gem>,
    /// `LingoteIndex` (mineral).
    pub ingot_index: i32,
    /// `CollectibleCardIndex`.
    pub card_index: i32,
    /// `CollectibleCardRarity`.
    pub card_rarity: i32,

    // ---- materials and crafting ----
    /// `Dorada`.
    pub dorada: i32,
    /// `Blodium`.
    pub blodium: i32,
    /// `FireEssence`.
    pub fire_essence: i32,
    /// `WaterEssence`.
    pub water_essence: i32,
    /// `EarthEssence`.
    pub earth_essence: i32,
    /// `WindEssence`.
    pub wind_essence: i32,
    /// `LingH`: iron ingots.
    pub iron_ingots: i32,
    /// `LingP`: silver ingots.
    pub silver_ingots: i32,
    /// `LingO`: gold ingots.
    pub gold_ingots: i32,
    /// `Coal`.
    pub coal: i32,
    /// `SkHerreria`: blacksmithing skill needed.
    pub blacksmith_skill: i32,
    /// `SkCarpinteria`.
    pub carpentry_skill: i32,
    /// `SKSastreria`.
    pub tailoring_skill: i32,
    /// `Madera`.
    pub wood: i32,
    /// `MaderaElfica`.
    pub elven_wood: i32,
    /// `Maderapino`.
    pub pine_wood: i32,
    /// `Elfico`.
    pub elven: i32,
    /// `Pino`.
    pub pine: i32,
    /// `PielLobo`.
    pub pelt_wolf: i32,
    /// `PielOsoPardo`.
    pub pelt_brown_bear: i32,
    /// `PielOsoPolaR`.
    pub pelt_polar_bear: i32,
    /// `PielLoboNegro`.
    pub pelt_black_wolf: i32,
    /// `PielTigre`.
    pub pelt_tiger: i32,
    /// `PielTigreBengala`.
    pub pelt_bengal_tiger: i32,
    /// `PuntosPesca`.
    pub fishing_points: i32,
    /// Alchemy ingredients of potions: `Mortero`.
    pub mortar: i32,
    /// `FrascoAlq`.
    pub alchemy_flask: i32,
    /// `HongoDeLuz`.
    pub light_mushroom: i32,
    /// `Esporas`.
    pub spores: i32,
    /// `Tuna`.
    pub prickly_pear: i32,
    /// `Cala`.
    pub calla_lily: i32,
    /// `ColaDeZorro`.
    pub foxtail: i32,
    /// `FlorOceano`.
    pub ocean_flower: i32,
    /// `FlorRoja`.
    pub red_flower: i32,
    /// `SemillasProsperas`.
    pub prosperous_seeds: i32,
    /// `SkPociones`.
    pub potion_skill: i32,
    /// `Crafteable` + `Materiales` / `TipoCrafteo` / `ProbCrafteo` / `CostoCrafteo`.
    pub recipe: Option<CraftRecipe>,
    /// `CatalizadorTipo`.
    pub catalyst_type: i32,
    /// `CatalizadorAumento` (only read when the type is set).
    pub catalyst_increase: i32,

    // ---- effects on use / world ----
    /// `SND1`.
    pub snd1: i32,
    /// `SND2`.
    pub snd2: i32,
    /// `SND3`.
    pub snd3: i32,
    /// `SndAura`.
    pub snd_aura: i32,
    /// `ParticulaGolpe`.
    pub hit_particle: i32,
    /// `ParticulaViaje`.
    pub travel_particle: i32,
    /// `ParticulaGolpeTime`.
    pub hit_particle_time: i32,
    /// `CreaParticula` (kept as text: the server stores a string).
    pub creates_particle: String,
    /// `CreaFX`.
    pub creates_fx: i32,
    /// `CreaGRH` (text).
    pub creates_grh: String,
    /// `CreaLuz` (text).
    pub creates_light: String,
    /// `CreaWav`.
    pub creates_wav: i32,
}

impl ObjData {
    /// Body graphic of an armor for a race and gender (`clothing_male` / `clothing_female`).
    pub fn clothing_for(&self, race: Race, female: bool) -> i32 {
        let i = race as usize - 1;
        if female {
            self.clothing_female[i]
        } else {
            self.clothing_male[i]
        }
    }

    /// Average defense (`def = (MinDef + MaxDef) / 2`).
    pub fn average_def(&self) -> f32 {
        (self.min_def + self.max_def) as f32 / 2.0
    }
}

/// Reads `obj.dat`.
pub fn load(recursos: &Path) -> Result<Loaded<Table<ObjData>>, super::Error> {
    let mut warnings = Vec::new();
    let ini = RawIni::load(recursos, FILE, &mut warnings)?;
    Ok(finish(&ini, warnings))
}

/// Parses the text of an `obj.dat` (already decoded).
pub fn parse_str(text: &str) -> Loaded<Table<ObjData>> {
    let mut warnings = Vec::new();
    let ini = RawIni::parse(text, FILE, &mut warnings);
    finish(&ini, warnings)
}

fn finish(ini: &RawIni, mut w: Vec<Warning>) -> Loaded<Table<ObjData>> {
    let declared = ini.sec(FILE, "INIT", &mut w).int("NumOBJs").max(0) as u32;
    let mut table = Table { declared, ..Table::default() };
    for (id, name) in ini.numbered("OBJ", FILE, &mut w) {
        if id > declared {
            w.push(Warning {
                file: FILE,
                section: name.clone(),
                key: String::new(),
                value: String::new(),
                message: format!("id above NumOBJs={declared}: the official server never loads it"),
            });
        }
        let mut sec = ini.sec(FILE, &name, &mut w);
        let obj = read_obj(&mut sec, declared);
        table.insert(id, obj);
    }
    Loaded { data: table, warnings: w }
}

// Field-by-field assignment keeps this function a line-for-line mirror of the VB loader.
#[allow(clippy::field_reassign_with_default)]
fn read_obj(s: &mut Sec, declared: u32) -> ObjData {
    let mut o = ObjData::default();
    o.name = s.text("Name");
    o.log = s.int("Log");
    o.no_log = s.int("NoLog");
    o.grh = s.int("GrhIndex");
    o.obj_type = ObjType::from_id(s.int("ObjType"));
    o.newbie = s.int("Newbie");
    o.non_transferable = s.int("Instransferible");
    o.destroys = s.int("Destruye");
    o.non_droppable = s.int("Intirable");
    o.skill_amount = s.int("CantidadSkill");
    o.skill_id = s.int("QueSkill");
    o.attribute_id = s.int("queatributo");
    o.attribute_increase = s.int("cuantoaumento");
    o.min_level = s.int("MinELV");
    o.max_level = s.int("MaxLEV");
    o.subtype = s.int("Subtipo");
    o.dorada = s.int("Dorada");
    o.blodium = s.int("Blodium");
    o.fire_essence = s.int("FireEssence");
    o.water_essence = s.int("WaterEssence");
    o.earth_essence = s.int("EarthEssence");
    o.wind_essence = s.int("WindEssence");
    o.durability = s.int("VidaUtil");
    o.regen_time = s.int("TiempoRegenerar");
    o.rank = s.int("Jerarquia");
    o.cooldown = s.int("CD");
    o.cooldown_type = s.int("CDType");
    o.improved_ranged_hit_chance = s.int("ImprovedRHit");
    o.improved_melee_hit_chance = s.int("ImprovedMHit");
    o.apply_effect_id = s.int("ApplyEffectId");
    o.rider_level = s.int("JineteLevel");
    o.faction_score = s.int("FactionScore");
    o.elemental_tags = s.int("ElementalTags");
    o.bow_category = s.int("BowCategory");
    o.arrow_category = s.int("ArrowCategory");
    o.repair_to = s.int("RepairTo");
    o.bindable = s.flag("Bindable");
    o.use_on_safe_area_only = s.flag("UseOnSafeAreaOnly");
    o.jail_object = s.flag("JailObject");
    o.no_tradeable_to_npc = s.flag("NoTradeableToNPC");

    // Keys the official server only reads for some object types.
    use ObjType::*;
    match o.obj_type {
        WorkingTools => {
            o.weapon_anim = s.int("Anim");
            o.power = s.int("Power");
        }
        Armor | SkinsArmours => {
            o.required_object = s.int("RequiereObjeto");
            o.royal = s.int("Real");
            o.chaos = s.int("Caos");
            o.leaders_only = s.nonzero("LeadersOnly");
            o.magic_resistance = s.int("ResistenciaMagica");
            o.winter = s.flag("Invernal");
            o.camouflage = s.flag("Camouflage");
            let speed = s.float("velocidad");
            o.speed = if speed == 0.0 { 1.0 } else { speed };
        }
        Shield | SkinsShields => {
            o.required_object = s.int("RequiereObjeto");
            o.shield_anim = s.int("Anim");
            o.royal = s.int("Real");
            o.chaos = s.int("Caos");
            o.leaders_only = s.nonzero("LeadersOnly");
            o.magic_resistance = s.int("ResistenciaMagica");
            o.percentage = s.int("Porcentaje");
        }
        Helmet | SkinsHelmets => {
            o.required_object = s.int("RequiereObjeto");
            o.helmet_anim = s.int("Anim");
            o.royal = s.int("Real");
            o.chaos = s.int("Caos");
            o.leaders_only = s.nonzero("LeadersOnly");
            o.magic_resistance = s.int("ResistenciaMagica");
        }
        Backpack | SkinsWings => {
            o.required_object = s.int("RequiereObjeto");
        }
        MagicalInstrument => {
            o.revive = s.nonzero("Revive");
            o.power = s.int("MagicPower");
        }
        Weapon | SkinsWeapons => {
            o.required_object = s.int("RequiereObjeto");
            o.weapon_anim = s.int("Anim");
            o.stabs = s.int("Apuñala");
            o.poisons = s.int("Envenena");
            o.paralyzes = s.int("Paraliza");
            o.dumbs = s.int("Estupidiza");
            o.burns = s.int("Incinera");
            o.max_hit = s.int("MaxHIT");
            o.min_hit = s.int("MinHIT");
            o.max_hit_npc = s.int("MaxHitToNPC");
            o.min_hit_npc = s.int("MinHitToNPC");
            o.min_armor_penetration_flat = s.int("MinArmorPenetrationFlat");
            o.max_armor_penetration_flat = s.int("MaxArmorPenetrationFlat");
            o.armor_penetration_percent = s.int("ArmorPenetrationPercent");
            o.extra_crit_and_stab_chance = s.int("ExtraCritAndStabChance");
            o.projectile = s.int("Proyectil");
            o.ammo = s.int("Municiones");
            o.power = s.int("MagicPower");
            o.royal = s.int("Real");
            o.chaos = s.int("Caos");
            o.leaders_only = s.nonzero("LeadersOnly");
            o.magic_effect = s.int("efectomagico");
            o.revive = s.nonzero("Revive");
            o.two_handed = s.int("DosManos");
            o.percentage = s.int("Porcentaje");
            let wt = s.int("WeaponType");
            o.weapon_type = WeaponType::from_id(wt);
            if wt != 0 && o.weapon_type.is_none() {
                s.warning("WeaponType", &wt.to_string(), "unknown weapon type id (ignored)");
            }
        }
        MusicalInstruments => {
            o.royal = s.int("Real");
            o.chaos = s.int("Caos");
            o.leaders_only = s.nonzero("LeadersOnly");
        }
        Doors | EmptyBottle | FullBottle => {
            o.open_index = s.int("IndexAbierta");
            o.closed_index = s.int("IndexCerrada");
            o.locked_index = s.int("IndexCerradaLlave");
        }
        Potions => {
            o.potion_type = s.int("TipoPocion");
            o.max_modifier = s.int("MaxModificador");
            o.min_modifier = s.int("MinModificador");
            o.effect_duration = s.int("DuracionEfecto");
            o.spell = s.int("Hechizo");
            o.mortar = s.int("Mortero");
            o.alchemy_flask = s.int("FrascoAlq");
            o.light_mushroom = s.int("HongoDeLuz");
            o.spores = s.int("Esporas");
            o.prickly_pear = s.int("Tuna");
            o.calla_lily = s.int("Cala");
            o.foxtail = s.int("ColaDeZorro");
            o.ocean_flower = s.int("FlorOceano");
            o.red_flower = s.int("FlorRoja");
            o.prosperous_seeds = s.int("SemillasProsperas");
            o.potion_skill = s.int("SkPociones");
            o.percentage = s.int("Porcentaje");
        }
        Ships | SkinsBoats => {
            o.required_object = s.int("RequiereObjeto");
            o.max_hit = s.int("MaxHIT");
            o.min_hit = s.int("MinHIT");
            o.speed = s.float("Velocidad");
        }
        Saddles => {
            o.max_hit = s.int("MaxHIT");
            o.min_hit = s.int("MinHIT");
            o.min_def = s.int("MINDEF");
            o.max_def = s.int("MAXDEF");
            o.royal = s.int("Real");
            o.chaos = s.int("Caos");
            o.leaders_only = s.nonzero("LeadersOnly");
            o.speed = s.float("Velocidad");
        }
        Arrows => {
            o.max_hit = s.int("MaxHIT");
            o.min_hit = s.int("MinHIT");
            o.max_hit_npc = s.int("MaxHitToNPC");
            o.min_hit_npc = s.int("MinHitToNPC");
            o.poisons = s.int("Envenena");
            o.paralyzes = s.int("Paraliza");
            o.dumbs = s.int("Estupidiza");
            o.burns = s.int("Incinera");
            o.magic_effect = s.int("efectomagico");
        }
        PassageTicket => {
            o.from_map = s.int("DesdeMap");
            o.to_map = s.int("Map");
            o.to_x = s.int("X");
            o.to_y = s.int("Y");
            o.needs_ship = s.int("NecesitaNave");
        }
        Donator => {
            o.to_map = s.int("Map");
            o.to_x = s.int("X");
            o.to_y = s.int("Y");
        }
        Quest => o.quest_id = s.int("QuestID"),
        Amulets => {
            o.magic_effect = s.int("efectomagico");
            o.revive = s.nonzero("Revive");
            if o.magic_effect == 12 {
                o.max_items = s.int("Peces");
            }
        }
        RecallStones => {
            o.rune_type = s.int("Runetype");
            o.from_map = s.int("DesdeMap");
            o.to_map = s.int("HastaMap");
            o.to_x = s.int("HastaX");
            o.to_y = s.int("HastaY");
            o.cooldown = s.int("Cooldown");
        }
        Teleport => o.radius = s.int("Radio"),
        CastleSpawner => o.castle_index = s.int("AssignedCastleIndex"),
        Chest => read_chest(s, &mut o),
        CollectibleCard => {
            o.card_index = s.int("CollectibleCardIndex");
            o.card_rarity = s.int("CollectibleCardRarity");
        }
        OreDeposit => {
            o.mineral_index = s.int("MineralIndex");
            let n = s.count("Gemas", 255);
            o.item_count = n as i32;
            for i in 1..=n {
                let key = format!("Gema{i}");
                let f = s.ints(&key, '-', 2);
                o.gems.push(Gem { obj: f[0], one_in: f[1] });
            }
        }
        UsableOnTarget => {
            o.max_hit = s.int("MaxHIT");
            o.min_hit = s.int("MinHIT");
            o.projectile = s.int("Proyectil");
        }
        RingAccessory => {
            o.magic_resistance = s.int("ResistenciaMagica");
            o.power = s.int("MagicPower");
        }
        Minerals => o.ingot_index = s.int("LingoteIndex"),
        ElementalRune => o.spell = s.int("Hechizo"),
        Parchment | SkinsSpells => o.required_object = s.int("RequiereObjeto"),
        _ => {}
    }

    o.magic_damage_bonus = s.int("MagicDamageBonus");
    o.magic_absolute_bonus = s.int("MagicAbsoluteBonus");
    o.magic_penetration = s.int("MagicPenetration");
    o.magic_effect = s.int("EfectoMagico");
    o.projectile_type = s.int("ProjectileType");
    o.min_skill = s.int("MinSkill");
    o.elven = s.int("Elfico");
    o.pine = s.int("Pino");
    o.snd1 = s.int("SND1");
    o.snd2 = s.int("SND2");
    o.snd3 = s.int("SND3");
    o.snd_aura = s.int("SndAura");
    o.never_cleaned = s.int("NoSeLimpia");
    o.auctionable = s.int("Subastable");
    o.hit_particle = s.int("ParticulaGolpe");
    o.travel_particle = s.int("ParticulaViaje");
    o.hit_particle_time = s.int("ParticulaGolpeTime");
    o.clothing = s.int("NumRopaje");
    o.clothing_male = [
        s.int("RopajeHumano"),
        s.int("RopajeElfo"),
        s.int("RopajeElfoOscuro"),
        s.int("RopajeGnomo"),
        s.int("RopajeEnano"),
        s.int("RopajeOrco"),
    ];
    o.clothing_female = [
        s.int("RopajeHumana"),
        s.int("RopajeElfa"),
        s.int("RopajeElfaOscura"),
        s.int("RopajeGnoma"),
        s.int("RopajeEnana"),
        s.int("RopajeOrca"),
    ];
    o.clothing_tall = s.int("RazaAltos");
    o.clothing_short = s.int("RazaBajos");
    o.spell_index = s.int("HechizoIndex");
    o.max_hp = s.int("MaxHP");
    o.min_hp = s.int("MinHP");
    o.female_only = s.nonzero("Mujer");
    o.male_only = s.nonzero("Hombre");
    o.pelt_wolf = s.int("PielLobo");
    o.pelt_brown_bear = s.int("PielOsoPardo");
    o.pelt_polar_bear = s.int("PielOsoPolaR");
    o.pelt_black_wolf = s.int("PielLoboNegro");
    o.pelt_tiger = s.int("PielTigre");
    o.pelt_bengal_tiger = s.int("PielTigreBengala");
    o.tailoring_skill = s.int("SKSastreria");
    o.iron_ingots = s.int("LingH");
    o.silver_ingots = s.int("LingP");
    o.gold_ingots = s.int("LingO");
    o.coal = s.int("Coal");
    o.blacksmith_skill = s.int("SkHerreria");
    o.creates_particle = s.text("CreaParticula");
    o.creates_fx = s.int("CreaFX");
    o.creates_grh = s.text("CreaGRH");
    o.creates_light = s.text("CreaLuz");
    o.creates_wav = s.int("CreaWav");
    o.min_hunger = s.int("MinHam");
    o.min_thirst = s.int("MinAgu");
    o.fishing_points = s.int("PuntosPesca");
    // Saddles read MINDEF/MAXDEF above; this is the general (later) read the VB does.
    o.min_def = s.int("MINDEF");
    o.max_def = s.int("MAXDEF");
    o.class_type = s.int("ClaseTipo");
    o.race_flags = [
        s.int("RazaHumana"),
        s.int("RazaElfa"),
        s.int("RazaDrow"),
        s.int("RazaGnoma"),
        s.int("RazaEnana"),
        s.int("RazaOrca"),
    ];
    o.value = s.int("Valor");
    o.crucial = s.int("Crucial");
    o.locked = s.int("Cerrada");
    if o.locked == 1 {
        o.key_obj = s.int("Llave");
    }
    o.key_code = s.int("Clave");
    o.text = s.text("Texto");
    o.grh_large = s.int("VGrande");
    o.grabbable = s.int("Agarrable");
    o.forum_id = s.text("ID");

    for i in 1..=NUM_CLASSES {
        let key = format!("CP{i}");
        let name = s.text(&key);
        if !name.is_empty() {
            match Class::from_name(&name) {
                Some(c) => o.forbidden_classes.push(c),
                None => s.warning(&key, &name, "unknown class name (ignored; the VB loader would fail here)"),
            }
        }
    }
    for i in 1..=NUM_RACES {
        let key = format!("RP{i}");
        let name = s.text(&key);
        if !name.is_empty() {
            match Race::from_name(&name) {
                Some(r) => o.forbidden_races.push(r),
                None => s.warning(&key, &name, "unknown race name (ignored)"),
            }
        }
    }

    let req = s.text("SkillRequerido");
    if !req.is_empty() {
        let f = s.fields("SkillRequerido", '-');
        match Skill::from_name(&f[0]) {
            Some(skill) => {
                let level = s.ints("SkillRequerido", '-', 2)[1];
                o.skill_requirement = Some(SkillRequirement { skill, level });
            }
            None => s.warning("SkillRequerido", &req, "unknown skill name (ignored)"),
        }
    }

    o.carpentry_skill = s.int("SkCarpinteria");
    o.wood = s.int("Madera");
    o.elven_wood = s.int("MaderaElfica");
    o.pine_wood = s.int("Maderapino");
    o.min_stamina = s.int("MinST");
    o.never_falls = s.int("NoSeCae");

    if s.int("Crafteable") == 1 {
        let raw = s.text("Materiales");
        if !raw.is_empty() {
            // VB `Split(str, "-", MAX_SLOTS_CRAFTEO)`: the last field keeps the remainder.
            let mut ingredients: Vec<i32> = Vec::new();
            for part in raw.splitn(MAX_CRAFT_SLOTS, '-') {
                let mut v = s.int_of("Materiales", &raw, part);
                if v < 0 || v as i64 > declared as i64 {
                    v = 0;
                }
                ingredients.push(v);
            }
            ingredients.sort_unstable();
            o.recipe = Some(CraftRecipe {
                ingredients,
                kind: s.int("TipoCrafteo"),
                chance: s.int("ProbCrafteo").clamp(0, 100),
                price: s.int("CostoCrafteo"),
            });
        }
    }

    o.catalyst_type = s.int("CatalizadorTipo");
    if o.catalyst_type != 0 {
        o.catalyst_increase = s.int("CatalizadorAumento");
    }
    o.shop_item = s.nonzero("NFT");

    // Not read by the official server.
    o.weight = s.float("Peso");
    o.info = s.text("Info");
    o.extra_text = s.text("ExtraStr");
    o
}

fn read_chest(s: &mut Sec, o: &mut ObjData) {
    let n = s.count("CantItem", 255);
    o.item_count = n as i32;
    // Like the server, only subtypes 1-3 have contents.
    if (1..=3).contains(&o.subtype) {
        if o.subtype == 2 {
            o.deliver_count = s.int("CantEntrega");
        }
        for i in 1..=n {
            let mut stack = ObjStack { obj: s.int(&format!("Item{i}")), amount: s.int(&format!("Cantidad{i}")), drop_roll: 0 };
            if o.subtype == 3 {
                stack.drop_roll = 101 - s.int(&format!("Drop{i}"));
            }
            o.contents.push(stack);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::recursos_dir;

    #[test]
    fn synthetic_entries() {
        let text = "\
[INIT]
NumOBJs=100
[OBJ1]
Name=Manzana Roja
ObjType=1
GrhIndex=506
Valor=2
MinHam=10
[OBJ2]
Name=Espada d'oro
ObjType=2
MinHIT=5
MaxHIT=+9
Apuñala=1
CP1=LADRON
CP2=clérigo
CP3=NoExiste
RP1=orco
WeaponType=7
Anim=33
[OBJ3]
Name=Armadura
ObjType=3
Velocidad=0
MINDEF=2
MAXDEF=bad
NumRopaje=88
Crafteable=1
Materiales=9-3-500-7
CostoCrafteo=50
ProbCrafteo=250
[OBJ300]
Name=Fuera
";
        let Loaded { data: t, warnings } = parse_str(text);
        assert_eq!(t.len(), 4);
        assert_eq!(t.declared, 100);
        let apple = t.get(1).unwrap();
        assert_eq!((apple.name.as_str(), apple.obj_type, apple.grh, apple.min_hunger), ("Manzana Roja", ObjType::UseOnce, 506, 10));
        let sword = t.get(2).unwrap();
        assert_eq!(sword.name, "Espada d'oro");
        assert_eq!((sword.min_hit, sword.max_hit, sword.stabs, sword.weapon_anim), (5, 9, 1, 33));
        assert_eq!(sword.weapon_type, Some(WeaponType::Axe));
        assert_eq!(sword.forbidden_classes, vec![Class::Thief, Class::Cleric]);
        assert_eq!(sword.forbidden_races, vec![Race::Orc]);
        let armor = t.get(3).unwrap();
        assert_eq!(armor.speed, 1.0, "armor speed 0 reads as 1");
        assert_eq!((armor.min_def, armor.max_def, armor.clothing), (2, 0, 88));
        let recipe = armor.recipe.as_ref().unwrap();
        assert_eq!(recipe.ingredients, vec![0, 3, 7, 9], "sorted, out-of-range id zeroed");
        assert_eq!((recipe.chance, recipe.price), (100, 50));
        assert!(t.get(4).is_none());
        // Warnings: unknown class, bad MAXDEF, id above NumOBJs.
        assert!(warnings.iter().any(|w| w.key == "CP3"));
        assert!(warnings.iter().any(|w| w.key == "MAXDEF" && w.section == "OBJ3"));
        assert!(warnings.iter().any(|w| w.section == "OBJ300"));
        // Armor keys are not read for the sword, weapon keys not for the apple.
        assert_eq!(apple.max_hit, 0);
    }

    #[test]
    fn official_objects_load() {
        let root = recursos_dir();
        if !root.join("Dat").join(FILE).is_file() {
            eprintln!("AO resources not found, skipping");
            return;
        }
        let Loaded { data: t, warnings } = load(&root).expect("obj.dat");
        assert_eq!(t.len(), 5199, "objects defined in obj.dat");
        assert_eq!(t.declared, 6754, "NumOBJs in [INIT]");
        assert_eq!(t.get(1).unwrap().name, "Manzana Roja");

        let apple = t.get(1).unwrap();
        assert_eq!((apple.obj_type, apple.grh, apple.value, apple.min_hunger, apple.crucial), (ObjType::UseOnce, 506, 2, 10, 1));
        assert_eq!(apple.text, "Restaura 10 puntos de hambre");

        // [OBJ2] Espada Larga: a sword for everyone but six classes.
        let sword = t.get(2).unwrap();
        assert_eq!(sword.name, "Espada Larga");
        assert_eq!((sword.obj_type, sword.grh, sword.value, sword.weapon_anim), (ObjType::Weapon, 49167, 800, 68));
        assert_eq!((sword.min_hit, sword.max_hit, sword.min_level, sword.max_level), (3, 5, 1, 28));
        assert_eq!(sword.weapon_type, Some(WeaponType::Sword));
        assert_eq!(
            sword.forbidden_classes,
            vec![Class::Pirate, Class::Hunter, Class::Druid, Class::Bard, Class::Thief, Class::Mage]
        );
        // [OBJ30] Armadura de Cuero: defense 7/11 and a body per race and gender.
        let armor = t.get(30).unwrap();
        assert_eq!((armor.obj_type, armor.min_def, armor.max_def, armor.speed), (ObjType::Armor, 7, 11, 1.0));
        assert_eq!((armor.tailoring_skill, armor.pelt_wolf, armor.max_level), (25, 75, 28));
        assert_eq!(armor.clothing_for(Race::Human, false), 63);
        assert_eq!(armor.clothing_for(Race::Elf, true), 2047);
        assert_eq!(armor.clothing_for(Race::Drow, false), 1547);
        assert_eq!(armor.clothing_for(Race::Orc, true), 1047);
        assert_eq!(armor.average_def(), 9.0);
        // [OBJ201] Cofre de Cascos Legendarios: chest subtype 2 with 8 stacks.
        let chest = t.get(201).unwrap();
        assert_eq!((chest.obj_type, chest.subtype, chest.deliver_count, chest.item_count), (ObjType::Chest, 2, 1, 8));
        assert_eq!(chest.contents.len(), 8);
        assert_eq!(chest.contents[0], ObjStack { obj: 2954, amount: 1, drop_roll: 0 });
        assert_eq!(chest.contents[7].obj, 1080);
        // A weapon, an armor and a door: pick the first of each type and check consistency
        // instead of hard-coding ids that only the file knows.
        let weapon = t.iter().map(|(_, o)| o).find(|o| o.obj_type == ObjType::Weapon && o.max_hit > 0).expect("a weapon");
        assert!(weapon.max_hit >= weapon.min_hit && weapon.weapon_anim > 0, "{weapon:?}");
        let armor = t.iter().map(|(_, o)| o).find(|o| o.obj_type == ObjType::Armor && o.max_def > 0).expect("an armor");
        assert!(armor.max_def >= armor.min_def && armor.speed > 0.0);
        let door = t.iter().map(|(_, o)| o).find(|o| o.obj_type == ObjType::Doors && o.locked == 1).expect("a locked door");
        assert!(door.open_index > 0 && door.closed_index > 0);
        assert!(t.iter().filter(|(_, o)| o.obj_type == ObjType::CollectibleCard).count() == 407);
        assert!(t.iter().any(|(_, o)| !o.forbidden_classes.is_empty()));
        assert!(t.iter().any(|(_, o)| o.female_only), "38 female-only items");
        assert!(t.iter().filter(|(_, o)| o.weight > 0.0).count() >= 24, "Peso entries");
        assert_eq!(t.iter().filter(|(_, o)| o.recipe.is_some()).count(), 12, "crafteable items");
        assert!(t.iter().filter(|(_, o)| o.obj_type == ObjType::Chest && !o.contents.is_empty()).count() > 0);
        // Apostrophes survive (the generic INI reader would cut them).
        assert!(warnings.iter().all(|w| w.file == FILE));
    }
}

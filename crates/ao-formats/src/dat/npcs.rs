//! `npcs.dat`: monsters, vendors, guards, trainers, bankers... (`[NPC500]`).
//!
//! Mirrors `LoadNpcInfoIntoCache` (`MODULO_NPCs.bas`), which the official server runs for
//! every `NPCn` section. Field comments give the original key. The `[INIT] NumNPCs` count
//! is not a limit: the server loads every `NPC<number>` section it finds.
use super::{Loaded, RawIni, Sec, Table, Warning};
use std::path::Path;

pub const FILE: &str = "npcs.dat";
/// `MaxDropCount`: most drops an NPC can have.
pub const MAX_DROPS: usize = 12;
/// `MAX_INVENTORY_SLOTS`: most items a vendor can sell.
pub const MAX_INVENTORY: usize = 42;
/// `RANGO_VISION_X` (`DEFAULT_NPC_VISION_RANGE_X`): default path-finding vision.
pub const DEFAULT_VISION: i32 = 15;
/// Sanity limit for the indexed lists (`Anim1..n`, `Sp1..n`, ...).
const MAX_LIST: usize = 255;

/// `e_NPCType`. Ids 14, 15 and 25 are unused by the server.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NpcType {
    /// 0 `Comun`: ordinary creature.
    #[default]
    Common,
    /// 1 `Revividor`: priest that resurrects.
    Reviver,
    /// 2 `GuardiaReal`.
    RoyalGuard,
    /// 3 `Entrenador`: sells creature training (uses `CI*`/`CN*`).
    Trainer,
    /// 4 `Banquero`.
    Banker,
    /// 5 `Enlistador`: faction recruiter.
    Recruiter,
    /// 6 `DRAGON`.
    Dragon,
    /// 7 `Timbero`: gambler.
    Gambler,
    /// 8 `GuardiasCaos`.
    ChaosGuard,
    /// 9 `ResucitadorNewbie`.
    NewbieReviver,
    /// 10 `Pirata`.
    Pirate,
    /// 11 `Veterinaria`.
    Vet,
    /// 12 `Gobernador`.
    Governor,
    /// 13 `GuardiaNpc`.
    NpcGuard,
    /// 16 `Subastador`: auctioneer.
    Auctioneer,
    /// 17 `Quest`.
    Quest,
    /// 18 `Pretoriano`.
    Praetorian,
    /// 19 `DummyTarget`.
    DummyTarget,
    /// 20 `EntregaPesca`.
    FishDelivery,
    /// 21 `AO20Shop`.
    Ao20Shop,
    /// 22 `AO20ShopPjs`.
    Ao20ShopPjs,
    /// 23 `EventMaster`.
    EventMaster,
    /// 24 `ArenaGuard`.
    ArenaGuard,
    /// 26 `Transporter`: sells trips between cities (uses `City*`).
    Transporter,
    /// 100 (Numic, TSAO `Correos`): post office, player mail.
    PostOffice,
    /// 101 (Numic, TSAO `BoveClan`): the clan vault.
    ClanVault,
    /// 102 (Numic, TSAO `cirujano`): changes the character's face.
    Surgeon,
    Unknown(i32),
}

impl NpcType {
    pub fn from_id(id: i32) -> NpcType {
        use NpcType::*;
        match id {
            0 => Common,
            1 => Reviver,
            2 => RoyalGuard,
            3 => Trainer,
            4 => Banker,
            5 => Recruiter,
            6 => Dragon,
            7 => Gambler,
            8 => ChaosGuard,
            9 => NewbieReviver,
            10 => Pirate,
            11 => Vet,
            12 => Governor,
            13 => NpcGuard,
            16 => Auctioneer,
            17 => Quest,
            18 => Praetorian,
            19 => DummyTarget,
            20 => FishDelivery,
            21 => Ao20Shop,
            22 => Ao20ShopPjs,
            23 => EventMaster,
            24 => ArenaGuard,
            26 => Transporter,
            100 => PostOffice,
            101 => ClanVault,
            102 => Surgeon,
            n => Unknown(n),
        }
    }

    pub fn id(self) -> i32 {
        use NpcType::*;
        match self {
            Common => 0,
            Reviver => 1,
            RoyalGuard => 2,
            Trainer => 3,
            Banker => 4,
            Recruiter => 5,
            Dragon => 6,
            Gambler => 7,
            ChaosGuard => 8,
            NewbieReviver => 9,
            Pirate => 10,
            Vet => 11,
            Governor => 12,
            NpcGuard => 13,
            Auctioneer => 16,
            Quest => 17,
            Praetorian => 18,
            DummyTarget => 19,
            FishDelivery => 20,
            Ao20Shop => 21,
            Ao20ShopPjs => 22,
            EventMaster => 23,
            ArenaGuard => 24,
            Transporter => 26,
            PostOffice => 100,
            ClanVault => 101,
            Surgeon => 102,
            Unknown(n) => n,
        }
    }
}

/// `e_TipoAI`: the `Movement` key, i.e. the NPC's behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Movement {
    /// 1 `Estatico`: stands still.
    Static,
    /// 2 `MueveAlAzar`: wanders.
    Wander,
    /// 3 `FixedInPos`.
    FixedInPos,
    /// 4 `NpcDefensa`: defends itself.
    Defend,
    /// 8 `SigueAmo`: follows its owner.
    FollowOwner,
    /// 9 `NpcAtacaNpc`.
    AttackNpc,
    /// 10 `GuardiaPersigueNpc`.
    GuardChasesNpc,
    /// 11 `SupportAndAttack`.
    SupportAndAttack,
    /// 12 `BGTankBehavior`.
    BgTank,
    /// 13 `BGSupportBehavior`.
    BgSupport,
    /// 14 `BGRangedBehavior`.
    BgRanged,
    /// 15 `BGBossBehavior`.
    BgBoss,
    /// 16 `BGBossReturnToOrigin`.
    BgBossReturn,
    /// 20 `Caminata`: follows a fixed walk (`Caminata1..n`).
    Walk,
    /// 21 `Invasion`.
    Invasion,
    /// 0 when the key is missing, or any other id.
    Unknown(i32),
}

impl Movement {
    pub fn from_id(id: i32) -> Movement {
        use Movement::*;
        match id {
            1 => Static,
            2 => Wander,
            3 => FixedInPos,
            4 => Defend,
            8 => FollowOwner,
            9 => AttackNpc,
            10 => GuardChasesNpc,
            11 => SupportAndAttack,
            12 => BgTank,
            13 => BgSupport,
            14 => BgRanged,
            15 => BgBoss,
            16 => BgBossReturn,
            20 => Walk,
            21 => Invasion,
            n => Unknown(n),
        }
    }

    pub fn id(self) -> i32 {
        use Movement::*;
        match self {
            Static => 1,
            Wander => 2,
            FixedInPos => 3,
            Defend => 4,
            FollowOwner => 8,
            AttackNpc => 9,
            GuardChasesNpc => 10,
            SupportAndAttack => 11,
            BgTank => 12,
            BgSupport => 13,
            BgRanged => 14,
            BgBoss => 15,
            BgBossReturn => 16,
            Walk => 20,
            Invasion => 21,
            Unknown(n) => n,
        }
    }
}

impl Default for Movement {
    fn default() -> Self {
        Movement::Unknown(0)
    }
}

/// `DropN=obj-chance-minQty-maxQty`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NpcDrop {
    /// Object id.
    pub obj: i32,
    /// Chance in percent (the server rolls against it).
    pub chance: i32,
    pub min_quantity: i32,
    pub max_quantity: i32,
}

/// `ObjN=obj-amount`: an item a vendor sells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InventoryItem {
    pub obj: i32,
    pub amount: i32,
    /// TSAO third field `ObjN=obj-amount-prob`: a creature drops it on death with chance
    /// `prob * (2 + DropMult)` in 200 (`NPC_TIRAR_ITEMS`). 0 = merchant stock / no drop.
    pub drop_prob: i32,
}

/// `SpN` + `CdN`: a spell the NPC casts and its cooldown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NpcSpell {
    /// Spell id (`Hechizos.dat`).
    pub spell: i32,
    pub cooldown: i32,
}

/// `CIn` + `CNn` (trainers): a creature the trainer offers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Creature {
    /// NPC id.
    pub npc: i32,
    pub name: String,
}

/// `DropQuestN=quest-obj-amount-probability`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuestDrop {
    pub quest: i32,
    pub obj: i32,
    pub amount: i32,
    pub probability: i32,
}

/// `CaminataN=offsetX:offsetY:wait`: one step of a fixed walk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalkStep {
    pub dx: i32,
    pub dy: i32,
    /// Wait after arriving.
    pub wait: i32,
}

/// `CityN=name-map-x-y-price` (transporters).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransportCity {
    pub name: String,
    pub map: i32,
    pub x: i32,
    pub y: i32,
    pub price: i32,
    /// Lowest level allowed (0 = any).
    pub min_level: i32,
    /// Price from level 30 on (0 = the same `price`).
    pub price_high: i32,
}

/// `HealsOwner=spell,secs,range[,CLASS]`: while its owner is hurt and within `range`
/// tiles, the pet casts `spell` on them every `secs` seconds (only for owners of `CLASS`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HealsOwner {
    pub spell: i32,
    pub secs: u32,
    pub range: i32,
    pub class: Option<String>,
}

/// TSAO behaviour by NPC number turned into data by tools/tsao/convert.py (all empty for AO20).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpecialNpc {
    /// `UniqueSummon=1`: a player may have only one of this summon at a time.
    pub unique_summon: bool,
    /// `CastsOnNpcs=1`: as a pet it casts its spell on the creature it fights.
    pub casts_on_npcs: bool,
    /// `Companion=1`: a pet that only follows its owner (never fights).
    pub companion: bool,
    /// `SpellDamage=min-max`: its spells hit players for this instead of the spell's roll.
    pub spell_damage: Option<(i32, i32)>,
    pub heals_owner: Option<HealsOwner>,
    /// `ImmuneSpells=12,...`: Hechizos.dat spells that do nothing to it.
    pub immune_spells: Vec<i32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Npc {
    // ---- identity and look ----
    /// `Name`.
    pub name: String,
    /// `SubName`.
    pub sub_name: String,
    /// `Desc`.
    pub description: String,
    /// `Nivel`.
    pub level: i32,
    /// `NpcType`.
    pub npc_type: NpcType,
    /// `Body`.
    pub body: i32,
    /// `Head`.
    pub head: i32,
    /// `Heading`.
    pub heading: i32,
    /// `CastAnimation`.
    pub cast_animation: i32,
    /// `Animaciones` + `Anim1..n`.
    pub animations: Vec<i32>,
    /// `Arma`.
    pub weapon_anim: i32,
    /// `Escudo`.
    pub shield_anim: i32,
    /// `Casco`.
    pub helmet_anim: i32,
    /// `Cart`.
    pub cart_anim: i32,
    /// `ShowName`.
    pub show_name: i32,
    /// `BodyIdle` (not read by the server; client).
    pub body_idle: i32,
    /// `Ataque1` (not read by the server; client).
    pub attack_anim1: i32,
    /// `Ataque2` (not read by the server; client).
    pub attack_anim2: i32,
    /// `NpcFaceGrh` (not read by the server; client).
    pub face_grh: i32,

    // ---- behaviour ----
    /// `Movement`.
    pub movement: Movement,
    /// `Attackable`.
    pub attackable: i32,
    /// `Hostile`.
    pub hostile: i32,
    /// `Comercia`: trades with players.
    pub trades: i32,
    /// `Craftea`: crafts for players.
    pub crafts: i32,
    /// `Domable`: can be tamed.
    pub tameable: i32,
    /// `MinTameLevel` (default 1).
    pub min_tame_level: i32,
    /// `Humanoide`.
    pub humanoid: i32,
    /// `Faccion`.
    pub faction: i32,
    /// `Alineacion` (`AIAlineacion`: 0 none, 1 royal, 2 chaos).
    pub alignment: i32,
    /// `ElementalTags`.
    pub elemental_tags: i32,
    /// `AguaValida`: can walk on water.
    pub water_valid: i32,
    /// `TierraInValida`: cannot walk on land.
    pub land_invalid: i32,
    /// `LavaValida`.
    pub lava_valid: i32,
    /// `AttackableByEveryone`.
    pub attackable_by_everyone: i32,
    /// `AttackRange`.
    pub attack_range: i32,
    /// `PreferedRange`.
    pub preferred_range: i32,
    /// `ProjectileType`.
    pub projectile_type: i32,
    /// `Distancia`.
    pub distance: i32,
    /// `Distancia` read again with default 15 (`PathFindingVision`).
    pub path_vision: i32,
    /// `Veneno`: poisons on hit.
    pub poison: i32,
    /// `RestriccionDeAtaque`: 0 attacks NPCs and users, 1 users only, 2 NPCs only.
    pub attack_restriction: i32,
    /// `RestriccionDeAyuda`: 1 helps NPCs, 2 helps users, 3 both.
    pub help_restriction: i32,
    /// `DontHitVisiblePlayers`.
    pub dont_hit_visible_players: bool,
    /// `AddToMapAiList`.
    pub add_to_map_ai_list: bool,
    /// `DisplayCastMessage`.
    pub display_cast_message: bool,
    /// `Team`.
    pub team: i32,
    /// `AfectaParalisis`.
    pub affected_by_paralysis: i32,
    /// `ImmuneToSpells`.
    pub immune_to_spells: i32,
    /// `GolpeExacto`.
    pub exact_hit: i32,
    /// `TranslationInmune`.
    pub translation_immune: bool,
    /// `GobernadorDe`.
    pub governor_of: i32,
    /// `Interface`.
    pub interface: i32,
    /// `TipoItems`.
    pub item_type: i32,
    /// `PuedeInvocar`.
    pub can_summon: i32,
    /// `OnlyForGuilds`.
    pub only_for_guilds: i32,
    /// `ShowKillerConsole`.
    pub show_killer_console: i32,
    /// `GlobalQuestBossIndex`.
    pub global_quest_boss_index: i32,
    /// `CollectibleCardIndex`.
    pub collectible_card_index: i32,

    // ---- combat stats ----
    /// `MinHP`.
    pub min_hp: i32,
    /// `MaxHP`.
    pub max_hp: i32,
    /// `MinHIT`.
    pub min_hit: i32,
    /// `MaxHIT`.
    pub max_hit: i32,
    /// `DEF`.
    pub defense: i32,
    /// `DEFm`: magic defense (`defM`).
    pub magic_defense_flat: i32,
    /// `MagicResistance`.
    pub magic_resistance: i32,
    /// `MagicDef`.
    pub magic_def: i32,
    /// `MagicBonus`.
    pub magic_bonus: i32,
    /// `PoderAtaque`.
    pub attack_power: i32,
    /// `PoderEvasion`.
    pub evasion_power: i32,
    /// `IntervaloAtaque` (ms; 0 means the server default of 2000).
    pub attack_interval: i32,
    /// `IntervaloMovimiento` (ms; 0 means 380).
    pub move_interval: i32,
    /// `IntervaloLanzarHechizo` (ms; 0 means 8000).
    pub spell_interval: i32,
    /// `CantidadInvocaciones`.
    pub summon_count: i32,

    // ---- rewards ----
    /// `GiveEXP`.
    pub exp: i32,
    /// `GiveEXPClan`.
    pub clan_exp: i32,
    /// `GiveGLD`.
    pub gold: i32,
    /// TSAO `GiveGLDMin` / `GiveGLDMax` (0, 0 = not used).
    pub gold_range: (i32, i32),
    /// Behaviour TSAO hard-coded by NPC number, as data written by tools/tsao.
    pub special: SpecialNpc,
    /// `InvReSpawn`.
    pub inventory_respawn: i32,
    /// `DropCount` + `Drop1..n` (at most 12).
    pub drops: Vec<NpcDrop>,
    /// `NumDropQuest` + `DropQuest1..n`.
    pub quest_drops: Vec<QuestDrop>,
    /// `NumQuest` + `QuestNumber1..n`: quests this NPC offers.
    pub quests: Vec<i32>,

    // ---- respawn ----
    /// `ReSpawn`.
    pub respawn: i32,
    /// `IntervaloRespawnMin`.
    pub respawn_interval_min: i32,
    /// `IntervaloRespawn` (the maximum).
    pub respawn_interval_max: i32,
    /// `InformarRespawn`.
    pub announce_respawn: i32,
    /// `OrigPos`: respawns at its original position.
    pub respawn_at_origin: i32,
    /// `BackUp`.
    pub backup: i32,

    // ---- vendors, trainers, transport ----
    /// `NROITEMS` + `Obj1..n` (at most 42).
    pub inventory: Vec<InventoryItem>,
    /// `NroCriaturas` + `CI*`/`CN*` (only for [`NpcType::Trainer`]).
    pub creatures: Vec<Creature>,
    /// `LanzaSpells` + `Sp1..n` / `Cd1..n`.
    pub spells: Vec<NpcSpell>,
    /// `RangoSpell` (0 when it casts nothing).
    pub spell_range: i32,
    /// `NROEXP` + `Exp1..n`: phrases it says.
    pub expressions: Vec<String>,
    /// `NumDestinos` + `Dest1..n`.
    pub destinations: Vec<String>,
    /// `MapEntryPrice`.
    pub map_entry_price: i32,
    /// `MapTargetEntry` (default 1).
    pub map_target_entry: i32,
    /// `MapTargetEntryX` (default 50).
    pub map_target_entry_x: i32,
    /// `MapTargetEntryY` (default 50).
    pub map_target_entry_y: i32,
    /// `ArenaEnabled`.
    pub arena_enabled: i32,
    /// `CaminataLen` + `Caminata1..n` (used when `movement` is [`Movement::Walk`]).
    pub walk: Vec<WalkStep>,
    /// `TransporterLevel` (only for [`NpcType::Transporter`]).
    pub transporter_level: i32,
    /// `CityCount` + `City1..n` (only for [`NpcType::Transporter`]).
    pub cities: Vec<TransportCity>,

    // ---- sounds ----
    /// `Snd1`.
    pub snd1: i32,
    /// `Snd2`.
    pub snd2: i32,
    /// `Snd3`.
    pub snd3: i32,
    /// `SndRespawn`.
    pub snd_respawn: i32,
    /// `SoundOpen`.
    pub sound_open: i32,
    /// `SoundClose`.
    pub sound_close: i32,

    // ---- availability ----
    /// `TESTONLY`: skipped on production servers.
    pub test_only: i32,
    /// `DISABLEDINBATTLESERVER`.
    pub disabled_in_battle_server: i32,
    /// `ONLYENABLEDINBATTLESERVER`.
    pub only_enabled_in_battle_server: i32,
    /// `REQUIRETOGGLE`: feature toggle that must be on.
    pub require_toggle: String,
}

impl Npc {
    pub fn attacks_users(&self) -> bool {
        matches!(self.attack_restriction, 0 | 1)
    }

    pub fn attacks_npcs(&self) -> bool {
        matches!(self.attack_restriction, 0 | 2)
    }

    pub fn helps_npcs(&self) -> bool {
        matches!(self.help_restriction, 1 | 3)
    }

    pub fn helps_users(&self) -> bool {
        matches!(self.help_restriction, 2 | 3)
    }
}

/// Reads `npcs.dat`.
pub fn load(recursos: &Path) -> Result<Loaded<Table<Npc>>, super::Error> {
    let mut warnings = Vec::new();
    let ini = RawIni::load(recursos, FILE, &mut warnings)?;
    Ok(finish(&ini, warnings))
}

/// Parses the text of an `npcs.dat` (already decoded).
pub fn parse_str(text: &str) -> Loaded<Table<Npc>> {
    let mut warnings = Vec::new();
    let ini = RawIni::parse(text, FILE, &mut warnings);
    finish(&ini, warnings)
}

fn finish(ini: &RawIni, mut w: Vec<Warning>) -> Loaded<Table<Npc>> {
    let declared = ini.sec(FILE, "INIT", &mut w).int("NumNPCs").max(0) as u32;
    let mut table = Table { declared, ..Table::default() };
    for (id, name) in ini.numbered("NPC", FILE, &mut w) {
        let mut sec = ini.sec(FILE, &name, &mut w);
        let npc = read_npc(&mut sec);
        table.insert(id, npc);
    }
    Loaded { data: table, warnings: w }
}

// Field-by-field assignment keeps this function a line-for-line mirror of the VB loader.
#[allow(clippy::field_reassign_with_default)]
fn read_npc(s: &mut Sec) -> Npc {
    let mut n = Npc::default();
    n.test_only = s.int("TESTONLY");
    n.disabled_in_battle_server = s.int("DISABLEDINBATTLESERVER");
    n.collectible_card_index = s.int("CollectibleCardIndex");
    n.only_enabled_in_battle_server = s.int("ONLYENABLEDINBATTLESERVER");
    n.require_toggle = s.text("REQUIRETOGGLE");
    n.name = s.text("Name");
    n.sub_name = s.text("SubName");
    n.description = s.text("Desc");
    n.level = s.int("Nivel");
    n.movement = Movement::from_id(s.int("Movement"));
    n.water_valid = s.int("AguaValida");
    n.land_invalid = s.int("TierraInValida");
    n.lava_valid = s.int("LavaValida");
    n.faction = s.int("Faccion");
    n.elemental_tags = s.int("ElementalTags");
    n.global_quest_boss_index = s.int("GlobalQuestBossIndex");
    n.npc_type = NpcType::from_id(s.int("NpcType"));
    n.body = s.int("Body");
    n.head = s.int("Head");
    n.heading = s.int("Heading");
    n.cast_animation = s.int("CastAnimation");
    let anims = s.count("Animaciones", MAX_LIST);
    n.animations = (1..=anims).map(|i| s.int(&format!("Anim{i}"))).collect();
    n.weapon_anim = s.int("Arma");
    n.shield_anim = s.int("Escudo");
    n.helmet_anim = s.int("Casco");
    n.cart_anim = s.int("Cart");
    n.attackable = s.int("Attackable");
    n.trades = s.int("Comercia");
    n.crafts = s.int("Craftea");
    n.hostile = s.int("Hostile");
    n.attack_range = s.int("AttackRange");
    n.projectile_type = s.int("ProjectileType");
    n.preferred_range = s.int("PreferedRange");
    n.exp = s.int("GiveEXP");
    n.distance = s.int("Distancia");
    n.clan_exp = s.int("GiveEXPClan");
    n.poison = s.int("Veneno");
    n.tameable = s.int("Domable");
    n.attackable_by_everyone = s.int("AttackableByEveryone");
    n.map_entry_price = s.int("MapEntryPrice");
    n.map_target_entry = s.int_or("MapTargetEntry", 1);
    n.map_target_entry_x = s.int_or("MapTargetEntryX", 50);
    n.map_target_entry_y = s.int_or("MapTargetEntryY", 50);
    n.arena_enabled = s.int("ArenaEnabled");
    n.gold = s.int("GiveGLD");
    n.attack_power = s.int("PoderAtaque");
    n.evasion_power = s.int("PoderEvasion");
    n.inventory_respawn = s.int("InvReSpawn");
    n.show_name = s.int("ShowName");
    n.governor_of = s.int("GobernadorDe");
    n.sound_open = s.int("SoundOpen");
    n.sound_close = s.int("SoundClose");
    n.attack_interval = s.int("IntervaloAtaque");
    n.move_interval = s.int("IntervaloMovimiento");
    n.spell_interval = s.int("IntervaloLanzarHechizo");
    n.respawn_interval_min = s.int("IntervaloRespawnMin");
    n.respawn_interval_max = s.int("IntervaloRespawn");
    n.announce_respawn = s.int("InformarRespawn");

    // The server clamps DropCount to 0..=MaxDropCount without complaint; we warn.
    let drops = s.count("DropCount", MAX_DROPS);
    for i in 1..=drops {
        let f = s.ints(&format!("Drop{i}"), '-', 4);
        n.drops.push(NpcDrop { obj: f[0], chance: f[1], min_quantity: f[2], max_quantity: f[3] });
    }
    n.min_tame_level = s.int_or("MinTameLevel", 1);
    n.only_for_guilds = s.int("OnlyForGuilds");
    n.show_killer_console = s.int("ShowKillerConsole");
    n.max_hp = s.int("MaxHP");
    n.min_hp = s.int("MinHP");
    n.max_hit = s.int("MaxHIT");
    n.min_hit = s.int("MinHIT");
    n.defense = s.int("DEF");
    n.magic_defense_flat = s.int("DEFm");
    n.magic_resistance = s.int("MagicResistance");
    n.magic_def = s.int("MagicDef");
    n.summon_count = s.int("CantidadInvocaciones");
    n.magic_bonus = s.int("MagicBonus");
    n.alignment = s.int("Alineacion");

    let items = s.count("NROITEMS", MAX_INVENTORY);
    for i in 1..=items {
        let f = s.ints(&format!("Obj{i}"), '-', 2);
        n.inventory.push(InventoryItem { obj: f[0], amount: f[1], drop_prob: f.get(2).copied().unwrap_or(0) });
    }
    // TSAO: gold straight to the killer, `RandomNumber(GiveGLDMin, GiveGLDMax)`.
    n.gold_range = (s.int("GiveGLDMin"), s.int("GiveGLDMax"));
    n.special = SpecialNpc {
        unique_summon: s.flag("UniqueSummon"),
        casts_on_npcs: s.flag("CastsOnNpcs"),
        companion: s.flag("Companion"),
        spell_damage: s.text("SpellDamage").split_once('-').and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?))),
        heals_owner: {
            let t = s.text("HealsOwner");
            let f: Vec<&str> = t.split(',').map(str::trim).collect();
            match f[..] {
                [sp, secs, range, ref rest @ ..] => match (sp.parse(), secs.parse(), range.parse()) {
                    (Ok(spell), Ok(secs), Ok(range)) => Some(HealsOwner { spell, secs, range, class: rest.first().map(|c| c.to_uppercase()).filter(|c| !c.is_empty()) }),
                    _ => None,
                },
                _ => None,
            }
        },
        immune_spells: s.text("ImmuneSpells").split(',').filter_map(|v| v.trim().parse().ok()).collect(),
    };
    n.humanoid = s.int("Humanoide");
    let spells = s.count("LanzaSpells", MAX_LIST);
    if spells > 0 {
        n.spell_range = s.int("RangoSpell");
    }
    for i in 1..=spells {
        n.spells.push(NpcSpell { spell: s.int(&format!("Sp{i}")), cooldown: s.int(&format!("Cd{i}")) });
    }
    if n.npc_type == NpcType::Trainer {
        let c = s.count("NroCriaturas", MAX_LIST);
        for i in 1..=c {
            n.creatures.push(Creature { npc: s.int(&format!("CI{i}")), name: s.text(&format!("CN{i}")) });
        }
    }
    n.attack_restriction = s.int("RestriccionDeAtaque");
    n.help_restriction = s.int("RestriccionDeAyuda");
    n.respawn = s.int("ReSpawn");
    n.dont_hit_visible_players = s.flag("DontHitVisiblePlayers");
    n.add_to_map_ai_list = s.flag("AddToMapAiList");
    n.display_cast_message = s.flag("DisplayCastMessage");
    n.team = s.int("Team");
    n.backup = s.int("BackUp");
    n.respawn_at_origin = s.int("OrigPos");
    n.affected_by_paralysis = s.int("AfectaParalisis");
    n.immune_to_spells = s.int("ImmuneToSpells");
    n.exact_hit = s.int("GolpeExacto");
    n.translation_immune = s.flag("TranslationInmune");
    n.snd1 = s.int("Snd1");
    n.snd2 = s.int("Snd2");
    n.snd3 = s.int("Snd3");
    n.snd_respawn = s.int("SndRespawn");

    let exp = s.count("NROEXP", MAX_LIST);
    n.expressions = (1..=exp).map(|i| s.text(&format!("Exp{i}"))).collect();
    let quests = s.count("NumQuest", MAX_LIST);
    n.quests = (1..=quests).map(|i| s.int(&format!("QuestNumber{i}"))).collect();
    let quest_drops = s.count("NumDropQuest", MAX_LIST);
    for i in 1..=quest_drops {
        let f = s.ints(&format!("DropQuest{i}"), '-', 4);
        n.quest_drops.push(QuestDrop { quest: f[0], obj: f[1], amount: f[2], probability: f[3] });
    }
    n.path_vision = s.int_or("Distancia", DEFAULT_VISION);
    let dests = s.count("NumDestinos", MAX_LIST);
    n.destinations = (1..=dests).map(|i| s.text(&format!("Dest{i}"))).collect();
    n.interface = s.int("Interface");
    n.item_type = s.int("TipoItems");
    n.can_summon = s.int("PuedeInvocar");

    let walk = s.count("CaminataLen", MAX_LIST);
    for i in 1..=walk {
        let key = format!("Caminata{i}");
        let raw = s.text(&key);
        let parts: Vec<&str> = raw.split(':').collect();
        // VB: fewer than three fields means an all-zero step.
        let step = if parts.len() >= 3 {
            WalkStep { dx: s.int_of(&key, &raw, parts[0]), dy: s.int_of(&key, &raw, parts[1]), wait: s.int_of(&key, &raw, parts[2]) }
        } else {
            if !raw.is_empty() {
                s.warning(&key, &raw, "expected offsetX:offsetY:wait (read as 0:0:0)");
            }
            WalkStep::default()
        };
        n.walk.push(step);
    }
    if n.npc_type == NpcType::Transporter {
        n.cities = Vec::new();
        let c = s.count("CityCount", MAX_LIST);
        n.transporter_level = s.int("TransporterLevel");
        for i in 1..=c {
            let key = format!("City{i}");
            // The first field is the city name (it may contain spaces, not hyphens); the
            // other four are numbers.
            let raw = s.text(&key);
            let mut parts = raw.split('-');
            let name = parts.next().unwrap_or("").trim().to_string();
            let mut num = |s: &mut Sec| parts.next().map_or(0, |p| s.int_of(&key, &raw, p));
            let (map, x, y, price) = (num(s), num(s), num(s), num(s));
            // Numic extension: optional `-minLevel-priceFromLevel30` (TSAO charged 5x
            // from level 30 and kept some cities for level 30+).
            let (min_level, price_high) = (num(s), num(s));
            n.cities.push(TransportCity { name, map, x, y, price, min_level, price_high });
        }
    }

    // Not read by the official server.
    n.body_idle = s.int("BodyIdle");
    n.attack_anim1 = s.int("Ataque1");
    n.attack_anim2 = s.int("Ataque2");
    n.face_grh = s.int("NpcFaceGrh");
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::recursos_dir;

    #[test]
    fn synthetic_npc() {
        let text = "\
[INIT]
NumNPCs=50
[NPC500]
Name=Lobo
Body=4372
Movement=2
DropCount=2
Drop1=2481-1-1-1
Drop2=7-50
Distancia=
[NPC501]
Name=Vendedor
NpcType=3
NroCriaturas=1
CI1=500
CN1=Lobo
NROITEMS=1
Obj1=12-100
LanzaSpells=1
Sp1=5
Cd1=30
RangoSpell=9
CaminataLen=1
Caminata1=1:2
[NPC502]
NpcType=26
CityCount=1
City1=Nix-34-50-60-2500
TransporterLevel=10
DropCount=99
";
        let Loaded { data: t, warnings } = parse_str(text);
        assert_eq!(t.len(), 3);
        let wolf = t.get(500).unwrap();
        assert_eq!((wolf.name.as_str(), wolf.body, wolf.movement), ("Lobo", 4372, Movement::Wander));
        assert_eq!(
            wolf.drops,
            vec![NpcDrop { obj: 2481, chance: 1, min_quantity: 1, max_quantity: 1 }, NpcDrop { obj: 7, chance: 50, min_quantity: 0, max_quantity: 0 }]
        );
        assert_eq!(wolf.min_tame_level, 1);
        assert_eq!((wolf.map_target_entry, wolf.map_target_entry_x, wolf.map_target_entry_y), (1, 50, 50));
        assert_eq!(wolf.path_vision, 0, "present but empty reads as 0, not the default");
        let trainer = t.get(501).unwrap();
        assert_eq!(trainer.creatures, vec![Creature { npc: 500, name: "Lobo".into() }]);
        assert_eq!(trainer.inventory, vec![InventoryItem { obj: 12, amount: 100, drop_prob: 0 }]);
        assert_eq!(trainer.spells, vec![NpcSpell { spell: 5, cooldown: 30 }]);
        assert_eq!(trainer.spell_range, 9);
        assert_eq!(trainer.path_vision, DEFAULT_VISION);
        let ferry = t.get(502).unwrap();
        assert_eq!(ferry.cities, vec![TransportCity { name: "Nix".into(), map: 34, x: 50, y: 60, price: 2500, ..Default::default() }]);
        assert_eq!(ferry.transporter_level, 10);
        assert!(ferry.drops.len() <= MAX_DROPS);
        assert!(warnings.iter().any(|w| w.key == "DropCount" && w.section == "NPC502"));
        assert!(warnings.iter().any(|w| w.key == "Caminata1"));
    }

    #[test]
    fn official_npcs_load() {
        let root = recursos_dir();
        if !root.join("Dat").join(FILE).is_file() {
            eprintln!("AO resources not found, skipping");
            return;
        }
        let Loaded { data: t, warnings } = load(&root).expect("npcs.dat");
        assert_eq!(t.len(), 907, "NPCs defined in npcs.dat");
        assert_eq!(t.declared, 1954, "NumNPCs in [INIT]");

        // [NPC500] Lobo.
        let wolf = t.get(500).unwrap();
        assert_eq!(wolf.name, "Lobo");
        assert_eq!((wolf.body, wolf.npc_type, wolf.movement, wolf.hostile, wolf.attackable), (4372, NpcType::Common, Movement::Wander, 1, 1));
        assert_eq!((wolf.min_hp, wolf.max_hp, wolf.min_hit, wolf.max_hit), (60, 60, 6, 12));
        assert_eq!((wolf.exp, wolf.gold, wolf.level), (30, 3, 5));
        assert_eq!((wolf.attack_power, wolf.evasion_power, wolf.attack_interval, wolf.move_interval), (20, 17, 2200, 300));
        assert_eq!(wolf.drops, vec![NpcDrop { obj: 2481, chance: 1, min_quantity: 1, max_quantity: 1 }]);
        assert_eq!((wolf.snd1, wolf.snd2, wolf.snd3), (35, 9, 1213));
        assert_eq!((wolf.body_idle, wolf.attack_anim1), (4373, 4371));

        // [NPC1] priest, [NPC3] banker (inline comment after PuedeInvocar), [NPC501] hen, [NPC502] snake.
        let priest = t.get(1).unwrap();
        assert_eq!(priest.name, "Juan XXI <Sacerdote>");
        assert_eq!((priest.npc_type, priest.body, priest.movement, priest.sound_open), (NpcType::Reviver, 6234, Movement::Static, 836));
        assert_eq!((priest.show_name, priest.face_grh, priest.collectible_card_index), (1, 7016, 137));
        assert!(priest.description.starts_with("¡Hola, hijo mío!"));
        let banker = t.get(3).unwrap();
        assert_eq!((banker.npc_type, banker.can_summon, banker.sound_close), (NpcType::Banker, 1, 916));
        let hen = t.get(501).unwrap();
        assert_eq!((hen.name.as_str(), hen.tameable, hen.distance, hen.path_vision, hen.humanoid), ("Gallina", 17, 9, 9, 1));
        assert!(hen.attacks_npcs() && !hen.attacks_users());
        assert_eq!(hen.drops, vec![NpcDrop { obj: 26, chance: 1, min_quantity: 1, max_quantity: 1 }]);
        assert_eq!(t.get(502).unwrap().poison, 1);
        assert_eq!(t.get(502).unwrap().path_vision, 9);

        // Every NPC with spells has valid spell ids; every vendor has items.
        assert!(t.iter().any(|(_, n)| !n.spells.is_empty()));
        assert!(t.iter().any(|(_, n)| n.trades > 0 && !n.inventory.is_empty()));
        // Transporters (NPC1902-1904): hyphenated `City1=Name-map-x-y-price`.
        let ferry = t.get(1903).unwrap();
        assert_eq!(ferry.npc_type, NpcType::Transporter);
        assert_eq!(ferry.cities[2], TransportCity { name: "Ciudad de Nix".into(), map: 34, x: 33, y: 26, price: 2000, ..Default::default() });
        assert_eq!(ferry.cities.len(), 3);
        // The data has no trainers, vendors have prices-by-object lists.
        assert!(t.iter().all(|(_, n)| n.npc_type != NpcType::Trainer));
        assert!(t.iter().all(|(_, n)| n.drops.len() <= MAX_DROPS && n.inventory.len() <= MAX_INVENTORY));
        assert!(warnings.iter().all(|w| w.file == FILE));
    }
}

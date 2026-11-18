//! Gathering work rules (mining, woodcutting, fishing, smelting), pure and shared by the
//! server (enforces) and the client (plans the gesture). OWNER: Opus A (docs/P4_CONTRATO.md).
//!
//! Sources (VB6, `argentum-online-server-classic/Codigo`):
//! - `ModAutomatedActions.bas`: `MIN_STA_REQUIRED`, `CLOSE/MEDIUM_DISTANCE_EXTRACTION`,
//!   `CheckResourceDistance` (Manhattan `Distancia`), `IntervalAutomatedAction` (intervalos.ini).
//! - `ModMining.bas` `MineMinerals`, `ModLumberjack.bas` `ChopWood`: the luck formula.
//! - `Trabajo.bas` `GetExtractResourceForLevel`, `ActualizarRecurso`.
//! - `modFishing.bas` `GetFishingChance`, `ObtenerPezRandom`; `FileIO.bas` `LoadPesca`,
//!   `BinarySearchPeces`, `LoadRecursosEspeciales`.
//! - `modSmelting.bas` `MineralsRequiredPerIngot`.
//! - `Modulo_UsUaRiOs.bas` `SubirSkill` (raise by use).
//! - `FileIO.bas` `CargarMapaFormatoCSM`: the objects section of the `.csm` (trees, deposits,
//!   forges, fishing areas), which `ao_formats::csm::AoMap` skips.
use ao_formats::ini::Ini;

/// `ModAutomatedActions.MIN_STA_REQUIRED`: stamina spent per gathering attempt.
pub const MIN_STA_REQUIRED: i32 = 5;
/// `CraftIngots`: stamina spent per smelting attempt (needs more than this).
pub const SMELT_STA: i32 = 2;
/// `CLOSE_DISTANCE_EXTRACTION`: trees, deposits, fishing net.
pub const CLOSE_DISTANCE: i32 = 1;
/// `MEDIUM_DISTANCE_EXTRACTION`: the forge.
pub const MEDIUM_DISTANCE: i32 = 3;
/// `IntervalAutomatedAction=4000` (intervalos.ini): one attempt every 4 s.
pub const WORK_INTERVAL_MS: u64 = 4000;

/// `e_WorkingToolSubType`.
pub const TOOL_FISHING_ROD: i32 = 1;
pub const TOOL_FISHING_NET: i32 = 2;
pub const TOOL_FELLING_AXE: i32 = 6;
pub const TOOL_MINER_PICKAXE: i32 = 8;

/// Skill indexes in `CharStats::skills` (`e_Skill` minus one). `Smelting` (26) does not
/// exist as a stored skill: smelting checks and raises Mining, like the VB.
pub const SKILL_WOODCUTTING: usize = 16;
pub const SKILL_FISHING: usize = 17;
pub const SKILL_MINING: usize = 18;

/// `Wood`, `ElvenWood`, `PinoWood` (Declares.bas).
pub const WOOD: u16 = 58;
pub const ELVEN_WOOD: u16 = 2781;
pub const PINE_WOOD: u16 = 3788;
/// `e_Minerales`.
pub const IRON_ORE: u16 = 192;
pub const SILVER_ORE: u16 = 193;
pub const GOLD_ORE: u16 = 194;
/// `OBJ_FISH_AREA` (= `FishingID` of Configuracion.ini), `OBJ_FISH_BANK`, `OBJ_SQUID_BANK`,
/// `OBJ_SHRIMP_BANK`: where a net can be thrown.
pub const FISH_AREA: u16 = 3740;
pub const NET_SPOTS: [u16; 4] = [FISH_AREA, 1992, 1990, 1991];
/// `e_Trigger.PESCAINVALIDA`.
pub const TRIGGER_NO_FISHING: u16 = 10;
/// `MAXSKILLPOINTS`.
pub const MAX_SKILL: u8 = 100;
/// `NONEXPERT_SKILL_CUTOFF` (the expert pendant is not ported).
pub const SKILL_CUTOFF: i32 = 10;
/// `SubirSkill`: `BonusExp = 5 * ExpMult`.
pub const SKILL_UP_EXP: u32 = 5;
/// `[JOBEXP] MiningExp / FellingExp / FishingExp` of Configuracion.ini (all 1).
pub const JOB_EXP: u32 = 1;

/// What a working tool does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Job {
    Chop,
    Mine,
    Fish,
    Smelt,
}

impl Job {
    /// `UseInvItem` (InvUsuario.bas): which work a tool subtype asks a target for.
    pub fn for_tool(subtype: i32) -> Option<Job> {
        match subtype {
            TOOL_FISHING_ROD | TOOL_FISHING_NET => Some(Job::Fish),
            TOOL_FELLING_AXE => Some(Job::Chop),
            TOOL_MINER_PICKAXE => Some(Job::Mine),
            _ => None,
        }
    }

    pub fn skill(self) -> usize {
        match self {
            Job::Chop => SKILL_WOODCUTTING,
            Job::Fish => SKILL_FISHING,
            Job::Mine | Job::Smelt => SKILL_MINING,
        }
    }

    /// `SkillsNames`.
    pub fn skill_name(self) -> &'static str {
        match self {
            Job::Chop => "Talar",
            Job::Fish => "Pescar",
            Job::Mine | Job::Smelt => "Minería",
        }
    }
}

/// `Distancia` on one map: Manhattan.
pub fn distance(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    (ax - bx).abs() + (ay - by).abs()
}

/// `Suerte = Int(-0.00125 * skill^2 - 0.3 * skill + 49)` (MineMinerals / ChopWood).
pub fn luck(skill: u8) -> i32 {
    let s = skill as f64;
    (-0.00125 * s * s - 0.3 * s + 49.0).floor() as i32
}

/// Upper bound of the success roll `RandomNumber(1, max)`: safe maps add 2 (mining) or 4
/// (woodcutting).
pub fn gather_roll_max(job: Job, skill: u8, safe_map: bool) -> i32 {
    let l = luck(skill);
    match (job, safe_map) {
        (Job::Mine, true) => l + 2,
        (Job::Chop, true) => l + 4,
        _ => l,
    }
}

/// Success of a gathering roll: `res <= 5` mining, `res < 6` woodcutting (the same).
pub fn gather_hit(roll: i32) -> bool {
    roll <= 5
}

/// Amount range of one successful extraction (`GetExtractResourceForLevel` for the Worker
/// class, `RandomNumber(1, 2)` otherwise), before `RecoleccionMult` (1).
pub fn extract_range(worker: bool, level: u32) -> (i32, i32) {
    if worker {
        let l = level as f64 + 0.000001;
        ((l / 3.6).floor() as i32, (l / 2.0).floor() as i32)
    } else {
        (1, 2)
    }
}

/// `GetFishingChance`: percent of a successful cast.
pub fn fishing_chance(skill: u8) -> i32 {
    match skill {
        0..=19 => 20,
        20..=39 => 35,
        40..=69 => 55,
        70..=99 => 68,
        _ => 80,
    }
}

/// Catch size: the Worker gets 2..6 with a net and 1..3 with a rod, everybody else 1.
pub fn catch_range(worker: bool, net: bool) -> (i32, i32) {
    match (worker, net) {
        (true, true) => (2, 6),
        (true, false) => (1, 3),
        _ => (1, 1),
    }
}

/// `MineralsRequiredPerIngot`: ore spent to make `cant` ingots.
pub fn minerals_per_ingot(mineral: u16, cant: u32) -> u32 {
    match mineral {
        IRON_ORE => 13 * cant,
        SILVER_ORE => 25 * cant,
        GOLD_ORE => 50 * cant,
        _ => 10_000,
    }
}

/// `ActualizarRecurso`: a used resource is back to full once `regen_secs` passed since its
/// last use (`last_use_ms` None = never used: nothing to refill).
pub fn refilled(amount: u32, full: u32, last_use_ms: Option<u64>, now_ms: u64, regen_secs: i32) -> (u32, bool) {
    match last_use_ms {
        Some(t) if now_ms.saturating_sub(t) as f64 / 1000.0 > regen_secs as f64 => (full, true),
        _ => (amount, false),
    }
}

/// `SubirSkill` preconditions: below 100, below the level cap, not starving or thirsty.
pub fn can_raise_skill(value: u8, level: u32, food: i32, water: i32) -> bool {
    value < MAX_SKILL && (value as u32) < crate::progression::skill_cap_for_level(level) && food > 0 && water > 0
}

/// `SubirSkill`: upper bound of `Aumenta = RandomNumber(1, Prob * DificultadSubirSkill)`;
/// the skill rises when the roll is below [`SKILL_CUTOFF`].
pub fn skill_roll_max(level: u32, difficulty: i32) -> i32 {
    let prob = (0.1 * (level as f64).powi(2) + 15.0).floor() as i32;
    (prob * difficulty.max(1)).max(1)
}

// ---------------------------------------------------------------- fish (pesca.dat)

#[derive(Clone, Debug, PartialEq)]
pub struct Fish {
    pub obj: u16,
    /// Cumulative weight after sorting by rod level (`Peces(i).data` after LoadPesca).
    pub cum: i64,
    pub level: i32,
    /// `D=1`: special fish (the VB starts a mini game; see systems/work.rs).
    pub special: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FishTable {
    pub fish: Vec<Fish>,
    /// `PesoPeces(0..=MaxLvlCania)`.
    pub weights: Vec<i64>,
    /// `PoderCanas(1..)` (index 0 unused).
    pub rod_power: Vec<i32>,
    /// `[Pesca]` of RecursosEspeciales.dat: (object, one in N).
    pub specials: Vec<(u16, i32)>,
}

impl FishTable {
    /// `LoadPesca` (pesca.dat) + `LoadRecursosEspeciales` (Pesca section).
    pub fn parse(pesca: &[u8], especiales: &[u8]) -> FishTable {
        let ini = Ini::parse(pesca);
        let sec = ini.section("PECES");
        let get = |k: &str| sec.and_then(|s| s.get(k)).cloned().unwrap_or_default();
        let count: usize = get("numpeces").trim().parse().unwrap_or(0);
        let max_lvl: usize = sec.and_then(|s| s.iter().find(|(k, _)| k.starts_with("maxlvlca")).map(|(_, v)| v.trim().parse().unwrap_or(0))).unwrap_or(0);
        let powers = ini.section("POWERCANAS");
        let mut rod_power = vec![0; max_lvl + 1];
        for (i, p) in rod_power.iter_mut().enumerate().skip(1) {
            *p = powers.and_then(|s| s.get(&format!("power{i}"))).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        }
        let mut fish = Vec::new();
        for i in 1..=count {
            let line = get(&format!("pez{i}"));
            let f: Vec<i64> = line.split('-').map(|x| x.trim().parse().unwrap_or(0)).collect();
            if f.len() < 4 {
                continue;
            }
            fish.push(Fish { obj: f[0] as u16, cum: f[1], level: (f[2] as i32).min(max_lvl as i32), special: f[3] == 1 });
        }
        // QuickSortPeces is not stable; a stable sort gives the same bands per level.
        fish.sort_by_key(|f| f.level);
        let mut weights = vec![0i64; max_lvl + 1];
        for f in fish.iter_mut() {
            for w in weights.iter_mut().skip(f.level.max(0) as usize) {
                *w += f.cum;
            }
            f.cum = weights[f.level.max(0) as usize];
        }
        let esp = Ini::parse(especiales);
        let ps = esp.section("Pesca");
        let n: usize = ps.and_then(|s| s.get("items")).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        let specials = (1..=n)
            .filter_map(|i| {
                let v = ps?.get(&format!("item{i}"))?;
                let (o, p) = v.split_once('-')?;
                Some((o.trim().parse().ok()?, p.trim().parse().ok()?))
            })
            .collect();
        FishTable { fish, weights, rod_power, specials }
    }

    /// `ObtenerPezRandom`: the roll range `[lo, hi]` (inclusive) for a rod power; only the
    /// fish of exactly that rod level can come out. None if that band is empty.
    pub fn roll_range(&self, power: i32) -> Option<(i64, i64)> {
        if self.weights.is_empty() {
            return None;
        }
        let p = power.clamp(0, self.weights.len() as i32 - 1) as usize;
        let max = self.weights[p];
        let min = if p > 0 { self.weights[p - 1] } else { 0 };
        if max <= 0 {
            return None;
        }
        Some(if max <= min { (0, max - 1) } else { (min, max - 1) })
    }

    /// `BinarySearchPeces`: the fish whose cumulative band holds `value`.
    pub fn fish_at(&self, value: i64) -> Option<&Fish> {
        let mut prev = 0;
        for f in &self.fish {
            if value >= prev && value < f.cum {
                return Some(f);
            }
            prev = f.cum;
        }
        None
    }
}

// ---------------------------------------------------------------- map objects (.csm)

/// One entry of the `.csm` objects section (`t_DatosObjs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapObject {
    pub x: i32,
    pub y: i32,
    pub obj: u16,
    pub amount: u16,
}

/// Reads the objects section of a `.csm` file (same layout as `ao_formats::csm`, which skips
/// it). TODO(integrator): move into `ao_formats::csm::AoMap` as an `objects` field.
pub fn parse_map_objects(data: &[u8]) -> Result<Vec<MapObject>, &'static str> {
    struct R<'a>(&'a [u8], usize);
    impl R<'_> {
        fn take(&mut self, n: usize) -> Result<&[u8], &'static str> {
            let s = self.0.get(self.1..self.1 + n).ok_or("truncado")?;
            self.1 += n;
            Ok(s)
        }
        fn i32(&mut self) -> Result<i32, &'static str> {
            Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
        }
        fn i16(&mut self) -> Result<i16, &'static str> {
            Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
        }
        fn string(&mut self) -> Result<(), &'static str> {
            let n = u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as usize;
            self.take(n).map(|_| ())
        }
        fn count(&mut self) -> Result<usize, &'static str> {
            let n = self.i32()?;
            if (0..=10_000).contains(&n) { Ok(n as usize) } else { Err("cuenta invalida") }
        }
    }
    let mut r = R(data, 0);
    let first = r.i32()?;
    let mut layers = [0usize; 5];
    let blocked = if first == 0x324C_3557 {
        let b = r.count()?;
        for l in layers.iter_mut() {
            *l = r.count()?;
        }
        b
    } else {
        if !(0..=10_000).contains(&first) {
            return Err("cabecera invalida");
        }
        for l in [0usize, 1, 3, 4] {
            layers[l] = r.count()?;
        }
        first as usize
    };
    let triggers = r.count()?;
    let lights = r.count()?;
    let particles = r.count()?;
    let _npcs = r.count()?;
    let objs = r.count()?;
    let _exits = r.count()?;
    // Map size + tMapDat (see ao_formats::csm::AoMap::parse).
    r.take(8)?;
    r.string()?;
    r.take(1)?;
    r.string()?;
    r.take(8)?;
    r.take(1)?;
    r.string()?;
    r.string()?;
    r.string()?;
    r.take(16)?;
    r.string()?;
    r.take(3)?;
    let skip = blocked * 5 + layers.iter().sum::<usize>() * 8 + triggers * 6 + particles * 8 + lights * 9;
    r.take(skip)?;
    let mut out = Vec::with_capacity(objs);
    for _ in 0..objs {
        let (x, y, obj, amount) = (r.i16()? as i32, r.i16()? as i32, r.i16()?, r.i16()?);
        if (1..=100).contains(&x) && (1..=100).contains(&y) && obj > 0 {
            out.push(MapObject { x, y, obj: obj as u16, amount: amount.max(0) as u16 });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luck_and_hits_follow_the_vb_formula() {
        assert_eq!(luck(0), 49);
        assert_eq!(luck(100), 6); // -12.5 - 30 + 49 = 6.5
        assert_eq!(gather_roll_max(Job::Mine, 0, true), 51);
        assert_eq!(gather_roll_max(Job::Chop, 0, true), 53);
        assert!(gather_hit(5) && !gather_hit(6));
    }

    #[test]
    fn worker_amounts_by_level() {
        assert_eq!(extract_range(true, 1), (0, 0));
        assert_eq!(extract_range(true, 10), (2, 5));
        assert_eq!(extract_range(true, 47), (13, 23));
        assert_eq!(extract_range(false, 47), (1, 2));
    }

    #[test]
    fn ingots_and_fishing_chance() {
        assert_eq!(minerals_per_ingot(IRON_ORE, 10), 130);
        assert_eq!(minerals_per_ingot(GOLD_ORE, 20), 1000);
        assert_eq!(minerals_per_ingot(1, 1), 10_000);
        assert_eq!(fishing_chance(0), 20);
        assert_eq!(fishing_chance(100), 80);
    }

    #[test]
    fn refill_only_after_regen_time() {
        assert_eq!(refilled(0, 100, None, 10_000_000, 1), (0, false));
        assert_eq!(refilled(3, 100, Some(0), 3_000_000, 3000), (3, false));
        assert_eq!(refilled(3, 100, Some(0), 3_000_001, 3000), (100, true));
    }

    #[test]
    fn skill_raise_limits() {
        assert!(can_raise_skill(0, 1, 100, 100));
        assert!(!can_raise_skill(3, 1, 100, 100)); // level 1 cap is 3
        assert!(!can_raise_skill(0, 1, 0, 100));
        assert_eq!(skill_roll_max(1, 1), 15);
        assert_eq!(skill_roll_max(10, 2), 50);
    }

    #[test]
    fn fish_table_bands_by_rod_level() {
        let pesca = b"[PECES]\nNumPeces=3\nMaxlvlca\xf1a=2\nPez1=10-30-1-0\nPez2=11-70-1-0\nPez3=12-50-2-1\n[POWERCANAS]\nPower1=2\nPower2=4\n";
        let t = FishTable::parse(pesca, b"[Pesca]\nItems=1\nItem1=372-150000\n");
        assert_eq!(t.weights, vec![0, 100, 150]);
        assert_eq!(t.roll_range(1), Some((0, 99)));
        assert_eq!(t.roll_range(2), Some((100, 149)));
        assert_eq!(t.fish_at(29).unwrap().obj, 10);
        assert_eq!(t.fish_at(30).unwrap().obj, 11);
        assert_eq!(t.fish_at(149).unwrap().obj, 12);
        assert_eq!(t.specials, vec![(372, 150000)]);
        assert_eq!(t.rod_power, vec![0, 2, 4]);
    }

    #[test]
    fn official_data_and_map_objects() {
        let root = ao_formats::paths::recursos_dir();
        let Ok(p) = std::fs::read(root.join("Dat").join("pesca.dat")) else { return };
        let t = FishTable::parse(&p, &std::fs::read(root.join("Dat").join("RecursosEspeciales.dat")).unwrap_or_default());
        assert_eq!(t.fish.len(), 34);
        assert_eq!(t.weights.len(), 7);
        assert!(t.roll_range(1).is_some());
        let map = std::fs::read(ao_formats::csm::map_path(&root, 1)).unwrap();
        let objs = parse_map_objects(&map).unwrap();
        // Ullathorpe: the forge at (32, 62). Trees: obj 59 in the town on the public repo
        // maps; 5, 14, 18 and 50 (palms) in the woods on the official client's.
        assert!(objs.iter().any(|o| (o.x, o.y, o.obj) == (32, 62, 383)));
        let trees = (1..=40u16).filter_map(|m| std::fs::read(ao_formats::csm::map_path(&root, m)).ok()).filter_map(|d| parse_map_objects(&d).ok()).flatten().filter(|o| matches!(o.obj, 5 | 14 | 18 | 50 | 59)).count();
        assert!(trees > 20, "choppable trees on the first maps: {trees}");
    }
}

//! TSAO progression data (written by tools/tsao/progress.py):
//! - `Premios.dat`: prizes bought with tournament points (TCP_HandleData1.bas `SPX`);
//! - `ItemsDonaciones.dat`: packs bought with donation credits (TCP_HandleData4.bas `DRX`),
//!   whose pseudo-objects 9995-9999 become typed [`PackReward`]s;
//! - `CanjeTS.dat`: the TS point shop (`FTSPTS`, hard-coded in the VB);
//! - `ClassBonus.dat`: the choices of levels 53/56/60 with their effect;
//! - `LevelPoints.dat`: points for reaching a level;
//! - `Consultas.dat`: the poll open at start-up.
use crate::ini::Ini;
use std::collections::HashMap;
use std::path::Path;

/// `(min, max)`.
pub type Range = (i32, i32);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prize {
    pub name: String,
    pub obj: u16,
    /// Tournament points per unit (`Requiere`).
    pub price: u32,
    pub hit: Range,
    pub def: Range,
    pub magic_hit: Range,
    pub magic_def: Range,
    pub desc: String,
}

/// `Mejorados.dat`: object `requires` + the Octarine gem become `prize.obj`
/// (InvUsuario.bas otGemaOctarina, TCP_HandleData1.bas `SPÑ`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Upgrade {
    pub requires: u16,
    /// What it becomes and its stats (`price` unused).
    pub result: Prize,
}

pub fn parse_upgrades(ini: &Ini) -> Vec<Upgrade> {
    (1..=count(ini.section("init"), "nummejorados"))
        .filter_map(|i| {
            let s = ini.section(&format!("item{i}"));
            s?;
            let result = Prize {
                name: get(s, "nombre").trim().to_string(),
                obj: int(s, "numobj") as u16,
                price: 0,
                hit: range(s, "ataqueminimo", "ataquemaximo"),
                def: range(s, "defensaminima", "defensamaxima"),
                magic_hit: range(s, "ataquemagicominimo", "ataquemagicomaximo"),
                magic_def: range(s, "defensamagicaminima", "defensamagicamaxima"),
                desc: get(s, "descripcion").to_string(),
            };
            Some(Upgrade { requires: int(s, "requiere") as u16, result })
        })
        .filter(|u| u.requires > 0 && u.result.obj > 0)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackReward {
    Item { obj: u16, amount: u32 },
    /// 9999.
    TournamentPoints(u32),
    /// 9998 / 9997: the red / golden dragon mount spells (52 / 51).
    Spell(u16),
    /// 9996: premium months. No premium system: packs with it cannot be bought.
    Premium(u32),
    /// 9995: a skin for an object. No skin system: packs with it cannot be bought.
    Skin { obj: u16, graphic: u32 },
}

impl PackReward {
    pub fn supported(&self) -> bool {
        !matches!(self, PackReward::Premium(_) | PackReward::Skin { .. })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pack {
    pub name: String,
    pub desc: String,
    /// Credits (`Valor`).
    pub price: u32,
    /// Preview: body for tall / short races (`Body` / `BodyB`) and aura.
    pub body: u16,
    pub body_short: u16,
    pub aura: u16,
    pub rewards: Vec<PackReward>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TsItem {
    pub obj: u16,
    pub amount: u32,
    pub price: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BonusEffect {
    /// Max HP points.
    Hp,
    /// The rest are tenths of a percent.
    Evasion,
    Shield,
    WeaponHit,
    WeaponDamage,
    ArrowHit,
    ArrowDamage,
    MagicResist,
    MagicDamage,
    /// Points off the stab roll range (Trabajo.bas).
    Stab,
}

impl BonusEffect {
    pub fn parse(s: &str) -> Option<BonusEffect> {
        use BonusEffect::*;
        Some(match s {
            "Hp" => Hp,
            "Evasion" => Evasion,
            "Shield" => Shield,
            "WeaponHit" => WeaponHit,
            "WeaponDamage" => WeaponDamage,
            "ArrowHit" => ArrowHit,
            "ArrowDamage" => ArrowDamage,
            "MagicResist" => MagicResist,
            "MagicDamage" => MagicDamage,
            "Stab" => Stab,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BonusOption {
    pub effect: BonusEffect,
    pub amount: i32,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct ClassBonus {
    /// Levels of the three choices (53, 56, 60).
    pub levels: Vec<u32>,
    /// Class (accent-free lower case Spanish name) -> tier -> the two options.
    pub classes: HashMap<String, Vec<[BonusOption; 2]>>,
}

impl ClassBonus {
    pub fn options(&self, class: &str, tier: usize) -> Option<&[BonusOption; 2]> {
        self.classes.get(&crate::dat::chests::fold(class))?.get(tier)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelPoints {
    pub level: u32,
    pub tournament: u32,
    pub ts: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Poll {
    pub question: String,
    pub level: u32,
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct TsaoProgress {
    pub prizes: Vec<Prize>,
    pub packs: Vec<Pack>,
    pub ts_shop: Vec<TsItem>,
    pub bonus: ClassBonus,
    pub level_points: Vec<LevelPoints>,
    pub poll: Option<Poll>,
    pub upgrades: Vec<Upgrade>,
}

fn get<'a>(s: Option<&'a HashMap<String, String>>, k: &str) -> &'a str {
    s.and_then(|s| s.get(k)).map(String::as_str).unwrap_or("")
}

fn int(s: Option<&HashMap<String, String>>, k: &str) -> i64 {
    get(s, k).trim().parse().unwrap_or(0)
}

fn range(s: Option<&HashMap<String, String>>, lo: &str, hi: &str) -> Range {
    (int(s, lo) as i32, int(s, hi) as i32)
}

fn count(s: Option<&HashMap<String, String>>, k: &str) -> usize {
    int(s, k).clamp(0, 1000) as usize
}

pub fn parse_prizes(ini: &Ini) -> Vec<Prize> {
    (1..=count(ini.section("init"), "numpremios"))
        .filter_map(|i| {
            let s = ini.section(&format!("premio{i}"));
            s?;
            Some(Prize {
                name: get(s, "nombre").to_string(),
                obj: int(s, "numobj") as u16,
                price: int(s, "requiere").max(0) as u32,
                hit: range(s, "ataqueminimo", "ataquemaximo"),
                def: range(s, "defensaminima", "defensamaxima"),
                magic_hit: range(s, "ataquemagicominimo", "ataquemagicomaximo"),
                magic_def: range(s, "defensamagicaminima", "defensamagicamaxima"),
                desc: get(s, "descripcion").to_string(),
            })
        })
        .filter(|p| p.obj > 0 && p.price > 0)
        .collect()
}

pub fn parse_packs(ini: &Ini) -> Vec<Pack> {
    (1..=count(ini.section("init"), "numpremios"))
        .filter_map(|i| {
            let s = ini.section(&format!("item{i}"));
            s?;
            let rewards = (1..=count(s, "numobjetos"))
                .filter_map(|j| {
                    let f: Vec<i64> = get(s, &format!("obj{j}")).split('-').map(|p| p.trim().parse().unwrap_or(0)).collect();
                    let (code, a, b) = (*f.first()?, f.get(1).copied().unwrap_or(0).max(0), f.get(2).copied().unwrap_or(0).max(0));
                    Some(match code {
                        9999 => PackReward::TournamentPoints(a as u32),
                        9998 => PackReward::Spell(52),
                        9997 => PackReward::Spell(51),
                        9996 => PackReward::Premium(a as u32),
                        9995 => PackReward::Skin { obj: a as u16, graphic: b as u32 },
                        o if o > 0 && a > 0 => PackReward::Item { obj: o as u16, amount: a as u32 },
                        _ => return None,
                    })
                })
                .collect();
            Some(Pack {
                name: get(s, "nombre").to_string(),
                desc: get(s, "descripcion").to_string(),
                price: int(s, "valor").max(0) as u32,
                body: int(s, "body") as u16,
                body_short: int(s, "bodyb") as u16,
                aura: int(s, "aura") as u16,
                rewards,
            })
        })
        .collect()
}

pub fn parse_ts_shop(ini: &Ini) -> Vec<TsItem> {
    (1..=count(ini.section("init"), "num"))
        .map(|i| {
            let s = ini.section(&format!("item{i}"));
            TsItem { obj: int(s, "obj") as u16, amount: int(s, "amount").max(1) as u32, price: int(s, "price").max(0) as u32 }
        })
        .filter(|t| t.obj > 0 && t.price > 0)
        .collect()
}

pub fn parse_bonus(ini: &Ini) -> ClassBonus {
    let levels = get(ini.section("init"), "levels").split(',').filter_map(|l| l.trim().parse().ok()).collect::<Vec<u32>>();
    let mut classes = HashMap::new();
    for (name, s) in ini.sections() {
        if name == "init" {
            continue;
        }
        let opt = |tier: usize, o: usize| -> Option<BonusOption> {
            let v = s.get(&format!("tier{tier}option{o}"))?;
            let mut f = v.splitn(3, ':');
            let effect = BonusEffect::parse(f.next()?.trim())?;
            let amount = f.next()?.trim().parse().ok()?;
            Some(BonusOption { effect, amount, text: f.next().unwrap_or("").trim().to_string() })
        };
        let tiers: Vec<[BonusOption; 2]> = (1..=levels.len()).map_while(|t| Some([opt(t, 1)?, opt(t, 2)?])).collect();
        if tiers.len() == levels.len() {
            classes.insert(crate::dat::chests::fold(name), tiers);
        }
    }
    ClassBonus { levels, classes }
}

pub fn parse_level_points(ini: &Ini) -> Vec<LevelPoints> {
    (1..=count(ini.section("init"), "num"))
        .map(|i| {
            let s = ini.section(&format!("level{i}"));
            LevelPoints { level: int(s, "level").max(0) as u32, tournament: int(s, "tournament").max(0) as u32, ts: int(s, "ts").max(0) as u32 }
        })
        .filter(|l| l.level > 0)
        .collect()
}

pub fn parse_poll(ini: &Ini) -> Option<Poll> {
    let s = ini.section("init");
    let question = get(s, "question").trim().to_string();
    let options: Vec<String> = (1..=count(s, "options")).map(|i| get(s, &format!("option{i}")).trim().to_string()).filter(|o| !o.is_empty()).collect();
    (!question.is_empty() && options.len() >= 2).then(|| Poll { question, level: int(s, "level").max(1) as u32, options })
}

/// Everything, from `<recursos>/Dat` (empty without the files: an AO20 resource set).
pub fn load(recursos: &Path) -> TsaoProgress {
    let dat = |f: &str| Ini::load(&recursos.join("Dat").join(f));
    TsaoProgress {
        prizes: parse_prizes(&dat("Premios.dat")),
        packs: parse_packs(&dat("ItemsDonaciones.dat")),
        ts_shop: parse_ts_shop(&dat("CanjeTS.dat")),
        bonus: parse_bonus(&dat("ClassBonus.dat")),
        level_points: parse_level_points(&dat("LevelPoints.dat")),
        poll: parse_poll(&dat("Consultas.dat")),
        upgrades: parse_upgrades(&dat("Mejorados.dat")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_type_their_pseudo_objects() {
        let ini = Ini::parse(b"[INIT]\nNumPremios=1\n[ITEM1]\nNombre=Pack\nValor=50\nNumObjetos=4\nObj1=1055-1\nObj2=9999-300\nObj3=9998-1\nObj4=9996-1\n");
        let p = &parse_packs(&ini)[0];
        assert_eq!(p.price, 50);
        assert_eq!(p.rewards, vec![PackReward::Item { obj: 1055, amount: 1 }, PackReward::TournamentPoints(300), PackReward::Spell(52), PackReward::Premium(1)]);
        assert!(!p.rewards.iter().all(PackReward::supported));
    }

    #[test]
    fn bonus_needs_every_tier() {
        let ini = Ini::parse("[INIT]\nLevels=53,56\n[MAGO]\nTier1Option1=Evasion:40:Aumenta tu evasion.\nTier1Option2=Hp:3:Aumenta en 3 puntos tu vida.\nTier2Option1=MagicDamage:15:x\nTier2Option2=MagicResist:15:y\n[ROTO]\nTier1Option1=Hp:1:z\n".as_bytes());
        let b = parse_bonus(&ini);
        assert_eq!(b.levels, vec![53, 56]);
        assert_eq!(b.options("Mago", 0).unwrap()[1].amount, 3);
        assert!(b.options("roto", 0).is_none());
    }
}

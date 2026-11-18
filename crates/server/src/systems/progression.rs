//! Attributes, skills, level-up gains, character creation values. OWNER: progression
//! system (P1, Sonnet 1).
//!
//! Ported from the VB6 server: `ConnectNewUser` + `RellenarInventario` (TCP.bas),
//! `CheckUserLevel` (Modulo_UsUaRiOs.bas) and `HandleModifySkills` (Protocol.bas). The pure
//! numbers live in `ao_rules::{progression, character}`; this file is the glue with the
//! world (messages, persistence pieces, anti-cheat).
use crate::db::CharacterState;
use crate::world::inventory::MAX_INVENTORY_SLOTS;
use crate::world::World;
use ao_formats::dat::balance::{Class, Race};
use ao_protocol::{Tone, S2C};
use ao_rules::character::{self, StartEquipment, START_FOOD_WATER, START_SKILL_POINTS};
use ao_rules::progression::{self as rules, AssignError, MAX_SKILL};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tracing::warn;

/// FX played on a level up (`PrepareMessageCreateFX(charindex, 106, 0, ...)`).
const LEVEL_UP_FX: u16 = 106;

/// `EquipamientoInicial.dat`, parsed once per `Recursos` folder.
fn start_equipment(recursos: &std::path::Path) -> Arc<StartEquipment> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<StartEquipment>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap();
    cache
        .entry(recursos.to_path_buf())
        .or_insert_with(|| {
            let path = recursos.join("Dat").join("EquipamientoInicial.dat");
            match std::fs::read(&path) {
                Ok(bytes) => Arc::new(StartEquipment::parse(&String::from_utf8_lossy(&bytes))),
                Err(e) => {
                    warn!(path = %path.display(), ?e, "no se pudo leer el equipamiento inicial");
                    Arc::new(StartEquipment::default())
                }
            }
        })
        .clone()
}

/// Hit points and mana of a brand-new character (they live in the character record, not in
/// `CharacterState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct StartVitals {
    pub hp: i32,
    pub mana: i32,
}

impl World {
    /// Hook (contract): fills a brand-new character's stats (attributes by race, initial
    /// stamina by class, starting skill points, items, spells) before its first save.
    ///
    /// The VB rolls the stamina dice (`RandomNumber(1, Agilidad \ 6)`); this hook takes `&self`
    /// so it derives the roll from the game clock. Wiring with `self.rng` is cleaner: see
    /// [`World::init_new_character_with_roll`]. Hit points and mana go in the character
    /// record: use [`World::new_character_vitals`].
    #[allow(dead_code)]
    pub(crate) fn init_new_character(&self, race: u8, class: u8, gender: u8) -> Option<CharacterState> {
        let attributes = Race::from_id(race as i32).map(|r| character::initial_attributes(&self.ao.balance, r))?;
        let mut h = self.now().0 ^ 0x9E37_79B9_7F4A_7C15 ^ ((race as u64) << 8 | (class as u64) << 16 | (gender as u64) << 24);
        h ^= h << 13;
        h ^= h >> 7;
        h ^= h << 17;
        let roll = 1 + (h % character::stamina_dice(&attributes) as u64) as i32;
        self.init_new_character_with_roll(race, class, gender, roll)
    }

    /// Same as [`World::init_new_character`] with an explicit stamina dice roll in
    /// `1..=Agilidad \ 6` (a roll of 1 counts as 2, like the VB).
    #[allow(dead_code)]
    pub(crate) fn init_new_character_with_roll(&self, race: u8, class: u8, gender: u8, stamina_roll: i32) -> Option<CharacterState> {
        if !character::valid_gender(gender) {
            return None;
        }
        let (race, class_id) = (Race::from_id(race as i32)?, Class::from_id(class as i32)?);
        let attributes = character::initial_attributes(&self.ao.balance, race);
        let sta = character::initial_stamina(stamina_roll.clamp(1, character::stamina_dice(&attributes)));
        let (inventory, spells) = start_equipment(&self.recursos).loadout_for(class_id.id(), race.id(), MAX_INVENTORY_SLOTS);
        Some(CharacterState {
            attributes,
            skills: vec![self.data.balance.start_skills.min(100); 24],
            skill_points: START_SKILL_POINTS,
            sta,
            max_sta: sta,
            food: START_FOOD_WATER,
            water: START_FOOD_WATER,
            gold: 0,
            bank_gold: 0,
            inventory,
            bank: Vec::new(),
            spells,
        })
    }

    /// Initial hit points and mana for `NewCharacter` (`ConnectNewUser`: `MaxHp = Constitucion`,
    /// `MaxMAN = Inteligencia * ManaInicial`).
    #[allow(dead_code)]
    pub(crate) fn new_character_vitals(&self, race: u8, class: u8) -> Option<StartVitals> {
        let attributes = character::initial_attributes(&self.ao.balance, Race::from_id(race as i32)?);
        let class = Class::from_id(class as i32)?;
        if let Some(s) = self.data.levels.as_ref().and_then(|t| t.start.as_ref()) {
            return Some(StartVitals { hp: s.hp, mana: s.mana_for(class.spanish_name(), attributes[character::ATTR_INTELLIGENCE]) });
        }
        Some(StartVitals { hp: character::initial_hp(&attributes), mana: character::initial_mana(&self.ao.balance, class, &attributes) })
    }

    /// Minimum and maximum hit bonus of a player (`MinHIT` / `MaxHit`: they only depend on
    /// class and level, so nothing is stored). For the combat system.
    #[allow(dead_code)]
    pub(crate) fn hit_range_of(&self, id: u32) -> (i32, i32) {
        let Some(p) = self.player(id) else { return (1, 2) };
        Class::from_id(p.class as i32).map_or((1, 2), |c| rules::hit_range(&self.ao.balance, c, p.level))
    }

    /// Hook (contract): called by combat's `give_exp` after `levels` level-ups (`p.level` and
    /// `p.exp` are already updated). Applies, level by level, `CheckUserLevel`: maximum mana,
    /// stamina and hit from class/attributes/level, a random hit-point gain around the class
    /// average, full health, free skill points, FX and messages.
    #[allow(dead_code)]
    pub(crate) fn on_level_up(&mut self, id: u32, levels: u32) {
        let Some(p) = self.player(id) else { return };
        let Some(class) = Class::from_id(p.class as i32) else { return };
        let ao = self.ao.clone();
        let bal = &ao.balance;
        let new_level = p.level.min(bal.max_level());
        let levels = levels.min(new_level.saturating_sub(1));
        if levels == 0 {
            return;
        }
        let first = new_level - levels + 1;
        let (constitution, intelligence) = (p.stats.attributes[character::ATTR_CONSTITUTION], p.stats.attributes[character::ATTR_INTELLIGENCE]);
        let (mut max_hp, mut max_mana, mut max_sta) = (p.max_hp, p.max_mana, p.stats.max_sta);
        let mut lines: Vec<(String, Tone)> = Vec::new();
        // TSAO table (data/levels.toml) for this class and race, if the data has one.
        let race_name = Race::from_id(p.race as i32).map(|r| r.spanish_name()).unwrap_or("");
        let table = self.data.levels.as_ref().and_then(|t| t.class(class.spanish_name())).and_then(|c| Some((c.clone(), c.race(race_name)?.clone())));
        let rng = &mut self.rng;
        for level in first..=new_level {
            let (old_sta, old_mana) = (max_sta, max_mana);
            let gain = if let Some((c, r)) = &table {
                max_mana = max_mana.saturating_add(c.mana_gain(intelligence));
                max_sta = (max_sta + c.sta).min(30_000);
                r.hp_gain(level, max_hp, &mut |lo, hi| if hi <= lo { lo } else { rng.range(lo, hi) })
            } else {
                let mut rnd = || rng.next_u32() as f64 / 4_294_967_296.0;
                max_mana = rules::max_mana(bal, class, intelligence, level);
                max_sta = rules::max_stamina(bal, class, level);
                rules::hp_gain(bal, class, constitution, level, max_hp, &mut rnd)
            };
            max_hp += gain;
            let (hit_min, hit_max) = rules::hit_range(bal, class, level);
            let (old_min, _) = rules::hit_range(bal, class, level - 1);
            lines.push((format!("Has subido al nivel {level}!"), Tone::Info));
            if gain > 0 {
                lines.push((format!("Has ganado {gain} puntos de vida. Tu vida actual es: {max_hp}"), Tone::Good));
            }
            if max_sta > old_sta {
                lines.push((format!("Has ganado {} puntos de energia. Tu energia actual es: {max_sta}", max_sta - old_sta), Tone::Good));
            }
            if max_mana > old_mana {
                lines.push((format!("Has ganado {} puntos de mana. Tu mana actual es: {max_mana}", max_mana - old_mana), Tone::Good));
            }
            if hit_min > old_min {
                lines.push((format!("Tu golpe minimo y maximo aumentaron en {} puntos. Tus danos actuales son {hit_min} / {hit_max}", hit_min - old_min), Tone::Combat));
            }
        }
        let skill_points = rules::skill_points_per_level(bal, class) as u32 * levels;
        let p = self.player_mut(id);
        p.max_hp = max_hp;
        p.hp = max_hp;
        p.max_mana = max_mana;
        p.mana = p.mana.min(max_mana);
        p.stats.max_sta = max_sta;
        p.stats.sta = p.stats.sta.min(max_sta);
        p.stats.skill_points = p.stats.skill_points.saturating_add(skill_points.min(u16::MAX as u32) as u16);
        let (map, hp, free) = (p.map, p.hp, p.stats.skill_points);
        let fx = ao_formats::profile::for_recursos(&self.recursos).fx.get("levelup").copied().unwrap_or(LEVEL_UP_FX);
        self.broadcast(map, &S2C::Fx { id, fx, loops: 0 }, None);
        for (text, tone) in lines {
            self.console(id, text, tone);
        }
        if skill_points > 0 {
            self.console(id, format!("Has ganado {skill_points} skillpoints! Dispones de {free} puntos libres, se cuidadoso al momento de usarlos."), Tone::Good);
        }
        self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
        self.send_stats(id);
        self.send_vitals(id);
        self.send_skills(id);
        self.level_rewards(id, first, new_level);
    }

    /// What levels `first..=last` give: TSAO gold below level 10, spells at a level, the
    /// newbie items taken away when the player stops being a newbie (QuitarNewbieObj) and
    /// the graduation out of the newbie area (warp + starter chest).
    fn level_rewards(&mut self, id: u32, first: u32, last: u32) {
        let newbie_max = self.data.balance.newbie_max_level;
        if first <= newbie_max + 1 && last > newbie_max {
            self.remove_newbie_items(id);
        }
        let Some(r) = self.data.levels.as_ref().map(|t| t.rewards.clone()) else { return };
        for level in first..=last {
            if level < r.gold_below_level && r.gold_per_level > 0 {
                let gold = r.gold_per_level.saturating_mul(level);
                let p = self.player_mut(id);
                p.inv.gold = p.inv.gold.saturating_add(gold);
                self.send_inventory_vitals(id);
                self.console(id, format!("Recibes {gold} monedas de oro por subir de nivel."), Tone::Good);
            }
            for [at, spell] in r.spells.iter().copied() {
                if at == level && self.ao.spells.get(spell).is_some() && !self.player(id).unwrap().spells.knows(spell as u16) && self.player_mut(id).spells.add(spell as u16).is_some() {
                    let name = self.ao.spells.get(spell).unwrap().name.clone();
                    self.console(id, format!("Has aprendido el hechizo {name}."), Tone::Good);
                    self.send_spellbook(id);
                }
            }
            if let Some(g) = r.graduation.as_ref().filter(|g| g.level == level) {
                let short = matches!(Race::from_id(self.player(id).unwrap().race as i32), Some(Race::Dwarf | Race::Gnome));
                let item = if short && g.item_short_races > 0 { g.item_short_races } else { g.item };
                self.console(id, "¡Has dejado de ser newbie! Te llevamos a la ciudad con un paquete de bienvenida.", Tone::Good);
                if !self.add_item(id, crate::world::inventory::InvItem { obj: item, amount: 1 }) {
                    let (m, x, y) = { let p = self.player(id).unwrap(); (p.map, p.x, p.y) };
                    self.drop_to_floor(m, x, y, crate::world::inventory::InvItem { obj: item, amount: 1 }, false);
                }
                self.teleport(id, g.map, g.x, g.y);
            }
        }
    }

    /// QuitarNewbieObj: every newbie object leaves the bag (unequipped first).
    pub(crate) fn remove_newbie_items(&mut self, id: u32) {
        let ao = self.ao.clone();
        let slots: Vec<(u8, u32)> = self.player(id).map_or(Vec::new(), |p| {
            (1..=MAX_INVENTORY_SLOTS as u8).filter_map(|s| p.inv.get(s).filter(|i| ao.objs.get(i.obj as u32).is_some_and(|o| o.newbie == 1)).map(|i| (s, i.amount))).collect()
        });
        for (slot, amount) in &slots {
            self.remove_from_slot(id, *slot, *amount);
        }
        if !slots.is_empty() {
            self.console(id, "Tus objetos de newbie desaparecen.", Tone::Info);
        }
    }

    /// `C2S::AssignSkill` (`HandleModifySkills`): spends free points on one skill. A skill
    /// that does not exist or more points than the player owns is a forged packet; points
    /// that would pass 100 are simply not spent.
    pub(crate) fn on_assign_skill(&mut self, id: u32, skill: u8, points: u16) {
        if !(1..=24).contains(&skill) {
            return self.strike(id, "skill inexistente");
        }
        let p = self.player_mut(id);
        let idx = skill as usize - 1;
        let (current, free) = (p.stats.skills[idx], p.stats.skill_points);
        match rules::assign_skill(current, free, points) {
            Err(AssignError::OverBudget) => self.strike(id, "mas puntos de skill de los que tiene"),
            Ok((_, 0)) if points == 0 => {}
            Ok((new, spent)) => {
                let p = self.player_mut(id);
                p.stats.skills[idx] = new;
                p.stats.skill_points -= spent;
                if spent < points {
                    self.console(id, format!("El skill no puede pasar de {MAX_SKILL}."), Tone::Warn);
                }
                self.send_skills(id);
            }
        }
    }

    /// Everything the client needs about a character that is not in `S2C::Stats`: attributes,
    /// skills, stamina, hunger, thirst and gold. Call it when a player enters the world.
    #[allow(dead_code)]
    pub(crate) fn send_progression(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        self.send(id, &S2C::Attributes { values: p.stats.attributes });
        self.send_skills(id);
        self.send_vitals(id);
    }

    pub(crate) fn send_skills(&self, id: u32) {
        if let Some(p) = self.player(id) {
            self.send(id, &S2C::Skills { values: p.stats.skills.to_vec(), free_points: p.stats.skill_points });
        }
    }

    pub(crate) fn send_vitals(&self, id: u32) {
        if let Some(p) = self.player(id) {
            self.send(id, &S2C::Vitals { sta: p.stats.sta, max_sta: p.stats.max_sta, food: p.stats.food, water: p.stats.water, gold: p.inv.gold });
        }
    }
}

/// Test bench shared by the in-crate tests of the progression and regeneration rules.
#[cfg(test)]
pub(crate) mod testkit {
    use crate::net::Event;
    use crate::world::World;
    use crate::Settings;
    use ao_protocol::{FrameReader, C2S, MAX_SERVER_FRAME, PROTOCOL_VERSION, S2C};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    /// A world with one dev-login player (id 1, human mage) and its message queue.
    pub struct Bench {
        pub world: World,
        pub id: u32,
        rx: UnboundedReceiver<Vec<u8>>,
        reader: FrameReader,
    }

    impl Bench {
        pub fn new() -> Bench {
            let mut world = World::new(Settings::defaults());
            let (tx, rx) = unbounded_channel();
            world.on_event(Event::Connected { id: 1, addr: "127.0.0.1:1".parse().unwrap(), tx });
            world.handle(1, C2S::Login { name: "Prueba".into(), version: PROTOCOL_VERSION });
            assert!(world.player(1).is_some(), "dev login must work in the test bench");
            let mut b = Bench { world, id: 1, rx, reader: FrameReader::default() };
            b.drain();
            b
        }

        /// Every message queued for the player since the last call.
        pub fn drain(&mut self) -> Vec<S2C> {
            while let Ok(bytes) = self.rx.try_recv() {
                self.reader.push(&bytes);
            }
            let mut out = Vec::new();
            while let Some(m) = self.reader.next::<S2C>(MAX_SERVER_FRAME) {
                out.push(m.expect("valid frame"));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::Bench;
    use super::*;
    use ao_rules::character::{ATTR_CONSTITUTION, ATTR_INTELLIGENCE};

    fn bench() -> Option<Bench> {
        ao_formats::paths::recursos_dir().join("Dat").is_dir().then(Bench::new)
    }

    #[test]
    fn new_human_mage_has_official_start_values() {
        let Some(b) = bench() else { return };
        let w = &b.world;
        let s = w.init_new_character_with_roll(1, 1, 1, 3).expect("valid");
        // Attributes: 18 + Human (+1 +1 0 +2 0): Str 19, Agi 19, Int 18, Con 20, Cha 18.
        assert_eq!(s.attributes, [19, 19, 18, 20, 18]);
        assert_eq!(s.skill_points, 10);
        assert_eq!(s.skills, vec![0; 24]);
        assert_eq!((s.food, s.water, s.gold), (100, 100, 0));
        assert_eq!((s.sta, s.max_sta), (60, 60), "dice 3 -> 20 * 3");
        // Common items 1-5, then the mage's: 4336 x850 ... robe 3502 worn.
        assert_eq!(s.inventory.len(), 10);
        assert_eq!(s.inventory[0], (1, 4335, 500, false));
        assert_eq!(s.inventory[9], (10, 3502, 1, true));
        assert_eq!(s.spells, vec![(1, 1), (2, 7), (3, 352)]);
        // Hit points = Constitution (20); mana = round(18 * 8.33) = 150.
        assert_eq!(w.new_character_vitals(1, 1), Some(StartVitals { hp: 20, mana: 150 }));
    }

    #[test]
    fn new_orc_warrior_and_gnome_mage() {
        let Some(b) = bench() else { return };
        let w = &b.world;
        // Orc (+3 +1 -2 +2 -1): Str 21, Agi 19, Int 16, Con 20, Cha 17. Warrior: no mana.
        let s = w.init_new_character_with_roll(6, 3, 2, 1).unwrap();
        assert_eq!(s.attributes, [21, 19, 16, 20, 17]);
        assert_eq!(s.sta, 40, "a dice roll of 1 counts as 2");
        assert_eq!(s.inventory.len(), 15);
        assert!(s.spells.is_empty());
        assert_eq!(w.new_character_vitals(6, 3), Some(StartVitals { hp: 20, mana: 0 }));
        // Gnome (-2 +3 +4 0 +2): Int 22, Con 18; mage mana = round(22 * 8.33) = 183.
        assert_eq!(w.new_character_vitals(4, 1), Some(StartVitals { hp: 18, mana: 183 }));
        assert_eq!(w.init_new_character_with_roll(4, 1, 1, 3).unwrap().attributes, [16, 21, 22, 18, 20]);
    }

    #[test]
    fn invalid_creation_is_refused_and_the_clock_roll_is_in_range() {
        let Some(b) = bench() else { return };
        let w = &b.world;
        assert!(w.init_new_character(0, 1, 1).is_none());
        assert!(w.init_new_character(7, 1, 1).is_none());
        assert!(w.init_new_character(1, 13, 1).is_none());
        assert!(w.init_new_character(1, 1, 3).is_none());
        let s = w.init_new_character(1, 1, 1).unwrap();
        assert!(s.sta == 40 || s.sta == 60);
    }

    #[test]
    fn level_up_applies_class_gains() {
        let Some(mut b) = bench() else { return };
        let id = b.id;
        // A human mage reaching level 2: Int 18, Con 20 (Balance.dat).
        let p = b.world.player_mut(id);
        p.class = 1;
        p.stats.attributes = [19, 19, 18, 20, 18];
        p.max_hp = 20;
        p.hp = 3;
        p.max_mana = 150;
        p.mana = 150;
        p.stats.max_sta = 40;
        p.level = 2;
        b.world.on_level_up(id, 1);
        let p = b.world.player(id).unwrap();
        // Mana: 150 + 2.65 * 18 = 197.7 -> 198. Stamina: 60 + 15. Skill points: +5.
        assert_eq!((p.max_mana, p.stats.max_sta, p.stats.skill_points), (198, 75, 5));
        // Hit points: 20 + (7 +- 2) and full health.
        assert!((25..=29).contains(&p.max_hp), "max hp {}", p.max_hp);
        assert_eq!(p.hp, p.max_hp);
        assert_eq!(b.world.hit_range_of(id), (2, 3));
        let log = b.drain();
        assert!(log.iter().any(|m| matches!(m, S2C::Fx { fx: 106, .. })));
        assert!(log.iter().any(|m| matches!(m, S2C::Skills { free_points: 5, .. })));
        assert!(log.iter().any(|m| matches!(m, S2C::Stats { level: 2, max_mana: 198, .. })));
        assert!(log.iter().any(|m| matches!(m, S2C::Console { text, .. } if text.contains("nivel 2"))));
    }

    #[test]
    fn several_levels_at_once_and_worker_points() {
        let Some(mut b) = bench() else { return };
        let id = b.id;
        let p = b.world.player_mut(id);
        p.class = 9; // Trabajador: 6 skill points per level, no mana, 15 stamina.
        p.stats.attributes = [19, 19, 18, 20, 18];
        p.max_hp = 20;
        p.max_mana = 0;
        p.mana = 0;
        p.level = 11;
        b.world.on_level_up(id, 10);
        let p = b.world.player(id).unwrap();
        assert_eq!(p.stats.skill_points, 60);
        assert_eq!(p.stats.max_sta, 60 + 10 * 15);
        assert_eq!(p.max_mana, 0);
        // Constitution 20, Worker life 8.5: average 8 per level -> 20 + 8*10 = 100 +- 10 (cap).
        assert!((90..=110).contains(&p.max_hp), "max hp {}", p.max_hp);
        let (_, hit_max) = b.world.hit_range_of(id);
        assert_eq!(hit_max, 3 * 10 + 2, "worker hits 3 per level");
        // Never past the cap, and never below level 2 for levels that did not happen.
        b.world.on_level_up(id, 0);
        assert_eq!(b.world.player(id).unwrap().stats.skill_points, 60);
    }

    #[test]
    fn assign_skill_respects_points_and_maximum() {
        let Some(mut b) = bench() else { return };
        let id = b.id;
        b.world.player_mut(id).stats.skill_points = 10;
        b.world.on_assign_skill(id, 5, 4);
        let p = b.world.player(id).unwrap();
        assert_eq!((p.stats.skills[4], p.stats.skill_points), (4, 6));
        assert!(b.drain().iter().any(|m| matches!(m, S2C::Skills { values, free_points: 6 } if values[4] == 4)));
        // Near the cap: only the points up to 100 are spent.
        let p = b.world.player_mut(id);
        p.stats.skills[0] = 98;
        b.world.on_assign_skill(id, 1, 5);
        let p = b.world.player(id).unwrap();
        assert_eq!((p.stats.skills[0], p.stats.skill_points), (100, 4));
        assert!(p.stats.skill_points == 4, "clamping is not punished");
        // Zero points: nothing happens.
        b.world.on_assign_skill(id, 2, 0);
        assert_eq!(b.world.player(id).unwrap().stats.skill_points, 4);
    }

    #[test]
    fn assign_skill_traps_are_strikes() {
        let Some(mut b) = bench() else { return };
        let id = b.id;
        b.world.player_mut(id).stats.skill_points = 3;
        // Dev characters may start with some skills (magic's dev kit): compare with the start.
        let before: u32 = b.world.player(id).unwrap().stats.skills.iter().map(|&s| s as u32).sum();
        b.world.on_assign_skill(id, 7, 4);
        b.world.on_assign_skill(id, 0, 1);
        b.world.on_assign_skill(id, 25, 1);
        b.world.on_assign_skill(id, 255, 1);
        let p = b.world.player(id).unwrap();
        assert_eq!((p.stats.skills[6], p.stats.skill_points), (0, 3), "nothing was spent");
        assert_eq!(p.stats.skills.iter().map(|&s| s as u32).sum::<u32>(), before);
        // Four strikes so far (kick at 8): keep cheating and the connection ends.
        for _ in 0..4 {
            assert!(b.world.player(id).is_some());
            b.world.on_assign_skill(id, 0, 1);
        }
        assert!(b.world.player(id).is_none(), "kicked after repeated cheating");
        assert!(b.drain().iter().any(|m| matches!(m, S2C::Kicked { .. })));
    }

    #[test]
    fn attribute_order_is_the_official_one() {
        assert_eq!((ATTR_INTELLIGENCE, ATTR_CONSTITUTION), (2, 3));
    }

    /// The TSAO data (`AO_TSAO`, default X:\dev\tsao_rec, + tsao/data/levels.toml; skipped
    /// without them): 70 levels, TSAO curve, life from the class/race table, TSAO start
    /// values, gold from GiveGLDMin/Max straight to the purse, the TSAO meditation FX.
    #[test]
    fn tsao_progression_gold_and_meditation() {
        let rec = std::path::PathBuf::from(std::env::var("AO_TSAO").unwrap_or_else(|_| "X:/dev/tsao_rec".into()));
        let table = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tsao/data/levels.toml");
        let (true, Ok(text)) = (rec.join("Dat").is_dir(), std::fs::read_to_string(&table)) else { return };
        let mut s = crate::Settings::defaults();
        s.recursos = rec;
        s.data.levels = Some(ao_rules::level_table::LevelTable::parse(&text).unwrap());
        s.start = Some((28, 50, 50));
        let mut w = World::new(s);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        w.on_event(crate::net::Event::Connected { id: 1, addr: "127.0.0.1:1".parse().unwrap(), tx });
        w.handle(1, ao_protocol::C2S::Login { name: "Tsao".into(), version: ao_protocol::PROTOCOL_VERSION });
        assert_eq!(w.ao.balance.max_level(), 70);
        // Start values: 21 life; a mage 100 + INT/6.
        let v = w.new_character_vitals(1, Class::Mage as u8).unwrap();
        assert_eq!(v.hp, 21);
        assert!(v.mana > 100);
        // Level 1 -> 2 needs 150 (Experiencia.dat Nivel1).
        let p = w.player_mut(1);
        (p.level, p.exp, p.max_hp, p.class, p.race) = (1, 0, 21, Class::Mage as u8, 1);
        w.give_exp(1, 149);
        assert_eq!(w.player(1).unwrap().level, 1);
        w.give_exp(1, 1);
        let p = w.player(1).unwrap();
        // Human mage, level 2: 21 + 19*7 < 155 -> RandomVeinteMax 8.
        assert_eq!((p.level, p.max_hp), (2, 29));
        // Gold: a TSAO creature with GiveGLDMin/Max pays into the purse.
        let (num, npc) = w.ao.npcs.iter().find(|(_, n)| n.gold_range.0 > 0 && n.gold_range.1 >= n.gold_range.0).map(|(i, n)| (i, n.gold_range)).expect("a creature with gold");
        let before = w.player(1).unwrap().inv.gold;
        assert!(w.ensure_map(28));
        w.drop_creature_loot(28, 50, 52, num, 1);
        let got = w.player(1).unwrap().inv.gold - before;
        assert!((npc.0 as u32..=npc.1 as u32).contains(&got), "{got} in {npc:?}");
        // Meditation: TSAO FX 4 below level 15.
        let p = w.player_mut(1);
        (p.max_mana, p.mana) = (100, 10);
        w.on_meditate(1);
        assert_eq!(w.player(1).unwrap().status.meditate_fx, 4);
        // Combat rules: TSAO. A mage without a staff: roll 10 -> 31 -> 70% = 22.
        assert!(w.tsao_rules());
        assert_eq!(w.tsao_spell_damage(1, true, 10, None), 22);
        assert_eq!(w.tsao_spell_damage(1, false, 10, None), 31);
        // The PvP layer of TSAO's Balance.dat: paladin vs warrior +25% then +28 flat.
        assert_eq!(w.pvp_balance().physical(100, "Paladín", "Guerrero", false), 153);

        // Newbie flow: newbie up to 9; 2..9 pay 600 * level; level 10 takes the newbie
        // items, warps to Tanaris (28, 54, 34) and gives the Starterpack (1560).
        w.data.balance.newbie_max_level = 9;
        let newbie_obj = w.ao.objs.iter().find(|(_, o)| o.newbie == 1).map(|(i, _)| i as u16).unwrap();
        w.add_item(1, crate::world::inventory::InvItem { obj: newbie_obj, amount: 1 });
        let p = w.player_mut(1);
        (p.level, p.exp, p.inv.gold) = (2, 0, 0);
        let need: u32 = (2..10).map(|l| w.ao.balance.exp_for_level(l).unwrap() as u32).sum();
        w.give_exp(1, need);
        let p = w.player(1).unwrap();
        assert_eq!((p.level, p.map), (10, 28));
        assert!((p.x - 54).abs() <= 2 && (p.y - 34).abs() <= 2);
        assert!(p.inv.gold >= (3..10).map(|l| 600 * l).sum::<u32>());
        let items: Vec<u16> = p.inv.slots.iter().flatten().map(|i| i.obj).collect();
        assert!(!items.contains(&newbie_obj), "newbie items gone");
        let pack = p.inv.slots.iter().position(|i| i.is_some_and(|i| i.obj == 1560)).expect("Starterpack") as u8 + 1;
        // Opening it gives the whole pack 3 (Random=0): 11 objects.
        let before = w.player(1).unwrap().inv.slots.iter().flatten().count();
        w.on_use_item(1, pack);
        let after = w.player(1).unwrap().inv.slots.iter().flatten().count();
        assert!(after >= before + 9, "{before} -> {after}");
    }
}

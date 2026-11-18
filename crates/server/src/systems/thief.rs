//! Classic skills of Tierras Sagradas (Trabajo.bas, TCP_HandleData*.bas): hiding, stealing,
//! sailing and digging up the treasure.
//!
//! Fixes over TSAO:
//! - `DoPermanecerOculto` added 50 to the roll with `<> "LADRON" Or <> "CAZADOR"`, always
//!   true, so thieves and hunters lost their edge staying hidden; here only other classes;
//! - leaving the boat was allowed anywhere (`Or Navegando = 1`), so a player could stand on
//!   water without a boat; here only next to land, and the player steps onto it;
//! - `RobarObjeto` silently gave up on objects above 800; here non-transferable, newbie,
//!   faction, key and boat objects are simply never stolen;
//! - the treasure countdown was one global shared by every digger; here it belongs to the
//!   player digging and moving cancels it.
use crate::clock::GameTime;
use crate::world::inventory::InvItem;
use crate::world::World;
use ao_formats::dat::balance::{Class, Skill};
use ao_formats::dat::obj::ObjType;
use ao_protocol::p4::skill::SkillC2S;
use ao_protocol::{Tone, S2C};

/// `PuedoTrabajar`: one active skill every 1.5 s.
const SKILL_INTERVAL_MS: u64 = 1500;
/// `Auditoria_Timer` x3: hidden players are re-checked every 3 s.
const HIDE_CHECK_MS: u64 = 3000;
/// TSAO dig: 30 one-second steps without moving.
const DIG_SECS: u64 = 30;
/// TSAO otMapaTesoro.
const TSAO_TREASURE_MAP: i32 = 38;

/// `Suerte` by skill (the same table for hiding, staying hidden and stealing).
fn luck(skill: u8, top: i32) -> i32 {
    match skill {
        0..=10 => 35,
        11..=20 => 30,
        21..=30 => 28,
        31..=40 => 24,
        41..=50 => 22,
        51..=60 => 20,
        61..=70 => 18,
        71..=80 => 15,
        81..=90 => 10,
        _ => top,
    }
}

/// The treasure of the world (modTesoros.bas): one at a time, somewhere on a few maps.
#[derive(Clone, Debug, Default)]
pub struct Treasure {
    pub at: Option<(u16, i32, i32)>,
    /// Player digging and when they finish.
    pub digger: Option<(u32, GameTime)>,
}

impl World {
    fn skill_of(&self, id: u32, s: Skill) -> u8 {
        self.player(id).map_or(0, |p| p.stats.skills.get(s.id() as usize - 1).copied().unwrap_or(0))
    }

    fn class_of(&self, id: u32) -> Option<Class> {
        self.player(id).and_then(|p| Class::from_id(p.class as i32))
    }

    pub(crate) fn on_skill(&mut self, id: u32, m: SkillC2S) {
        let now = self.now();
        let Some(p) = self.player(id) else { return };
        if p.combat.dead {
            return self.console(id, "¡¡Estás muerto!!", Tone::Warn);
        }
        if now < p.status.skill_ready {
            return;
        }
        self.player_mut(id).status.skill_ready = now.after_ms(SKILL_INTERVAL_MS);
        match m {
            SkillC2S::Hide => self.do_hide(id),
            SkillC2S::Steal { x, y } => self.do_steal(id, x, y),
            SkillC2S::Tame { x, y } => self.do_tame(id, x, y),
            SkillC2S::Dig => self.do_dig(id),
        }
    }

    /// `DoDomar`: pets come with their own AI (next step); for now it says so.
    fn do_tame(&mut self, id: u32, _x: i32, _y: i32) {
        self.console(id, "Domar criaturas todavía no está disponible.", Tone::Info);
    }

    // ------------------------------------------------------------ hiding

    /// `DoOcultarse`.
    fn do_hide(&mut self, id: u32) {
        let p = self.player(id).unwrap();
        if p.status.sailing.is_some() {
            return self.console(id, "No puedes ocultarte navegando.", Tone::Warn);
        }
        if p.status.hidden {
            return self.console(id, "Ya estás oculto.", Tone::Info);
        }
        let mut suerte = luck(self.skill_of(id, Skill::Hide), 7);
        if !matches!(self.class_of(id), Some(Class::Thief | Class::Hunter)) {
            suerte += 50;
        }
        if self.roll(1, suerte) <= 5 {
            let now = self.now();
            let s = &mut self.player_mut(id).status;
            s.hidden = true;
            s.hidden_check = now.after_ms(HIDE_CHECK_MS);
            self.refresh_status(id);
            self.console(id, "¡Te has escondido entre las sombras!", Tone::Good);
        } else {
            self.console(id, "¡No has logrado esconderte!", Tone::Info);
        }
    }

    /// Shows a hidden player again (speaking, attacking, casting, sailing, being found).
    pub(crate) fn reveal(&mut self, id: u32) {
        if self.player(id).is_some_and(|p| p.status.hidden) {
            self.player_mut(id).status.hidden = false;
            self.refresh_status(id);
            self.console(id, "¡Has vuelto a ser visible!", Tone::Info);
        }
    }

    /// HOOK (movement): a step reveals everyone but thieves, hunters and warriors, and
    /// stops digging.
    pub(crate) fn thief_on_move(&mut self, id: u32) {
        if self.player(id).is_some_and(|p| p.status.hidden) && !matches!(self.class_of(id), Some(Class::Thief | Class::Hunter | Class::Warrior)) {
            self.reveal(id);
        }
        if self.treasure.digger.is_some_and(|(d, _)| d == id) {
            self.treasure.digger = None;
            self.console(id, "Dejas de cavar.", Tone::Info);
        }
    }

    /// Timer: `DoPermanecerOculto` every 3 s, and the treasure dig.
    pub(crate) fn tick_thief(&mut self, now: GameTime) {
        for id in self.player_ids() {
            let Some(p) = self.player(id) else { continue };
            if !p.status.hidden || now < p.status.hidden_check {
                continue;
            }
            self.player_mut(id).status.hidden_check = now.after_ms(HIDE_CHECK_MS);
            let mut suerte = luck(self.skill_of(id, Skill::Hide), 10);
            if !matches!(self.class_of(id), Some(Class::Thief | Class::Hunter)) {
                suerte += 50;
            }
            if self.roll(1, suerte) > 9 {
                self.reveal(id);
            }
        }
        if let Some((id, done)) = self.treasure.digger {
            if now >= done {
                self.treasure.digger = None;
                self.treasure_found(id);
            }
        }
    }

    // ------------------------------------------------------------ stealing

    /// `DoRobar` (+ `RobarObjeto` for thieves).
    fn do_steal(&mut self, id: u32, x: i32, y: i32) {
        let p = self.player(id).unwrap();
        let (map, px, py) = (p.map, p.x, p.y);
        let Some(v) = self.player_ids().into_iter().find(|v| *v != id && self.player(*v).is_some_and(|q| q.map == map && q.x == x && q.y == y)) else {
            return self.console(id, "No hay a quién robarle ahí.", Tone::Info);
        };
        let m = &self.instance(map).map;
        if m.safe || m.trigger(px, py) == 4 || m.trigger(x, y) == 4 {
            return self.console(id, "No puedes robar en zona segura.", Tone::Warn);
        }
        if (px - x).abs() + (py - y).abs() > 2 {
            return self.console(id, "Estás demasiado lejos.", Tone::Info);
        }
        let vp = self.player(v).unwrap();
        if vp.combat.dead || vp.level <= self.data.balance.newbie_max_level {
            return self.console(id, "No puedes robarle a ese personaje.", Tone::Warn);
        }
        let (thief, vname, tname) = (self.class_of(id) == Some(Class::Thief), vp.name.clone(), self.player(id).unwrap().name.clone());
        self.reveal(id);
        if self.roll(1, luck(self.skill_of(id, Skill::Steal), 5)) >= 3 {
            self.console(id, "¡No has logrado robar nada!", Tone::Info);
            self.console(v, format!("¡{tname} ha intentado robarte!"), Tone::Warn);
            return;
        }
        if thief && self.roll(1, 50) < 25 {
            return self.steal_item(id, v, &vname);
        }
        let gold = self.player(v).unwrap().inv.gold;
        if gold == 0 {
            return self.console(id, format!("{vname} no tiene oro."), Tone::Info);
        }
        let n = (if thief { self.roll(100, 1000) } else { self.roll(1, 100) } as u32).min(gold);
        self.player_mut(v).inv.gold -= n;
        let p = self.player_mut(id);
        p.inv.gold = p.inv.gold.saturating_add(n);
        self.send_inventory_vitals(id);
        self.send_inventory_vitals(v);
        self.console(id, format!("Le has robado {n} monedas de oro a {vname}."), Tone::Good);
    }

    /// `RobarObjeto`: 1-5 units of a random unequipped, stealable stack.
    fn steal_item(&mut self, id: u32, v: u32, vname: &str) {
        let ao = self.ao.clone();
        let vp = self.player(v).unwrap();
        let slots: Vec<(u8, InvItem)> = (1..=vp.inv.usable_slots())
            .filter_map(|s| vp.inv.get(s).map(|i| (s, i)))
            .filter(|(s, i)| {
                !vp.inv.equipped.is_equipped(*s)
                    && ao.objs.get(i.obj as u32).is_some_and(|o| o.non_transferable == 0 && o.newbie == 0 && o.royal == 0 && o.chaos == 0 && !matches!(o.obj_type, ObjType::Keys | ObjType::Ships))
                    && i.obj != ao_rules::items::GOLD_OBJ
            })
            .collect();
        if slots.is_empty() {
            return self.console(id, format!("{vname} no tiene nada que robar."), Tone::Info);
        }
        let (slot, item) = slots[self.roll(0, slots.len() as i32 - 1) as usize];
        let n = (self.roll(1, 5) as u32).min(item.amount);
        self.remove_from_slot(v, slot, n);
        let stolen = InvItem { obj: item.obj, amount: n };
        if !self.add_item(id, stolen) {
            let (m, x, y) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
            self.drop_to_floor(m, x, y, stolen, false);
        }
        let name = ao.objs.get(item.obj as u32).map_or(String::new(), |o| o.name.clone());
        self.console(id, format!("Le has robado {n} {name} a {vname}."), Tone::Good);
    }

    // ------------------------------------------------------------ sailing

    /// `UseInvItem otBarcos` + `DoNavega`: board next to water (level 30 in TSAO), leave
    /// only next to land.
    pub(crate) fn use_boat(&mut self, id: u32, slot: u8, min_level: u32) {
        let p = self.player(id).unwrap();
        let (map, x, y, level) = (p.map, p.x, p.y, p.level);
        let obj = p.inv.get(slot).map(|i| i.obj);
        let m = &self.instance(map).map;
        let water = |x: i32, y: i32| m.blocked(x, y) & ao_formats::csm::FLAG_WATER != 0;
        let around = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)];
        if p.status.sailing.is_some() {
            // Step onto the first free land tile around.
            let land = around.into_iter().find(|&(ax, ay)| ao_rules::walk::standable(m, ax, ay) && !self.occupied(map, ax, ay));
            let Some((lx, ly)) = land else { return self.console(id, "Tienes que estar junto a la costa para desembarcar.", Tone::Warn) };
            self.player_mut(id).status.sailing = None;
            self.teleport(id, map, lx, ly);
            self.restore_look_after_boat(id);
            return self.console(id, "Has desembarcado.", Tone::Info);
        }
        if level < min_level {
            return self.console(id, format!("Necesitas nivel {min_level} para navegar."), Tone::Warn);
        }
        let sea = around.into_iter().find(|&(ax, ay)| water(ax, ay) && ao_rules::walk::in_bounds(ax, ay) && !self.occupied(map, ax, ay));
        let Some((sx, sy)) = sea else { return self.console(id, "Tienes que estar junto al agua para navegar.", Tone::Warn) };
        self.reveal(id);
        let body = obj.and_then(|o| self.ao.objs.get(o as u32)).map_or(0, |o| o.clothing.max(0) as u16);
        let p = self.player_mut(id);
        p.status.sailing = obj;
        p.body = body;
        p.head = 0;
        let pl = self.player_mut(id);
        (pl.x, pl.y) = (sx, sy);
        self.broadcast(map, &S2C::Moved { id, x: sx, y: sy, dir: ao_rules::Dir::Down }, None);
        self.send_look(id);
        self.refresh_status(id);
        self.console(id, "¡Has comenzado a navegar!", Tone::Good);
    }

    fn restore_look_after_boat(&mut self, id: u32) {
        let prof = ao_formats::profile::for_recursos(&self.recursos);
        let ao = self.ao.clone();
        let p = self.player_mut(id);
        let armor = p.inv.equipped.armor.and_then(|s| p.inv.get(s)).and_then(|i| ao.objs.get(i.obj as u32)).map(|o| o.clothing.max(0) as u16).filter(|b| *b > 0);
        if p.combat.dead {
            p.body = prof.dead_body;
            p.head = prof.dead_head;
        } else {
            p.body = armor.unwrap_or_else(|| prof.naked_body(p.race, p.gender));
            p.head = p.combat.alive_look.map_or(p.head, |l| l.1).max(p.head);
        }
        self.send_look(id);
        self.refresh_status(id);
    }

    // ------------------------------------------------------------ treasure

    /// Places the treasure on a random free tile of one of `Tesoros.dat [MAPAS]`.
    pub(crate) fn place_treasure(&mut self) {
        let maps = treasure_maps(&self.recursos);
        if maps.is_empty() {
            return;
        }
        for _ in 0..200 {
            let m = maps[self.roll(0, maps.len() as i32 - 1) as usize];
            if !self.ensure_map(m) {
                continue;
            }
            let (x, y) = (self.roll(10, 90), self.roll(10, 90));
            if ao_rules::walk::standable(&self.instance(m).map, x, y) {
                self.treasure.at = Some((m, x, y));
                tracing::info!(map = m, x, y, "tesoro escondido");
                return;
            }
        }
    }

    /// HOOK (inventory): reading a treasure map tells where it is.
    pub(crate) fn read_treasure_map(&mut self, id: u32) {
        if self.treasure.at.is_none() {
            self.place_treasure();
        }
        match self.treasure.at {
            Some((m, x, y)) => {
                let name = self.instance(m).map.name.clone();
                self.console(id, format!("El mapa señala un tesoro en {name} (mapa {m}), en {x}, {y}. Ve allí y escribe /desenterrar."), Tone::Good)
            }
            None => self.console(id, "El mapa está en blanco.", Tone::Info),
        }
    }

    /// `/DESENTERRAR`: on the exact tile, spending a treasure map; 30 s without moving.
    fn do_dig(&mut self, id: u32) {
        let p = self.player(id).unwrap();
        let here = (p.map, p.x, p.y);
        if self.treasure.at != Some(here) {
            return self.console(id, "Aquí no hay ningún tesoro.", Tone::Info);
        }
        if self.treasure.digger.is_some() {
            return self.console(id, "Alguien ya está cavando aquí.", Tone::Info);
        }
        let ao = self.ao.clone();
        let Some(slot) = (1..=p.inv.usable_slots()).find(|s| p.inv.get(*s).is_some_and(|i| ao.objs.get(i.obj as u32).is_some_and(|o| o.tsao_type == TSAO_TREASURE_MAP))) else {
            return self.console(id, "Necesitas un mapa del tesoro para desenterrarlo.", Tone::Warn);
        };
        self.remove_from_slot(id, slot, 1);
        let done = self.now().after_secs(DIG_SECS);
        self.treasure.digger = Some((id, done));
        self.console(id, format!("Comienzas a cavar... no te muevas durante {DIG_SECS} segundos."), Tone::Good);
    }

    /// Reward (General.bas): `MinOro..MaxOro` gold, 15% an extra object; a new treasure.
    fn treasure_found(&mut self, id: u32) {
        let (min, max, extra) = treasure_reward(&self.recursos);
        let gold = self.roll(min, max).max(0) as u32;
        let p = self.player_mut(id);
        p.inv.gold = p.inv.gold.saturating_add(gold);
        self.send_inventory_vitals(id);
        self.console(id, format!("¡Has encontrado el tesoro! {gold} monedas de oro."), Tone::Good);
        if extra > 0 && self.roll(1, 20) > 17 {
            self.add_item(id, InvItem { obj: extra, amount: 1 });
        }
        self.treasure.at = None;
        self.place_treasure();
    }
}

/// `Tesoros.dat [MAPAS] MapaN`.
fn treasure_maps(recursos: &std::path::Path) -> Vec<u16> {
    let ini = ao_formats::ini::Ini::load(&recursos.join("Dat").join("Tesoros.dat"));
    let Some(s) = ini.section("mapas") else { return Vec::new() };
    let n: usize = s.get("num").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    (1..=n).filter_map(|i| s.get(&format!("mapa{i}")).and_then(|v| v.trim().parse().ok())).collect()
}

/// `[EXTRAS] MinOro / MaxOro` and the extra object (TSAO: 936).
fn treasure_reward(recursos: &std::path::Path) -> (i32, i32, u16) {
    let ini = ao_formats::ini::Ini::load(&recursos.join("Dat").join("Tesoros.dat"));
    let s = ini.section("extras");
    let g = |k: &str, d: i32| s.and_then(|s| s.get(k)).and_then(|v| v.trim().parse().ok()).unwrap_or(d);
    (g("minoro", 0), g("maxoro", 0), g("objextra", 0) as u16)
}

//! Work system: gathering work: mining, woodcutting, fishing, smelting. OWNER: Opus A (docs/P4_CONTRATO.md).
//!
//! Ported from the VB6 server (`Codigo/`):
//! - `Protocol.bas` `HandleStartAutomatedAction` -> [`World::work_start`] with
//!   `CanUserFish`/`ValidateFishingPosition` (modFishing), `CanUserExtractResource` +
//!   `ValidResourceAtPos` + `CheckResourceDistance` (ModAutomatedActions),
//!   `CanUserExtractWood` (ModLumberjack), `CanUserExtractMinerals` (ModMining),
//!   `CanUserSmelt` (modSmelting); `StartAutomatedAction`.
//! - `ModAutomatedActions.RunAutomatedActions` -> [`World::tick_work`] (one global run every
//!   4 s), `DecreaseUserStamina` -> `work_spend_sta`.
//! - `ModMining.MineMinerals`, `ModLumberjack.ChopWood` -> `work_gather`;
//!   `Trabajo.ActualizarRecurso` (refill), `GetExtractResourceForLevel`, `GiveExpWhileWorking`.
//! - `modFishing.PerformFishing` -> `work_fish` (`ObtenerPezRandom`, special rewards of
//!   RecursosEspeciales.dat, fishing-area depletion).
//! - `modSmelting.SmeltMinerals` / `CraftIngots` -> `work_smelt`.
//! - `Modulo_UsUaRiOs.SubirSkill` -> `work_raise_skill` (raise by use; this port had none).
//!
//! Differences on purpose: the server also checks that the tool matches the job (the VB
//! trusts the client's skill byte), that the target is inside the view range, and a far
//! target says "Esta demasiado lejos." (the VB sends msg 324 "Portal a ¬1", a bug). Not
//! ported: the special-fish mini game (the fish is given directly), the unique-map fish
//! watchman, the fishing-area relocation (`CreateFishingPool`), throwing the net at a
//! character, and sailing (no boats in this port: net fishing always refuses).
use crate::clock::GameTime;
use crate::world::inventory::InvItem;
use crate::world::work::{ActiveWork, Resource};
use crate::world::World;
use ao_formats::dat::obj::ObjType;
use ao_formats::csm::FLAG_WATER;
use ao_protocol::p4::work::WorkKind;
use ao_protocol::{Tone, WorkC2S, WorkS2C, S2C};
use ao_rules::work::{self as rules, Job};
use std::collections::HashMap;

/// Messages of SP_LocalMsg.dat.
const MSG_NO_ENERGY: &str = "¡No tengo energía!"; // 2129
const MSG_TIRED: &str = "Estás muy cansado."; // 93
const MSG_EQUIP_TOOL: &str = "Antes de usar la herramienta deberías equipártela."; // 376
const MSG_TOO_FAR: &str = "Esta demasiado lejos."; // 424
const MSG_NET_AREA: &str = "Para pescar con red deberás buscar un área de pesca."; // 595
const MSG_NO_FISHING: &str = "Zona de pesca no Autorizada. Busca otro lugar para hacerlo."; // 596
const MSG_BLODIUM: &str = "El pico minero especial solo puede extraer minerales del yacimiento de Blodium."; // 597
const MSG_ELVEN: &str = "Sólo puedes talar árboles elficos con un hacha élfica."; // 601
const MSG_CLASS_ORE: &str = "Tu clase no tiene el conocimiento suficiente para trabajar este mineral."; // 607
const MSG_MORTALS: &str = "Los mortales no pueden fundir este mineral."; // 608
const MSG_NOT_ENOUGH_ORE: &str = "No tienes suficientes minerales para hacer un lingote."; // 645
const MSG_NO_MORE_FISH: &str = "No hay mas peces aqui."; // 649
const MSG_MINED: &str = "¡Has extraído algunos minerales!"; // 651
const MSG_COAST: &str = "Acércate a la costa para pescar."; // 1021
const MSG_NET_BOAT: &str = "Solo se puede pescar embarcado con una red, se puede pescar desde la orilla con una caña."; // 1436
/// Not an official message: the VB silently ignores a tile without the resource.
const MSG_NOTHING_HERE: &str = "No hay nada para trabajar ahí.";

/// `e_Class.Trabajador`.
const CLASS_WORKER: u8 = 9;

impl World {
    pub(crate) fn on_work(&mut self, id: u32, msg: WorkC2S) {
        match msg {
            WorkC2S::Start { kind, x, y } => self.work_start(id, kind, x, y),
            WorkC2S::Stop => {
                if self.player(id).is_some_and(|p| p.work.active.is_some()) {
                    self.work_cancel(id);
                }
            }
        }
    }

    /// Every world tick: `RunAutomatedActions` once every `IntervalAutomatedAction`.
    pub(crate) fn tick_work(&mut self, now: GameTime) {
        if now < self.works.next_run {
            return;
        }
        self.works.next_run = now.after_ms(rules::WORK_INTERVAL_MS);
        let workers: Vec<u32> = self.works.workers.iter().copied().collect();
        for id in workers {
            self.work_run(id, now);
        }
    }

    pub(crate) fn work_on_enter(&mut self, id: u32) {
        let _ = id;
    }

    pub(crate) fn work_on_leave(&mut self, id: u32) {
        self.works.workers.remove(&id);
    }

    /// `ResetUserAutomatedActions` + tells the client. For the integrator: call it when the
    /// player is hit, drops an item, etc. (the VB resets the work there too).
    pub(crate) fn work_cancel(&mut self, id: u32) {
        self.works.workers.remove(&id);
        if let Some(s) = self.sessions.get_mut(&id).and_then(|s| s.player.as_mut()) {
            s.work.active = None;
        }
        self.send(id, &S2C::Work(WorkS2C::Stopped));
    }

    fn work_refuse(&mut self, id: u32, why: Option<&str>) {
        if let Some(w) = why {
            self.console(id, w, Tone::Info);
        }
        self.work_cancel(id);
    }

    /// The (slot, obj) of the equipped working tool.
    fn work_tool(&self, id: u32) -> Option<(u8, u16)> {
        let p = self.player(id)?;
        let slot = p.inv.equipped.tool?;
        let it = p.inv.get(slot)?;
        let o = self.ao.objs.get(it.obj as u32)?;
        (o.obj_type == ObjType::WorkingTools).then_some((slot, it.obj))
    }

    /// The map object on a tile (resources loaded lazily from the `.csm`).
    fn work_resource(&mut self, map: u16, x: i32, y: i32) -> Option<Resource> {
        if !self.works.resources.contains_key(&map) {
            let mut table = HashMap::new();
            let path = ao_formats::csm::map_path(&self.recursos, map);
            if let Ok(objs) = std::fs::read(path).map_err(|_| "io").and_then(|b| rules::parse_map_objects(&b)) {
                for o in objs {
                    let Some(d) = self.ao.objs.get(o.obj as u32) else { continue };
                    // CargarMapaFormatoCSM: trees and deposits start full.
                    let amount = match d.obj_type {
                        ObjType::Trees | ObjType::OreDeposit => d.durability.max(0) as u32,
                        _ => o.amount as u32,
                    };
                    table.insert((o.x, o.y), Resource { obj: o.obj, amount, last_use: None });
                }
            }
            self.works.resources.insert(map, table);
        }
        self.works.resources[&map].get(&(x, y)).copied()
    }

    fn work_fish_table(&mut self) -> &rules::FishTable {
        if self.works.fish.is_none() {
            let dat = self.recursos.join("Dat");
            let p = std::fs::read(dat.join("pesca.dat")).unwrap_or_default();
            let e = std::fs::read(dat.join("RecursosEspeciales.dat")).unwrap_or_default();
            self.works.fish = Some(rules::FishTable::parse(&p, &e));
        }
        self.works.fish.as_ref().unwrap()
    }

    // ---------------------------------------------------------------- start

    /// `HandleStartAutomatedAction` (plus the generic checks of `HandleWorkLeftClick`).
    fn work_start(&mut self, id: u32, kind: WorkKind, x: i32, y: i32) {
        let Some(p) = self.player(id) else { return };
        let (map, px, py, class, privileged) = (p.map, p.x, p.y, p.class, false);
        if self.is_dead(id) || !ao_formats::csm::on_map(x, y) {
            return self.work_refuse(id, None);
        }
        // InRangoVision.
        if (x - px).abs() > ao_rules::VIEW_TILES_W / 2 || (y - py).abs() > ao_rules::VIEW_TILES_H / 2 {
            return self.work_refuse(id, None);
        }
        let item = match kind {
            WorkKind::Smelt { slot } => {
                // CanUserSmelt.
                if class != CLASS_WORKER {
                    return self.work_refuse(id, Some(MSG_CLASS_ORE));
                }
                if privileged {
                    return self.work_refuse(id, None);
                }
                if rules::distance(px, py, x, y) > rules::MEDIUM_DISTANCE {
                    return self.work_refuse(id, Some(MSG_TOO_FAR));
                }
                let forge = self.work_resource(map, x, y).and_then(|r| self.ao.objs.get(r.obj as u32)).is_some_and(|o| o.obj_type == ObjType::Forge);
                if !forge {
                    return self.work_refuse(id, Some(MSG_NOTHING_HERE));
                }
                let ore = self.player(id).and_then(|p| p.inv.get(slot)).filter(|it| self.ao.objs.get(it.obj as u32).is_some_and(|o| o.obj_type == ObjType::Minerals));
                let Some(ore) = ore else { return self.work_refuse(id, Some(MSG_NOT_ENOUGH_ORE)) };
                (slot, ore.obj)
            }
            _ => {
                let Some(tool) = self.work_tool(id) else { return self.work_refuse(id, Some(MSG_EQUIP_TOOL)) };
                let t = self.ao.objs.get(tool.1 as u32).unwrap();
                let (subtype, tool_elven, tool_blodium) = (t.subtype, t.elven, t.blodium);
                let job = match kind {
                    WorkKind::Chop => Job::Chop,
                    WorkKind::Mine => Job::Mine,
                    _ => Job::Fish,
                };
                if Job::for_tool(subtype) != Some(job) {
                    return self.work_refuse(id, Some(MSG_EQUIP_TOOL));
                }
                if let Err(why) = self.work_check_gather(id, job, subtype, tool_elven, tool_blodium, x, y) {
                    return self.work_refuse(id, why);
                }
                tool
            }
        };
        let w = ActiveWork { kind, map, x, y, from: (px, py), item };
        self.player_mut(id).work.active = Some(w);
        self.works.workers.insert(id);
        self.send(id, &S2C::Work(WorkS2C::Started { kind, x, y }));
    }

    /// Target checks of each gathering job. `Err(None)` = silent refusal (like the VB).
    #[allow(clippy::too_many_arguments)]
    fn work_check_gather(&mut self, id: u32, job: Job, subtype: i32, tool_elven: i32, tool_blodium: i32, x: i32, y: i32) -> Result<(), Option<&'static str>> {
        let p = self.player(id).unwrap();
        let (map, px, py) = (p.map, p.x, p.y);
        let res = self.work_resource(map, x, y);
        let res_obj = res.and_then(|r| self.ao.objs.get(r.obj as u32));
        match job {
            Job::Fish => {
                // ValidateFishingPosition.
                let inst = &self.maps[&map].map;
                if inst.blocked(x, y) & FLAG_WATER == 0 || inst.trigger(px, py) == rules::TRIGGER_NO_FISHING {
                    return Err(Some(MSG_NO_FISHING));
                }
                let on_water = inst.blocked(px, py) & FLAG_WATER != 0;
                if subtype == rules::TOOL_FISHING_ROD {
                    if on_water {
                        return Err(Some(MSG_NET_BOAT));
                    }
                    let near = ao_rules::Dir::ALL.iter().any(|d| {
                        let (dx, dy) = d.delta();
                        inst.blocked(px + dx, py + dy) & FLAG_WATER != 0
                    });
                    if !near {
                        return Err(Some(MSG_COAST));
                    }
                } else {
                    // No sailing in this port: `flags.Navegando` is never set.
                    if !on_water {
                        return Err(Some(MSG_NET_BOAT));
                    }
                    if !res.is_some_and(|r| rules::NET_SPOTS.contains(&r.obj)) {
                        return Err(Some(MSG_NET_AREA));
                    }
                    if rules::distance(px, py, x, y) > rules::CLOSE_DISTANCE {
                        return Err(Some(MSG_TOO_FAR));
                    }
                }
                if inst.zone.eq_ignore_ascii_case("DUNGEON") {
                    return Err(Some(MSG_NO_FISHING));
                }
            }
            Job::Chop | Job::Mine => {
                let want = if job == Job::Chop { ObjType::Trees } else { ObjType::OreDeposit };
                let Some(o) = res_obj.filter(|o| o.obj_type == want) else { return Err(Some(MSG_NOTHING_HERE)) };
                if rules::distance(px, py, x, y) > rules::CLOSE_DISTANCE {
                    return Err(Some(MSG_TOO_FAR));
                }
                if job == Job::Chop && o.elven > 0 && tool_elven <= 0 {
                    return Err(Some(MSG_ELVEN));
                }
                if job == Job::Mine && o.blodium > 0 && tool_blodium <= 0 {
                    return Err(Some(MSG_BLODIUM));
                }
            }
            Job::Smelt => {}
        }
        Ok(())
    }

    // ---------------------------------------------------------------- runs

    fn work_run(&mut self, id: u32, now: GameTime) {
        let Some(w) = self.player(id).and_then(|p| p.work.active) else {
            self.works.workers.remove(&id);
            return;
        };
        // What the VB resets elsewhere: walking, unequipping, dying, changing map.
        let p = self.player(id).unwrap();
        let moved = p.map != w.map || (p.x, p.y) != w.from;
        let changed = match w.kind {
            WorkKind::Smelt { .. } => false,
            _ => self.work_tool(id) != Some(w.item),
        };
        if moved || changed || self.is_dead(id) {
            return self.work_cancel(id);
        }
        match w.kind {
            WorkKind::Chop => self.work_gather(id, Job::Chop, w, now),
            WorkKind::Mine => self.work_gather(id, Job::Mine, w, now),
            WorkKind::Fish => self.work_fish(id, w),
            WorkKind::Smelt { .. } => self.work_smelt(id, w),
        }
    }

    /// `DecreaseUserStamina` / `QuitarSta`.
    fn work_spend_sta(&mut self, id: u32, need: i32) -> bool {
        let p = self.player_mut(id);
        if p.stats.sta > need {
            p.stats.sta -= need;
            self.send_vitals(id);
            true
        } else {
            self.console(id, MSG_NO_ENERGY, Tone::Info);
            self.console(id, MSG_TIRED, Tone::Info);
            self.work_cancel(id);
            false
        }
    }

    fn work_safe(&self, map: u16) -> bool {
        self.maps.get(&map).is_some_and(|m| m.map.safe)
    }

    /// Gives `item`, or drops it at the player's feet when it does not fit
    /// (`MeterItemEnInventario` / `TirarItemAlPiso`). Never with 0.
    fn work_give_or_drop(&mut self, id: u32, item: InvItem) {
        if item.amount == 0 {
            return;
        }
        if !self.add_item(id, item) {
            let p = self.player(id).unwrap();
            let (map, x, y) = (p.map, p.x, p.y);
            self.drop_to_floor(map, x, y, item, false);
        }
    }

    /// `MineMinerals` / `ChopWood`.
    fn work_gather(&mut self, id: u32, job: Job, w: ActiveWork, now: GameTime) {
        if !self.work_spend_sta(id, rules::MIN_STA_REQUIRED) {
            return;
        }
        let p = self.player(id).unwrap();
        let (skill, level, worker) = (p.stats.skills[job.skill()], p.level, p.class == CLASS_WORKER);
        let max = rules::gather_roll_max(job, skill, self.work_safe(w.map));
        let roll = self.rng.range(1, max.max(1));
        if rules::gather_hit(roll) {
            let Some(mut res) = self.work_resource(w.map, w.x, w.y) else { return self.work_cancel(id) };
            let Some(d) = self.ao.objs.get(res.obj as u32) else { return self.work_cancel(id) };
            let (full, regen, elven, pine, mineral) = (d.durability.max(0) as u32, d.regen_time, d.elven, d.pine, d.mineral_index);
            // ActualizarRecurso, then "last use".
            res.amount = rules::refilled(res.amount, full, res.last_use.map(|t| t.ms()), now.ms(), regen).0;
            res.last_use = Some(now);
            let (lo, hi) = rules::extract_range(worker, level);
            let mut amount = self.rng.range(lo, hi).max(0) as u32;
            let exhausted = amount > res.amount;
            if exhausted {
                amount = res.amount;
            }
            res.amount -= amount;
            self.works.resources.get_mut(&w.map).unwrap().insert((w.x, w.y), res);
            let obj = match job {
                Job::Mine => mineral.max(0) as u16,
                _ if elven == 1 => rules::ELVEN_WOOD,
                _ if pine == 1 => rules::PINE_WOOD,
                _ => rules::WOOD,
            };
            self.work_give_or_drop(id, InvItem { obj, amount });
            self.send(id, &S2C::Work(WorkS2C::Gathered { obj, amount }));
            if job == Job::Mine {
                self.console(id, MSG_MINED, Tone::Info);
            }
            // GiveExpWhileWorking (feature gain_exp_while_working = 1).
            self.give_exp(id, rules::JOB_EXP);
            if exhausted {
                self.work_cancel(id);
            }
        } else {
            self.send(id, &S2C::Work(WorkS2C::Missed));
        }
        self.work_raise_skill(id, job);
    }

    /// `PerformFishing`.
    fn work_fish(&mut self, id: u32, w: ActiveWork) {
        if !self.work_spend_sta(id, rules::MIN_STA_REQUIRED) {
            return;
        }
        let Some(tool) = self.ao.objs.get(w.item.1 as u32) else { return self.work_cancel(id) };
        let (net, power) = (tool.subtype == rules::TOOL_FISHING_NET, tool.power);
        let safe = self.work_safe(w.map);
        let p = self.player(id).unwrap();
        let (skill, worker) = (p.stats.skills[rules::SKILL_FISHING], p.class == CLASS_WORKER);
        if self.rng.range(1, 100) <= rules::fishing_chance(skill) {
            self.work_catch(id, w, net, power, worker, safe);
        } else {
            self.send(id, &S2C::Work(WorkS2C::Missed));
        }
        if !safe {
            self.work_raise_skill(id, Job::Fish);
        }
    }

    fn work_catch(&mut self, id: u32, w: ActiveWork, net: bool, power: i32, worker: bool, safe: bool) {
        let Some((lo, hi)) = self.work_fish_table().roll_range(power) else { return };
        let v = lo + (self.rng.next_u32() as i64 % (hi - lo + 1));
        let Some(fish) = self.work_fish_table().fish_at(v).map(|f| f.obj) else { return };
        let (alo, ahi) = rules::catch_range(worker, net);
        let mut amount = self.rng.range(alo, ahi) as u32;
        if worker && power >= 2 {
            self.give_exp(id, rules::JOB_EXP);
        }
        // Fishing-area depletion (only outside safe maps).
        if !safe {
            if let Some(mut r) = self.work_resource(w.map, w.x, w.y).filter(|r| r.obj == rules::FISH_AREA) {
                let empty = amount > r.amount;
                if empty {
                    amount = r.amount;
                    self.console(id, MSG_NO_MORE_FISH, Tone::Warn);
                }
                r.amount -= amount;
                self.works.resources.get_mut(&w.map).unwrap().insert((w.x, w.y), r);
                if empty {
                    self.work_cancel(id);
                }
            }
        }
        if amount == 0 {
            return;
        }
        if !self.add_item(id, InvItem { obj: fish, amount }) {
            return self.work_cancel(id);
        }
        self.send(id, &S2C::Work(WorkS2C::Gathered { obj: fish, amount }));
        let specials = self.work_fish_table().specials.clone();
        for (obj, one_in) in specials {
            let n = if net { one_in * 2 } else { one_in };
            if self.rng.range(1, n.max(1)) == 1 {
                self.work_give_or_drop(id, InvItem { obj, amount: 1 });
                let name = self.ao.objs.get(obj as u32).map(|o| o.name.clone()).unwrap_or_default();
                self.console(id, format!("¡Has conseguido {name}!"), Tone::Good);
            }
        }
    }

    /// `SmeltMinerals` + `CraftIngots`.
    fn work_smelt(&mut self, id: u32, w: ActiveWork) {
        let (slot, ore) = w.item;
        let Some(d) = self.ao.objs.get(ore as u32) else { return self.work_cancel(id) };
        let (required, ingot) = (d.min_skill, d.ingot_index);
        if required > 100 {
            return self.work_refuse(id, Some(MSG_MORTALS));
        }
        let skill = self.player(id).unwrap().stats.skills[rules::SKILL_MINING] as i32;
        if skill < required {
            let m = format!("No tenés conocimientos de minería suficientes para trabajar este mineral. Necesitas {required} puntos en minería.");
            return self.work_refuse(id, Some(&m));
        }
        if self.player(id).unwrap().stats.sta <= rules::SMELT_STA {
            self.console(id, MSG_NO_ENERGY, Tone::Info);
            return self.work_refuse(id, Some(MSG_TIRED));
        }
        self.player_mut(id).stats.sta -= rules::SMELT_STA;
        self.send_vitals(id);
        let cant = self.rng.range(10, 20) as u32;
        let need = rules::minerals_per_ingot(ore, cant);
        let have = self.player(id).unwrap().inv.get(slot).filter(|it| it.obj == ore).map_or(0, |it| it.amount);
        if have < need {
            return self.work_refuse(id, Some(MSG_NOT_ENOUGH_ORE));
        }
        self.remove_from_slot(id, slot, need);
        let obj = ingot.max(0) as u16;
        self.work_give_or_drop(id, InvItem { obj, amount: cant });
        self.send(id, &S2C::Work(WorkS2C::Gathered { obj, amount: cant }));
        self.work_raise_skill(id, Job::Smelt);
    }

    /// `SubirSkill`: raise by use, with the level cap, hunger/thirst and the 5-exp bonus.
    pub(crate) fn work_raise_skill(&mut self, id: u32, job: Job) {
        let difficulty = self.ao.balance.extra.skill_up_difficulty;
        let p = self.player(id).unwrap();
        let (value, level) = (p.stats.skills[job.skill()], p.level);
        if !rules::can_raise_skill(value, level, p.stats.food, p.stats.water) {
            return;
        }
        if self.rng.range(1, rules::skill_roll_max(level, difficulty)) >= rules::SKILL_CUTOFF {
            return;
        }
        let new = value + 1;
        self.player_mut(id).stats.skills[job.skill()] = new;
        self.console(id, format!("¡Has mejorado tu habilidad {} en un punto! Ahora tienes {new} pts.", job.skill_name()), Tone::Good);
        self.send_skills(id);
        if level < self.ao.balance.max_level() {
            self.give_exp(id, rules::SKILL_UP_EXP);
        }
    }
}

/// Paths that need a Worker or high skills: driven on the world directly.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Event;
    use crate::Settings;
    use ao_protocol::{C2S, PROTOCOL_VERSION};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    fn world() -> World {
        let mut s = Settings::defaults();
        s.start = Some((1, 50, 50));
        s.npcs = false;
        World::new(s)
    }

    fn login(w: &mut World, id: u32) -> UnboundedReceiver<Vec<u8>> {
        let (tx, rx) = unbounded_channel();
        w.on_event(Event::Connected { id, addr: "127.0.0.1:9".parse().unwrap(), tx });
        w.on_event(Event::Frame { id, msg: C2S::Login { name: format!("Obrero{id}"), version: PROTOCOL_VERSION } });
        rx
    }

    #[test]
    fn worker_smelts_iron_and_needs_skill() {
        let mut w = world();
        let _rx = login(&mut w, 1);
        // Next to the forge of Ullathorpe (32, 62).
        w.teleport(1, 1, 33, 63);
        w.add_item(1, InvItem { obj: rules::IRON_ORE, amount: 600 });
        let slot = (1..=24).find(|s| w.player(1).unwrap().inv.get(*s).is_some_and(|i| i.obj == rules::IRON_ORE)).unwrap();
        // Not a worker: refused.
        w.on_work(1, WorkC2S::Start { kind: WorkKind::Smelt { slot }, x: 32, y: 62 });
        assert!(w.player(1).unwrap().work.active.is_none());
        w.player_mut(1).class = CLASS_WORKER;
        // Worker without the 25 Mining points: refused on the first run.
        w.on_work(1, WorkC2S::Start { kind: WorkKind::Smelt { slot }, x: 32, y: 62 });
        assert!(w.player(1).unwrap().work.active.is_some());
        w.tick_work(GameTime(4000));
        assert!(w.player(1).unwrap().work.active.is_none());
        // With the skill: ingots (386) appear and ore is spent 13 per ingot.
        w.player_mut(1).stats.skills[rules::SKILL_MINING] = 30;
        w.player_mut(1).stats.sta = 40;
        w.on_work(1, WorkC2S::Start { kind: WorkKind::Smelt { slot }, x: 32, y: 62 });
        w.tick_work(GameTime(8000));
        let inv = &w.player(1).unwrap().inv;
        let ingots: u32 = inv.slots.iter().flatten().filter(|i| i.obj == 386).map(|i| i.amount).sum();
        let ore: u32 = inv.slots.iter().flatten().filter(|i| i.obj == rules::IRON_ORE).map(|i| i.amount).sum();
        assert!((10..=20).contains(&ingots), "{ingots}");
        assert_eq!(ore, 600 - 13 * ingots);
        assert_eq!(w.player(1).unwrap().stats.sta, 38);
    }

    #[test]
    fn worker_chops_more_and_skill_rises_by_use() {
        let mut w = world();
        let _rx = login(&mut w, 1);
        w.add_item(1, InvItem { obj: 127, amount: 1 });
        let slot = (1..=24).find(|s| w.player(1).unwrap().inv.get(*s).is_some_and(|i| i.obj == 127)).unwrap();
        w.player_mut(1).class = CLASS_WORKER;
        w.player_mut(1).level = 20;
        w.equip(1, slot, false);
        assert!(w.work_tool(1).is_some(), "axe equipped");
        {
            let p = w.player_mut(1);
            p.class = CLASS_WORKER;
            p.level = 20;
            p.stats.skills[rules::SKILL_WOODCUTTING] = 100;
            p.stats.sta = 10_000;
        }
        // Below the tree (82, 19) of Ullathorpe.
        w.teleport(1, 1, 82, 20);
        let mut t = 0;
        let mut got = 0;
        for _ in 0..40 {
            if w.player(1).unwrap().work.active.is_none() {
                w.on_work(1, WorkC2S::Start { kind: WorkKind::Chop, x: 82, y: 19 });
                assert!(w.player(1).unwrap().work.active.is_some());
            }
            t += 4000;
            w.tick_work(GameTime(t));
            got = w.player(1).unwrap().inv.slots.iter().flatten().filter(|i| i.obj == rules::WOOD).map(|i| i.amount).sum();
            if got > 0 {
                break;
            }
        }
        assert!(got >= 5, "worker level 20 gets 5..10 per hit, got {got}");
        // Inventory full: the wood falls at the worker's feet (TirarItemAlPiso).
        for s in 1..=24u8 {
            if w.player(1).unwrap().inv.get(s).is_none() {
                *w.player_mut(1).inv.cell_mut(s).unwrap() = Some(InvItem { obj: 1000 + s as u16, amount: 1 });
            }
        }
        let wood_slot = (1..=24).find(|s| w.player(1).unwrap().inv.get(*s).is_some_and(|i| i.obj == rules::WOOD)).unwrap();
        *w.player_mut(1).inv.cell_mut(wood_slot).unwrap() = Some(InvItem { obj: 999, amount: 1 });
        let before = w.maps[&1].ground.tiles.values().filter(|i| i.obj == rules::WOOD).count();
        for _ in 0..60 {
            if w.player(1).unwrap().work.active.is_none() {
                w.on_work(1, WorkC2S::Start { kind: WorkKind::Chop, x: 82, y: 19 });
            }
            t += 4000;
            w.tick_work(GameTime(t));
            if w.maps[&1].ground.tiles.values().filter(|i| i.obj == rules::WOOD).count() > before {
                break;
            }
        }
        assert!(w.maps[&1].ground.tiles.values().any(|i| i.obj == rules::WOOD), "wood on the floor");
        // Skill raise by use, from 0 (cap at level 20 is 50).
        w.player_mut(1).stats.skills[rules::SKILL_WOODCUTTING] = 0;
        for _ in 0..200 {
            w.work_raise_skill(1, Job::Chop);
        }
        let s = w.player(1).unwrap().stats.skills[rules::SKILL_WOODCUTTING];
        assert!(s > 0 && s <= 50, "{s}");
    }
}

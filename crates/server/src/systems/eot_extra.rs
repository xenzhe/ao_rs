//! The effect-over-time kinds that act on the map or on the equipment, ported from the
//! official AO20 server: `EffectProvoke` / `EffectProvoked` (3, 4), `clsTrap` with
//! `CreateTrap`, `PlaceTrap`, `CanAddTrapAt` and `ModMap.ActivateTrap` (5), `DrunkEffect`
//! (6), `TranslationEffect` (7), `AttrackEffect` (11), `DelayedBlast` with
//! `CreateDelayedBlast` and `UseHandCannon` (12) and `UnequipItem` with
//! `CreateUnequip` and `SistemaCombate.DesequiparObjetoDeUnGolpe` (13). Also the buff
//! bar message (`WriteSendSkillCdUpdate`) for every effect of a player.
//!
//! Differences with the VB, on purpose:
//! - `TranslationEffect.Setup` checked `TranslationInmune` and then `SetBaseDot` cleared
//!   the flag it had set, so immune creatures were pushed anyway. Here they are not.
//! - `EffectProvoked` lowered the damage of a taunted player only between
//!   `TargetWillAttack` and `TargetDidHit`; here the same reduction is computed for the
//!   blow itself (`attack_mods`), so a miss can never leave it applied.
//! - Traps are not drawn on the client (`eUpdateTrap` has no message here yet): they work
//!   on the server, the owner sees them in the buff bar.
//! - The hand cannon's blast rolls the object's `MinHIT..MaxHIT` (the VB passes it through
//!   `GetUserDamageWithItem`, which adds the strength and level bonuses of melee).
use super::eot::Who;
use crate::clock::GameTime;
use crate::world::spellbook::{ActiveEot, EotCaster};
use crate::world::World;
use ao_formats::csm::{BLOCK_ALL, FLAG_WATER};
use ao_formats::dat::eots::{EotDef, EotKind};
use ao_formats::dat::obj::ObjType;
use ao_protocol::{buff_kind, Tone, S2C};
use ao_rules::eot::Modifiers;
use ao_rules::{walk, Dir};

/// `EffectProvoke.TaunteffectId`: what the taunt leaves on everyone around.
const TAUNTED_EOT: u16 = 4;
/// `EffectsOverTime.UnequipEffectId`.
const UNEQUIP_EOT: u16 = 23;
/// `DrunkEffect`: stupid from 5 drinks, back to normal at 3.
const DRUNK_DUMB_FROM: u16 = 5;
const DRUNK_DUMB_UNTIL: u16 = 3;
/// `PlaceTrap`: a fourth trap disables the oldest.
const MAX_TRAPS: usize = 3;
/// `clsTrap.ACTIVATE_TRAP_WAW_FILE`.
const TRAP_WAV: u16 = 210;
/// `UseHandCannon`: farthest tile.
const HAND_CANNON_RANGE: f64 = 10.0;

/// `e_UssableOnTarget` (Subtipo of `otUsableOntarget`).
const USE_ON_RESURRECT: i32 = 1;
const USE_ON_TRAP: i32 = 2;
const USE_ON_HARPOON: i32 = 3;
const USE_ON_HAND_CANNON: i32 = 4;

/// `e_InventorySlotMask` bits this server can lock (`flags.DisabledSlot`).
pub(crate) const SLOT_WEAPON: u8 = 1;
pub(crate) const SLOT_SHIELD: u8 = 2;
pub(crate) const SLOT_HELM: u8 = 4;

/// `e_TrapTypes`.
const TRAP_PARALYZE: i32 = 1;
const TRAP_EXPLOSIVE: i32 = 2;
const TRAP_POISON: i32 = 3;

/// The 8-way step `TranslationEffect.PerformAction` takes towards a target offset.
pub(crate) fn step_towards(tx: f64, ty: f64) -> (i32, i32) {
    let diff = tx.abs() - ty.abs();
    let sgn = |v: f64| if v > 0.0 { 1 } else if v < 0.0 { -1 } else { 0 };
    if diff.abs() < 0.3 {
        (sgn(tx), sgn(ty))
    } else if diff > 0.0 {
        (sgn(tx), 0)
    } else {
        (0, sgn(ty))
    }
}

fn dir_of(dx: i32, dy: i32) -> Dir {
    if dx.abs() >= dy.abs() {
        if dx >= 0 {
            Dir::Right
        } else {
            Dir::Left
        }
    } else if dy >= 0 {
        Dir::Down
    } else {
        Dir::Up
    }
}

fn dist(a: (i32, i32), b: (i32, i32)) -> f64 {
    (((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2)) as f64).sqrt()
}

/// `CalculateElementalTagsModifiers`: the damage times the matrix cell of every attacker
/// element against every defender element (bit i = element i + 1).
pub(crate) fn elemental_factor(attacker: i32, defender: i32, matrix: &[[f32; ao_formats::dat::balance::ELEMENT_TAGS]; ao_formats::dat::balance::ELEMENT_TAGS]) -> f64 {
    let mut f = 1.0f64;
    if attacker == 0 || defender == 0 {
        return f;
    }
    for (a, row) in matrix.iter().enumerate() {
        if attacker & (1 << a) == 0 {
            continue;
        }
        for (d, cell) in row.iter().enumerate() {
            if defender & (1 << d) != 0 {
                f *= *cell as f64;
            }
        }
    }
    f
}

/// `e_EffectType` sent for an effect (`buffType`, 1..5).
fn kind_of(def: &EotDef) -> u8 {
    match def.buff_type {
        1..=5 => def.buff_type as u8,
        _ => buff_kind::INFO_BUFF,
    }
}

impl World {
    pub(crate) fn who_pos(&self, who: Who) -> Option<(u16, i32, i32)> {
        match who {
            Who::User(id) => self.player(id).map(|p| (p.map, p.x, p.y)),
            Who::Npc(m, n) => self.maps.get(&m)?.npcs.get(&n).filter(|n| n.alive()).map(|n| (m, n.x, n.y)),
        }
    }

    // ------------------------------------------------------------ buff bar

    /// Time shown for an effect: (remaining, total) in ms. Transforms and songs have no end.
    fn eot_times(&self, e: &ActiveEot, def: &EotDef) -> (u32, u32) {
        let now = self.now();
        let clamp = |v: u64| v.min(u32::MAX as u64 / 2) as u32;
        if let Some(t) = e.ends {
            let total = def.tick_ms * if def.kind == EotKind::Shield && def.ticks > 0 { def.ticks as u64 } else { 1 };
            return (clamp(t.since(now).max(1)), clamp(total));
        }
        match def.kind {
            EotKind::ModifyHealth | EotKind::ModifyMana | EotKind::Translation | EotKind::PullTarget => {
                let total = def.tick_ms * def.ticks.max(1) as u64;
                let left = e.next_tick.since(now) + def.tick_ms * e.ticks_left.saturating_sub(1) as u64;
                (clamp(left.max(1)), clamp(total))
            }
            EotKind::Drunk => (clamp(e.next_tick.since(now).max(1)), clamp(def.tick_ms)),
            _ => (u32::MAX / 2, 0),
        }
    }

    /// `WriteSendSkillCdUpdate` for an effect of a player (`on` false = it ended).
    pub(crate) fn send_eot_buff(&self, who: Who, e: &ActiveEot, def: &EotDef, on: bool) {
        let Who::User(id) = who else { return };
        let icon = if def.kind == EotKind::Unequip {
            match e.slot_mask {
                SLOT_WEAPON => 18,
                SLOT_SHIELD => 19,
                SLOT_HELM => 20,
                _ => 0,
            }
        } else {
            def.client_effect_type_id
        };
        if icon == 0 {
            return;
        }
        let (remaining_ms, total_ms) = if on { self.eot_times(e, def) } else { (0, 0) };
        self.send(id, &S2C::Buff { icon, uid: e.uid as i32, remaining_ms, total_ms, kind: kind_of(def), stacks: e.stacks });
    }

    /// `CalculateElementalTagsModifiers` behind the `elemental_tags` toggle: the player's
    /// weapon against the creature (`UserDañoNpc`, `HechizoPropNPC`).
    pub(crate) fn elemental_scale(&self, id: u32, map: u16, nid: u32, dmg: i64) -> i64 {
        if !self.cfg.world.elemental_tags {
            return dmg;
        }
        let Some(p) = self.player(id) else { return dmg };
        let weapon = p.inv.equipped.weapon.and_then(|s| p.inv.get(s)).and_then(|i| self.ao.objs.get(i.obj as u32)).map_or(0, |o| o.elemental_tags);
        let npc = self.maps.get(&map).and_then(|i| i.npcs.get(&nid)).and_then(|n| self.ao.npcs.get(n.def as u32)).map_or(0, |d| d.elemental_tags);
        (dmg as f64 * elemental_factor(weapon, npc, &self.ao.balance.elemental_matrix)) as i64
    }

    // ------------------------------------------------------------ taunt (3, 4)

    /// `EffectProvoke.Setup`: the taunter loses its invisibility and everyone it may attack
    /// in the `Area` square around it is taunted (EOT4) towards it.
    pub(crate) fn taunt_area(&mut self, caster: Who, def: &EotDef) {
        let Some((map, cx, cy)) = self.who_pos(caster) else { return };
        if let Who::User(id) = caster {
            let st = &mut self.player_mut(id).status;
            if st.invisible_until.take().is_some() {
                self.refresh_status(id);
            }
        }
        let half = def.area / 2;
        let (x0, y0) = ((cx - half).max(1), (cy - half).max(1));
        let (x1, y1) = ((cx - half + def.area).min(99), (cy - half + def.area).min(99));
        let Who::User(uid) = caster else { return };
        for y in y0..=y1 {
            for x in x0..=x1 {
                if let Some(t) = self.player_at(map, x, y) {
                    if self.spell_can_hit_user(uid, t).is_ok() && !self.has_eot(Who::User(t), TAUNTED_EOT) {
                        self.apply_eot(caster, Who::User(t), TAUNTED_EOT, false);
                    }
                } else if let Some(n) = self.instance(map).npc_at(x, y) {
                    if self.user_may_hit_npc(uid, map, n) && !self.has_eot(Who::Npc(map, n), TAUNTED_EOT) {
                        self.apply_eot(caster, Who::Npc(map, n), TAUNTED_EOT, false);
                    }
                }
            }
        }
    }

    /// `EffectProvoked.Setup` on a creature: it goes for the player who taunted it.
    pub(crate) fn taunted_npc_targets(&mut self, map: u16, nid: u32, caster: EotCaster) {
        if let EotCaster::User(pid) = caster {
            let n = self.npc_mut(map, nid);
            n.target = Some(pid);
            n.attacked_by = Some(pid);
            n.hostile = true;
        }
    }

    /// `eTaunted`: the player a creature was taunted by, while it lasts and is around.
    pub(crate) fn npc_taunter(&self, map: u16, nid: u32) -> Option<u32> {
        let table = self.eots();
        let n = self.maps.get(&map)?.npcs.get(&nid)?;
        n.status.eots.iter().find_map(|e| match (table.get(&e.eot).map(|d| d.kind), e.caster) {
            (Some(EotKind::Taunted), EotCaster::User(p)) if self.player(p).is_some_and(|p| p.map == map && !p.combat.dead) => Some(p),
            _ => None,
        })
    }

    /// The attacker's modifiers for a blow on `target`: `EffectProvoked.ApplyReduction`
    /// takes `PhysicalDamageDone` / `MagicDamageDone` off a taunted player who hits anyone
    /// but its taunter.
    pub(crate) fn attack_mods(&self, attacker: Who, target: Who) -> Modifiers {
        let mut m = self.modifiers(attacker);
        let Who::User(id) = attacker else { return m };
        let Some(p) = self.player(id) else { return m };
        if p.status.multishot {
            m.physical_bonus -= self.ao.balance.extra.multi_shot_reduction;
        }
        let table = self.eots();
        for e in &p.status.eots {
            let Some(d) = table.get(&e.eot).filter(|d| d.kind == EotKind::Taunted) else { continue };
            if e.caster != target.caster() {
                m.magic_bonus += d.mods.magic_done;
                m.physical_bonus += d.mods.physical_done;
            }
        }
        m
    }

    /// `UserCanAttackNpc` without messages (taunts, traps).
    fn user_may_hit_npc(&self, id: u32, map: u16, nid: u32) -> bool {
        let Some(n) = self.maps.get(&map).and_then(|i| i.npcs.get(&nid)).filter(|n| n.alive()) else { return false };
        n.attackable && n.pet.owner != Some(id) && self.faction_npc_attack_refusal(id, n.def).is_none() && self.npc_owner_rule(id, map, nid) == super::npc_owner::OwnerRule::Free
    }

    // ------------------------------------------------------------ drunk (6)

    /// `AddOrResetEffect` (drinks, potions with `ApplyEffectId`).
    pub(crate) fn add_or_reset_effect(&mut self, id: u32, eot: i32) {
        if eot <= 0 || eot > u16::MAX as i32 {
            return;
        }
        let eot = eot as u16;
        match self.find_eot(Who::User(id), eot, EotCaster::User(id)) {
            None => {
                self.apply_eot(Who::User(id), Who::User(id), eot, false);
            }
            Some(i) if self.eots().get(&eot).is_some_and(|d| d.override_) => self.reset_eot_at(Who::User(id), i),
            Some(_) => {}
        }
    }

    /// `IBaseEffectOverTime.Reset` of the kinds that keep their instance: drunk stacks,
    /// pushes and pulls start their steps over. False = the generic replace applies.
    pub(crate) fn reset_eot_at(&mut self, who: Who, i: usize) {
        let table = self.eots();
        let Some(e) = self.eot_list(who).and_then(|l| l.get(i)).cloned() else { return };
        let Some(def) = table.get(&e.eot) else { return };
        let now = self.now();
        match def.kind {
            EotKind::Drunk => {
                let l = &mut self.eot_list_mut(who).unwrap()[i];
                l.stacks += 1;
                l.next_tick = now.after_ms(def.tick_ms.max(1));
                let e = l.clone();
                self.send_eot_buff(who, &e, def, true);
                if let Who::User(id) = who {
                    if e.stacks >= DRUNK_DUMB_FROM {
                        // WriteDumb until the stacks drop back to 3.
                        let until = now.after_ms(def.tick_ms.max(1) * (e.stacks - DRUNK_DUMB_UNTIL) as u64);
                        let st = &mut self.player_mut(id).status;
                        if st.dumb_until.is_none_or(|u| u < until) {
                            st.dumb_until = Some(until);
                        }
                        self.refresh_status(id);
                    }
                }
            }
            EotKind::Translation | EotKind::PullTarget => {
                let l = &mut self.eot_list_mut(who).unwrap()[i];
                l.ticks_left = def.ticks;
            }
            _ => {
                self.remove_eot_at(who, i);
                self.apply_eot(e.caster_who().unwrap_or(who), who, e.eot, false);
            }
        }
    }

    /// One `DrunkEffect.Update` tick: a stack wears off.
    pub(crate) fn drunk_tick(&mut self, who: Who, i: usize, def: &EotDef, now: GameTime) -> bool {
        let l = &mut self.eot_list_mut(who).unwrap()[i];
        l.stacks = l.stacks.saturating_sub(1);
        l.next_tick = now.after_ms(def.tick_ms.max(1));
        let e = l.clone();
        if e.stacks > 0 {
            self.send_eot_buff(who, &e, def, true);
        }
        e.stacks > 0
    }

    // ------------------------------------------------------------ push and pull (7, 11)

    /// `TranslationEffect.Setup`: the direction away from the caster, the child effect on
    /// a creature (Flecha Cargada taunts it 3 s) and the first step right away.
    pub(crate) fn translation_start(&mut self, who: Who, uid: u32, def: &EotDef, caster: Who) -> bool {
        if let Who::Npc(m, n) = who {
            let immune = self.maps.get(&m).and_then(|i| i.npcs.get(&n)).and_then(|n| self.ao.npcs.get(n.def as u32)).is_some_and(|d| d.translation_immune);
            if immune {
                return false;
            }
            if def.apply_effect_id > 0 && !self.has_eot(who, def.apply_effect_id) {
                self.apply_eot(caster, who, def.apply_effect_id, false);
            }
        }
        let (Some((_, tx, ty)), Some((_, cx, cy))) = (self.who_pos(who), self.who_pos(caster)) else { return false };
        let (dx, dy) = ((tx - cx) as f64, (ty - cy) as f64);
        let len = (dx * dx + dy * dy).sqrt();
        if len == 0.0 {
            return false;
        }
        let Some(i) = self.eot_index(who, uid) else { return false };
        let l = &mut self.eot_list_mut(who).unwrap()[i];
        l.start = (tx, ty);
        l.dir = (dx / len, dy / len);
        l.ticks_left = def.ticks.saturating_sub(1);
        self.translation_step(who, uid)
    }

    /// `TranslationEffect.PerformAction`: one tile along the line; blocked = the end.
    pub(crate) fn translation_step(&mut self, who: Who, uid: u32) -> bool {
        let Some(i) = self.eot_index(who, uid) else { return false };
        let l = &mut self.eot_list_mut(who).unwrap()[i];
        l.step += 1;
        let (start, dir, step) = (l.start, l.dir, l.step as f64);
        let Some((_, x, y)) = self.who_pos(who) else { return false };
        let point = (dir.0 * step + start.0 as f64, dir.1 * step + start.1 as f64);
        let (sx, sy) = step_towards(point.0 - x as f64, point.1 - y as f64);
        self.shove(who, x + sx, y + sy)
    }

    /// `AttrackEffect.PerformAction`: one tile towards the caster (blocked = it waits).
    pub(crate) fn pull_step(&mut self, who: Who, caster: Who) -> bool {
        let (Some((_, x, y)), Some((_, cx, cy))) = (self.who_pos(who), self.who_pos(caster)) else { return false };
        let (sx, sy) = step_towards((cx - x) as f64, (cy - y) as f64);
        self.shove(who, x + sx, y + sy);
        true
    }

    /// `IsValidPos` + `TranslateUserPos` / `TranslateNpcChar`, then `ActivateTrap`.
    fn shove(&mut self, who: Who, nx: i32, ny: i32) -> bool {
        let Some((map, x, y)) = self.who_pos(who) else { return false };
        if (nx, ny) == (x, y) || !walk::in_bounds(nx, ny) || self.occupied(map, nx, ny) {
            return false;
        }
        let dir = dir_of(nx - x, ny - y);
        match who {
            Who::User(id) => {
                let sailing = self.player(id).is_some_and(|p| p.status.sailing.is_some());
                let m = &self.instance(map).map;
                if !walk::can_enter_as(m, nx, ny, dir, sailing) || m.exit(nx, ny).is_some() {
                    return false;
                }
                let p = self.player_mut(id);
                p.x = nx;
                p.y = ny;
                let d = p.dir;
                self.broadcast(map, &S2C::Moved { id, x: nx, y: ny, dir: d }, None);
            }
            Who::Npc(_, nid) => {
                let def = self.instance(map).npcs[&nid].def;
                let Some(d) = self.ao.npcs.get(def as u32) else { return false };
                if !self.npc_tile_ok(map, d, nx, ny, dir) {
                    return false;
                }
                let inst = self.maps.get_mut(&map).unwrap();
                inst.move_npc(nid, nx, ny);
                let d = inst.npcs[&nid].dir;
                self.broadcast(map, &S2C::Moved { id: nid, x: nx, y: ny, dir: d }, None);
            }
        }
        self.activate_trap(who, map, nx, ny);
        true
    }

    // ------------------------------------------------------------ delayed blast (12)

    /// `CreateDelayedBlast`: the blast waits `TickTime x distance` (the shot's flight).
    pub(crate) fn create_delayed_blast(&mut self, caster: Who, map: u16, x: i32, y: i32, eot: u16, obj: u16) -> bool {
        let table = self.eots();
        let Some(def) = table.get(&eot).filter(|d| d.kind == EotKind::DelayedBlast) else { return false };
        let Some((_, cx, cy)) = self.who_pos(caster) else { return false };
        let now = self.now();
        let wait = (def.tick_ms as f64 * dist((cx, cy), (x, y))) as u64;
        let e = ActiveEot { uid: super::eot::next_uid(), eot, caster: caster.caster(), ends: Some(now.after_ms(wait.max(1))), tile: Some((map, x, y)), obj, ..Default::default() };
        let Some(list) = self.eot_list_mut(caster) else { return false };
        list.push(e);
        true
    }

    /// `DelayedBlast.DoBlast`: FX and sound on the tile, then damage on the `Area` square,
    /// less away from the centre (`GetDamageForTile`). A player caster hits itself too.
    pub(crate) fn do_blast(&mut self, caster: Who, e: &ActiveEot, def: &EotDef) {
        let Some((map, tx, ty)) = e.tile else { return };
        if !self.maps.contains_key(&map) {
            return;
        }
        if def.on_hit_wav > 0 {
            self.broadcast(map, &S2C::Sound { wav: def.on_hit_wav, x: tx, y: ty }, None);
        }
        let area = def.area.max(1);
        let (x0, y0) = ((tx - area / 2).max(1), (ty - area / 2).max(1));
        let (x1, y1) = ((tx - area / 2 + area - 1).min(99), (ty - area / 2 + area - 1).min(99));
        let blast = ((area as f64 / 2.0).max(1.0)) * 2.0;
        let base = |w: &mut World| -> i32 {
            match caster {
                Who::User(_) => w.ao.objs.get(e.obj as u32).map_or(0, |o| w.rng.range(o.min_hit.min(o.max_hit), o.min_hit.max(o.max_hit))),
                Who::Npc(m, n) => {
                    let d = w.maps.get(&m).and_then(|i| i.npcs.get(&n)).and_then(|n| w.ao.npcs.get(n.def as u32)).map(|d| (d.min_hit, d.max_hit));
                    d.map_or(0, |(a, b)| w.rng.range(a.min(b), a.max(b)))
                }
            }
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let pct = (blast - dist((x, y), (tx, ty))) / blast;
                if let Some(t) = self.player_at(map, x, y) {
                    let ok = match caster {
                        Who::User(c) => c == t || self.spell_can_hit_user(c, t).is_ok(),
                        Who::Npc(m, n) => m == map && self.maps[&m].npcs.contains_key(&n) && self.npc_enemy(m, n, t),
                    };
                    if ok {
                        let dmg = ((base(self) as f64 * pct).max(0.0)) as i64;
                        let dmg = match caster {
                            Who::User(_) => ao_rules::eot::scale(ao_rules::eot::scale(dmg, self.attack_mods(caster, Who::User(t)).physical_factor(false)), self.modifiers(Who::User(t)).physical_taken()) as i32,
                            _ => ao_rules::eot::scale(dmg, self.modifiers(Who::User(t)).physical_taken()) as i32,
                        };
                        let src = match caster {
                            Who::User(c) => super::combat::Source::Player(c),
                            Who::Npc(m, n) => super::combat::Source::Npc(m, n),
                        };
                        if dmg > 0 {
                            self.hurt_player(t, dmg, src);
                        }
                    }
                } else if let Some(n) = self.instance(map).npc_at(x, y) {
                    // NPC cannons against creatures (NpcDamageToNpc) are not ported.
                    if let Who::User(c) = caster {
                        if self.user_may_hit_npc(c, map, n) {
                            let dmg = (base(self) as f64 * pct).max(0.0) as i64;
                            let dmg = ao_rules::eot::scale(ao_rules::eot::scale(dmg, self.attack_mods(caster, Who::Npc(map, n)).physical_factor(true)), self.modifiers(Who::Npc(map, n)).physical_taken()) as i32;
                            if dmg > 0 {
                                self.damage_npc(map, n, dmg, c);
                            }
                        }
                    }
                }
            }
        }
        if def.on_hit_fx > 0 {
            // FxPiso: the FX on the tile (shown on whoever stands there, if anyone).
            if let Some(t) = self.player_at(map, tx, ty).or_else(|| self.instance(map).npc_at(tx, ty)) {
                self.broadcast(map, &S2C::Fx { id: t, fx: def.on_hit_fx, loops: 0 }, None);
            }
        }
    }

    // ------------------------------------------------------------ traps (5)

    fn can_add_trap_at(&mut self, map: u16, x: i32, y: i32) -> bool {
        if !walk::in_bounds(x, y) || self.trap_at(map, x, y).is_some() || self.occupied(map, x, y) {
            return false;
        }
        let inst = self.instance(map);
        inst.map.blocked(x, y) & (BLOCK_ALL | FLAG_WATER) == 0 && inst.ground.at(x, y).is_none() && self.map_object_at(map, x, y).is_none()
    }

    /// The trap on a tile: (owner, index in its list).
    fn trap_at(&self, map: u16, x: i32, y: i32) -> Option<(u32, usize)> {
        let table = self.eots();
        self.instance(map).players.iter().copied().find_map(|pid| {
            let p = self.player(pid)?;
            p.status.eots.iter().position(|e| e.tile == Some((map, x, y)) && table.get(&e.eot).is_some_and(|d| d.kind == EotKind::Trap)).map(|i| (pid, i))
        })
    }

    /// `PlaceTrap`: within `TrapPlacementRange`, on a free tile; a fourth disables the
    /// oldest. The trap object is spent.
    fn place_trap(&mut self, id: u32, slot: u8, x: i32, y: i32, eot: u16) -> bool {
        let (map, px, py) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
        if dist((x, y), (px, py)) > self.cfg.world.trap_placement_range as f64 {
            self.console(id, "Estás demasiado lejos.", Tone::Info);
            return false;
        }
        if !self.can_add_trap_at(map, x, y) {
            self.console(id, "No podés poner una trampa ahí.", Tone::Warn);
            return false;
        }
        let table = self.eots();
        let Some(def) = table.get(&eot).filter(|d| d.kind == EotKind::Trap).cloned() else {
            self.console(id, "Esa trampa no funciona.", Tone::Warn);
            return false;
        };
        let p = self.player(id).unwrap();
        let traps: Vec<usize> = p.status.eots.iter().enumerate().filter(|(_, e)| table.get(&e.eot).is_some_and(|d| d.kind == EotKind::Trap)).map(|(i, _)| i).collect();
        if traps.len() >= MAX_TRAPS {
            // ElapsedTime is largest for the oldest: the one that ends first.
            let oldest = traps.into_iter().min_by_key(|i| p.status.eots[*i].ends).unwrap();
            self.remove_eot_at(Who::User(id), oldest);
        }
        let now = self.now();
        let e = ActiveEot { uid: super::eot::next_uid(), eot, caster: EotCaster::User(id), ends: Some(now.after_ms(def.tick_ms.max(1))), tile: Some((map, x, y)), ..Default::default() };
        self.send_eot_buff(Who::User(id), &e, &def, true);
        self.player_mut(id).status.eots.push(e);
        self.remove_from_slot(id, slot, 1);
        true
    }

    /// `ModMap.ActivateTrap` + `clsTrap.CanAffectTarget` + `trigger`: only creatures the
    /// owner may attack set a trap off (players never do).
    pub(crate) fn activate_trap(&mut self, target: Who, map: u16, x: i32, y: i32) {
        let Who::Npc(_, nid) = target else { return };
        let Some((owner, i)) = self.trap_at(map, x, y) else { return };
        if !self.user_may_hit_npc(owner, map, nid) {
            return;
        }
        let table = self.eots();
        let e = self.player(owner).unwrap().status.eots[i].clone();
        let Some(def) = table.get(&e.eot).cloned() else { return };
        self.broadcast(map, &S2C::Sound { wav: TRAP_WAV, x, y }, None);
        if def.on_hit_wav > 0 {
            self.broadcast(map, &S2C::Sound { wav: def.on_hit_wav, x, y }, None);
        }
        self.remove_eot_at(Who::User(owner), i);
        self.console(owner, "¡Tu trampa se ha activado!", Tone::Combat);
        let fx = |w: &World| {
            if def.on_hit_fx > 0 {
                w.broadcast(map, &S2C::Fx { id: nid, fx: def.on_hit_fx, loops: 0 }, None);
            }
        };
        match def.subtype {
            TRAP_PARALYZE => {
                // NPCs.Paralice with TrapParaliceDuration x 6.5 creature ticks; a spell's
                // Duration is x 39 of them for `Duration` seconds here.
                let ms = self.cfg.world.trap_paralyze_secs * 1000 / 6;
                let now = self.now();
                let n = self.npc_mut(map, nid);
                n.status.paralyzed_until = Some(now.after_ms(ms.max(1)));
                n.status.immobilized_until = None;
                n.path.clear();
                self.npc_attacked_by(map, nid, owner);
                fx(self);
            }
            TRAP_EXPLOSIVE => {
                let (a, b) = (def.power_min.abs(), def.power_max.abs());
                let dmg = self.rng.range(a.min(b), a.max(b));
                fx(self);
                self.damage_npc(map, nid, dmg, owner);
            }
            TRAP_POISON => {
                fx(self);
                if def.apply_effect_id > 0 {
                    self.apply_eot(Who::User(owner), target, def.apply_effect_id, false);
                }
                self.npc_attacked_by(map, nid, owner);
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ objects used on a tile

    /// `WorkLeftClick` with `TargetableItem` for `otUsableOntarget` (`UseInvItem` already
    /// checked the slot): traps and the hand cannon.
    pub(crate) fn on_use_item_at(&mut self, id: u32, slot: u8, tx: i32, ty: i32) {
        let Some(p) = self.player(id) else { return };
        let Some(item) = p.inv.get(slot) else { return };
        if p.combat.dead || p.hp <= 0 {
            return self.console(id, "¡¡Estás muerto!!", Tone::Info);
        }
        if (tx - p.x).abs() > ao_rules::magic::CHEAT_RANGE_X || (ty - p.y).abs() > ao_rules::magic::CHEAT_RANGE_Y {
            return self.strike(id, "objeto usado fuera de rango");
        }
        let ao = self.ao.clone();
        let Some(o) = ao.objs.get(item.obj as u32).filter(|o| o.obj_type == ObjType::UsableOnTarget) else { return };
        let now = self.now();
        if o.cooldown > 0 && p.inv.cooldowns.get(&o.cooldown_type).is_some_and(|t| now < *t) {
            return self.console(id, "Debes esperar para volver a usarlo.", Tone::Info);
        }
        let done = match o.subtype {
            USE_ON_TRAP => self.place_trap(id, slot, tx, ty, o.magic_effect.clamp(0, u16::MAX as i32) as u16),
            USE_ON_HAND_CANNON => self.use_hand_cannon(id, o, tx, ty),
            USE_ON_HARPOON | USE_ON_RESURRECT => {
                // The harpoon only slows (EOT19, speed: not on this server); the relic
                // resurrects (ResurrectWithItem, not ported).
                self.console(id, "Ese objeto todavía no está disponible.", Tone::Warn);
                false
            }
            _ => false,
        };
        if done && o.cooldown > 0 {
            self.player_mut(id).inv.cooldowns.insert(o.cooldown_type, now.after_ms(o.cooldown as u64));
        }
    }

    /// `UseHandCannon`: a delayed blast on the tile (the object is not spent).
    fn use_hand_cannon(&mut self, id: u32, o: &ao_formats::dat::obj::ObjData, tx: i32, ty: i32) -> bool {
        let p = self.player(id).unwrap();
        let (map, x, y) = (p.map, p.x, p.y);
        if dist((tx, ty), (x, y)) > HAND_CANNON_RANGE {
            self.console(id, "Estás demasiado lejos.", Tone::Info);
            return false;
        }
        let obj = self.ao.objs.iter().find(|(_, d)| std::ptr::eq(*d, o)).map_or(0, |(i, _)| i as u16);
        if !self.create_delayed_blast(Who::User(id), map, tx, ty, o.apply_effect_id.clamp(0, u16::MAX as i32) as u16, obj) {
            return false;
        }
        if o.snd1 > 0 {
            self.broadcast(map, &S2C::Sound { wav: o.snd1 as u16, x, y }, None);
        }
        let st = &mut self.player_mut(id).status;
        if st.invisible_until.take().is_some() {
            self.refresh_status(id);
        }
        true
    }

    // ------------------------------------------------------------ unequip (13)

    /// `CreateUnequip`: the slot stays locked 8 s (only with `bandit_unequip_bonus`).
    fn create_unequip(&mut self, id: u32, mask: u8) {
        if !self.cfg.world.bandit_unequip_bonus {
            return;
        }
        let table = self.eots();
        let Some(def) = table.get(&UNEQUIP_EOT).cloned() else { return };
        let now = self.now();
        let e = ActiveEot { uid: super::eot::next_uid(), eot: UNEQUIP_EOT, caster: EotCaster::User(id), ends: Some(now.after_ms(def.tick_ms.max(1))), slot_mask: mask, ..Default::default() };
        self.send_eot_buff(Who::User(id), &e, &def, true);
        self.player_mut(id).status.eots.push(e);
    }

    /// `flags.DisabledSlot` in `EquiparInvItem`: refused while an unequip effect holds it.
    pub(crate) fn equip_disabled(&self, id: u32, slot: u8) -> bool {
        let Some(p) = self.player(id) else { return false };
        let Some(o) = p.inv.get(slot).and_then(|i| self.ao.objs.get(i.obj as u32)) else { return false };
        let mask = match o.obj_type {
            ObjType::Weapon => SLOT_WEAPON,
            ObjType::Shield => SLOT_SHIELD,
            ObjType::Helmet => SLOT_HELM,
            _ => return false,
        };
        let locked = p.status.eots.iter().any(|e| e.slot_mask & mask != 0);
        if locked {
            self.console(id, "Todavía no podés equiparte eso.", Tone::Info);
        }
        locked
    }

    /// `PuedeDesequiparDeUnGolpe` + `ProbabilidadDesequipar` + `DesequiparObjetoDeUnGolpe`:
    /// bandits and thieves, bare-handed or with knuckles, may knock something off by the
    /// body part they hit.
    pub(crate) fn try_disarm(&mut self, a: u32, v: u32, place: i32) {
        use ao_formats::dat::balance::WeaponType;
        let Some(pa) = self.player(a) else { return };
        let weapon = pa.inv.equipped.weapon.and_then(|s| pa.inv.get(s)).and_then(|i| self.ao.objs.get(i.obj as u32));
        if weapon.is_some_and(|w| w.weapon_type != Some(WeaponType::Knuckle)) {
            return;
        }
        // GetSkillRequiredForWeapon: bare hands and knuckles both use Wrestling.
        use ao_formats::dat::balance::{Class, Skill};
        let wrestling = pa.stats.skills[Skill::Wrestling as usize - 1] as f64;
        let chance = match pa.class {
            c if c == Class::Bandit as u8 => (if self.cfg.world.bandit_unequip_bonus { 0.2 } else { 0.15 } * wrestling) as i32,
            c if c == Class::Thief as u8 => 33,
            _ => return,
        };
        if self.rng.range(1, 100) > chance {
            return;
        }
        let Some(pv) = self.player(v) else { return };
        let eq = pv.inv.equipped;
        let (helm, weapon, shield) = (eq.helmet.is_some(), eq.weapon.is_some(), eq.shield.is_some());
        // e_PartesCuerpo: 1 head; 4, 5, 6 arms and torso; 2, 3 legs.
        let pick = match place {
            1 => [(helm, SLOT_HELM), (weapon, SLOT_WEAPON), (shield, SLOT_SHIELD)],
            2 | 3 => [(shield, SLOT_SHIELD), (weapon, SLOT_WEAPON), (helm, SLOT_HELM)],
            _ => [(weapon, SLOT_WEAPON), (shield, SLOT_SHIELD), (helm, SLOT_HELM)],
        };
        let Some(&(_, mask)) = pick.iter().find(|(has, _)| *has) else {
            return self.console(a, "No has logrado desequipar ningun item a tu oponente!", Tone::Combat);
        };
        let (name_a, name_v) = (self.player(a).unwrap().name.clone(), pv.name.clone());
        let slot = match mask {
            SLOT_HELM => eq.helmet,
            SLOT_WEAPON => eq.weapon,
            _ => eq.shield,
        };
        if let Some(s) = slot {
            self.unequip(v, s);
        }
        let (mine, theirs) = match mask {
            SLOT_HELM => ("Has logrado desequipar el casco de tu oponente!".to_string(), format!("{name_a} te ha desequipado el casco.")),
            SLOT_WEAPON => ("Has logrado desarmar a tu oponente!".to_string(), format!("{name_a} te ha desarmado.")),
            _ => (format!("Has logrado desequipar el escudo de {name_v}."), format!("{name_a} te ha desequipado el escudo.")),
        };
        self.console(a, mine, Tone::Combat);
        self.console(v, theirs, Tone::Combat);
        self.create_unequip(v, mask);
    }
}

/// Official spells and objects (Hechizos.dat, obj.dat) through the real entry points.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::eot::tests::{T, MAP};
    use crate::world::inventory::InvItem;

    const BOW: u16 = 479;
    const ARROW: u16 = 480;

    fn eots(t: &T, who: Who) -> Vec<u16> {
        t.w.eot_list(who).map_or_else(Vec::new, |l| l.iter().map(|e| e.eot).collect())
    }

    fn bow_and_arrows(t: &mut T, id: u32, arrows: u32) {
        let p = t.w.player_mut(id);
        p.inv.slots[0] = Some(InvItem { obj: BOW, amount: 1 });
        p.inv.slots[1] = Some(InvItem { obj: ARROW, amount: arrows });
        p.inv.equipped.weapon = Some(1);
        p.inv.equipped.ammo = Some(2);
    }

    fn buffs(t: &mut T, id: u32) -> Vec<(u16, u32)> {
        t.drain(id).into_iter().filter_map(|m| if let S2C::Buff { icon, remaining_ms, .. } = m { Some((icon, remaining_ms)) } else { None }).collect()
    }

    #[test]
    fn provoke_taunts_around_and_weakens_blows_on_others() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        t.join(2, 3, 53, 50);
        t.learn(1, 246);
        t.dummy(900_001, 50, 55);
        t.drain(1);
        t.cast(1, 246, 50, 50);
        assert!(eots(&t, Who::User(1)).contains(&5), "the taunter carries EOT5");
        assert!(buffs(&mut t, 1).iter().any(|b| b.0 == 4 && b.1 > 0), "and sees it in the buff bar");
        let npc = Who::Npc(MAP, 900_001);
        assert!(eots(&t, npc).contains(&4));
        assert_eq!(t.w.npc_taunter(MAP, 900_001), Some(1));
        assert_eq!(t.w.instance(MAP).npcs[&900_001].target, Some(1), "the creature goes for the taunter");
        assert!(eots(&t, Who::User(2)).contains(&4));
        // EffectProvoked: -90 % against anyone but the taunter.
        assert!((t.w.attack_mods(Who::User(2), npc).physical_factor(true) - 0.1).abs() < 1e-6);
        assert!((t.w.attack_mods(Who::User(2), Who::User(1)).physical_factor(false) - 1.0).abs() < 1e-6);
        assert_eq!(t.w.modifiers(Who::User(2)).physical_bonus, 0.0, "nothing permanent");
        t.drain(1);
        t.advance(10_100);
        assert!(eots(&t, npc).is_empty() && eots(&t, Who::User(1)).is_empty(), "10 s");
        assert!(buffs(&mut t, 1).iter().any(|b| b.0 == 4 && b.1 == 0), "the icon goes away");
    }

    #[test]
    fn charged_arrow_hurts_and_pushes_a_creature_away() {
        let mut t = T::new();
        t.join(1, 4, 50, 50);
        t.learn(1, 248);
        bow_and_arrows(&mut t, 1, 10);
        t.dummy(900_001, 53, 50);
        t.cast(1, 248, 53, 50);
        let n = &t.w.instance(MAP).npcs[&900_001];
        assert!(n.hp < 100_000, "half a ranged hit");
        assert!(eots(&t, Who::Npc(MAP, 900_001)).contains(&12), "Flecha Cargada taunts it 3 s");
        t.advance(400);
        let n = &t.w.instance(MAP).npcs[&900_001];
        assert!(n.x > 53 && n.y == 50, "pushed along the line, now at {:?}", (n.x, n.y));
        assert!(n.x <= 58, "five steps at most");
        assert_eq!(t.w.player(1).unwrap().inv.get(2).unwrap().amount, 10, "the arrow is not spent");
        // Without a bow: RequireWeaponType.
        t.w.player_mut(1).inv.equipped.weapon = None;
        t.drain(1);
        let x = t.w.instance(MAP).npcs[&900_001].x;
        t.cast(1, 248, x, 50);
        assert!(t.said(1, "arma que requiere"));
    }

    #[test]
    fn cannon_blast_waits_for_the_shot_and_hits_the_square() {
        let mut t = T::new();
        t.join(1, 3, 56, 50);
        t.dummy(900_001, 50, 50);
        t.w.npc_mut(MAP, 900_001).attacked_by = Some(1);
        let hp = t.w.player(1).unwrap().hp;
        t.w.npc_cast_spell(MAP, 900_001, 1, 256);
        assert!(eots(&t, Who::Npc(MAP, 900_001)).contains(&20));
        assert_eq!(t.w.player(1).unwrap().hp, hp, "70 ms x 6 tiles of flight");
        t.advance(500);
        assert!(eots(&t, Who::Npc(MAP, 900_001)).is_empty());
        assert!(t.w.player(1).unwrap().hp < hp, "the blast went off on the tile");
    }

    #[test]
    fn hand_cannon_blasts_creatures_on_the_tile() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        t.dummy(900_001, 56, 50);
        t.w.player_mut(1).inv.slots[0] = Some(InvItem { obj: 3808, amount: 1 });
        t.w.on_use_item_at(1, 1, 56, 50);
        t.advance(600);
        assert!(t.w.instance(MAP).npcs[&900_001].hp < 100_000);
        assert!(t.w.player(1).unwrap().inv.get(1).is_some(), "the cannon is kept");
        // Too far (after the 15 s CDType 10 cooldown).
        t.advance(16_000);
        t.drain(1);
        t.w.on_use_item_at(1, 1, 62, 50);
        assert!(t.said(1, "demasiado lejos"));
    }

    #[test]
    fn traps_go_off_under_creatures_and_only_three_stay() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        for s in 0..4 {
            t.w.player_mut(1).inv.slots[s] = Some(InvItem { obj: 3784, amount: 1 });
        }
        t.w.on_use_item_at(1, 1, 52, 52);
        assert!(t.w.player(1).unwrap().inv.get(1).is_none(), "the trap object is spent");
        assert!(buffs(&mut t, 1).iter().any(|b| b.0 == 5));
        for (i, x) in [(2u8, 53), (3, 54), (4, 55)] {
            t.advance(800);
            t.w.player_mut(1).inv.cooldowns.clear();
            t.w.on_use_item_at(1, i, x, 52);
        }
        let traps: Vec<_> = t.w.player(1).unwrap().status.eots.iter().filter_map(|e| e.tile).collect();
        assert_eq!(traps.len(), 3, "the fourth disables the oldest: {traps:?}");
        assert!(!traps.contains(&(MAP, 52, 52)));
        // A creature steps on (54, 52).
        t.dummy(900_001, 54, 51);
        assert!(t.w.npc_move(MAP, 900_001, ao_rules::Dir::Down));
        let hp = t.w.instance(MAP).npcs[&900_001].hp;
        assert!((100_000 - 150..=100_000 - 100).contains(&hp), "100..150: {hp}");
        assert_eq!(t.w.player(1).unwrap().status.eots.iter().filter(|e| e.tile.is_some()).count(), 2);
        // Players never set it off; out of range is refused.
        t.w.player_mut(1).inv.slots[5] = Some(InvItem { obj: 3784, amount: 1 });
        t.advance(800);
        t.w.player_mut(1).inv.cooldowns.clear();
        t.drain(1);
        t.w.on_use_item_at(1, 6, 50, 58);
        assert!(t.said(1, "demasiado lejos"));
    }

    #[test]
    fn wine_stacks_drunk_and_five_make_you_dumb() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        for _ in 0..5 {
            t.w.add_or_reset_effect(1, 10);
        }
        let e = t.w.player(1).unwrap().status.eots.iter().find(|e| e.eot == 10).cloned().unwrap();
        assert_eq!(e.stacks, 5);
        assert!(t.w.player(1).unwrap().status.dumb_until.is_some(), "WriteDumb from 5");
        t.advance(20_100);
        assert_eq!(t.w.player(1).unwrap().status.eots.iter().find(|e| e.eot == 10).unwrap().stacks, 4, "one stack every 20 s");
        t.advance(80_000);
        assert!(t.w.player(1).unwrap().status.eots.iter().all(|e| e.eot != 10));
        // Drinking wine from the inventory goes through the same path.
        t.w.player_mut(1).inv.slots[0] = Some(InvItem { obj: 42, amount: 1 });
        t.w.on_use_item(1, 1);
        assert!(t.w.has_eot(Who::User(1), 10));
    }

    #[test]
    fn a_bandit_disarms_and_the_slot_stays_locked() {
        use ao_formats::dat::balance::Class;
        let mut t = T::new();
        t.join(1, Class::Bandit as u8, 50, 50);
        t.join(2, 3, 51, 50);
        let sword = t.first_obj(|o| o.obj_type == ObjType::Weapon && o.projectile == 0 && o.weapon_type == Some(ao_formats::dat::balance::WeaponType::Sword));
        t.give_equipped(2, 1, sword);
        t.w.player_mut(1).dir = Dir::Right;
        t.w.player_mut(2).hp = 1_000_000;
        t.w.player_mut(2).max_hp = 1_000_000;
        for _ in 0..200 {
            if t.w.player(2).unwrap().inv.equipped.weapon.is_none() {
                break;
            }
            t.advance(1_000);
            t.seq += 1;
            t.w.on_melee(1, t.seq);
        }
        assert!(t.w.player(2).unwrap().inv.equipped.weapon.is_none(), "20 % per hit with 100 wrestling");
        assert!(t.w.player(2).unwrap().status.eots.iter().any(|e| e.eot == 23 && e.slot_mask == SLOT_WEAPON));
        t.drain(2);
        t.w.on_equip_item(2, 1);
        assert!(t.w.player(2).unwrap().inv.equipped.weapon.is_none() && t.said(2, "Todavía no"));
        t.advance(8_100);
        // A sword any level may wear again.
        let ok = t.first_obj(|o| o.obj_type == ObjType::Weapon && o.projectile == 0 && o.weapon_type == Some(ao_formats::dat::balance::WeaponType::Sword) && o.max_level == 0 && o.min_level <= 50 && o.newbie == 0);
        t.w.player_mut(2).inv.slots[0] = Some(InvItem { obj: ok, amount: 1 });
        t.drain(2);
        t.w.on_equip_item(2, 1);
        let said: Vec<String> = t.drain(2).into_iter().filter_map(|m| if let S2C::Console { text, .. } = m { Some(text) } else { None }).collect();
        assert!(t.w.player(2).unwrap().inv.equipped.weapon.is_some(), "8 s later it can be equipped: {said:?}");
    }

    #[test]
    fn elemental_matrix_multiplies_every_pair() {
        let mut m = [[1.0f32; 4]; 4];
        m[0][2] = 2.0;
        m[1][2] = 0.5;
        m[0][3] = 3.0;
        assert_eq!(elemental_factor(0b0001, 0b0100, &m), 2.0);
        assert_eq!(elemental_factor(0b0011, 0b0100, &m), 1.0, "x2 then x0.5");
        assert_eq!(elemental_factor(0b0001, 0b1100, &m), 6.0);
        assert_eq!(elemental_factor(0, 0b0100, &m), 1.0);
        // Official matrix and toggle: off by default, x matrix when on.
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        t.dummy(900_001, 51, 50);
        assert_eq!(t.w.elemental_scale(1, MAP, 900_001, 100), 100);
    }

    #[test]
    fn push_and_pull_steps() {
        assert_eq!(step_towards(3.0, 0.1), (1, 0));
        assert_eq!(step_towards(2.0, 2.1), (1, 1));
        assert_eq!(step_towards(-0.2, -3.0), (0, -1));
        // AttrackEffect (no official effect uses it): a test definition pulls a creature.
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        t.dummy(900_001, 55, 50);
        let def = EotDef { kind: EotKind::PullTarget, ticks: 3, tick_ms: 100, ..Default::default() };
        let who = Who::Npc(MAP, 900_001);
        let e = ActiveEot { uid: 77, eot: 9999, caster: EotCaster::User(1), ticks_left: 3, ..Default::default() };
        t.w.eot_list_mut(who).unwrap().push(e);
        for _ in 0..3 {
            t.w.pull_step(who, Who::User(1));
        }
        let _ = def;
        assert_eq!(t.w.instance(MAP).npcs[&900_001].x, 52, "three tiles closer");
    }
}

impl ActiveEot {
    /// The caster as an effect source, when there is one.
    pub(crate) fn caster_who(&self) -> Option<Who> {
        match self.caster {
            EotCaster::User(id) => Some(Who::User(id)),
            EotCaster::Npc(m, n) => Some(Who::Npc(m, n)),
            EotCaster::None => None,
        }
    }
}

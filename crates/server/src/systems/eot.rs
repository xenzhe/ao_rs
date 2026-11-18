//! Effects over time on players and creatures, ported from the official AO20 server
//! (`EffectsOverTime.bas`: `CreateEffect`, `FindEffectOnTarget`, `ClearEffectList`, the
//! `Target*` callbacks, `ApplyEotModifier`; and the classes `UpdateHpOverTime`,
//! `UpdateManaOverTime`, `StatModifier`, `EffectProvoked`, `ProtectEffect`,
//! `BonusDamageEffect`, `TransformEffect`, `ApplyEffectOnHit`, `MultipleAttacks`,
//! `ApplyEffectToParty`).
//!
//! Differences with the VB, on purpose:
//! - Modifiers are rebuilt from the active effects (`modifiers`) instead of a running
//!   sum. `BonusDamageEffect` (Penitencia) calls `OnRemove` itself when its hits run out
//!   and the list calls it again on removal, so in AO20 the 30 % weakness is removed twice
//!   and the target ends up 30 % *tougher* for good; that cannot happen here.
//! - `ProtectEffect` and `StatModifier` ignore the new id on `Reset`: casting Protección
//!   Divina 3 over Protección Divina 1 only refreshed the small shield. Here the new
//!   effect replaces the old one.
//! - `MultipleAttacks` never reset its blow timer, so both extra blows went out on
//!   consecutive frames; here they are 100 ms apart.
//!
//! Not ported (the spells that use them still say "todavía no está disponible" and cost
//! nothing): taunt (3), traps (5), drunk (6), push/pull (7, 11), delayed blast (12),
//! unequip (13, behind a feature flag off by default), and movement speed: this server
//! has a fixed walking pace, so speed-only effects (Eco Veloz, Desplazamiento Veloz,
//! Acelerar Mascotas) do nothing and stay refused.
use crate::clock::GameTime;
use crate::world::spellbook::{ActiveEot, EotCaster, Transform};
use crate::world::World;
use ao_formats::dat::eots::{EotDef, EotKind, EotModifiers};
use ao_protocol::{Tone, S2C};
use ao_rules::eot::{self as rules, EffectKey, Modifiers};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) type EotTable = Arc<HashMap<u16, EotDef>>;

/// `Dat/EffectsOverTime.dat` of a resource set, read once.
pub(crate) fn eot_table(recursos: &Path) -> EotTable {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, EotTable>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    cache
        .entry(recursos.to_path_buf())
        .or_insert_with(|| {
            // Windows-1252 file: only ASCII keys and numbers matter.
            let bytes = std::fs::read(recursos.join("Dat").join("EffectsOverTime.dat")).unwrap_or_default();
            Arc::new(ao_formats::dat::eots::parse(&String::from_utf8_lossy(&bytes)))
        })
        .clone()
}

fn next_uid() -> u32 {
    static UID: AtomicU32 = AtomicU32::new(0);
    UID.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
}

/// An effect target or caster (`t_AnyReference`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Who {
    User(u32),
    Npc(u16, u32),
}

impl Who {
    pub fn caster(self) -> EotCaster {
        match self {
            Who::User(id) => EotCaster::User(id),
            Who::Npc(m, n) => EotCaster::Npc(m, n),
        }
    }
}

/// `MultipleAttacks`: delay between the extra blows.
const EXTRA_BLOW_MS: u64 = 100;
/// `e_StatusMask.eCastOnlyOnSelf` in `ApplyStatusMask`.
const MASK_CAST_ONLY_ON_SELF: i32 = 8;

fn kind_ported(k: EotKind) -> bool {
    use EotKind::*;
    matches!(k, ModifyHealth | ModifyMana | ApplyModifiers | Taunted | Shield | DamageBonus | Transform | ApplyEffectOnHit | MultipleAttacks | ApplyEffectToParty)
}

/// Kinds whose `EotModifiers` hold while they last (`ApplyEotModifier` in their `Setup`).
fn applies_modifiers(k: EotKind) -> bool {
    use EotKind::*;
    matches!(k, ApplyModifiers | Taunted | Shield | DamageBonus | Transform | ApplyEffectToParty)
}

fn has_non_speed_effect(d: &EotDef) -> bool {
    let m = EotModifiers { speed: 0.0, ..d.mods };
    m != EotModifiers::default() || d.cc_immunity
}

/// Does effect `id` do anything on this server? (Kind ported, and not only movement speed.)
pub(crate) fn eot_works(table: &HashMap<u16, EotDef>, id: u16) -> bool {
    fn go(t: &HashMap<u16, EotDef>, id: u16, depth: u8) -> bool {
        let Some(d) = t.get(&id) else { return false };
        if !kind_ported(d.kind) || depth > 3 {
            return false;
        }
        match d.kind {
            EotKind::ApplyModifiers | EotKind::Taunted => has_non_speed_effect(d),
            EotKind::ApplyEffectOnHit => go(t, d.apply_effect_id, depth + 1),
            EotKind::ApplyEffectToParty => go(t, d.apply_effect_id, depth + 1) || has_non_speed_effect(d),
            _ => true,
        }
    }
    go(table, id, 0)
}

impl World {
    pub(crate) fn eots(&self) -> EotTable {
        eot_table(&self.recursos)
    }

    fn eot_alive(&self, who: Who) -> bool {
        match who {
            Who::User(id) => self.player(id).is_some_and(|p| !p.combat.dead && p.hp > 0),
            Who::Npc(m, n) => self.maps.get(&m).and_then(|i| i.npcs.get(&n)).is_some_and(|n| n.alive()),
        }
    }

    fn eot_list(&self, who: Who) -> Option<&Vec<ActiveEot>> {
        match who {
            Who::User(id) => self.player(id).map(|p| &p.status.eots),
            Who::Npc(m, n) => self.maps.get(&m)?.npcs.get(&n).map(|n| &n.status.eots),
        }
    }

    fn eot_list_mut(&mut self, who: Who) -> Option<&mut Vec<ActiveEot>> {
        match who {
            Who::User(id) => {
                self.player(id)?;
                Some(&mut self.player_mut(id).status.eots)
            }
            Who::Npc(m, n) => self.maps.get_mut(&m)?.npcs.get_mut(&n).map(|n| &mut n.status.eots),
        }
    }

    // ------------------------------------------------------------ queries

    /// `t_Modifiers` of a player or creature: the sum of its active buffs and debuffs.
    pub(crate) fn modifiers(&self, who: Who) -> Modifiers {
        let mut m = Modifiers::default();
        let Some(list) = self.eot_list(who).filter(|l| !l.is_empty()) else { return m };
        let table = self.eots();
        for e in list {
            let Some(d) = table.get(&e.eot) else { continue };
            if applies_modifiers(d.kind) {
                m.add(&d.mods, e.scale, d.physical_bonus_pve_only);
            } else if d.kind == EotKind::MultipleAttacks {
                // MultipleAttacks.Setup: only PhysicalDamageDone, never PvE-only.
                m.add(&EotModifiers { physical_done: d.mods.physical_done, ..Default::default() }, 0.0, false);
            }
        }
        m
    }

    /// `eCCInmunity` from an active effect (no paralysis or immobilisation).
    pub(crate) fn cc_immune(&self, who: Who) -> bool {
        let table = self.eots();
        self.eot_list(who).is_some_and(|l| l.iter().any(|e| table.get(&e.eot).is_some_and(|d| d.cc_immunity && applies_modifiers(d.kind))))
    }

    /// `flags.ActiveTransform`: npcs.dat number of the form worn (0 = none).
    pub(crate) fn active_transform(&self, id: u32) -> u16 {
        self.player(id).and_then(|p| p.status.transform).map_or(0, |t| t.npc)
    }

    /// `eCastOnlyOnSelf` (wolf, bear and spirit forms).
    pub(crate) fn cast_only_on_self(&self, id: u32) -> bool {
        let table = self.eots();
        self.eot_list(Who::User(id)).is_some_and(|l| {
            l.iter().any(|e| table.get(&e.eot).is_some_and(|d| d.kind == EotKind::Transform && (d.cast_only_on_self || d.status_mask & MASK_CAST_ONLY_ON_SELF != 0)))
        })
    }

    /// `FindEffectOnTarget`: index of the effect on `who` that a new `eot` from `caster`
    /// would collide with.
    pub(crate) fn find_eot(&self, who: Who, eot: u16, caster: EotCaster) -> Option<usize> {
        let table = self.eots();
        let def = table.get(&eot)?;
        self.eot_list(who)?.iter().position(|e| {
            let Some(d) = table.get(&e.eot) else { return false };
            let key = EffectKey { eot: e.eot, kind: d.kind, shared_type_id: d.shared_type_id, caster: e.caster.key() };
            rules::same_effect(def, eot, caster.key(), &key)
        })
    }

    pub(crate) fn has_eot(&self, who: Who, eot: u16) -> bool {
        self.eot_list(who).is_some_and(|l| l.iter().any(|e| e.eot == eot))
    }

    /// Cast-time part of `HandleHechizoUsuario` / `HandleHechizoNPC`: the effect must exist
    /// here, and a target already under it is refused unless it can be overridden.
    pub(crate) fn eot_cast_check(&self, caster: Who, target: Who, eot: u16) -> Result<(), &'static str> {
        let table = self.eots();
        if !eot_works(&table, eot) {
            return Err("Ese hechizo todavía no está disponible.");
        }
        if self.find_eot(target, eot, caster.caster()).is_some() && !table[&eot].override_ {
            return Err("El objetivo ya está afectado por ese efecto.");
        }
        Ok(())
    }

    // ------------------------------------------------------------ create / remove

    /// `CreateEffect`, or `Effect.Reset` when the target has a colliding one. Returns false
    /// when nothing was applied; `from_cast` and the same song or form already active
    /// switches it off (their `Reset` sets `RemoveMe`), which costs nothing.
    pub(crate) fn apply_eot(&mut self, caster: Who, target: Who, eot: u16, from_cast: bool) -> bool {
        let table = self.eots();
        let Some(def) = table.get(&eot).cloned() else { return false };
        if !kind_ported(def.kind) || !self.eot_alive(target) {
            return false;
        }
        if let Some(i) = self.find_eot(target, eot, caster.caster()) {
            let old = self.eot_list(target).unwrap()[i].eot;
            if !def.override_ {
                return false;
            }
            if from_cast && old == eot && matches!(def.kind, EotKind::ApplyEffectToParty | EotKind::Transform) {
                self.remove_eot_at(target, i);
                return false;
            }
            self.remove_eot_at(target, i);
        }
        let now = self.now();
        let e = self.new_eot(caster, eot, &def, now);
        let uid = e.uid;
        self.eot_list_mut(target).unwrap().push(e);
        self.on_eot_start(target, uid, &def);
        true
    }

    fn new_eot(&mut self, caster: Who, eot: u16, def: &EotDef, now: GameTime) -> ActiveEot {
        use EotKind::*;
        let total = matches!(def.kind, ApplyModifiers | Taunted | Shield | DamageBonus | ApplyEffectOnHit | MultipleAttacks);
        let shield = if def.kind == Shield { self.rng.range(def.power_min.min(def.power_max), def.power_min.max(def.power_max)) } else { 0 };
        // StatModifier.Setup: AffectedByMagicBonus scales by the caster's weapon.
        let scale = match caster {
            Who::User(id) if def.affected_by_magic_bonus => self
                .player(id)
                .and_then(|p| p.inv.equipped.weapon.and_then(|s| p.inv.get(s)))
                .and_then(|it| self.ao.objs.get(it.obj as u32))
                .map_or(0.0, |o| o.magic_damage_bonus as f32 / 100.0),
            _ => 0.0,
        };
        ActiveEot {
            uid: next_uid(),
            eot,
            caster: caster.caster(),
            ticks_left: def.ticks,
            next_tick: now.after_ms(def.tick_ms.max(1)),
            ends: total.then(|| now.after_ms(def.tick_ms.max(1))),
            shield,
            hits_left: if matches!(def.kind, DamageBonus | ApplyEffectOnHit | MultipleAttacks) { def.ticks } else { 0 },
            scale,
            pending_blows: 0,
            blow_at: now,
        }
    }

    fn eot_index(&self, who: Who, uid: u32) -> Option<usize> {
        self.eot_list(who)?.iter().position(|e| e.uid == uid)
    }

    fn on_eot_start(&mut self, who: Who, uid: u32, def: &EotDef) {
        if def.cc_immunity {
            if let Who::User(id) = who {
                let st = &mut self.player_mut(id).status;
                st.paralyzed_until = None;
                st.immobilized_until = None;
            } else if let Who::Npc(m, n) = who {
                let n = self.npc_mut(m, n);
                n.status.paralyzed_until = None;
                n.status.immobilized_until = None;
            }
        }
        match (def.kind, who) {
            (EotKind::Transform, Who::User(id)) => self.transform_into(id, def.npc_id),
            (EotKind::ApplyEffectToParty, Who::User(id)) if !self.party_song_pulse(id, def) => {
                if let Some(i) = self.eot_index(who, uid) {
                    self.remove_eot_at(who, i);
                }
            }
            _ => {}
        }
    }

    /// Removes effect `i` and undoes what it changed (`RemoveEffectAtPos` + `OnRemove`).
    pub(crate) fn remove_eot_at(&mut self, who: Who, i: usize) {
        let Some(list) = self.eot_list_mut(who) else { return };
        if i >= list.len() {
            return;
        }
        let e = list.remove(i);
        let table = self.eots();
        let Some(def) = table.get(&e.eot) else { return };
        if let (EotKind::Transform, Who::User(id)) = (def.kind, who) {
            self.transform_back(id);
        }
    }

    /// `ClearEffectList`: all of them (death, `ClearForDeath`; none is `KeepAfterDead`).
    pub(crate) fn clear_eots(&mut self, who: Who) {
        while self.eot_list(who).is_some_and(|l| !l.is_empty()) {
            self.remove_eot_at(who, 0);
        }
    }

    // ------------------------------------------------------------ transform

    /// `TransformEffect.TransformChacter`: the player wears the creature's body and head.
    fn transform_into(&mut self, id: u32, npc: u16) {
        let Some((body, head)) = self.ao.npcs.get(npc as u32).map(|d| (d.body.clamp(0, u16::MAX as i32) as u16, d.head.clamp(0, u16::MAX as i32) as u16)) else { return };
        let p = self.player_mut(id);
        let saved = p.status.transform.map_or((p.body, p.head), |t| (t.body, t.head));
        p.status.transform = Some(Transform { npc, body: saved.0, head: saved.1 });
        p.body = body;
        p.head = head;
        self.send_look(id);
        self.console(id, "Te has transformado.", Tone::Good);
    }

    /// `ReturnCharToNormal`: back to the body the equipment gives and the own head.
    fn transform_back(&mut self, id: u32) {
        let Some(t) = self.player(id).and_then(|p| p.status.transform) else { return };
        let prof = ao_formats::profile::for_recursos(&self.recursos);
        let ao = self.ao.clone();
        let p = self.player_mut(id);
        p.status.transform = None;
        if !p.combat.dead {
            let armor = p.inv.equipped.armor.and_then(|s| p.inv.get(s)).and_then(|i| ao.objs.get(i.obj as u32)).map(|o| o.clothing.max(0) as u16).filter(|b| *b > 0);
            p.body = armor.unwrap_or_else(|| if t.body > 0 { t.body } else { prof.naked_body(p.race, p.gender) });
            p.head = t.head;
        }
        self.send_look(id);
        self.console(id, "Has vuelto a tu forma normal.", Tone::Info);
    }

    // ------------------------------------------------------------ party songs

    /// `ApplyEffectToParty.PerformAction`: the singer gets `ApplyEffectId`, the members of
    /// its party on the map within `Area` tiles get `SecondaryEffectId`; then the song
    /// costs `TickManaConsumption`. False when the mana ran out (the song ends).
    fn party_song_pulse(&mut self, id: u32, def: &EotDef) -> bool {
        let Some(p) = self.player(id) else { return false };
        let (map, x, y) = (p.map, p.x, p.y);
        if def.tick_fx > 0 {
            self.broadcast(map, &S2C::Fx { id, fx: def.tick_fx, loops: 0 }, None);
        }
        let mut members = self.party_members_online(id);
        if members.is_empty() {
            members.push(id);
        }
        for m in members {
            let Some(mp) = self.player(m) else { continue };
            let dist = (((mp.x - x).pow(2) + (mp.y - y).pow(2)) as f64).sqrt();
            if mp.map != map || (m != id && dist >= def.area as f64) {
                continue;
            }
            let child = if m == id { def.apply_effect_id } else { def.secondary_effect_id };
            if child > 0 {
                self.apply_eot(Who::User(id), Who::User(m), child, false);
            }
        }
        // ModifyMana / ModifyStamina with CancelIfNotEnought.
        let p = self.player_mut(id);
        if p.mana < def.tick_mana || p.stats.sta < def.tick_stamina {
            return false;
        }
        p.mana -= def.tick_mana;
        p.stats.sta -= def.tick_stamina;
        self.send_stats(id);
        true
    }

    // ------------------------------------------------------------ tick

    /// `UpdateEffectOverTime` for one target.
    pub(crate) fn tick_eots(&mut self, who: Who, now: GameTime) {
        if self.eot_list(who).is_none_or(|l| l.is_empty()) {
            return;
        }
        if !self.eot_alive(who) {
            return self.clear_eots(who);
        }
        let table = self.eots();
        let mut i = 0;
        while let Some(e) = self.eot_list(who).and_then(|l| l.get(i).cloned()) {
            let Some(def) = table.get(&e.eot) else {
                self.remove_eot_at(who, i);
                continue;
            };
            if e.ends.is_some_and(|t| now >= t) || !self.eot_requirements_ok(who, def) {
                self.remove_eot_at(who, i);
                continue;
            }
            let keep = match def.kind {
                EotKind::ModifyHealth | EotKind::ModifyMana | EotKind::ApplyEffectToParty | EotKind::Transform if now >= e.next_tick => {
                    if e.ticks_left == 0 {
                        false
                    } else {
                        let l = &mut self.eot_list_mut(who).unwrap()[i];
                        l.ticks_left -= 1;
                        l.next_tick = now.after_ms(def.tick_ms.max(1));
                        let alive = self.eot_pulse(who, &e, def);
                        if !self.eot_alive(who) {
                            return;
                        }
                        alive && (e.ticks_left > 1 || matches!(def.kind, EotKind::ApplyEffectToParty | EotKind::Transform))
                    }
                }
                EotKind::MultipleAttacks if e.pending_blows > 0 && now >= e.blow_at => {
                    let l = &mut self.eot_list_mut(who).unwrap()[i];
                    l.pending_blows -= 1;
                    l.blow_at = now.after_ms(EXTRA_BLOW_MS);
                    let done = l.pending_blows == 0 && l.hits_left == 0;
                    if let Who::User(id) = who {
                        self.attack_front(id);
                    }
                    if self.eot_index(who, e.uid) != Some(i) {
                        continue;
                    }
                    !done
                }
                _ => true,
            };
            if !keep {
                if let Some(j) = self.eot_index(who, e.uid) {
                    self.remove_eot_at(who, j);
                }
                continue;
            }
            i += 1;
        }
    }

    /// `StatModifier.Update`: `RequireWeapon/Shield/Armor/Helm` and `RequireTransform`
    /// are checked all the time (unequipping ends the buff).
    fn eot_requirements_ok(&self, who: Who, def: &EotDef) -> bool {
        let Who::User(id) = who else { return true };
        let Some(p) = self.player(id) else { return false };
        let eq = &p.inv.equipped;
        if def.require_weapon && eq.weapon.is_none() || def.require_shield && eq.shield.is_none() || def.require_armor && eq.armor.is_none() || def.require_helm && eq.helmet.is_none() {
            return false;
        }
        if def.require_transform > 0 && p.status.transform.map_or(0, |t| t.npc) != def.require_transform {
            return false;
        }
        // TransformEffect.TargetChangeTerrain: no forms on a boat.
        !(def.kind == EotKind::Transform && p.status.sailing.is_some())
    }

    /// One tick of a health, mana, song or form effect. False ends the effect.
    fn eot_pulse(&mut self, who: Who, e: &ActiveEot, def: &EotDef) -> bool {
        match def.kind {
            EotKind::ModifyHealth => {
                self.hp_over_time_tick(who, e, def);
                true
            }
            EotKind::ModifyMana => {
                if let Who::User(id) = who {
                    let amount = self.rng.range(def.power_min.min(def.power_max), def.power_min.max(def.power_max));
                    if def.tick_fx > 0 {
                        let map = self.player(id).unwrap().map;
                        self.broadcast(map, &S2C::Fx { id, fx: def.tick_fx, loops: 0 }, None);
                    }
                    let p = self.player_mut(id);
                    p.mana = (p.mana + amount).clamp(0, p.max_mana);
                    self.send_stats(id);
                }
                true
            }
            EotKind::ApplyEffectToParty => match who {
                Who::User(id) => self.party_song_pulse(id, def),
                _ => false,
            },
            EotKind::Transform => {
                let Who::User(id) = who else { return false };
                let p = self.player_mut(id);
                if p.mana < def.tick_mana || p.stats.sta < def.tick_stamina {
                    return false;
                }
                p.mana -= def.tick_mana;
                p.stats.sta -= def.tick_stamina;
                self.send_stats(id);
                true
            }
            _ => true,
        }
    }

    /// `UpdateHpOverTime.PerformAction`: the roll scaled by the caster's bonuses
    /// (`AffectedBy*` flags) and the target's reductions, then `DoDamageOrHeal`.
    fn hp_over_time_tick(&mut self, who: Who, e: &ActiveEot, def: &EotDef) {
        let caster = match e.caster {
            EotCaster::User(id) if self.player(id).is_some() => Some(Who::User(id)),
            EotCaster::Npc(m, n) if self.eot_alive(Who::Npc(m, n)) => Some(Who::Npc(m, n)),
            _ => None,
        };
        let cm = caster.map(|c| self.modifiers(c)).unwrap_or_default();
        let mut bonus = 1.0f64;
        if def.affected_by_magic_bonus {
            bonus += cm.magic_bonus as f64;
        }
        if def.affected_by_physical_bonus {
            bonus += cm.physical_bonus as f64;
            if matches!(who, Who::Npc(..)) {
                bonus += cm.physical_bonus_pve as f64;
            }
        }
        if def.affected_by_magic_healing {
            bonus += cm.magic_healing as f64;
        }
        let roll = self.rng.range(def.power_min.min(def.power_max), def.power_min.max(def.power_max));
        let tm = self.modifiers(who);
        let mut hp = rules::scale(roll as i64, bonus);
        if def.affected_by_magic_reduction {
            hp = rules::scale(hp, tm.magic_taken());
        }
        if def.affected_by_physical_reduction {
            hp = rules::scale(hp, tm.physical_taken());
        }
        if def.affected_by_self_healing {
            hp = rules::scale(hp, tm.self_healing_factor());
        }
        let hp = hp as i32;
        match who {
            Who::User(id) => {
                let map = self.player(id).unwrap().map;
                if def.tick_fx > 0 {
                    self.broadcast(map, &S2C::Fx { id, fx: def.tick_fx, loops: 0 }, None);
                }
                if hp >= 0 {
                    self.heal_player(id, hp);
                } else {
                    let src = match caster {
                        Some(Who::User(c)) => super::combat::Source::Player(c),
                        Some(Who::Npc(m, n)) => super::combat::Source::Npc(m, n),
                        None => super::combat::Source::Named("Un efecto"),
                    };
                    self.hurt_player(id, -hp, src);
                }
            }
            Who::Npc(map, nid) => {
                if def.tick_fx > 0 {
                    self.broadcast(map, &S2C::Fx { id: nid, fx: def.tick_fx, loops: 0 }, None);
                }
                if hp >= 0 {
                    let n = self.npc_mut(map, nid);
                    n.hp = (n.hp + hp).min(n.max_hp);
                    let (hp, max_hp) = (n.hp, n.max_hp);
                    self.broadcast(map, &S2C::Hp { id: nid, hp, max_hp }, None);
                } else {
                    let attacker = if let Some(Who::User(c)) = caster { c } else { 0 };
                    self.damage_npc(map, nid, -hp, attacker);
                }
            }
        }
    }

    // ------------------------------------------------------------ combat callbacks

    /// `TargetApplyDamageReduction` + `TargetWasDamaged` (`DoDamageOrHeal` with a negative
    /// amount): shields soak the hit, damage-bonus debuffs count it, stat buffs show their
    /// `OnHitFx`. Returns what is left of `dmg`.
    pub(crate) fn eot_on_damaged(&mut self, who: Who, mut dmg: i32) -> i32 {
        if dmg <= 0 || self.eot_list(who).is_none_or(|l| l.is_empty()) {
            return dmg;
        }
        let table = self.eots();
        let (map, fx_id) = match who {
            Who::User(id) => (self.player(id).map_or(0, |p| p.map), id),
            Who::Npc(m, n) => (m, n),
        };
        let mut i = 0;
        while let Some(e) = self.eot_list(who).and_then(|l| l.get(i).cloned()) {
            let Some(def) = table.get(&e.eot) else { break };
            if def.on_hit_fx > 0 && matches!(def.kind, EotKind::ApplyModifiers | EotKind::Shield) {
                self.broadcast(map, &S2C::Fx { id: fx_id, fx: def.on_hit_fx, loops: 0 }, None);
            }
            match def.kind {
                EotKind::Shield if dmg > 0 => {
                    if e.shield > dmg {
                        self.eot_list_mut(who).unwrap()[i].shield -= dmg;
                        dmg = 0;
                    } else {
                        dmg -= e.shield;
                        self.remove_eot_at(who, i);
                        if let Who::User(id) = who {
                            self.console(id, "Tu protección se ha desvanecido.", Tone::Info);
                        }
                        continue;
                    }
                }
                EotKind::DamageBonus => {
                    let l = &mut self.eot_list_mut(who).unwrap()[i];
                    l.hits_left = l.hits_left.saturating_sub(1);
                    if l.hits_left == 0 {
                        self.remove_eot_at(who, i);
                        continue;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        dmg
    }

    /// `ApplyEffectOnHit.TargetDidHit`: the attacker's on-hit effects land on `victim`
    /// (a child effect, e.g. Filo de Toxina's poison or Desgarro's bleeding).
    pub(crate) fn eot_on_hit(&mut self, attacker: u32, victim: Who) {
        let table = self.eots();
        let holders: Vec<ActiveEot> = self.eot_list(Who::User(attacker)).map_or_else(Vec::new, |l| l.iter().filter(|e| table.get(&e.eot).is_some_and(|d| d.kind == EotKind::ApplyEffectOnHit)).cloned().collect());
        for h in holders {
            let def = &table[&h.eot];
            if !self.eot_alive(victim) {
                return;
            }
            let child = def.apply_effect_id;
            match self.find_eot(victim, child, EotCaster::User(attacker)) {
                None => {
                    self.apply_eot(Who::User(attacker), victim, child, false);
                }
                Some(j) if def.override_ => {
                    self.remove_eot_at(victim, j);
                    self.apply_eot(Who::User(attacker), victim, child, false);
                }
                Some(_) => {}
            }
            if def.ticks > 0 {
                if let Some(i) = self.eot_index(Who::User(attacker), h.uid) {
                    let l = &mut self.eot_list_mut(Who::User(attacker)).unwrap()[i];
                    l.hits_left = l.hits_left.saturating_sub(1);
                    if l.hits_left == 0 {
                        self.remove_eot_at(Who::User(attacker), i);
                    }
                }
            }
        }
    }

    /// `MultipleAttacks.TargetWillAttackPosition`: a melee swing queues the extra blows.
    /// Only bare hands or knuckles; any other weapon ends the effect.
    pub(crate) fn eot_will_attack(&mut self, id: u32) {
        let table = self.eots();
        let Some(i) = self.eot_list(Who::User(id)).and_then(|l| l.iter().position(|e| table.get(&e.eot).is_some_and(|d| d.kind == EotKind::MultipleAttacks))) else { return };
        let knuckles = {
            let p = self.player(id).unwrap();
            match p.inv.equipped.weapon.and_then(|s| p.inv.get(s)).and_then(|it| self.ao.objs.get(it.obj as u32)) {
                None => true,
                Some(w) => w.weapon_type == Some(ao_formats::dat::balance::WeaponType::Knuckle),
            }
        };
        if !knuckles {
            return self.remove_eot_at(Who::User(id), i);
        }
        let blows = table[&self.eot_list(Who::User(id)).unwrap()[i].eot].subtype.clamp(0, 10) as u8;
        let now = self.now();
        let l = &mut self.eot_list_mut(Who::User(id)).unwrap()[i];
        if l.hits_left == 0 {
            return;
        }
        l.hits_left -= 1;
        l.pending_blows = blows;
        l.blow_at = now.after_ms(EXTRA_BLOW_MS);
    }

    /// Ticks the effects of every creature on `map` (called with their status).
    pub(crate) fn tick_npc_eots(&mut self, map: u16, now: GameTime) {
        let Some(inst) = self.maps.get(&map) else { return };
        let ids: Vec<u32> = inst.npcs.iter().filter(|(_, n)| !n.status.eots.is_empty()).map(|(id, _)| *id).collect();
        for nid in ids {
            self.tick_eots(Who::Npc(map, nid), now);
        }
    }
}

/// Spells of the official Hechizos.dat cast through `on_cast` (the real entry point), on
/// a world with a virtual clock.
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::clock::{Clock, VirtualClock};
    use crate::db::{CharacterRecord, CharacterState};
    use crate::world::inventory::InvItem;
    use crate::world::Session;
    use crate::Settings;
    use ao_formats::dat::obj::ObjType;
    use ao_protocol::FrameReader;
    use ao_rules::Dir;
    use tokio::sync::mpsc::UnboundedReceiver;

    pub const MAP: u16 = 2;

    pub struct T {
        pub w: World,
        pub clock: VirtualClock,
        pub rx: HashMap<u32, UnboundedReceiver<Vec<u8>>>,
        pub seq: u32,
    }

    impl T {
        pub fn new() -> T {
            let mut s = Settings::defaults();
            s.data.balance.creatures_per_map = 0;
            let clock = VirtualClock::new();
            clock.set(GameTime(10_000));
            s.clock = Clock::Virtual(clock.clone());
            let mut w = World::new(s);
            assert!(w.ensure_map(MAP));
            T { w, clock, rx: HashMap::new(), seq: 0 }
        }

        /// A level-50 character with plenty of mana, stamina and Magic, no spells.
        pub fn join(&mut self, id: u32, class: u8, x: i32, y: i32) {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            self.rx.insert(id, rx);
            self.w.sessions.insert(id, Session::new(tx, "127.0.0.1:9".parse().unwrap()));
            let r = CharacterRecord { id: 0, account_id: 0, name: format!("P{id}"), race: 1, class, gender: 1, body: 1, head: 7, level: 50, exp: 0, hp: 500, max_hp: 500, mana: 5000, max_mana: 5000, map: MAP, x, y };
            assert!(self.w.spawn_player(id, &r, &CharacterState::default(), None));
            let p = self.w.player_mut(id);
            p.stats.sta = 20_000;
            p.stats.max_sta = 20_000;
            p.stats.skills = [100; crate::world::stats::NUM_SKILLS];
            p.faction.safe = false;
            p.spells.slots.iter_mut().for_each(|s| *s = 0);
        }

        pub fn learn(&mut self, id: u32, spell: u16) {
            self.w.player_mut(id).spells.add(spell);
        }

        /// `C2S::Cast` on tile (x, y), after the global interval.
        /// Cooldowns are reset: these tests are about the effects, not the timers.
        pub fn cast(&mut self, id: u32, spell: u16, x: i32, y: i32) {
            self.advance(100);
            let sp = &mut self.w.player_mut(id).spells;
            sp.cooldowns.clear();
            sp.next_cast = GameTime::ZERO;
            self.seq += 1;
            self.w.on_cast(id, spell, x, y, self.seq);
        }

        pub fn advance(&mut self, ms: u64) {
            let step = 50;
            let mut left = ms;
            while left > 0 {
                let d = left.min(step);
                self.clock.set(GameTime(self.w.now().0 + d));
                left -= d;
                let now = self.w.now();
                self.w.tick_magic(now);
                self.w.tick_npc_status(MAP, now);
            }
        }

        pub fn drain(&mut self, id: u32) -> Vec<S2C> {
            let rx = self.rx.get_mut(&id).unwrap();
            let mut r = FrameReader::default();
            while let Ok(b) = rx.try_recv() {
                r.push(&b);
            }
            let mut out = Vec::new();
            while let Some(Ok(m)) = r.next::<S2C>(1 << 24) {
                out.push(m);
            }
            out
        }

        pub fn said(&mut self, id: u32, needle: &str) -> bool {
            self.drain(id).iter().any(|m| matches!(m, S2C::Console { text, .. } if text.contains(needle)))
        }

        pub fn give_equipped(&mut self, id: u32, slot: u8, obj: u16) {
            let p = self.w.player_mut(id);
            p.inv.slots[slot as usize - 1] = Some(InvItem { obj, amount: 1 });
            let o = self.w.ao.objs.get(obj as u32).unwrap().obj_type;
            let eq = &mut self.w.player_mut(id).inv.equipped;
            match o {
                ObjType::Weapon => eq.weapon = Some(slot),
                ObjType::Shield => eq.shield = Some(slot),
                ObjType::Armor => eq.armor = Some(slot),
                _ => panic!("not equipment"),
            }
        }

        pub fn first_obj(&self, f: impl Fn(&ao_formats::dat::obj::ObjData) -> bool) -> u16 {
            self.w.ao.objs.iter().find(|(_, o)| f(o)).map(|(id, _)| id as u16).expect("official object")
        }

        /// A static, harmless creature with lots of life (npcs.dat 500, Lobo).
        pub fn dummy(&mut self, nid: u32, x: i32, y: i32) {
            let mut npc = self.w.make_npc(500, x, y).expect("Lobo in npcs.dat");
            npc.hp = 100_000;
            npc.max_hp = 100_000;
            npc.hostile = false;
            npc.movement = ao_formats::dat::npcs::Movement::Static;
            self.w.maps.get_mut(&MAP).unwrap().add_npc(nid, npc);
        }

        pub fn mana(&self, id: u32) -> i32 {
            self.w.player(id).unwrap().mana
        }
    }

    fn eot_of(t: &T, id: u32) -> Vec<u16> {
        t.w.player(id).unwrap().status.eots.iter().map(|e| e.eot).collect()
    }

    #[test]
    fn afilar_adds_damage_against_creatures_only_and_ends_without_a_weapon() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        t.learn(1, 274);
        // Without a weapon: RequireWeapon is on the effect, the cast still works but the
        // buff falls off on the next tick.
        let sword = t.first_obj(|o| o.obj_type == ObjType::Weapon && o.projectile == 0 && o.max_hit > 0 && o.weapon_type != Some(ao_formats::dat::balance::WeaponType::Knuckle));
        t.give_equipped(1, 1, sword);
        let sta = t.w.player(1).unwrap().stats.sta;
        t.cast(1, 274, 50, 50);
        assert_eq!(eot_of(&t, 1), vec![49]);
        assert!(t.w.player(1).unwrap().stats.sta < sta, "Afilar costs its stamina");
        let m = t.w.modifiers(Who::User(1));
        assert_eq!((m.linear_bonus(true), m.linear_bonus(false)), (15, 0), "+15 against creatures, nothing in PvP");
        t.w.player_mut(1).inv.equipped.weapon = None;
        t.advance(100);
        assert!(eot_of(&t, 1).is_empty(), "unequipping the weapon ends it");
    }

    #[test]
    fn divine_protection_soaks_damage_and_a_stronger_level_replaces_it() {
        let mut t = T::new();
        t.join(1, 2, 50, 50);
        t.join(2, 3, 51, 50);
        t.learn(1, 270);
        t.learn(1, 257);
        let mana = t.mana(1);
        t.cast(1, 270, 51, 50);
        assert_eq!(eot_of(&t, 2), vec![43]);
        assert_eq!(t.mana(1), mana - 250);
        let shield = t.w.player(2).unwrap().status.eots[0].shield;
        assert!((20..=30).contains(&shield));
        let hp = t.w.player(2).unwrap().hp;
        t.w.damage_player(2, 10, "Test");
        assert_eq!(t.w.player(2).unwrap().hp, hp, "fully absorbed");
        t.w.damage_player(2, 100, "Test");
        assert_eq!(t.w.player(2).unwrap().hp, hp - (100 - (shield - 10)), "the rest goes through and the shield breaks");
        assert!(eot_of(&t, 2).is_empty());
        // Level 1 then level 3: same SharedTypeId, the new one replaces it (the VB only
        // refreshed the timer of the small one).
        t.cast(1, 270, 51, 50);
        t.cast(1, 257, 51, 50);
        assert_eq!(eot_of(&t, 2), vec![29]);
        assert!(t.w.player(2).unwrap().status.eots[0].shield >= 100);
        t.advance(10_000);
        assert!(eot_of(&t, 2).is_empty(), "TickTime 10 s");
    }

    #[test]
    fn penitence_weakens_a_creature_for_one_hit_and_only_once() {
        let mut t = T::new();
        t.join(1, 2, 50, 50);
        t.learn(1, 260);
        t.dummy(900_001, 52, 50);
        t.cast(1, 260, 52, 50);
        let who = Who::Npc(MAP, 900_001);
        assert!((t.w.modifiers(who).physical_taken() - 1.3).abs() < 1e-6, "takes 30 % more");
        t.w.damage_npc(MAP, 900_001, 10, 1);
        assert!(t.w.eot_list(who).unwrap().is_empty(), "Ticks=1: one hit");
        assert_eq!(t.w.modifiers(who), Modifiers::default(), "and nothing is left behind (VB removed it twice)");
    }

    #[test]
    fn wolf_form_changes_the_look_limits_casts_and_toggles_off_for_free() {
        let mut t = T::new();
        t.join(1, 8, 50, 50);
        t.join(2, 3, 51, 50);
        t.learn(1, 258);
        t.learn(1, 12);
        let wolf_body = t.w.ao.npcs.get(900).unwrap().body as u16;
        t.cast(1, 258, 50, 50);
        let p = t.w.player(1).unwrap();
        assert_eq!((p.body, t.w.active_transform(1)), (wolf_body, 900));
        let m = t.w.modifiers(Who::User(1));
        assert_eq!((m.linear, m.hit, m.evasion), (35, 50, 40));
        // LimitCastOnlyOnSelf.
        t.drain(1);
        t.cast(1, 12, 51, 50);
        let said: Vec<String> = t.drain(1).into_iter().filter_map(|m| if let S2C::Console { text, .. } = m { Some(text) } else { None }).collect();
        assert!(said.iter().any(|s| s.contains("sólo podés lanzarte")), "{said:?}");
        // Cast again: CancelActiveEffect switches it off and costs nothing.
        let sta = t.w.player(1).unwrap().stats.sta;
        t.cast(1, 258, 50, 50);
        let p = t.w.player(1).unwrap();
        assert_eq!((p.body, p.stats.sta, t.w.active_transform(1)), (1, sta, 0));
        // Dying in wolf form: the ghost rises human.
        t.cast(1, 258, 50, 50);
        t.w.damage_player(1, 100_000, "Test");
        assert_eq!(t.w.player(1).unwrap().combat.alive_look.map(|l| l.0), Some(1));
        assert!(eot_of(&t, 1).is_empty());
    }

    #[test]
    fn desgarro_needs_the_bear_form() {
        let mut t = T::new();
        t.join(1, 8, 50, 50);
        t.learn(1, 266);
        t.learn(1, 259);
        t.drain(1);
        t.cast(1, 266, 50, 50);
        assert!(t.said(1, "requiere la forma"));
        assert!(eot_of(&t, 1).is_empty());
        t.cast(1, 259, 50, 50);
        t.cast(1, 266, 50, 50);
        let said: Vec<String> = t.drain(1).into_iter().filter_map(|m| if let S2C::Console { text, .. } = m { Some(text) } else { None }).collect();
        assert_eq!(eot_of(&t, 1), vec![31, 36], "{said:?}");
        // Leaving the form drops Desgarro (RequireTransform checked on every tick).
        t.cast(1, 259, 50, 50);
        t.advance(100);
        assert!(eot_of(&t, 1).is_empty());
    }

    #[test]
    fn quick_blows_throw_two_extra_blows_bare_handed() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        t.learn(1, 253);
        t.dummy(900_001, 51, 50);
        t.w.player_mut(1).dir = Dir::Right;
        t.cast(1, 253, 50, 50);
        assert_eq!(eot_of(&t, 1), vec![24]);
        assert!((t.w.modifiers(Who::User(1)).physical_factor(false) - 0.35).abs() < 1e-6);
        t.advance(1_000);
        t.drain(1);
        t.seq += 1;
        t.w.on_melee(1, t.seq);
        t.advance(400);
        let swings = t.drain(1).iter().filter(|m| matches!(m, S2C::Attack { attacker: 1, .. })).count();
        assert_eq!(swings, 3, "the blow and two more");
    }

    #[test]
    fn toxin_edge_poisons_what_the_blow_hits() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        t.learn(1, 281);
        t.dummy(900_001, 51, 50);
        t.w.player_mut(1).dir = Dir::Right;
        t.cast(1, 281, 50, 50);
        assert_eq!(eot_of(&t, 1), vec![46]);
        let who = Who::Npc(MAP, 900_001);
        for _ in 0..30 {
            if t.w.has_eot(who, 42) {
                break;
            }
            t.advance(1_000);
            t.seq += 1;
            t.w.on_melee(1, t.seq);
        }
        assert!(t.w.has_eot(who, 42), "a hit leaves the poison");
        let hp = t.w.instance(MAP).npcs[&900_001].hp;
        t.advance(2_100);
        assert!(t.w.instance(MAP).npcs[&900_001].hp < hp, "and it hurts over time");
    }

    #[test]
    fn evasion_song_buffs_the_singer_and_the_party_and_costs_mana_per_pulse() {
        let mut t = T::new();
        t.join(1, 6, 50, 50);
        t.learn(1, 254);
        let mana = t.mana(1);
        t.cast(1, 254, 50, 50);
        assert!(eot_of(&t, 1).contains(&25) && eot_of(&t, 1).contains(&27));
        assert_eq!(t.w.modifiers(Who::User(1)).evasion, 40);
        assert_eq!(t.mana(1), mana - 50 - 10, "the spell plus the first pulse");
        t.advance(10_000);
        assert_eq!(t.mana(1), mana - 50 - 20, "a pulse every 10 s");
        // Casting it again stops the song, free.
        let m = t.mana(1);
        t.cast(1, 254, 50, 50);
        assert!(!eot_of(&t, 1).contains(&25));
        assert_eq!(t.mana(1), m);
    }

    #[test]
    fn speed_only_and_unported_effects_are_refused_for_free() {
        let mut t = T::new();
        t.join(1, 6, 50, 50);
        t.learn(1, 252);
        let mana = t.mana(1);
        t.drain(1);
        t.cast(1, 252, 50, 50);
        assert!(t.said(1, "todavía no está disponible"));
        assert_eq!(t.mana(1), mana);
        assert!(eot_of(&t, 1).is_empty());
        let table = t.w.eots();
        assert!(!eot_works(&table, 5), "taunt");
        assert!(!eot_works(&table, 20), "delayed blast");
        assert!(eot_works(&table, 1) && eot_works(&table, 29) && eot_works(&table, 30));
    }

    /// HechizoSobreArea: the square around the tile, less damage away from the centre,
    /// nothing outside, nothing spent when it reaches no one.
    #[test]
    fn area_spell_hits_the_square_with_falloff() {
        use ao_formats::dat::spells::{SpellEffects, SpellType};
        let mut t = T::new();
        t.join(1, 1, 50, 50);
        let (sid, side) = t
            .w
            .ao
            .spells
            .iter()
            .find(|(_, s)| s.spell_type == SpellType::Area && s.effects.contains(SpellEffects::DO_DAMAGE) && matches!(s.area_affects, 2 | 3) && (3..=5).contains(&s.area_radius) && s.hp.min > 20)
            .map(|(i, s)| (i as u16, s.area_radius))
            .expect("an official area spell against creatures");
        t.learn(1, sid);
        // Right edge of the square: an even side leans right (x - half .. x + side - 1 - half).
        let edge = side - 1 - (side - 1) / 2;
        t.dummy(900_001, 55, 55);
        t.dummy(900_002, 55 + edge, 55);
        t.dummy(900_003, 55 + edge + 1, 55);
        // Nobody there: refused, free.
        let mana = t.mana(1);
        let empty = (40..60)
            .flat_map(|x| (40..60).map(move |y| (x, y)))
            .find(|c| ao_rules::eot::area_tiles(*c, side).all(|(x, y)| t.w.instance(MAP).npc_at(x, y).is_none() && t.w.player_at(MAP, x, y).is_none()))
            .expect("an empty square");
        t.cast(1, sid, empty.0, empty.1);
        assert_eq!(t.mana(1), mana, "an empty area costs nothing");
        t.cast(1, sid, 55, 55);
        assert!(t.mana(1) < mana);
        let hp = |t: &T, n: u32| 100_000 - t.w.instance(MAP).npcs[&n].hp;
        assert!(hp(&t, 900_001) > 0, "centre hit");
        assert!(hp(&t, 900_002) > 0, "edge hit");
        assert_eq!(hp(&t, 900_003), 0, "outside the square");
    }

    #[test]
    fn restoration_heals_over_time() {
        let mut t = T::new();
        t.join(1, 2, 50, 50);
        t.learn(1, 268);
        t.w.player_mut(1).hp = 100;
        t.cast(1, 268, 50, 50);
        t.advance(30_100);
        let hp = t.w.player(1).unwrap().hp;
        assert!((100 + 20..=100 + 60).contains(&hp), "10 ticks of 2..6: {hp}");
        assert!(eot_of(&t, 1).is_empty());
    }
}

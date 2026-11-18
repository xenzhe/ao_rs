//! Creature combat behaviours beyond melee chasing, ported from `AI_NPC.bas`:
//! - `AI_RangeAttack`: archers and other `AttackRange > 1` creatures shoot from afar and
//!   keep their `PreferedRange`, backing off and circling (`ComputeNpcRangedRetreatDestination`,
//!   `EnsureNpcOrbitDirection`, `FlipNpcOrbitDirection`).
//! - `AI_SupportAndAttackNpc` + `TrySupportThenAttackSpells`: casters that first help
//!   (remove paralysis, heal) and otherwise attack (paralyse, then damage), with a cooldown
//!   per spell slot (`CanCastSpell`: `Spells(i).Cd`).
//! - `NpcLanzaSpellInmovilizado`: an immobilised caster only casts along the line it faces.
//!
//! Not ported: the battleground behaviours (`AI_BG*`), invasions and cross-map routes.
//! Helping is decided with the creature's alignment against the player (`EsEnemigo`) instead
//! of `FactionCanHelpFaction`, and creatures help any other wild creature (AO20 limits it
//! to the same `Team`, which only battleground NPCs set).
use crate::clock::GameTime;
use crate::world::entities::EntityId;
use crate::world::World;
use ao_formats::dat::npcs::Npc as NpcData;
use ao_formats::dat::spells::SpellEffects as Fx;
use ao_protocol::S2C;
use ao_rules::npc_tactics as rules;
use ao_rules::pathfinding::heading_to;
use ao_rules::Dir;

/// `e_BehaviorFlags` from `RestriccionDeAtaque` / `RestriccionDeAyuda` (OpenNPC).
fn attacks_users(d: &NpcData) -> bool {
    matches!(d.attack_restriction, 0 | 1)
}
fn helps_users(d: &NpcData) -> bool {
    matches!(d.help_restriction, 2 | 3)
}
fn helps_npcs(d: &NpcData) -> bool {
    matches!(d.help_restriction, 1 | 3)
}

/// What a support/attack caster chose.
enum Cast {
    User(EntityId),
    Npc(u32),
}

impl World {
    fn npc_pos(&self, map: u16, nid: u32) -> (i32, i32) {
        let n = &self.instance(map).npcs[&nid];
        (n.x, n.y)
    }

    /// `SelectNearestUser` (+ `SelectCurrentTarget`): the aggressor while in sight, else
    /// the closest valid target. Concealed players only when adjacent.
    fn ranged_target(&self, map: u16, nid: u32, d: &NpcData, now: GameTime) -> (Option<EntityId>, f64) {
        let pos = self.npc_pos(map, nid);
        let aggressor = self.instance(map).npcs[&nid].attacked_by;
        let mut players: Vec<EntityId> = self.instance(map).players.iter().copied().collect();
        players.sort_unstable();
        let mut best: (Option<EntityId>, f64) = (None, f64::MAX);
        for pid in players {
            if !self.valid_target(map, nid, d, pid, aggressor) {
                continue;
            }
            let p = self.player(pid).unwrap();
            let dist = rules::distance(pos, (p.x, p.y));
            if dist >= 2.0 && self.user_concealed(pid, now) {
                continue;
            }
            if dist < best.1 {
                best = (Some(pid), dist);
            }
        }
        if let Some(a) = aggressor.filter(|a| self.valid_target(map, nid, d, *a, aggressor) && !self.user_concealed(*a, now)) {
            let p = self.player(a).unwrap();
            return (Some(a), rules::distance(pos, (p.x, p.y)));
        }
        best
    }

    /// `AI_RangeAttack`.
    pub(crate) fn range_attack(&mut self, map: u16, nid: u32, d: &NpcData, now: GameTime) {
        let pos = self.npc_pos(map, nid);
        let (target, dist) = self.ranged_target(map, nid, d, now);
        self.npc_mut(map, nid).target = target;
        let tpos = target.and_then(|t| self.player(t)).map(|p| (p.x, p.y));
        // Perform the attack.
        if let (Some(t), Some(tp)) = (target, tpos) {
            if !self.npc_paralyzed(map, nid) && now >= self.instance(map).npcs[&nid].next_attack {
                if dist <= d.attack_range as f64 {
                    self.npc_face(map, nid, heading_to(pos, tp));
                    self.npc_mut(map, nid).next_attack = now.after_ms(if d.attack_interval > 0 { d.attack_interval as u64 } else { 2000 });
                    self.npc_attacks_player(map, nid, t);
                } else {
                    self.walk_towards(map, nid, tp, now);
                }
            }
        }
        if !self.instance(map).npcs.get(&nid).is_some_and(|n| n.alive()) || self.npc_paralyzed(map, nid) || self.npc_rooted(map, nid) {
            return;
        }
        // Then the movement: keep the preferred range.
        let pos = self.npc_pos(map, nid);
        let preferred = d.preferred_range as f64;
        let home = self.instance(map).npcs[&nid].home;
        match tpos {
            Some(tp) if dist < preferred => {
                let dest = self.ranged_retreat_destination(map, nid, d, tp, preferred, now);
                if dest != pos {
                    self.walk_towards(map, nid, dest, now);
                } else {
                    // Fallback: one step straight away from the threat.
                    let away = heading_to(tp, pos);
                    self.npc_move(map, nid, away);
                }
            }
            Some(_) if dist.round() == preferred => {}
            Some(tp) if dist > preferred => self.walk_towards(map, nid, tp, now),
            _ if pos != home => self.walk_towards(map, nid, home, now),
            _ => self.npc_face(map, nid, Dir::Down),
        }
    }

    /// `EnsureNpcOrbitDirection`: keeps one orbit side for `ORBIT_REEVALUATE_MS`.
    fn orbit_dir(&mut self, map: u16, nid: u32, now: GameTime) -> i32 {
        let side = if self.rng.range(0, 1) == 0 { -1 } else { 1 };
        let st = &mut self.npc_mut(map, nid).status;
        if st.orbit_dir == 0 || now >= st.orbit_until {
            st.orbit_dir = side;
            st.orbit_until = now.after_ms(rules::ORBIT_REEVALUATE_MS);
        }
        st.orbit_dir as i32
    }

    /// `ComputeNpcRangedRetreatDestination`: two tries, flipping the orbit side when the
    /// tile is not walkable or is where it already stands.
    fn ranged_retreat_destination(&mut self, map: u16, nid: u32, d: &NpcData, target: (i32, i32), preferred: f64, now: GameTime) -> (i32, i32) {
        let pos = self.npc_pos(map, nid);
        if preferred <= 0.0 {
            return pos;
        }
        for _ in 0..2 {
            let side = self.orbit_dir(map, nid, now);
            let c = rules::retreat_candidate(pos, target, preferred, side);
            if c != pos && self.npc_tile_ok(map, d, c.0, c.1, Dir::Down) && !self.occupied(map, c.0, c.1) {
                return c;
            }
            // FlipNpcOrbitDirection.
            let st = &mut self.npc_mut(map, nid).status;
            st.orbit_dir = -side as i8;
            st.orbit_until = now.after_ms(rules::ORBIT_REEVALUATE_MS);
        }
        pos
    }

    // ------------------------------------------------------------ spells

    /// `CanCastSpell` for slot `i` (`Cd` seconds since its last use).
    fn npc_slot_ready(&self, map: u16, nid: u32, i: usize, now: GameTime) -> bool {
        self.instance(map).npcs[&nid].status.spell_ready.get(i).is_none_or(|t| now >= *t)
    }

    fn npc_slot_used(&mut self, map: u16, nid: u32, i: usize, cd_secs: i32, now: GameTime) {
        let st = &mut self.npc_mut(map, nid).status;
        if st.spell_ready.len() <= i {
            st.spell_ready.resize(i + 1, GameTime::ZERO);
        }
        st.spell_ready[i] = now.after_secs(cd_secs.max(0) as u64);
    }

    /// First ready slot whose spell has any of `effects`.
    fn npc_slot_with(&self, map: u16, nid: u32, d: &NpcData, effects: u32, now: GameTime) -> Option<usize> {
        d.spells.iter().enumerate().position(|(i, s)| self.npc_slot_ready(map, nid, i, now) && self.ao.spells.get(s.spell.max(0) as u32).is_some_and(|sp| sp.effects.0 & effects != 0))
    }

    /// `AI_SupportAndAttackNpc`: when the spell interval allows, help then attack; always
    /// wander near the origin.
    pub(crate) fn support_and_attack(&mut self, map: u16, nid: u32, d: &NpcData, now: GameTime) {
        if !self.npc_paralyzed(map, nid) && now >= self.instance(map).npcs[&nid].next_spell {
            let cast = self.try_support_spell(map, nid, d, now) || self.try_attack_spell(map, nid, d, now);
            if cast {
                self.npc_mut(map, nid).next_spell = now.after_ms(if d.spell_interval > 0 { d.spell_interval as u64 } else { 8000 });
            }
        }
        if self.instance(map).npcs.get(&nid).is_some_and(|n| n.alive()) {
            self.wander_near_origin(map, nid, now);
        }
    }

    /// `SelectSupportSpellAndTarget` + `TryCastHelpSpell`: remove paralysis first, then
    /// heal; players (`eHelpUsers`) before creatures (`eHelpNpc`); within `RangoSpell`.
    fn try_support_spell(&mut self, map: u16, nid: u32, d: &NpcData, now: GameTime) -> bool {
        if !helps_users(d) && !helps_npcs(d) {
            return false;
        }
        let pos = self.npc_pos(map, nid);
        let range = d.spell_range as f64;
        let mut players: Vec<EntityId> = self.instance(map).players.iter().copied().collect();
        players.sort_unstable();
        let mut npcs: Vec<u32> = self.instance(map).npcs.iter().filter(|(id, n)| **id != nid && n.alive() && n.pet.owner.is_none()).map(|(id, _)| *id).collect();
        npcs.sort_unstable();
        for need in [Fx::REMOVE_PARALYSIS, Fx::DO_HEAL] {
            let Some(slot) = self.npc_slot_with(map, nid, d, need, now) else { continue };
            let mut choice = None;
            if helps_users(d) {
                choice = players
                    .iter()
                    .copied()
                    .find(|pid| {
                        let Some(p) = self.player(*pid) else { return false };
                        let wants = if need == Fx::DO_HEAL { p.hp < p.max_hp } else { !self.can_move(*pid) };
                        p.hp > 0 && wants && self.in_vision(map, nid, d, *pid) && !self.npc_enemy(map, nid, *pid) && rules::distance(pos, (p.x, p.y)) < range
                    })
                    .map(Cast::User);
            }
            if choice.is_none() && helps_npcs(d) {
                choice = npcs
                    .iter()
                    .copied()
                    .find(|v| {
                        let n = &self.instance(map).npcs[v];
                        let wants = if need == Fx::DO_HEAL { n.hp < n.max_hp } else { self.npc_paralyzed(map, *v) || self.npc_rooted(map, *v) };
                        wants && rules::distance(pos, (n.x, n.y)) < range
                    })
                    .map(Cast::Npc);
            }
            if let Some(c) = choice {
                let spell = d.spells[slot].spell as u16;
                self.npc_slot_used(map, nid, slot, d.spells[slot].cooldown, now);
                match c {
                    Cast::User(pid) => self.npc_cast_spell(map, nid, pid, spell),
                    Cast::Npc(v) => self.npc_spell_on_npc(map, nid, v, spell),
                }
                return true;
            }
        }
        false
    }

    /// `SelectAttackSpellAndTarget` + `TryCastAttackSpell`: its current target or the
    /// nearest valid player in `RangoSpell`; paralysis before damage.
    fn try_attack_spell(&mut self, map: u16, nid: u32, d: &NpcData, now: GameTime) -> bool {
        if !attacks_users(d) {
            return false;
        }
        let pos = self.npc_pos(map, nid);
        let range = d.spell_range as f64;
        let current = self.instance(map).npcs[&nid].target;
        let ok = |w: &World, pid: EntityId| w.player(pid).is_some_and(|p| w.valid_target(map, nid, d, pid, current) && !w.user_concealed(pid, now) && rules::distance(pos, (p.x, p.y)) < range);
        let mut target = current.filter(|t| ok(self, *t));
        if target.is_none() {
            let mut players: Vec<EntityId> = self.instance(map).players.iter().copied().collect();
            players.sort_unstable();
            target = players.into_iter().find(|p| ok(self, *p));
        }
        let Some(t) = target else { return false };
        let Some(slot) = self.npc_slot_with(map, nid, d, Fx::PARALYZE | Fx::IMMOBILIZE, now).or_else(|| self.npc_slot_with(map, nid, d, Fx::DO_DAMAGE, now)) else { return false };
        let spell = d.spells[slot].spell as u16;
        self.npc_slot_used(map, nid, slot, d.spells[slot].cooldown, now);
        let tp = self.player(t).map(|p| (p.x, p.y)).unwrap();
        self.npc_face(map, nid, heading_to(pos, tp));
        self.npc_mut(map, nid).target = Some(t);
        self.npc_cast_spell(map, nid, t, spell);
        true
    }

    /// `NpcLanzaSpellSobreNpc` for help and harm: healing, removing paralysis, damage.
    pub(crate) fn npc_spell_on_npc(&mut self, map: u16, caster: u32, v: u32, spell: u16) {
        let Some(s) = self.ao.spells.get(spell as u32).cloned() else { return };
        let Some((tx, ty)) = self.maps.get(&map).and_then(|m| m.npcs.get(&v)).filter(|n| n.alive()).map(|n| (n.x, n.y)) else { return };
        self.broadcast(map, &S2C::Effect { spell, from: caster, tx, ty }, None);
        if s.fx > 0 {
            self.broadcast(map, &S2C::Fx { id: v, fx: s.fx as u16, loops: s.loops.clamp(0, 255) as u8 }, None);
        }
        if s.effects.contains(Fx::REMOVE_PARALYSIS) {
            let n = self.npc_mut(map, v);
            n.status.paralyzed_until = None;
            n.status.immobilized_until = None;
        }
        let amount = self.rng.range(s.hp.min.min(s.hp.max), s.hp.max.max(s.hp.min)).max(0);
        if s.effects.contains(Fx::DO_HEAL) {
            let n = self.npc_mut(map, v);
            n.hp = (n.hp + amount).min(n.max_hp);
            let (hp, max_hp) = (n.hp, n.max_hp);
            self.broadcast(map, &S2C::Hp { id: v, hp, max_hp }, None);
        } else if s.effects.contains(Fx::DO_DAMAGE) {
            self.npc_damaged_by_npc(map, v, amount, caster);
        }
    }

    /// `NpcLanzaSpellInmovilizado`: a creature that cannot move only casts at a player in
    /// the straight line it faces; one that can move always may.
    pub(crate) fn npc_can_cast_at(&self, map: u16, nid: u32, pid: EntityId) -> bool {
        if !(self.npc_rooted(map, nid) || self.npc_paralyzed(map, nid)) {
            return true;
        }
        let n = &self.instance(map).npcs[&nid];
        let Some(p) = self.player(pid).filter(|p| p.map == map) else { return false };
        match n.dir {
            Dir::Up => n.x == p.x && n.y > p.y,
            Dir::Right => n.y == p.y && n.x < p.x,
            Dir::Down => n.x == p.x && n.y < p.y,
            Dir::Left => n.y == p.y && n.x > p.x,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::eot::tests::{T, MAP};
    use ao_formats::dat::npcs::Movement;

    /// An archer of the official npcs.dat (AttackRange > 1 and a PreferedRange).
    fn archer(t: &T) -> u16 {
        t.w.ao.npcs.iter().filter(|(_, d)| d.attack_range > 2 && d.preferred_range >= 3 && d.hostile == 1 && d.max_hp > 0 && d.max_hit > 0).map(|(i, _)| i as u16).next().expect("a ranged creature in npcs.dat")
    }

    #[test]
    fn archers_shoot_from_afar_and_back_off_when_crowded() {
        let mut t = T::new();
        let number = archer(&t);
        let (ax, ay) = t.w.free_tile_near(MAP, 50, 50, 3);
        let mut npc = t.w.make_npc(number, ax, ay).unwrap();
        npc.movement = Movement::Wander;
        npc.hostile = true;
        npc.hp = 100_000;
        npc.max_hp = 100_000;
        t.w.maps.get_mut(&MAP).unwrap().add_npc(900_010, npc);
        let d = t.w.ao.npcs.get(number as u32).unwrap().clone();
        let range = d.attack_range.min(4);
        let (px, py) = t.w.free_tile_near(MAP, ax + range, ay, 1);
        t.join(1, 3, px, py);
        let mut shots = 0;
        for _ in 0..40 {
            t.advance(100);
            let now = t.w.now();
            t.w.range_attack(MAP, 900_010, &d, now);
            shots += t.drain(1).iter().filter(|m| matches!(m, ao_protocol::S2C::Attack { attacker: 900_010, .. })).count();
        }
        assert!(shots > 0, "it attacks from {range} tiles");
        // Stand next to it: it walks away towards its preferred range.
        let n = &t.w.instance(MAP).npcs[&900_010];
        let (nx, ny) = (n.x, n.y);
        let (px, py) = t.w.free_tile_near(MAP, nx + 1, ny, 1);
        t.w.player_mut(1).x = px;
        t.w.player_mut(1).y = py;
        let before = ao_rules::npc_tactics::distance((nx, ny), (px, py));
        for _ in 0..30 {
            t.advance(400);
            let now = t.w.now();
            t.w.range_attack(MAP, 900_010, &d, now);
        }
        let n = &t.w.instance(MAP).npcs[&900_010];
        let p = t.w.player(1).unwrap();
        assert!(ao_rules::npc_tactics::distance((n.x, n.y), (p.x, p.y)) > before, "it kited away");
    }

    #[test]
    fn spell_slots_have_their_own_cooldown() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        t.dummy(900_020, 53, 50);
        let mut d = t.w.ao.npcs.get(500).unwrap().clone();
        d.spells = vec![ao_formats::dat::npcs::NpcSpell { spell: 1, cooldown: 30 }];
        d.attack_restriction = 1;
        d.spell_range = 10;
        d.hostile = 1;
        let now = t.w.now();
        let hp = t.w.player(1).unwrap().hp;
        assert!(t.w.try_attack_spell(MAP, 900_020, &d, now), "Dardo Mágico at the player");
        assert!(t.w.player(1).unwrap().hp < hp);
        assert!(!t.w.try_attack_spell(MAP, 900_020, &d, now), "slot on cooldown");
        t.advance(30_000);
        let now = t.w.now();
        assert!(t.w.try_attack_spell(MAP, 900_020, &d, now));
    }

    #[test]
    fn supporters_free_paralysed_allies_before_attacking() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        t.dummy(900_030, 52, 50);
        t.dummy(900_031, 54, 50);
        let mut d = t.w.ao.npcs.get(500).unwrap().clone();
        let remover = t.w.ao.spells.iter().find(|(_, s)| s.effects.contains(ao_formats::dat::spells::SpellEffects::REMOVE_PARALYSIS)).map(|(i, _)| i as i32).unwrap();
        d.spells = vec![ao_formats::dat::npcs::NpcSpell { spell: remover, cooldown: 5 }, ao_formats::dat::npcs::NpcSpell { spell: 1, cooldown: 5 }];
        d.help_restriction = 1;
        d.attack_restriction = 1;
        d.spell_range = 10;
        let now = t.w.now();
        t.w.npc_mut(MAP, 900_031).status.paralyzed_until = Some(now.after_secs(60));
        t.w.support_and_attack(MAP, 900_030, &d, now);
        assert!(!t.w.npc_paralyzed(MAP, 900_031), "the ally was freed first");
    }
}

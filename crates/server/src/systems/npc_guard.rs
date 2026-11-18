//! City guards against hostile creatures, ported from `AI_NPC.bas`:
//! `TryGuardAttackHostileNpc` (royal and chaos guards paralyse, then smite a wild hostile
//! creature within 8 tiles of their post, every 1.5 s), `BuscarNpcEnArea` and the
//! `GuardiaPersigueNpc` movement (walk to it and hit it while it stays near the post).
//!
//! Not checked: `BuscarNpcEnArea` skips a creature that the player it targets is fighting
//! (`UserList(..).flags.NPCAtacado`); this server does not keep that flag, so a guard may
//! join a fight a player started.
use crate::clock::GameTime;
use crate::world::World;
use ao_formats::dat::npcs::NpcType;
use ao_formats::dat::spells::SpellEffects as Fx;
use ao_protocol::S2C;

const GUARD_PARALYZE_SPELL: u16 = 25;
const GUARD_DAMAGE_SPELL: u16 = 204;
const GUARD_NPC_VISION_RANGE: i32 = 8;
const GUARD_NPC_SPELL_INTERVAL_MS: u64 = 1_500;
/// `DIAMETRO_VISION_GUARDIAS_NPCS`.
const GUARD_VISION_DIAMETER: i32 = 7;

fn is_guard(t: NpcType) -> bool {
    matches!(t, NpcType::RoyalGuard | NpcType::ChaosGuard)
}

fn distancia(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

impl World {
    /// What a guard may hunt: a living wild creature that was born hostile, may be attacked
    /// and is not a guard.
    fn guard_prey(&self, map: u16, v: u32) -> bool {
        self.maps.get(&map).and_then(|i| i.npcs.get(&v)).is_some_and(|n| n.alive() && n.old_hostile && n.attackable && n.pet.owner.is_none() && !is_guard(n.kind))
    }

    /// `BuscarNpcEnArea`: the first prey in the square around the post (x, then y order).
    fn guard_search(&self, map: u16, nid: u32, radius: i32) -> Option<u32> {
        let (hx, hy) = self.instance(map).npcs[&nid].home;
        for x in hx - radius..=hx + radius {
            for y in hy - radius..=hy + radius {
                if let Some(v) = self.instance(map).npc_at(x, y).filter(|v| *v != nid) {
                    if self.guard_prey(map, v) {
                        return Some(v);
                    }
                }
            }
        }
        None
    }

    /// `TryGuardAttackHostileNpc`. True when the guard is busy with a creature (the rest of
    /// its AI waits).
    pub(crate) fn guard_attacks_hostile_npc(&mut self, map: u16, nid: u32, now: GameTime) -> bool {
        let n = &self.instance(map).npcs[&nid];
        if !is_guard(n.kind) {
            return false;
        }
        let mut target = n.status.target_npc.filter(|v| self.guard_prey(map, *v));
        if target.is_none() {
            target = self.guard_search(map, nid, GUARD_NPC_VISION_RANGE);
        }
        self.npc_mut(map, nid).status.target_npc = target;
        let Some(v) = target else { return false };
        let n = &self.instance(map).npcs[&nid];
        let (pos, vpos) = ((n.x, n.y), self.instance(map).npcs.get(&v).map_or((0, 0), |t| (t.x, t.y)));
        if distancia(pos, vpos) > GUARD_NPC_VISION_RANGE {
            self.npc_mut(map, nid).status.target_npc = None;
            return false;
        }
        if self.npc_paralyzed(map, nid) || now < self.instance(map).npcs[&nid].next_spell {
            return true;
        }
        self.npc_mut(map, nid).next_spell = now.after_ms(GUARD_NPC_SPELL_INTERVAL_MS);
        let immune = self.ao.npcs.get(self.instance(map).npcs[&v].def as u32).is_none_or(|d| d.affected_by_paralysis != 0);
        if !self.npc_paralyzed(map, v) && !immune {
            self.guard_cast(map, nid, v, GUARD_PARALYZE_SPELL, now);
        }
        if self.guard_prey(map, v) {
            self.guard_cast(map, nid, v, GUARD_DAMAGE_SPELL, now);
        }
        true
    }

    /// `NpcLanzaSpellSobreNpc` for the two guard spells: paralysis and damage.
    fn guard_cast(&mut self, map: u16, a: u32, v: u32, spell: u16, now: GameTime) {
        let Some(s) = self.ao.spells.get(spell as u32).cloned() else { return };
        let Some((tx, ty)) = self.instance(map).npcs.get(&v).map(|n| (n.x, n.y)) else { return };
        self.broadcast(map, &S2C::Effect { spell, from: a, tx, ty }, None);
        if s.fx > 0 {
            self.broadcast(map, &S2C::Fx { id: v, fx: s.fx as u16, loops: s.loops.clamp(0, 255) as u8 }, None);
        }
        if s.effects.contains(Fx::PARALYZE) {
            let n = self.npc_mut(map, v);
            n.status.paralyzed_until = Some(now.after_secs(s.duration.max(1) as u64));
            n.path.clear();
        }
        if s.effects.contains(Fx::DO_DAMAGE) {
            let dmg = self.rng.range(s.hp.min.min(s.hp.max), s.hp.min.max(s.hp.max)).max(0);
            self.npc_damaged_by_npc(map, v, dmg, a);
        }
    }

    /// `AI_GuardiaPersigueNpc` (`Movement=10`): goes for a prey near its post and hits it;
    /// back to the post when there is none.
    pub(crate) fn guard_chases_npc(&mut self, map: u16, nid: u32, now: GameTime) {
        let n = &self.instance(map).npcs[&nid];
        let (pos, home) = ((n.x, n.y), n.home);
        let target = n.status.target_npc.filter(|v| self.guard_prey(map, *v));
        match target {
            Some(v) => {
                let vpos = self.instance(map).npcs.get(&v).map_or(pos, |t| (t.x, t.y));
                if distancia(pos, vpos) <= 1 && now >= self.instance(map).npcs[&nid].next_attack {
                    self.npc_mut(map, nid).next_attack = now.after_ms(1_500);
                    self.npc_attacks_npc(map, nid, v);
                }
                let r = (((vpos.0 - home.0).pow(2) + (vpos.1 - home.1).pow(2)) as f64).sqrt();
                if r <= (GUARD_VISION_DIAMETER / 2) as f64 {
                    self.walk_towards(map, nid, vpos, now);
                } else {
                    self.npc_mut(map, nid).status.target_npc = None;
                    self.walk_towards(map, nid, home, now);
                }
            }
            None => {
                let found = self.guard_search(map, nid, GUARD_VISION_DIAMETER / 2);
                self.npc_mut(map, nid).status.target_npc = found;
                if pos != home {
                    self.walk_towards(map, nid, home, now);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::systems::eot::tests::{T, MAP};
    use ao_formats::dat::npcs::NpcType;

    #[test]
    fn a_guard_paralyses_and_smites_a_hostile_creature_near_its_post() {
        let mut t = T::new();
        t.join(1, 1, 30, 30);
        t.dummy(900_001, 50, 50);
        t.dummy(900_002, 53, 50);
        {
            let g = t.w.npc_mut(MAP, 900_001);
            g.kind = NpcType::RoyalGuard;
            g.home = (50, 50);
        }
        // Only our two creatures count (the official map has its own wolves around).
        for (id, n) in t.w.maps.get_mut(&MAP).unwrap().npcs.iter_mut() {
            if *id != 900_001 {
                n.old_hostile = false;
            }
        }
        // A tame creature is left alone.
        let now = t.w.now();
        assert!(!t.w.guard_attacks_hostile_npc(MAP, 900_001, now));
        let prey = t.w.npc_mut(MAP, 900_002);
        prey.old_hostile = true;
        let info = (prey.attackable, prey.alive(), prey.pet.owner, prey.kind);
        let now = t.w.now();
        assert!(t.w.guard_prey(MAP, 900_002), "{info:?}");
        let p = |t: &T, n: u32| (t.w.instance(MAP).npcs[&n].x, t.w.instance(MAP).npcs[&n].y, t.w.instance(MAP).npcs[&n].home);
        assert_eq!(t.w.guard_search(MAP, 900_001, 8), Some(900_002), "{:?} {:?}", p(&t, 900_001), p(&t, 900_002));
        assert!(t.w.guard_attacks_hostile_npc(MAP, 900_001, now));
        assert!(t.w.instance(MAP).npcs.get(&900_002).is_none_or(|n| !n.alive() || n.hp < 100_000), "Ira de ReyarB hits it");
    }
}

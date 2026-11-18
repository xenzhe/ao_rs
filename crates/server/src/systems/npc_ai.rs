//! NPCs of the real maps: creation from the .csm + npcs.dat, AI, movement and respawn.
//! OWNER: NPC system (P1, Opus D). Ported from the official AO20 server:
//! - `FileIO.CargarMapaFormatoCSM` + `MODULO_NPCs.OpenNPC`: every NPC listed in the map is
//!   created on its tile with its npcs.dat definition (safe maps too).
//! - `modNpcAiLoop`: the AI runs every 100 ms, only on maps with players
//!   (`NpcRequiresAiUpdate`), and each NPC acts when its `IntervaloMovimiento` allows.
//! - `AI_NPC.NpcAI` and the behaviours it dispatches (see each function).
//! - `MODULO_NPCs.MoveNPCChar` / `LegalWalkNPC`, `PathFinding.IsWalkable` + `SeekPath`.
//! - `MODULO_NPCs.MuereNpc` / `ProcessRespawnQueue` / `ReSpawnNpc` / `CrearNPC`.
//!
//! The made-up creatures of `data/creatures.toml` are no longer spawned: every map gets
//! exactly its official NPCs (a map without NPCs in its .csm stays empty, like in AO20).
use crate::clock::GameTime;
use crate::world::entities::{EntityId, Npc};
use crate::world::World;
use ao_formats::dat::npcs::{Movement, Npc as NpcData, NpcType};
use ao_formats::csm::{BLOCK_ALL, FLAG_WATER};
use ao_protocol::S2C;
use ao_rules::pathfinding::{self, heading_to, Query};
use ao_rules::{walk, Dir};

/// `DEFAULT_NPC_AI_INTERVAL_MS` (modNpcAiLoop.bas).
pub const AI_INTERVAL_MS: u64 = 100;
/// `RANGO_VISION_X` / `RANGO_VISION_Y` (= `DEFAULT_NPC_VISION_RANGE_X/Y`, Consts.bas).
pub const VISION_X: i32 = 15;
pub const VISION_Y: i32 = 13;
/// `NPC_SPELL_RANGE_X` / `_Y` defaults (Consts.bas).
const SPELL_RANGE_X: i32 = 11;
const SPELL_RANGE_Y: i32 = 9;
/// `NpcDeltaLevelPenalties` / `NpcDeltaLevelCloseRange` (Configuracion.ini).
const DELTA_LEVEL_PENALTIES: i32 = 4;
const DELTA_LEVEL_CLOSE_RANGE: i32 = 3;
/// Defaults applied by `OpenNPC` when npcs.dat leaves the interval at 0.
const DEFAULT_MOVE_MS: i32 = 380;
const DEFAULT_ATTACK_MS: i32 = 2000;
const DEFAULT_SPELL_MS: i32 = 8000;
/// `e_Trigger.POSINVALIDA`.
const TRIGGER_INVALID_POS: u16 = 3;
/// `MAXSPAWNATTEMPS` (Declares.bas) and the `ClosestLegalPosNPC` radius used by `CrearNPC`.
const MAX_SPAWN_ATTEMPTS: u32 = 15;
const SPAWN_SEARCH_RADIUS: i32 = 10;
/// `Magic_and_Punch` feature toggle (Example.feature_toggle.ini: value=1).
const MAGIC_AND_PUNCH: bool = true;

/// `Distancia`: Manhattan distance (same map).
fn distancia(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

fn dir_from_heading(h: i32) -> Dir {
    match h {
        1 => Dir::Up,
        2 => Dir::Right,
        4 => Dir::Left,
        _ => Dir::Down,
    }
}

fn ms(v: i32, default: i32) -> u64 {
    if v > 0 { v as u64 } else { default as u64 }
}

fn is_guard(t: NpcType) -> bool {
    matches!(t, NpcType::RoyalGuard | NpcType::ChaosGuard)
}

impl World {
    // ------------------------------------------------------------ creation

    /// Called by `ensure_map` for non-safe maps; kept under its old name. Creates the
    /// official NPCs of the map (idempotent).
    pub(crate) fn spawn_creatures(&mut self, map: u16) {
        self.populate_map(map);
    }

    /// `CargarMapaFormatoCSM`: one NPC per .csm entry, on its tile, `Orig` = that tile.
    /// Entries whose number is not in npcs.dat or whose name is empty are skipped.
    pub(crate) fn populate_map(&mut self, map: u16) {
        let Some(inst) = self.maps.get_mut(&map) else { return };
        if inst.populated {
            return;
        }
        inst.populated = true;
        if !self.map_npcs {
            return;
        }
        let list = inst.map.npcs.clone();
        let ao = self.ao.clone();
        let now = self.now();
        for e in list {
            let Some(d) = ao.npcs.get(e.npc as u32) else {
                tracing::debug!(map, x = e.x, y = e.y, npc = e.npc, "NPC del mapa sin definicion en npcs.dat");
                continue;
            };
            if d.name.is_empty() {
                continue;
            }
            // Hostile creatures placed inside the start sanctuary move out of it.
            let (x, y) = if d.hostile != 0 && self.in_start_sanctuary(map, e.x, e.y) {
                let b = &self.data.balance;
                match self.closest_spawn_pos(map, d, e.x, e.y, b.start_sanctuary + 6) {
                    Some(p) => p,
                    None => continue,
                }
            } else {
                (e.x, e.y)
            };
            let npc = self.new_npc(e.npc, d, x, y, now);
            let id = self.next_npc;
            self.next_npc += 1;
            let msg = npc.spawn_msg(id);
            self.maps.get_mut(&map).unwrap().add_npc(id, npc);
            self.broadcast(map, &msg, None);
        }
    }

    /// Live NPC for npcs.dat number `number` at (x, y), not placed on any map.
    #[cfg(test)]
    pub(crate) fn make_npc(&mut self, number: u16, x: i32, y: i32) -> Option<Npc> {
        let ao = self.ao.clone();
        let d = ao.npcs.get(number as u32)?;
        let now = self.now();
        Some(self.new_npc(number, d, x, y, now))
    }

    /// `OpenNPC`: live state from the npcs.dat definition.
    pub(crate) fn new_npc(&mut self, number: u16, d: &NpcData, x: i32, y: i32, now: GameTime) -> Npc {
        let respawn_secs = self.rng.range(d.respawn_interval_min.max(0), d.respawn_interval_max.max(d.respawn_interval_min).max(0)) as u32;
        let vision = if d.path_vision > 0 { d.path_vision } else { VISION_X };
        Npc {
            def: number,
            name: d.name.clone(),
            body: d.body.max(0) as u16,
            head: d.head.max(0) as u16,
            x,
            y,
            dir: dir_from_heading(d.heading),
            // OpenNPC: Stats.MinHp = MinHP (current), Stats.MaxHp = MaxHP.
            hp: d.min_hp,
            max_hp: d.max_hp,
            home: (x, y),
            next_move: now,
            next_attack: now,
            dead_until: None,
            kind: d.npc_type,
            movement: d.movement,
            hostile: d.hostile == 1,
            attackable: d.attackable == 1,
            old_movement: d.movement,
            old_hostile: d.hostile == 1,
            target: None,
            attacked_by: None,
            vision,
            original_vision: vision,
            path: Vec::new(),
            path_dest: (0, 0),
            next_path_at: GameTime::ZERO,
            next_spell: now,
            walk_step: 0,
            respawn_secs,
            respawn_at: None,
            pet: Default::default(),
            status: Default::default(),
        }
    }

    // ------------------------------------------------------------ tick

    /// Every world tick: lazily populates loaded maps (safe ones are not populated by
    /// `ensure_map`), handles deaths/respawns everywhere and runs the AI loop on maps with
    /// players. Maps and NPCs go in a stable order (determinism with the world seed).
    pub(crate) fn tick_npcs(&mut self, now: GameTime) {
        let mut maps: Vec<u16> = self.maps.keys().copied().collect();
        maps.sort_unstable();
        for map in maps {
            if !self.instance(map).populated {
                self.populate_map(map);
            }
            self.tick_respawns(map, now);
            self.tick_pets(map, now);
            let inst = self.instance(map);
            // NpcRequiresAiUpdate: only maps with users.
            if inst.players.is_empty() || now < inst.next_ai {
                continue;
            }
            self.maps.get_mut(&map).unwrap().next_ai = now.after_ms(AI_INTERVAL_MS);
            let ids = self.instance(map).npc_ids().to_vec();
            for nid in ids {
                let Some(n) = self.instance(map).npcs.get(&nid) else { continue };
                if !n.alive() || now < n.next_move {
                    continue;
                }
                let Some(d) = self.ao.npcs.get(n.def as u32) else { continue };
                // IntervaloPermiteMoverse
                let step = ms(d.move_interval, DEFAULT_MOVE_MS);
                self.npc_mut(map, nid).next_move = now.after_ms(step);
                // TIMER_AI_Timer: a paralysed creature does nothing at all.
                if self.npc_paralyzed(map, nid) {
                    continue;
                }
                self.npc_ai(map, nid, now);
            }
        }
    }

    /// `MuereNpc` (respawn part) + `ProcessRespawnQueue`: combat marks the NPC dead
    /// (`dead_until`); here it leaves its tile, and either never comes back (`ReSpawn=1`)
    /// or comes back after its rolled `IntervaloRespawn` seconds (0 = right away).
    fn tick_respawns(&mut self, map: u16, now: GameTime) {
        let ids = self.instance(map).npc_ids().to_vec();
        for nid in ids {
            let Some(n) = self.instance(map).npcs.get(&nid) else { continue };
            if n.alive() {
                continue;
            }
            if !n.attackable || n.kind == NpcType::DummyTarget {
                // `Attackable=0` NPCs (bankers, priests...) cannot die in AO20 and training
                // dummies never die (`MuereNpc`); if another system killed one anyway, it
                // stays where it was instead of respawning at a random tile.
                tracing::warn!(map, nid, name = %n.name, "NPC no atacable marcado como muerto: se restaura");
                let n = self.npc_mut(map, nid);
                n.dead_until = None;
                n.hp = n.max_hp;
                let msg = n.spawn_msg(nid);
                self.broadcast(map, &msg, None);
                continue;
            }
            match n.respawn_at {
                None => {
                    // Summons, companions and trainer creatures were made for someone: gone.
                    let respawns = !n.pet.no_respawn && self.ao.npcs.get(n.def as u32).is_some_and(|d| d.respawn == 0);
                    let (x, y, secs) = (n.x, n.y, n.respawn_secs);
                    let inst = self.maps.get_mut(&map).unwrap();
                    inst.unplace(nid, x, y);
                    if !respawns {
                        inst.remove_npc(nid);
                        continue;
                    }
                    let n = inst.npcs.get_mut(&nid).unwrap();
                    n.respawn_at = Some(now.after_secs(secs as u64));
                    if secs == 0 {
                        self.respawn_npc(map, nid, now);
                    }
                }
                Some(t) if now >= t => self.respawn_npc(map, nid, now),
                Some(_) => {}
            }
        }
    }

    /// `ReSpawnNpc` → `CrearNPC`: back at `Orig` with `OrigPos=1`, else on a random legal
    /// tile of the map (which becomes its new `Orig`). Same id, so clients just see a Spawn.
    fn respawn_npc(&mut self, map: u16, nid: u32, now: GameTime) {
        let def = self.instance(map).npcs[&nid].def;
        let home = self.instance(map).npcs[&nid].home;
        let Some(d) = self.ao.npcs.get(def as u32).cloned() else { return };
        let pos = if d.respawn_at_origin == 1 && self.npc_spawn_ok(map, &d, home.0, home.1) {
            Some(home)
        } else {
            let mut found = None;
            for _ in 0..MAX_SPAWN_ATTEMPTS {
                let (cx, cy) = (self.rng.range(walk::MIN_X + 2, walk::MAX_X - 2), self.rng.range(walk::MIN_Y + 2, walk::MAX_Y - 2));
                found = self.closest_spawn_pos(map, &d, cx, cy, SPAWN_SEARCH_RADIUS);
                if found.is_some() {
                    break;
                }
            }
            found.or_else(|| self.closest_spawn_pos(map, &d, 50, 50, 50))
        };
        let Some((x, y)) = pos else {
            // CrearNPC gives up (eFailToFindSpawnPos): try again in a while.
            self.npc_mut(map, nid).respawn_at = Some(now.after_secs(5));
            return;
        };
        let mut fresh = self.new_npc(def, &d, x, y, now);
        if d.respawn_at_origin == 1 {
            fresh.home = home;
        }
        let msg = fresh.spawn_msg(nid);
        let inst = self.maps.get_mut(&map).unwrap();
        *inst.npcs.get_mut(&nid).unwrap() = fresh;
        inst.move_npc(nid, x, y);
        self.broadcast(map, &msg, None);
    }

    /// `ClosestLegalPosNPC`: square rings around (cx, cy).
    fn closest_spawn_pos(&self, map: u16, d: &NpcData, cx: i32, cy: i32, radius: i32) -> Option<(i32, i32)> {
        for r in 0..=radius {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) == r && self.npc_spawn_ok(map, d, cx + dx, cy + dy) {
                        return Some((cx + dx, cy + dy));
                    }
                }
            }
        }
        None
    }

    /// `ValidNPCSpawnPos`: `LegalPos` (no exit, not fully blocked, water/land, free),
    /// `TestSpawnTrigger` and no player around (`HayPCarea`).
    fn npc_spawn_ok(&self, map: u16, d: &NpcData, x: i32, y: i32) -> bool {
        if !walk::in_bounds(x, y) {
            return false;
        }
        let inst = self.instance(map);
        let m = &inst.map;
        let b = m.blocked(x, y);
        let water = b & FLAG_WATER != 0;
        if m.exit(x, y).is_some() || b & BLOCK_ALL == BLOCK_ALL || (water && d.water_valid != 1) || (!water && d.land_invalid != 0) {
            return false;
        }
        let t = m.trigger(x, y);
        if !(t < 1 || (t > 3 && t < 12)) || self.occupied(map, x, y) || (d.hostile != 0 && self.in_start_sanctuary(map, x, y)) {
            return false;
        }
        !inst.players.iter().filter_map(|p| self.player(*p)).any(|p| (p.x - x).abs() <= VISION_X && (p.y - y).abs() <= VISION_Y)
    }

    // ------------------------------------------------------------ AI

    /// `NpcAI`: dispatch on `Movement`; creatures with an owner (`SigueAmo`, `NpcAtacaNpc`)
    /// run `pet_ai` (systems/pets.rs). Not ported yet (they stand still): `GuardiaPersigueNpc` (6 `GuardiaNpc` in the maps),
    /// `SupportAndAttack`, the `BG*` battleground behaviours, `Invasion`, cross-map routes,
    /// `TryGuardAttackHostileNpc`, `NpcDummyUpdate` and `AI_RangeAttack` kiting.
    fn npc_ai(&mut self, map: u16, nid: u32, now: GameTime) {
        // NPCAI: a creature with a master follows/fights for it (systems/pets.rs).
        if self.instance(map).npcs[&nid].pet.owner.is_some() {
            return self.pet_ai(map, nid, now);
        }
        let n = &self.instance(map).npcs[&nid];
        let ao = self.ao.clone();
        let Some(d) = ao.npcs.get(n.def as u32) else { return };
        let (movement, hostile, kind) = (n.movement, n.hostile, n.kind);
        match movement {
            Movement::Wander => {
                if hostile && d.attack_range > 1 {
                    // AI_RangeAttack: shoots from afar and keeps its PreferedRange.
                    self.range_attack(map, nid, d, now);
                } else if hostile {
                    self.chase_nearest_user(map, nid, d, now);
                } else {
                    self.wander_near_origin(map, nid, now);
                }
            }
            Movement::FixedInPos => {
                if hostile {
                    if is_guard(kind) && d.attack_range > 1 {
                        self.range_attack(map, nid, d, now);
                    } else if is_guard(kind) {
                        self.chase_nearest_user(map, nid, d, now);
                    } else {
                        self.attack_from_pos(map, nid, d, now);
                    }
                }
            }
            Movement::Defend => self.follow_aggressor(map, nid, d, now),
            Movement::Walk => self.walk_route(map, nid, d, now),
            Movement::SupportAndAttack => self.support_and_attack(map, nid, d, now),
            _ => {}
        }
    }

    /// `PerseguirUsuarioCercano`: picks a target (enemy in front, else the nearest
    /// attackable or nearest visible valid user; a passive NPC only its aggressor) and
    /// attacks/chases it; with no target it wanders back near its origin.
    fn chase_nearest_user(&mut self, map: u16, nid: u32, d: &NpcData, now: GameTime) {
        let n = &self.instance(map).npcs[&nid];
        let was_passive = !n.old_hostile;
        let aggressor = n.attacked_by;
        let pos = (n.x, n.y);
        let mut target = None;
        let front = self.user_in_front(map, nid);
        if was_passive {
            // Someone hit it: the aggressor is the only target that matters.
        } else if let Some(u) = front.filter(|u| self.npc_enemy(map, nid, *u)) {
            target = Some(u);
        } else {
            let mut players: Vec<EntityId> = self.instance(map).players.iter().copied().collect();
            players.sort_unstable();
            let (mut best, mut best_d) = (None, i32::MAX);
            for pid in players {
                if !self.valid_target(map, nid, d, pid, aggressor) {
                    continue;
                }
                let p = self.player(pid).unwrap();
                let dist = distancia(pos, (p.x, p.y));
                // UsuarioAtacableConMelee: an invisible or hidden player is only a target
                // when standing right next to the creature.
                if dist > 1 && self.user_concealed(pid, now) {
                    continue;
                }
                if dist < best_d {
                    best = Some(pid);
                    best_d = dist;
                }
            }
            target = best;
        }
        if was_passive {
            match aggressor {
                Some(a) if self.in_vision(map, nid, d, a) => target = Some(a),
                _ => self.npc_mut(map, nid).attacked_by = None,
            }
        }
        self.npc_mut(map, nid).target = target;
        if let Some(t) = target {
            let tp = self.player(t).map(|p| (p.x, p.y)).unwrap_or(pos);
            self.npc_face(map, nid, heading_to(pos, tp));
            self.attack_target(map, nid, d, now);
        } else if !is_guard(self.instance(map).npcs[&nid].kind) {
            self.restore_old_movement(map, nid);
            self.wander_near_origin(map, nid, now);
        } else {
            let home = self.instance(map).npcs[&nid].home;
            if distancia(pos, home) > 0 {
                self.walk_towards(map, nid, home, now);
            } else {
                self.npc_face(map, nid, dir_from_heading(d.heading));
            }
        }
    }

    /// `AI_AtacarUsuarioObjetivo`: spell if it casts and the interval allows, else melee
    /// on the user in front; then keeps closing the distance.
    fn attack_target(&mut self, map: u16, nid: u32, d: &NpcData, now: GameTime) {
        let n = &self.instance(map).npcs[&nid];
        let Some(t) = n.target else { return };
        let Some(tp) = self.player(t).filter(|p| p.map == map).map(|p| (p.x, p.y)) else { return };
        let pos = (n.x, n.y);
        let stuck = distancia(pos, tp) <= 1;
        let casts = !d.spells.is_empty();
        let magic = casts && now >= n.next_spell;
        let target_alive = self.player(t).is_some_and(|p| p.hp > 0);
        let melee = stuck && target_alive && (!casts || (MAGIC_AND_PUNCH && !d.dont_hit_visible_players));
        let heading = heading_to(pos, tp);
        if magic {
            self.npc_mut(map, nid).next_spell = now.after_ms(ms(d.spell_interval, DEFAULT_SPELL_MS));
            // NpcLanzaSpellInmovilizado: rooted casters only along the line they face.
            if self.npc_can_cast_at(map, nid, t) {
                self.npc_face(map, nid, heading);
                self.npc_cast_one(map, nid, d, t);
            }
        } else if melee {
            self.npc_face(map, nid, heading);
            if self.user_in_front(map, nid) == Some(t) && now >= self.instance(map).npcs[&nid].next_attack && self.npc_enemy(map, nid, t) {
                self.npc_mut(map, nid).next_attack = now.after_ms(ms(d.attack_interval, DEFAULT_ATTACK_MS));
                self.npc_attacks_player(map, nid, t);
            }
        }
        if !self.instance(map).npcs.contains_key(&nid) || !self.instance(map).npcs[&nid].alive() {
            return;
        }
        if target_alive {
            // "If it has no path but is next to the user, don't waste time on paths."
            if self.instance(map).npcs[&nid].path.is_empty() && stuck {
                return;
            }
            let tp = self.player(t).map(|p| (p.x, p.y)).unwrap_or(tp);
            self.walk_towards(map, nid, tp, now);
        } else {
            self.wander_near_origin(map, nid, now);
        }
    }

    /// `NpcLanzaUnSpell`: random spell of its list at the target, within the NPC spell
    /// range. Target-type rules (self, area, NPCs) belong to the magic system's hook.
    fn npc_cast_one(&mut self, map: u16, nid: u32, d: &NpcData, target: EntityId) {
        let n = &self.instance(map).npcs[&nid];
        let Some(p) = self.player(target) else { return };
        if (n.x - p.x).abs() > SPELL_RANGE_X || (n.y - p.y).abs() > SPELL_RANGE_Y || p.hp <= 0 {
            return;
        }
        let i = self.rng.range(0, d.spells.len() as i32 - 1) as usize;
        let spell = d.spells[i].spell;
        if spell > 0 {
            self.npc_cast_spell(map, nid, target, spell as u16);
        }
    }

    /// `AttackFromPos` (FixedInPos, hostile, not a guard): faces the nearest user and
    /// casts or hits whoever stands in front, never leaving its tile.
    fn attack_from_pos(&mut self, map: u16, nid: u32, d: &NpcData, now: GameTime) {
        let n = &self.instance(map).npcs[&nid];
        let pos = (n.x, n.y);
        let mut players: Vec<EntityId> = self.instance(map).players.iter().copied().collect();
        players.sort_unstable();
        let near = players
            .into_iter()
            .filter(|pid| self.valid_target(map, nid, d, *pid, None))
            .min_by_key(|pid| self.player(*pid).map_or(i32::MAX, |p| distancia(pos, (p.x, p.y))));
        let Some(t) = near else {
            if !self.instance(map).npcs[&nid].old_hostile {
                self.restore_old_movement(map, nid);
            }
            return;
        };
        self.npc_mut(map, nid).target = Some(t);
        let tp = self.player(t).map(|p| (p.x, p.y)).unwrap();
        self.npc_face(map, nid, heading_to(pos, tp));
        if !d.spells.is_empty() && now >= self.instance(map).npcs[&nid].next_spell {
            self.npc_mut(map, nid).next_spell = now.after_ms(ms(d.spell_interval, DEFAULT_SPELL_MS));
            self.npc_cast_one(map, nid, d, t);
        } else if let Some(front) = self.user_in_front(map, nid) {
            if now >= self.instance(map).npcs[&nid].next_attack {
                self.npc_mut(map, nid).next_attack = now.after_ms(ms(d.attack_interval, DEFAULT_ATTACK_MS));
                self.npc_attacks_player(map, nid, front);
            }
        }
    }

    /// `SeguirAgresor` (NpcDefensa): goes after whoever attacked it while it is a valid
    /// target, then returns to its old behaviour.
    fn follow_aggressor(&mut self, map: u16, nid: u32, d: &NpcData, now: GameTime) {
        let n = &self.instance(map).npcs[&nid];
        match n.target {
            Some(t) if self.valid_target(map, nid, d, t, n.attacked_by) => self.attack_target(map, nid, d, now),
            _ => self.restore_old_movement(map, nid),
        }
    }

    /// `RestoreOldMovement`.
    fn restore_old_movement(&mut self, map: u16, nid: u32) {
        let n = self.npc_mut(map, nid);
        n.target = None;
        n.vision = n.original_vision;
        n.movement = n.old_movement;
        n.hostile = n.old_hostile;
        n.attacked_by = None;
    }

    /// `AI_CaminarSinRumboCercaDeOrigen`: farther than 4 from `Orig` walks back, else one
    /// time in six takes a random step.
    pub(crate) fn wander_near_origin(&mut self, map: u16, nid: u32, now: GameTime) {
        let n = &self.instance(map).npcs[&nid];
        let (pos, home) = ((n.x, n.y), n.home);
        if distancia(pos, home) > 4 {
            self.walk_towards(map, nid, home, now);
        } else if self.rng.range(1, 6) == 3 {
            let dir = dir_from_heading(self.rng.range(1, 4));
            self.npc_move(map, nid, dir);
        }
    }

    /// `HacerCaminata`: walks the npcs.dat route (`CaminataN` offsets from `Orig`), waiting
    /// at each point.
    fn walk_route(&mut self, map: u16, nid: u32, d: &NpcData, now: GameTime) {
        if d.walk.is_empty() {
            return;
        }
        let n = &self.instance(map).npcs[&nid];
        let i = n.walk_step % d.walk.len();
        let step = d.walk[i];
        let dest = (n.home.0 + step.dx, n.home.1 + step.dy);
        let pos = (n.x, n.y);
        let next = (i + 1) % d.walk.len();
        if pos != dest {
            let moved = self.npc_move(map, nid, heading_to(pos, dest));
            let n = self.npc_mut(map, nid);
            if !moved || (n.x, n.y) == dest {
                n.next_move = now.after_ms(step.wait.max(0) as u64);
                n.walk_step = next;
            }
        } else {
            self.npc_mut(map, nid).walk_step = next;
        }
    }

    /// `AI_CaminarConRumbo`: follows the cached path, recomputing it (`SeekPath` with
    /// `Closest`) when the destination changed, at most every 250 ms. A failed search
    /// widens a hostile chaser's vision by `PATH_VISION_DELTA`.
    pub(crate) fn walk_towards(&mut self, map: u16, nid: u32, dest: (i32, i32), now: GameTime) {
        let n = &self.instance(map).npcs[&nid];
        let pos = (n.x, n.y);
        if pos == dest {
            self.npc_mut(map, nid).path.clear();
            return;
        }
        let needs_new = n.path.is_empty() || n.path_dest != dest;
        if needs_new {
            let recompute = n.path.is_empty() || now >= n.next_path_at;
            self.npc_mut(map, nid).path_dest = dest;
            if !recompute {
                self.follow_path(map, nid);
                return;
            }
            let found = self.seek_path(map, nid, dest);
            let n = self.npc_mut(map, nid);
            n.next_path_at = now.after_ms(pathfinding::RECOMPUTE_COOLDOWN_MS);
            match found {
                Some(p) => {
                    n.path = p;
                    self.follow_path(map, nid);
                }
                None => {
                    n.path.clear();
                    if n.hostile && n.target.is_some() {
                        n.vision = (n.vision + pathfinding::PATH_VISION_DELTA).min(pathfinding::MAX_VISION_RANGE);
                    }
                }
            }
        } else {
            self.follow_path(map, nid);
        }
    }

    /// `FollowPath`: one step along the cached path. A blocked step drops the path so the
    /// next run searches again.
    fn follow_path(&mut self, map: u16, nid: u32) {
        let n = &self.instance(map).npcs[&nid];
        let Some(&next) = n.path.first() else { return };
        let dir = heading_to((n.x, n.y), next);
        if distancia((n.x, n.y), next) == 1 && self.npc_move(map, nid, dir) {
            self.npc_mut(map, nid).path.remove(0);
        } else {
            self.npc_mut(map, nid).path.clear();
        }
    }

    fn seek_path(&mut self, map: u16, nid: u32, dest: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        let n = &self.instance(map).npcs[&nid];
        let q = Query { from: (n.x, n.y), to: dest, vision: n.vision, max_steps: pathfinding::MAX_PATH_LENGTH, noise: pathfinding::DEFAULT_NOISE, closest: true };
        let (target, def) = (n.target, n.def);
        let ao = self.ao.clone();
        let d = ao.npcs.get(def as u32)?;
        // A local xorshift seeded from the world RNG: deterministic, and the search only
        // needs a shared borrow of the world.
        let mut seed = self.rng.next_u32() | 1;
        // Tiles taken by users other than the target, gathered once per search.
        let mut users = vec![false; 101 * 101];
        for pid in &self.instance(map).players {
            if Some(*pid) == target {
                continue;
            }
            if let Some(p) = self.player(*pid).filter(|p| walk::in_bounds(p.x, p.y)) {
                users[(p.y * 101 + p.x) as usize] = true;
            }
        }
        let world: &World = self;
        pathfinding::seek_path(
            &q,
            |x, y, dir| !users[(y * 101 + x) as usize] && world.npc_walkable(map, d, x, y, dir),
            || {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed
            },
        )
    }

    /// `IsWalkable` (PathFinding.bas): no NPC, no user except the target, no exit, water
    /// / land per the NPC, no invalid-position trigger, side not blocked.
    /// (Users are checked by the caller with a per-search grid.)
    fn npc_walkable(&self, map: u16, d: &NpcData, x: i32, y: i32, dir: Dir) -> bool {
        self.instance(map).npc_at(x, y).is_none() && self.npc_tile_ok(map, d, x, y, dir)
    }

    /// Static part of `LegalWalkNPC` (bounds, exit, water/land, trigger, blocked side).
    /// Doors for guards (`HayPuerta`) are not modelled.
    pub(crate) fn npc_tile_ok(&self, map: u16, d: &NpcData, x: i32, y: i32, dir: Dir) -> bool {
        if !walk::in_bounds(x, y) {
            return false;
        }
        let m = &self.instance(map).map;
        let b = m.blocked(x, y);
        let water = b & FLAG_WATER != 0;
        !(m.exit(x, y).is_some()
            || (water && d.water_valid != 1)
            || (!water && d.land_invalid != 0)
            || m.trigger(x, y) == TRIGGER_INVALID_POS
            || b & dir.block_bit() != 0
            || (d.hostile != 0 && self.in_start_sanctuary(map, x, y)))
    }

    /// Inside the creature-free area around the start point (`balance.start_sanctuary`).
    pub(crate) fn in_start_sanctuary(&self, map: u16, x: i32, y: i32) -> bool {
        let b = &self.data.balance;
        b.start_sanctuary > 0 && map == b.start_map && (x - b.start_x).abs().max((y - b.start_y).abs()) <= b.start_sanctuary
    }

    /// `MoveNPCChar`: one step if `LegalWalkNPC` allows it (living users and NPCs block).
    /// Broadcasts `Moved` only when it actually moved or turned.
    pub(crate) fn npc_move(&mut self, map: u16, nid: u32, dir: Dir) -> bool {
        // AI_NPC.bas: `If flags.Inmovilizado = 1 Then Exit Function` before every step.
        if self.npc_rooted(map, nid) {
            return false;
        }
        let n = &self.instance(map).npcs[&nid];
        let (dx, dy) = dir.delta();
        let (tx, ty) = (n.x + dx, n.y + dy);
        let Some(d) = self.ao.npcs.get(n.def as u32) else { return false };
        let ok = self.npc_tile_ok(map, d, tx, ty, dir) && !self.occupied(map, tx, ty);
        if !ok {
            return false;
        }
        let inst = self.maps.get_mut(&map).unwrap();
        inst.move_npc(nid, tx, ty);
        inst.npcs.get_mut(&nid).unwrap().dir = dir;
        self.broadcast(map, &S2C::Moved { id: nid, x: tx, y: ty, dir }, None);
        true
    }

    /// `ChangeNPCChar` (heading only): sent only when it changes.
    pub(crate) fn npc_face(&mut self, map: u16, nid: u32, dir: Dir) {
        let n = self.npc_mut(map, nid);
        if n.dir == dir {
            return;
        }
        n.dir = dir;
        let (x, y) = (n.x, n.y);
        self.broadcast(map, &S2C::Moved { id: nid, x, y, dir }, None);
    }

    /// `NPCHasAUserInFront`: living user on the tile it faces.
    pub(crate) fn user_in_front(&self, map: u16, nid: u32) -> Option<EntityId> {
        let n = &self.instance(map).npcs[&nid];
        let (dx, dy) = n.dir.delta();
        self.player_at(map, n.x + dx, n.y + dy).filter(|u| self.player(*u).is_some_and(|p| p.hp > 0))
    }

    /// `EnRangoVision`: rectangle of `Distancia` (npcs.dat) or 15 x 13.
    pub(crate) fn in_vision(&self, map: u16, nid: u32, d: &NpcData, pid: EntityId) -> bool {
        let n = &self.instance(map).npcs[&nid];
        let (lx, ly) = if d.distance != 0 { (d.distance, d.distance) } else { (VISION_X, VISION_Y) };
        self.player(pid).is_some_and(|p| p.map == map && (p.x - n.x).abs() <= lx && (p.y - n.y).abs() <= ly)
    }

    /// `EsObjetivoValido`: in vision, level rule, enemy, alive.
    /// Invisible (spell) or hidden (skill): `flags.invisible + flags.Oculto <> 0`.
    pub(crate) fn user_concealed(&self, pid: EntityId, now: GameTime) -> bool {
        self.player(pid).is_some_and(|p| p.status.invisible_until.is_some_and(|u| now < u) || p.status.hidden)
    }

    pub(crate) fn valid_target(&self, map: u16, nid: u32, d: &NpcData, pid: EntityId, aggressor: Option<EntityId>) -> bool {
        if !self.in_vision(map, nid, d, pid) {
            return false;
        }
        let n = &self.instance(map).npcs[&nid];
        let Some(p) = self.player(pid) else { return false };
        if d.level > 0 {
            let level_ok = p.level as i32 - d.level <= DELTA_LEVEL_PENALTIES;
            let close = (p.x - n.x).abs().max((p.y - n.y).abs()) <= DELTA_LEVEL_CLOSE_RANGE;
            if !(level_ok || close || aggressor == Some(pid)) {
                return false;
            }
        }
        // GMs and players "en consulta" are P2 (no privileges yet).
        p.hp > 0 && self.npc_enemy(map, nid, pid)
    }

    /// `EsEnemigo`: the aggressor always; alignment rules for royal/chaos NPCs.
    pub(crate) fn npc_enemy(&self, map: u16, nid: u32, pid: EntityId) -> bool {
        let n = &self.instance(map).npcs[&nid];
        if n.attacked_by == Some(pid) {
            return true;
        }
        // `CanAttackNotOwner` (NPC ownership by its first attacker) is P2: anyone may.
        let status = self.player_faction_status(pid);
        match self.ao.npcs.get(n.def as u32).map_or(0, |d| d.alignment) {
            1 => status % 2 != 1,
            2 => status % 2 != 0,
            _ => true,
        }
    }

    /// `Status(UserIndex)`: 0 criminal, 1 citizen, 2 chaos, 3 royal army... (faction system).
    fn player_faction_status(&self, pid: EntityId) -> i32 {
        self.faction_status(pid) as i32
    }

    // ------------------------------------------------------------ hooks for others

    /// Combat/magic call this when a player hits an NPC (`NPCAtacado`, NPC part): a non
    /// static NPC turns hostile and goes for the attacker.
    pub(crate) fn npc_attacked_by(&mut self, map: u16, nid: u32, pid: EntityId) {
        let Some(n) = self.maps.get_mut(&map).and_then(|i| i.npcs.get_mut(&nid)) else { return };
        if n.movement != Movement::Static {
            n.target = Some(pid);
            n.hostile = true;
            n.attacked_by = Some(pid);
        }
    }

    pub(crate) fn npc_mut(&mut self, map: u16, nid: u32) -> &mut Npc {
        self.maps.get_mut(&map).unwrap().npcs.get_mut(&nid).unwrap()
    }
}

/// In-crate scenarios on a `World` built directly (no sockets): kill/respawn, dead players,
/// chase around a wall, and the per-tick cost. Bot-driven scenarios are in
/// `tests/p1_npcs.rs`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{Clock, VirtualClock};
    use crate::net::Event;
    use crate::Settings;
    use ao_formats::csm::tile;
    use ao_protocol::{FrameReader, C2S, PROTOCOL_VERSION};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    const LOBO: u16 = 500;

    struct Bench {
        w: World,
        clock: VirtualClock,
        t: GameTime,
    }

    impl Bench {
        fn new(map: u16, x: i32, y: i32) -> Bench {
            let mut s = Settings::defaults();
            let clock = VirtualClock::new();
            s.clock = Clock::Virtual(clock.clone());
            s.start = Some((map, x, y));
            let mut w = World::new(s);
            assert!(w.ensure_map(map));
            Bench { w, clock, t: GameTime::ZERO }
        }

        fn join(&mut self, id: u32, name: &str) -> UnboundedReceiver<Vec<u8>> {
            let (tx, rx) = unbounded_channel();
            self.w.on_event(Event::Connected { id, addr: "127.0.0.1:9".parse().unwrap(), tx });
            self.w.handle(id, C2S::Login { name: name.into(), version: PROTOCOL_VERSION });
            assert!(self.w.player(id).is_some(), "dev login");
            rx
        }

        fn run(&mut self, ms: u64) {
            let end = self.t.after_ms(ms);
            while self.t < end {
                self.t = self.t.after_ms(50);
                self.clock.set(self.t);
                self.w.tick(self.t);
            }
        }

        fn place(&mut self, id: u32, x: i32, y: i32) {
            let p = self.w.player_mut(id);
            p.x = x;
            p.y = y;
        }

        /// Empties a square of the map (no blocks, triggers or NPCs).
        fn clear(&mut self, map: u16, x0: i32, y0: i32, x1: i32, y1: i32) {
            self.run(50); // populate
            let inst = self.w.maps.get_mut(&map).unwrap();
            for y in y0..=y1 {
                for x in x0..=x1 {
                    inst.map.blocked[tile(x, y)] = 0;
                    inst.map.triggers[tile(x, y)] = 0;
                    assert!(inst.map.exit(x, y).is_none());
                }
            }
            for id in inst.npc_ids().to_vec() {
                inst.remove_npc(id);
            }
        }

        fn spawn(&mut self, map: u16, number: u16, x: i32, y: i32) -> u32 {
            let ao = self.w.ao.clone();
            let d = ao.npcs.get(number as u32).unwrap();
            let npc = self.w.new_npc(number, d, x, y, self.t);
            let id = self.w.next_npc;
            self.w.next_npc += 1;
            self.w.maps.get_mut(&map).unwrap().add_npc(id, npc);
            id
        }

        fn npc(&self, map: u16, id: u32) -> &Npc {
            &self.w.instance(map).npcs[&id]
        }
    }

    fn frames(rx: &mut UnboundedReceiver<Vec<u8>>) -> Vec<S2C> {
        let mut r = FrameReader::default();
        while let Ok(b) = rx.try_recv() {
            r.push(&b);
        }
        let mut out = Vec::new();
        while let Some(Ok(m)) = r.next::<S2C>(1 << 20) {
            out.push(m);
        }
        out
    }

    fn console_has(msgs: &[S2C], needle: &str) -> bool {
        msgs.iter().any(|m| matches!(m, S2C::Console { text, .. } if text.contains(needle)))
    }

    #[test]
    fn hostile_creature_goes_around_a_wall_to_reach_the_player() {
        let mut b = Bench::new(M, 55, 50);
        let _rx = b.join(1, "Presa");
        b.clear(M, 30, 35, 70, 65);
        b.place(1, 55, 50);
        let wall: Vec<(i32, i32)> = (44..=56).map(|y| (50, y)).collect();
        for &(x, y) in &wall {
            b.w.maps.get_mut(&M).unwrap().map.blocked[tile(x, y)] = BLOCK_ALL;
        }
        let nid = b.spawn(M, LOBO, 45, 50);
        let hp0 = b.w.player(1).unwrap().hp;
        let mut trail = vec![(45, 50)];
        for _ in 0..400 {
            b.run(100);
            let n = b.npc(M, nid);
            if trail.last() != Some(&(n.x, n.y)) {
                trail.push((n.x, n.y));
            }
            if distancia((n.x, n.y), (55, 50)) == 1 {
                break;
            }
        }
        let n = b.npc(M, nid);
        assert_eq!(distancia((n.x, n.y), (55, 50)), 1, "reaches the player, trail {trail:?}");
        assert!(trail.iter().all(|t| !wall.contains(t)), "never walks through the wall");
        assert!(trail.iter().any(|t| t.1 < 44 || t.1 > 56), "goes round the end of the wall");
        assert_eq!(b.npc(M, nid).target, Some(1));
        // Then it hits through the combat hook (npc_attacks_player): the player's health
        // changes at some point (combat decides hit/miss and damage).
        let mut hit = false;
        for _ in 0..200 {
            b.run(100);
            let p = b.w.player(1).unwrap();
            hit |= p.hp != hp0 || (p.map, p.x, p.y) != (M, 55, 50);
        }
        assert!(hit, "the wolf attacks");
    }

    /// `UsuarioAtacableConMelee`: an invisible or hidden player is not chased from afar,
    /// but one right next to the creature is still a target.
    #[test]
    fn invisible_and_hidden_players_are_not_chased_from_afar() {
        for hidden in [false, true] {
            let mut b = Bench::new(M, 55, 50);
            let _rx = b.join(1, "Sombra");
            b.clear(M, 30, 35, 70, 65);
            b.place(1, 55, 50);
            let conceal = |b: &mut Bench| {
                let far = b.t.after_ms(3_600_000);
                let st = &mut b.w.player_mut(1).status;
                if hidden {
                    st.hidden = true;
                } else {
                    st.invisible_until = Some(far);
                }
            };
            conceal(&mut b);
            let nid = b.spawn(M, LOBO, 51, 50);
            for _ in 0..30 {
                b.run(100);
                conceal(&mut b);
            }
            let n = b.npc(M, nid);
            assert!(n.target != Some(1) && distancia((n.x, n.y), (55, 50)) > 1, "hidden={hidden}: not chased, wolf at {:?}", (n.x, n.y));
            // Right next to it: attacked anyway.
            b.place(1, n.x + 1, n.y);
            let mut targeted = false;
            for _ in 0..30 {
                b.run(100);
                conceal(&mut b);
                targeted |= b.npc(M, nid).target == Some(1);
            }
            assert!(targeted, "hidden={hidden}: next to the wolf, it attacks");
        }
    }

    #[test]
    fn killed_npc_respawns_after_its_interval() {
        let mut b = Bench::new(M, 55, 50);
        let mut rx = b.join(1, "Testigo");
        b.clear(M, 30, 35, 70, 65);
        b.place(1, 40, 40);
        let nid = b.spawn(M, LOBO, 60, 60);
        b.w.npc_mut(M, nid).respawn_secs = 3;
        // Killed through combat (damage_npc -> kill_npc sets dead_until).
        b.w.damage_npc(M, nid, 9999, 1);
        assert!(!b.npc(M, nid).alive());
        b.run(100);
        assert!(b.w.instance(M).npc_at(60, 60).is_none(), "a dead NPC frees its tile");
        frames(&mut rx);
        b.run(2700);
        assert!(!b.npc(M, nid).alive(), "not back before 3 s");
        b.run(400);
        let n = b.npc(M, nid);
        assert!(n.alive() && n.hp == n.max_hp, "back with full health");
        assert!(b.w.instance(M).npc_at(n.x, n.y) == Some(nid));
        assert!(frames(&mut rx).iter().any(|m| matches!(m, S2C::Spawn { id, .. } if *id == nid)), "clients see it come back");
    }

    #[test]
    fn npc_without_respawn_is_gone_for_good() {
        let mut b = Bench::new(M, 55, 50);
        b.clear(M, 30, 35, 70, 65);
        let ao = b.w.ao.clone();
        let (number, _) = ao.npcs.iter().find(|(_, d)| d.respawn == 1 && d.attackable == 1).expect("an NPC with ReSpawn=1");
        let nid = b.spawn(M, number as u16, 60, 60);
        b.w.npc_mut(M, nid).dead_until = Some(b.t);
        b.run(10_000);
        assert!(!b.w.instance(M).npcs.contains_key(&nid));
    }

    /// Bosque Este de Ullathorpe: open forest without exits in the middle.
    const M: u16 = 12;

    #[test]
    fn dead_player_only_talks_to_priests() {
        // Juan XXI <Sacerdote> (78, 67) and Krix <Finanzas Goliath> (75, 47) in Ullathorpe.
        let mut b = Bench::new(1, 75, 49);
        let mut rx = b.join(1, "Fantasma");
        b.run(100);
        b.w.player_mut(1).hp = 0;
        frames(&mut rx);
        b.w.on_interact(1, 75, 47);
        let m = frames(&mut rx);
        assert!(console_has(&m, DEAD_TEXT), "the banker refuses the dead: {m:?}");
        assert!(!m.iter().any(|m| matches!(m, S2C::NpcSay { .. } | S2C::BankOpen { .. })));

        b.place(1, 78, 69);
        b.w.on_interact(1, 78, 67);
        let m = frames(&mut rx);
        assert!(console_has(&m, "resucitado"), "the priest resurrects: {m:?}");
        assert!(m.iter().any(|m| matches!(m, S2C::NpcSay { .. })), "and the dead can read the priest");

        // Alive and hurt: the priest heals.
        b.w.player_mut(1).hp = 1;
        b.w.on_interact(1, 78, 67);
        frames(&mut rx);
        let p = b.w.player(1).unwrap();
        assert_eq!(p.hp, p.max_hp);

        // Too far (Manhattan 6 > 5).
        b.w.player_mut(1).hp = 1;
        b.place(1, 78, 73);
        b.w.on_interact(1, 78, 67);
        assert!(console_has(&frames(&mut rx), "lejos"));
        assert_eq!(b.w.player(1).unwrap().hp, 1);
    }

    const DEAD_TEXT: &str = "muerto";

    #[test]
    fn non_attackable_npcs_cannot_be_moved_by_a_kill() {
        let mut b = Bench::new(1, 75, 49);
        let _rx = b.join(1, "Atracador");
        b.run(100);
        let banker = b.w.instance(1).npc_at(75, 47).expect("Krix");
        assert!(!b.npc(1, banker).attackable);
        b.w.damage_npc(1, banker, 9999, 1);
        b.run(2000);
        let n = b.npc(1, banker);
        assert!(n.alive() && (n.x, n.y) == (75, 47), "the banker stays at his desk");
    }

    /// Cost of `tick_npcs` with a player on Ullathorpe (all its NPCs, static) and one on
    /// the densest creature map (Dungeon Newbie, 61 hostile wanderers chasing).
    #[test]
    fn tick_cost() {
        let mut b = Bench::new(1, 50, 50);
        let _a = b.join(1, "Uno");
        assert!(b.w.ensure_map(264));
        // 20 players spread over the densest creature map.
        let mut rxs = Vec::new();
        for i in 0..20u32 {
            rxs.push(b.join(2 + i, &format!("Cazador{i}")));
            let (x, y) = b.w.free_tile_near(264, 25 + (i as i32 % 5) * 12, 25 + (i as i32 / 5) * 12, 10);
            b.w.teleport(2 + i, 264, x, y);
        }
        b.run(1000);
        let npcs: usize = b.w.maps.values().map(|i| i.npcs.len()).sum();
        let mut worst = std::time::Duration::ZERO;
        let start = std::time::Instant::now();
        let ticks = 1200; // 60 s of game time
        for _ in 0..ticks {
            let t0 = std::time::Instant::now();
            b.run(50);
            worst = worst.max(t0.elapsed());
            // Keep the hunted players alive (and on the map) so the chase never stops.
            for i in 0..20u32 {
                let p = b.w.player_mut(2 + i);
                p.hp = p.max_hp;
            }
            for rx in &mut rxs {
                while rx.try_recv().is_ok() {}
            }
        }
        let avg = start.elapsed() / ticks;
        let hunting = b.w.instance(264).npcs.values().filter(|n| n.target.is_some()).count();
        let on264 = b.w.instance(264).players.len();
        println!("tick: {npcs} NPCs en 2 mapas, {on264} jugadores en el 264 ({hunting} NPCs persiguiendo), media {avg:?}/tick, peor {worst:?}");
        assert!(avg < std::time::Duration::from_millis(5), "average tick {avg:?}");
    }

    /// Load measurement for the "one actor per map" decision: 400 players over the 20
    /// maps with most creatures, each walking (real `Walk` messages, validated) every
    /// 250 ms, all AI running. Reports the world's time per 50 ms tick (messages included).
    /// `cargo test -p server --release -- --ignored load_400 --nocapture`
    #[test]
    #[ignore]
    fn load_400_players() {
        let mut b = Bench::new(1, 50, 50);
        let mut by_npcs: Vec<(usize, u16)> = (1..=300u16)
            .filter_map(|m| ao_formats::AoMap::load(&b.w.recursos, m).ok().map(|a| (a.npcs.len(), m)))
            .collect();
        by_npcs.sort_by(|a, b| b.cmp(a));
        let maps: Vec<u16> = by_npcs.iter().take(20).map(|(_, m)| *m).collect();
        let mut rxs = Vec::new();
        for i in 0..400u32 {
            let id = 10 + i;
            rxs.push(b.join(id, &format!("Carga{i}")));
            let map = maps[i as usize % maps.len()];
            assert!(b.w.ensure_map(map));
            let (x, y) = b.w.free_tile_near(map, 20 + (i as i32 * 7) % 60, 20 + (i as i32 * 13) % 60, 10);
            b.w.teleport(id, map, x, y);
        }
        b.run(1000);
        let dirs = [ao_rules::Dir::Up, ao_rules::Dir::Right, ao_rules::Dir::Down, ao_rules::Dir::Left];
        let mut seq = vec![0u32; 400];
        let (mut total, mut worst) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
        let ticks = 1200u32; // 60 s
        for t in 0..ticks {
            let t0 = std::time::Instant::now();
            // A fifth of the players step this tick (each one every 250 ms).
            for i in (t % 5..400).step_by(5) {
                seq[i as usize] += 1;
                let dir = dirs[((i + t / 20) % 4) as usize];
                b.w.handle(10 + i, C2S::Walk { dir, seq: seq[i as usize] });
            }
            b.run(50);
            let dt = t0.elapsed();
            total += dt;
            worst = worst.max(dt);
            for i in 0..400u32 {
                let p = b.w.player_mut(10 + i);
                p.hp = p.max_hp;
            }
            for rx in &mut rxs {
                while rx.try_recv().is_ok() {}
            }
        }
        let npcs: usize = b.w.maps.values().map(|i| i.npcs.len()).sum();
        let online = b.w.player_ids().len();
        println!("carga: {online} jugadores, {} mapas, {npcs} NPCs; media {:?}/tick de 50 ms, peor {worst:?}", maps.len(), total / ticks);
    }
}

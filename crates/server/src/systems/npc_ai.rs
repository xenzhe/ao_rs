//! Creature population and behaviour: wander near home, chase and bite visible players.
use crate::world::entities::Npc;
use crate::world::World;
use ao_protocol::S2C;
use ao_rules::{tile_distance, walk, Dir};
use crate::clock::GameTime;

impl World {
    /// Fills a freshly loaded, non-safe map with creatures picked by `spawn_weight`.
    pub(crate) fn spawn_creatures(&mut self, map: u16) {
        let defs = self.data.creatures.clone();
        let total: u32 = defs.iter().map(|d| d.spawn_weight).sum();
        if total == 0 {
            return;
        }
        for _ in 0..self.data.balance.creatures_per_map {
            let mut roll = self.rng.next_u32() % total;
            let def = defs.iter().find(|d| {
                if roll < d.spawn_weight {
                    true
                } else {
                    roll -= d.spawn_weight;
                    false
                }
            });
            let Some(def) = def else { continue };
            let mut pos = None;
            for _ in 0..500 {
                let (x, y) = (self.rng.range(walk::MIN_X + 3, walk::MAX_X - 3), self.rng.range(walk::MIN_Y + 3, walk::MAX_Y - 3));
                if walk::standable(&self.instance(map).map, x, y) && !self.occupied(map, x, y) {
                    pos = Some((x, y));
                    break;
                }
            }
            let Some((x, y)) = pos else { continue };
            let now = self.now();
            let id = self.next_npc;
            self.next_npc += 1;
            let npc = Npc {
                def: def.id,
                name: def.name.clone(),
                body: def.body,
                head: def.head,
                x,
                y,
                dir: Dir::Down,
                hp: def.hp,
                max_hp: def.hp,
                home: (x, y),
                next_move: now,
                next_attack: now,
                dead_until: None,
            };
            self.maps.get_mut(&map).unwrap().npcs.insert(id, npc);
        }
    }

    /// Sorted iteration: HashMap order is random and would make RNG use non-deterministic.
    pub(crate) fn tick_npcs(&mut self, now: GameTime) {
        let mut maps: Vec<u16> = self.maps.keys().copied().collect();
        maps.sort_unstable();
        for map in maps {
            let mut ids: Vec<u32> = self.instance(map).npcs.keys().copied().collect();
            ids.sort_unstable();
            for nid in ids {
                self.tick_npc(map, nid, now);
            }
        }
    }

    fn tick_npc(&mut self, map: u16, nid: u32, now: GameTime) {
        let n = &self.instance(map).npcs[&nid];
        if let Some(t) = n.dead_until {
            if now >= t {
                self.respawn_npc(map, nid);
            }
            return;
        }
        if now < n.next_move.min(n.next_attack) {
            return;
        }
        let Some(def) = self.data.creature(n.def).cloned() else { return };
        let (nx, ny, home) = (n.x, n.y, n.home);

        // Closest player in aggro range and line of sight.
        let m = &self.instance(map).map;
        let target = self
            .instance(map)
            .players
            .iter()
            .filter_map(|pid| self.player(*pid).map(|p| (*pid, p.x, p.y)))
            .filter(|(_, x, y)| tile_distance((nx, ny), (*x, *y)) <= def.aggro_range && walk::line_of_sight(m, nx, ny, *x, *y))
            .min_by_key(|(pid, x, y)| ((x - nx).abs() + (y - ny).abs(), *pid));

        match target {
            Some((pid, px, py)) if (px - nx).abs() + (py - ny).abs() == 1 => {
                if now >= n.next_attack {
                    let dmg = self.rng.range(def.damage[0], def.damage[1]);
                    self.npc_mut(map, nid).next_attack = now.after_ms(def.attack_ms);
                    self.damage_player(pid, dmg, &def.name);
                }
            }
            Some((_, px, py)) => {
                if now >= n.next_move {
                    let (dx, dy) = (px - nx, py - ny);
                    let primary = Dir::towards(dx, dy);
                    let secondary = Dir::towards(if primary.delta().0 != 0 { 0 } else { dx }, if primary.delta().1 != 0 { 0 } else { dy });
                    if !self.npc_step(map, nid, primary) {
                        self.npc_step(map, nid, secondary);
                    }
                    self.npc_mut(map, nid).next_move = now.after_ms(def.step_ms);
                }
            }
            None => {
                if now >= n.next_move {
                    let dir = Dir::ALL[self.rng.range(0, 3) as usize];
                    let (dx, dy) = dir.delta();
                    if tile_distance((nx + dx, ny + dy), home) <= def.wander_range {
                        self.npc_step(map, nid, dir);
                    }
                    let wait = self.rng.range(1200, 2800) as u64;
                    self.npc_mut(map, nid).next_move = now.after_ms(wait);
                }
            }
        }
    }

    fn npc_mut(&mut self, map: u16, nid: u32) -> &mut Npc {
        self.maps.get_mut(&map).unwrap().npcs.get_mut(&nid).unwrap()
    }

    fn npc_step(&mut self, map: u16, nid: u32, dir: Dir) -> bool {
        let (x, y) = { let n = &self.instance(map).npcs[&nid]; (n.x, n.y) };
        let (dx, dy) = dir.delta();
        let (tx, ty) = (x + dx, y + dy);
        let m = &self.instance(map).map;
        let ok = walk::can_enter(m, tx, ty, dir) && m.exit(tx, ty).is_none() && !self.occupied(map, tx, ty);
        let n = self.npc_mut(map, nid);
        n.dir = dir;
        if ok {
            n.x = tx;
            n.y = ty;
        }
        let msg = S2C::Moved { id: nid, x: n.x, y: n.y, dir };
        self.broadcast(map, &msg, None);
        ok
    }

    fn respawn_npc(&mut self, map: u16, nid: u32) {
        let home = self.instance(map).npcs[&nid].home;
        let (x, y) = self.free_tile_near(map, home.0, home.1, 3);
        let n = self.npc_mut(map, nid);
        n.dead_until = None;
        n.hp = n.max_hp;
        n.x = x;
        n.y = y;
        let msg = n.spawn_msg(nid);
        self.broadcast(map, &msg, None);
    }
}

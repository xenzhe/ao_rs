//! Walking: pace (anti speed-hack), sequence (anti replay), official AO20 tile rules,
//! occupancy, and map exits.
use crate::world::World;
use ao_protocol::S2C;
use ao_formats::csm::{TRIGGER_AUTO_RESURRECT, TRIGGER_EXIT_ONLY_DEAD, TRIGGER_PATREON_ONLY, TRIGGER_WORKER_ONLY};
use ao_rules::{walk, Dir};

/// `e_Class.Trabajador`.
const CLASS_WORKER: u8 = 9;

impl World {
    /// Tile rules that depend on who walks (`LegalPos`): workers-only and Patreon tiles.
    fn tile_allows(&self, id: u32, map: u16, x: i32, y: i32) -> bool {
        let t = self.instance(map).map.trigger(x, y);
        let gm = self.gm_rank(id) != ao_protocol::p4::gm::GmRank::User;
        match t {
            TRIGGER_WORKER_ONLY => gm || self.player(id).is_some_and(|p| p.class == CLASS_WORKER),
            TRIGGER_PATREON_ONLY => gm,
            _ => true,
        }
    }

    /// `ResucitarOCurar`: the dead come back with full life; the hurt are healed.
    fn auto_resurrect(&mut self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let (map, hurt) = (p.map, p.hp < p.max_hp);
        if self.is_dead(id) {
            self.resurrect(id);
            self.player_mut(id).status.poisoned = false;
            self.broadcast(map, &S2C::Fx { id, fx: 35, loops: 1 }, None);
            self.console(id, "¡¡Has sido resucitado!!", ao_protocol::Tone::Good);
        } else if hurt {
            let p = self.player_mut(id);
            p.hp = p.max_hp;
            p.status.poisoned = false;
            let (hp, max_hp) = (p.hp, p.max_hp);
            self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
            self.send_stats(id);
            self.broadcast(map, &S2C::Fx { id, fx: 9, loops: 1 }, None);
            self.console(id, "¡¡Has sido curado!!", ao_protocol::Tone::Good);
        }
    }

    /// Faces a direction without moving (others see the new heading).
    pub(crate) fn on_turn(&mut self, id: u32, dir: Dir) {
        let p = self.player_mut(id);
        if p.dir == dir {
            return;
        }
        p.dir = dir;
        let (map, x, y) = (p.map, p.x, p.y);
        self.broadcast(map, &S2C::Moved { id, x, y, dir }, Some(id));
    }

    pub(crate) fn on_walk(&mut self, id: u32, dir: Dir, seq: u32) {
        // Paced by arrival time: a busy world tick must not turn queued steps into a burst.
        let now = self.frame_at.unwrap_or_else(|| self.now());
        let (map, x, y, seq_ok, pace_ok) = {
            let p = self.player_mut(id);
            let seq_ok = p.seq.accept(seq);
            let pace_ok = seq_ok && p.walk.try_step(now);
            (p.map, p.x, p.y, seq_ok, pace_ok)
        };
        if !seq_ok {
            return self.strike(id, "paquete repetido (replay)");
        }
        if !pace_ok {
            self.send(id, &S2C::Correct { seq, x, y });
            return self.strike(id, "caminar demasiado rapido (speedhack)");
        }
        // Paralysis/immobilization (magic): refused without a strike, the client may not
        // know yet that it was paralysed.
        if !self.can_move(id) {
            self.send(id, &S2C::Correct { seq, x, y });
            return;
        }

        let (dx, dy) = dir.delta();
        let (nx, ny) = (x + dx, y + dy);
        let sailing = self.player(id).is_some_and(|p| p.status.sailing.is_some());
        let legal = walk::can_enter_as(&self.instance(map).map, nx, ny, dir, sailing) && !self.occupied(map, nx, ny) && self.tile_allows(id, map, nx, ny);
        if !legal {
            // Not a cheat by itself: prediction can race with someone moving in.
            self.player_mut(id).dir = dir;
            self.send(id, &S2C::Correct { seq, x, y });
            self.broadcast(map, &S2C::Moved { id, x, y, dir }, Some(id));
            return;
        }

        let p = self.player_mut(id);
        p.x = nx;
        p.y = ny;
        p.dir = dir;
        self.broadcast(map, &S2C::Moved { id, x: nx, y: ny, dir }, Some(id));
        self.thief_on_move(id);
        let trigger = self.instance(map).map.trigger(nx, ny);
        if trigger == TRIGGER_AUTO_RESURRECT {
            self.auto_resurrect(id);
        }
        // TRANSFER_ONLY_DEAD: the exit here ignores the living.
        let dead = self.is_dead(id);
        if let Some(exit) = self.instance(map).map.exit(nx, ny).filter(|_| trigger != TRIGGER_EXIT_ONLY_DEAD || dead) {
            // CheckMapRestrictions: refused, the player stays on the exit tile. The same
            // notice is not repeated while trying again (`flags.UltimoMensaje`).
            if let Some(why) = self.map_entry_refusal(id, exit.map) {
                let p = self.player_mut(id);
                if p.last_refusal.as_deref() != Some(why.as_str()) {
                    p.last_refusal = Some(why.clone());
                    self.console(id, why, ao_protocol::Tone::Warn);
                }
                return;
            }
            self.player_mut(id).last_refusal = None;
            // A teleport object on the tile scatters arrivals within its `Radio`.
            let radius = self.map_object_at(map, nx, ny).and_then(|o| self.ao.objs.get(o as u32)).filter(|d| d.obj_type == ao_formats::dat::obj::ObjType::Teleport).map_or(0, |d| d.radius.max(0));
            let (mut dx, mut dy) = (exit.dest_x, exit.dest_y);
            if radius > 0 {
                dx = self.rng.range(dx - radius, dx + radius).clamp(1, 100);
                dy = self.rng.range(dy - radius, dy + radius).clamp(1, 100);
            }
            self.teleport(id, exit.map, dx, dy);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::clock::{Clock, GameTime, VirtualClock};
    use crate::net::Event;
    use crate::world::World;
    use crate::Settings;
    use ao_formats::csm::TRIGGER_AUTO_RESURRECT;
    use ao_protocol::{C2S, PROTOCOL_VERSION};
    use ao_rules::{walk, Dir};

    /// An AUTORESU tile of Ullathorpe with a free tile next to it: (tile, from, dir).
    fn auto_res_tile(w: &World) -> ((i32, i32), (i32, i32), Dir) {
        let m = &w.instance(1).map;
        for y in 2..100 {
            for x in 2..100 {
                if m.trigger(x, y) != TRIGGER_AUTO_RESURRECT {
                    continue;
                }
                for d in Dir::ALL {
                    let (dx, dy) = d.delta();
                    let from = (x - dx, y - dy);
                    if walk::standable(m, from.0, from.1) && m.trigger(from.0, from.1) != TRIGGER_AUTO_RESURRECT && walk::can_enter(m, x, y, d) {
                        return ((x, y), from, d);
                    }
                }
            }
        }
        panic!("no AUTORESU tile on map 1");
    }

    #[test]
    fn floor_items_vanish_after_the_cleanup_time() {
        let mut s = Settings::defaults();
        let clock = VirtualClock::new();
        s.clock = Clock::Virtual(clock.clone());
        s.npcs = false;
        let mut w = World::new(s);
        w.ensure_map(1);
        clock.set(GameTime(1000));
        w.make_obj(1, 50, 50, crate::world::inventory::InvItem { obj: 1093, amount: 1 });
        let at = |w: &World| w.instance(1).ground.at(50, 50).is_some();
        clock.set(GameTime(1000 + 14 * 60_000));
        w.tick_floor_cleanup(GameTime(1000 + 14 * 60_000));
        assert!(at(&w), "still there after 14 minutes");
        clock.set(GameTime(1000 + 16 * 60_000));
        w.tick_floor_cleanup(GameTime(1000 + 16 * 60_000));
        assert!(!at(&w), "gone after 15 minutes");
    }

    #[test]
    fn auto_resurrect_tiles_heal_the_hurt_and_raise_the_dead() {
        let mut s = Settings::defaults();
        let clock = VirtualClock::new();
        s.clock = Clock::Virtual(clock.clone());
        s.start = Some((1, 50, 50));
        s.npcs = false;
        let mut w = World::new(s);
        w.ensure_map(1);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        w.on_event(Event::Connected { id: 1, addr: "127.0.0.1:9".parse().unwrap(), tx });
        w.handle(1, C2S::Login { name: "Herido".into(), version: PROTOCOL_VERSION });
        let (tile, from, dir) = auto_res_tile(&w);
        let mut t = 1000u64;
        let mut seq = 0;
        let mut step = |w: &mut World, d: Dir| {
            t += 1000;
            clock.set(GameTime(t));
            seq += 1;
            w.on_walk(1, d, seq);
        };
        let back = match dir {
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
            Dir::Left => Dir::Right,
            Dir::Right => Dir::Left,
        };
        // Hurt: healed.
        w.teleport(1, 1, from.0, from.1);
        w.player_mut(1).hp = 1;
        step(&mut w, dir);
        let p = w.player(1).unwrap();
        assert_eq!((p.x, p.y), tile);
        assert_eq!(p.hp, p.max_hp, "healed on the tile");
        // Dead: back on its feet.
        step(&mut w, back);
        let p = w.player_mut(1);
        p.hp = 0;
        p.combat.dead = true;
        step(&mut w, dir);
        assert!(!w.is_dead(1), "resurrected on the tile");
        assert_eq!(w.player(1).map(|p| p.hp == p.max_hp), Some(true));
    }
}

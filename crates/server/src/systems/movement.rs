//! Walking: pace (anti speed-hack), sequence (anti replay), official AO20 tile rules,
//! occupancy, and map exits.
use crate::world::World;
use ao_protocol::S2C;
use ao_rules::{walk, Dir};

impl World {
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
        let now = self.now();
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
        let legal = walk::can_enter(&self.instance(map).map, nx, ny, dir) && !self.occupied(map, nx, ny);
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
        if let Some(exit) = self.instance(map).map.exit(nx, ny) {
            self.teleport(id, exit.map, exit.dest_x, exit.dest_y);
        }
    }
}

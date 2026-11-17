//! Player actions. Each one checks the same rules as the server first (prediction and
//! courtesy): a legit player never sends a packet the server would count as cheating.
use crate::Game;
use ao_protocol::{Tone, C2S};
use ao_rules::{tile_distance, walk, Dir, WALK_MS};

impl Game {
    /// Walks one tile with client-side prediction; the server confirms or corrects.
    /// Returns whether a step was sent.
    pub fn try_step(&mut self, dir: Dir, now: f64) -> bool {
        let Some(map) = &self.map else { return false };
        let Some(me) = self.ents.get(&self.my_id) else { return false };
        if me.moving() || (now - self.last_step) * 1000.0 < WALK_MS as f64 {
            return false;
        }
        let (dx, dy) = dir.delta();
        let (nx, ny) = (me.x + dx, me.y + dy);
        let free = walk::can_enter(map, nx, ny, dir) && !self.occupied(nx, ny);
        let me = self.ents.get_mut(&self.my_id).unwrap();
        me.dir = dir;
        if free {
            me.step_to(nx, ny);
            self.last_step = now;
            let seq = self.next_seq();
            self.send(&C2S::Walk { dir, seq });
        }
        free
    }

    /// Casts the selected spell at a tile. Returns whether the cast was sent (a local
    /// refusal explains itself in the console).
    pub fn cast_at(&mut self, tx: i32, ty: i32) -> bool {
        let Some(spell) = self.spells.get(self.selected).cloned() else { return false };
        let now = self.now;
        let blocked = self.cd_until.get(&spell.id).is_some_and(|t| now < *t) || self.pending.get(&spell.id).is_some_and(|t| now < *t);
        let refuse = if blocked {
            Some("Debes esperar para volver a lanzar ese hechizo.")
        } else if self.stats.mana.get() < spell.mana {
            Some("No tienes suficiente mana.")
        } else if self.me().is_some_and(|me| tile_distance((me.x, me.y), (tx, ty)) > spell.range) {
            Some("Estas demasiado lejos.")
        } else {
            None
        };
        if let Some(reason) = refuse {
            self.say(reason, Tone::Warn);
            return false;
        }
        let seq = self.next_seq();
        self.pending.insert(spell.id, now + 0.6);
        self.send(&C2S::Cast { spell: spell.id, tx, ty, seq });
        true
    }

    /// Next sequence number for a Walk/Cast (strictly increasing, see the server's SeqGuard).
    pub fn next_seq(&mut self) -> u32 {
        self.seq += 1;
        self.seq
    }

    pub fn send_chat(&mut self, text: String) {
        if !text.trim().is_empty() {
            self.send(&C2S::Say { text });
        }
    }
}

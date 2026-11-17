//! Player actions. Each one checks the same rules as the server first (prediction and
//! courtesy): a legit player never sends a packet the server would count as cheating.
use super::Game;
use ao_protocol::{Tone, C2S};
use ao_rules::{tile_distance, walk, Dir, WALK_MS};
use macroquad::prelude::get_time;

impl Game {
    /// Walks one tile with client-side prediction; the server confirms or corrects.
    pub fn try_step(&mut self, dir: Dir, now: f64) {
        let Some(map) = &self.map else { return };
        let Some(me) = self.ents.get(&self.my_id) else { return };
        if me.moving() || (now - self.last_step) * 1000.0 < WALK_MS as f64 {
            return;
        }
        let (dx, dy) = dir.delta();
        let (nx, ny) = (me.x + dx, me.y + dy);
        let free = walk::can_enter(map, nx, ny, dir) && !self.occupied(nx, ny);
        let me = self.ents.get_mut(&self.my_id).unwrap();
        me.dir = dir;
        if free {
            me.step_to(nx, ny);
            self.seq += 1;
            self.last_step = now;
            let seq = self.seq;
            self.send(&C2S::Walk { dir, seq });
        }
    }

    /// Casts the selected spell at a tile.
    pub fn cast_at(&mut self, tx: i32, ty: i32) {
        let Some(spell) = self.spells.get(self.selected).cloned() else { return };
        let now = get_time();
        let blocked = self.cd_until.get(&spell.id).is_some_and(|t| now < *t) || self.pending.get(&spell.id).is_some_and(|t| now < *t);
        if blocked {
            return self.say("Debes esperar para volver a lanzar ese hechizo.", Tone::Warn);
        }
        if self.stats.mana.get() < spell.mana {
            return self.say("No tienes suficiente mana.", Tone::Warn);
        }
        if let Some(me) = self.me() {
            if tile_distance((me.x, me.y), (tx, ty)) > spell.range {
                return self.say("Estas demasiado lejos.", Tone::Warn);
            }
        }
        self.seq += 1;
        let seq = self.seq;
        self.pending.insert(spell.id, now + 0.6);
        self.send(&C2S::Cast { spell: spell.id, tx, ty, seq });
    }

    pub fn send_chat(&mut self, text: String) {
        if !text.trim().is_empty() {
            self.send(&C2S::Say { text });
        }
    }
}

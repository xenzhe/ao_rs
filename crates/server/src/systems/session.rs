//! Login, logout, map entry/transitions, kicks and cheat strikes.
use crate::security::{RateLimiter, SeqGuard, Strikes, WalkLimiter};
use crate::world::entities::Player;
use crate::world::World;
use ao_protocol::{Tone, PROTOCOL_VERSION, S2C};
use ao_rules::progression::exp_for_level;
use ao_rules::{valid_name, Dir, WALK_MS};
use std::collections::HashMap;
use tracing::{info, warn};

impl World {
    pub(crate) fn on_login(&mut self, id: u32, name: String, version: u16) {
        if self.player(id).is_some() {
            return self.kick(id, "login duplicado");
        }
        let refuse = if version != PROTOCOL_VERSION {
            Some("Version del cliente desactualizada.")
        } else if !valid_name(&name) {
            Some("Nombre invalido (3-16 letras o numeros).")
        } else if self.sessions.values().filter_map(|s| s.player.as_ref()).any(|p| p.name.eq_ignore_ascii_case(&name)) {
            Some("Ese personaje ya esta conectado.")
        } else {
            None
        };
        if let Some(reason) = refuse {
            self.send(id, &S2C::Kicked { reason: reason.into() });
            self.close(id);
            self.sessions.remove(&id);
            return;
        }

        let (map, sx, sy) = self.start;
        if !self.ensure_map(map) {
            self.send(id, &S2C::Kicked { reason: "El servidor no encuentra los mapas de AO.".into() });
            self.close(id);
            self.sessions.remove(&id);
            return;
        }
        let (x, y) = self.free_tile_near(map, sx, sy, 4);
        let now = self.now();
        let b = &self.data.balance;
        let hash = name.bytes().fold(7u32, |a, c| a.wrapping_mul(31).wrapping_add(c as u32));
        let sec = &self.cfg.security;
        let player = Player {
            name: name.clone(),
            map,
            x,
            y,
            dir: Dir::Down,
            body: b.player_bodies[(hash % b.player_bodies.len() as u32) as usize],
            head: b.player_heads[((hash / 7) % b.player_heads.len() as u32) as usize],
            hp: b.start_hp,
            max_hp: b.start_hp,
            mana: b.start_mana,
            max_mana: b.start_mana,
            level: 1,
            exp: 0,
            walk: WalkLimiter::new(WALK_MS, sec.walk_tolerance, now),
            seq: SeqGuard::default(),
            cooldowns: HashMap::new(),
            strikes: Strikes::new(sec.strike_decay_secs * 1000, now),
            chat: RateLimiter::new(4, 3000),
        };
        info!(id, %name, map, x, y, "entra");
        self.send(id, &S2C::Welcome { id });
        self.send(id, &S2C::SpellBook { spells: self.data.spells.clone() });
        self.sessions.get_mut(&id).unwrap().player = Some(player);
        self.enter_map(id, map, x, y);
        self.send_stats(id);
        self.console(id, "Bienvenido a Argentum (prototipo en Rust, mapas de AO20).", Tone::Good);
        self.broadcast_all(&S2C::Console { text: format!("{name} se conecto."), color: Tone::Info }, Some(id));
    }

    pub(crate) fn logout(&mut self, id: u32) {
        let Some(session) = self.sessions.remove(&id) else { return };
        if let Some(p) = session.player {
            info!(id, name = %p.name, "sale");
            if let Some(inst) = self.maps.get_mut(&p.map) {
                inst.players.remove(&id);
            }
            self.broadcast(p.map, &S2C::Despawn { id }, None);
            self.broadcast_all(&S2C::Console { text: format!("{} se desconecto.", p.name), color: Tone::Info }, None);
        }
    }

    pub(crate) fn kick(&mut self, id: u32, reason: &str) {
        warn!(id, reason, "expulsado");
        self.send(id, &S2C::Kicked { reason: reason.to_string() });
        self.close(id);
        self.logout(id);
    }

    /// Records a cheat attempt; enough of them (with decay) ends in a kick.
    pub(crate) fn strike(&mut self, id: u32, what: &str) {
        let limit = self.cfg.security.kick_strikes;
        let now = self.now();
        let p = self.player_mut(id);
        let count = p.strikes.add(now);
        warn!(id, name = %p.name, what, count, limit, "anticheat");
        if count >= limit {
            self.kick(id, &format!("Expulsado por el servidor: {what}"));
        }
    }

    /// Places the player on a map: the client loads it and receives everyone there.
    pub(crate) fn enter_map(&mut self, id: u32, map: u16, x: i32, y: i32) {
        let old = {
            let p = self.player_mut(id);
            let old = p.map;
            p.map = map;
            p.x = x;
            p.y = y;
            old
        };
        if let Some(inst) = self.maps.get_mut(&old) {
            inst.players.remove(&id);
        }
        self.maps.get_mut(&map).unwrap().players.insert(id);

        self.send(id, &S2C::EnterMap { map, x, y });
        let inst = self.instance(map);
        for pid in &inst.players {
            if let Some(o) = self.player(*pid) {
                self.send(id, &o.spawn_msg(*pid));
            }
        }
        for (nid, n) in &inst.npcs {
            if n.alive() {
                self.send(id, &n.spawn_msg(*nid));
            }
        }
        let me = self.player(id).unwrap().spawn_msg(id);
        self.broadcast(map, &me, Some(id));
        let m = &self.instance(map).map;
        self.console(id, format!("Entras a {}{}.", m.name, if m.safe { " (zona segura)" } else { "" }), Tone::Info);
    }

    pub(crate) fn teleport(&mut self, id: u32, map: u16, x: i32, y: i32) {
        if !self.ensure_map(map) {
            return self.console(id, "Ese camino esta cerrado.", Tone::Warn);
        }
        let old = self.player(id).unwrap().map;
        self.broadcast(old, &S2C::Despawn { id }, Some(id));
        let (fx, fy) = self.free_tile_near(map, x, y, 2);
        self.enter_map(id, map, fx, fy);
    }

    pub(crate) fn send_stats(&self, id: u32) {
        if let Some(p) = self.player(id) {
            self.send(id, &S2C::Stats {
                hp: p.hp,
                max_hp: p.max_hp,
                mana: p.mana,
                max_mana: p.max_mana,
                level: p.level,
                exp: p.exp,
                next_exp: exp_for_level(&self.data.balance, p.level),
            });
        }
    }
}

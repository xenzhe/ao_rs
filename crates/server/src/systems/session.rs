//! Login, logout, map entry/transitions, kicks and cheat strikes.
use crate::db::{CharacterRecord, CharacterState};
use crate::world::bank::BankState;
use crate::world::combat_state::CombatState;
use crate::world::inventory::{InvItem, Inventory};
use crate::world::spellbook::{Spellbook, StatusEffects};
use crate::world::stats::CharStats;
use crate::security::{RateLimiter, SeqGuard, Strikes, WalkLimiter};
use crate::world::entities::Player;
use crate::world::World;
use ao_protocol::{Tone, PROTOCOL_VERSION, S2C};
use ao_rules::progression::exp_for_level;
use ao_rules::{valid_name, Dir, WALK_MS};
use std::collections::HashMap;
use tracing::{info, warn};

// ---------------------------------------------------------------- db <-> player pieces

fn stats_from(s: &CharacterState) -> CharStats {
    let mut skills = [0u8; 24];
    for (i, v) in s.skills.iter().take(24).enumerate() {
        skills[i] = *v;
    }
    CharStats { attributes: s.attributes, skills, skill_points: s.skill_points, sta: s.sta, max_sta: s.max_sta, food: s.food, water: s.water, ..CharStats::default() }
}

fn inventory_from(s: &CharacterState) -> Inventory {
    let mut inv = Inventory { gold: s.gold, ..Inventory::default() };
    for &(slot, obj, amount, equipped) in &s.inventory {
        if let Some(cell) = inv.slots.get_mut((slot as usize).wrapping_sub(1)) {
            *cell = Some(InvItem { obj, amount });
            if equipped {
                // Which equipment slot it fills (weapon, armor...) is decided from obj.dat
                // by the inventory system when the player enters the world.
                inv.pending_equipped.push(slot);
            }
        }
    }
    inv
}

fn bank_from(s: &CharacterState) -> BankState {
    let mut bank = BankState { gold: s.bank_gold, ..BankState::default() };
    for &(slot, obj, amount) in &s.bank {
        if let Some(cell) = bank.slots.get_mut((slot as usize).wrapping_sub(1)) {
            *cell = Some(InvItem { obj, amount });
        }
    }
    bank
}

fn spellbook_from(s: &CharacterState) -> Spellbook {
    let mut book = Spellbook::default();
    for &(slot, spell) in &s.spells {
        if let Some(cell) = book.slots.get_mut((slot as usize).wrapping_sub(1)) {
            *cell = spell;
        }
    }
    book
}

/// Everything persistent about a player, for `Db::save_full`.
pub(crate) fn state_of(p: &Player) -> CharacterState {
    let slots = |v: &[Option<InvItem>]| v.iter().enumerate().filter_map(|(i, s)| s.map(|it| ((i + 1) as u8, it))).collect::<Vec<_>>();
    CharacterState {
        // A potion bonus is temporary: the real attributes are saved.
        attributes: p.stats.attr_backup.unwrap_or(p.stats.attributes),
        skills: p.stats.skills.to_vec(),
        skill_points: p.stats.skill_points,
        sta: p.stats.sta,
        max_sta: p.stats.max_sta,
        food: p.stats.food,
        water: p.stats.water,
        gold: p.inv.gold,
        bank_gold: p.bank.gold,
        inventory: slots(&p.inv.slots).into_iter().map(|(s, it)| (s, it.obj, it.amount, p.inv.equipped.is_equipped(s))).collect(),
        bank: slots(&p.bank.slots).into_iter().map(|(s, it)| (s, it.obj, it.amount)).collect(),
        spells: p.spells.slots.iter().enumerate().filter(|(_, s)| **s != 0).map(|(i, s)| ((i + 1) as u8, *s)).collect(),
    }
}

impl World {
    /// Development login without an account (`dev_login` in config): a throwaway character.
    pub(crate) fn on_login(&mut self, id: u32, name: String, version: u16) {
        if self.player(id).is_some() {
            return self.kick(id, "login duplicado");
        }
        if !self.cfg.dev_login {
            return self.kick(id, "entrada sin cuenta deshabilitada");
        }
        let refuse = if version != PROTOCOL_VERSION {
            Some("Version del cliente desactualizada.".to_string())
        } else if !valid_name(&name) {
            Some("Nombre invalido (3-16 letras o numeros).".to_string())
        } else if self.sessions.values().filter_map(|s| s.player.as_ref()).any(|p| p.name.eq_ignore_ascii_case(&name)) {
            Some("Ese personaje ya esta conectado.".to_string())
        } else {
            let ip = self.sessions.get(&id).map(|s| s.addr.ip());
            self.gm_login_refusal(&name, ip)
        };
        if let Some(reason) = refuse {
            self.send(id, &S2C::Kicked { reason });
            self.close(id);
            self.sessions.remove(&id);
            return;
        }

        let (map, x, y) = self.start;
        let b = &self.data.balance;
        let hash = name.bytes().fold(7u32, |a, c| a.wrapping_mul(31).wrapping_add(c as u32));
        let record = CharacterRecord {
            id: 0,
            account_id: 0,
            name: name.clone(),
            race: 1,
            class: 1,
            gender: 1,
            body: b.player_bodies[(hash % b.player_bodies.len() as u32) as usize],
            head: b.player_heads[((hash / 7) % b.player_heads.len() as u32) as usize],
            level: 1,
            exp: 0,
            hp: b.start_hp,
            max_hp: b.start_hp,
            mana: b.start_mana,
            max_mana: b.start_mana,
            map,
            x,
            y,
        };
        if !self.spawn_player(id, &record, &CharacterState::default(), None) {
            self.sessions.remove(&id);
        }
    }

    /// Puts a character in the world (account or dev login). Returns false if its map
    /// could not be loaded (the connection is then closed).
    pub(crate) fn spawn_player(&mut self, id: u32, c: &CharacterRecord, state: &CharacterState, char_id: Option<i64>) -> bool {
        if !self.ensure_map(c.map) {
            self.send(id, &S2C::Kicked { reason: "El servidor no encuentra los mapas de AO.".into() });
            self.close(id);
            return false;
        }
        let (x, y) = self.free_tile_near(c.map, c.x, c.y, 4);
        let now = self.now();
        let sec = &self.cfg.security;
        let player = Player {
            char_id,
            last_refusal: None,
            target_door: None,
            target_floor: None,
            going_home: None,
            speed: 1.0,
            mounted: None,
            rider_level: 1,
            race: c.race,
            class: c.class,
            gender: c.gender,
            name: c.name.clone(),
            map: c.map,
            x,
            y,
            dir: Dir::Down,
            body: c.body,
            head: c.head,
            // hp 0 = dead (ghost): logging out does not resurrect anyone.
            hp: c.hp.clamp(0, c.max_hp),
            max_hp: c.max_hp,
            mana: c.mana.clamp(0, c.max_mana),
            max_mana: c.max_mana,
            level: c.level,
            exp: c.exp,
            walk: WalkLimiter::new(WALK_MS, sec.walk_tolerance, now),
            seq: SeqGuard::default(),
            cooldowns: HashMap::new(),
            strikes: Strikes::new(sec.strike_decay_secs * 1000, now),
            chat: RateLimiter::new(4, 3000),
            stats: stats_from(state),
            inv: inventory_from(state),
            bank: bank_from(state),
            spells: spellbook_from(state),
            status: StatusEffects::default(),
            combat: CombatState { dead: c.hp <= 0, ..CombatState::default() },
            trade: Default::default(),
            guild: Default::default(),
            party: Default::default(),
            faction: Default::default(),
            quest: Default::default(),
            work: Default::default(),
            craft: Default::default(),
            gm: Default::default(),
            event: Default::default(),
            tsao: Default::default(),
        };
        let name = c.name.clone();
        info!(id, %name, map = c.map, x, y, "entra");
        self.send(id, &S2C::Welcome { id });
        self.sessions.get_mut(&id).unwrap().player = Some(player);
        self.enter_map(id, c.map, x, y);
        self.restore_equipment(id);
        self.send_progression(id);
        self.send_look(id);
        self.send_stats(id);
        self.magic_on_enter(id);
        self.social_on_enter(id);
        self.console(id, "Bienvenido a Numic AO.", Tone::Good);
        self.mail_on_enter(id);
        self.tsao_on_enter(id);
        self.broadcast_all(&S2C::Console { text: format!("{name} se conecto."), color: Tone::Info }, Some(id));
        true
    }

    /// Periodic autosave of every online account character (and the pieces each system
    /// keeps outside the character row), so a crash loses at most one interval.
    pub(crate) fn autosave_all(&mut self) {
        let ids = self.player_ids();
        for id in &ids {
            if let Some(p) = self.player(*id) {
                self.save_player(p);
            }
            self.quest_save(*id);
            self.faction_save(*id);
        }
        if !ids.is_empty() {
            tracing::debug!(players = ids.len(), "autoguardado");
        }
    }

    pub(crate) fn logout(&mut self, id: u32) {
        if self.player(id).is_some() {
            self.social_on_leave(id);
            self.pets_on_leave(id);
        }
        let Some(session) = self.sessions.remove(&id) else { return };
        if let Some(p) = session.player {
            info!(id, name = %p.name, "sale");
            self.save_player(&p);
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
        self.close_bank(id);
        self.close_trades(id);
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
        self.pets_on_map_change(id, old);

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
        // Looping FX already playing on the map (meditation), like MakeUserChar sends them.
        for pid in &inst.players {
            if let Some(fx) = self.player(*pid).map(|o| o.status.meditate_fx).filter(|f| *f != 0 && *pid != id) {
                self.send(id, &S2C::Fx { id: *pid, fx, loops: 255 });
            }
        }
        let me = self.player(id).unwrap().spawn_msg(id);
        self.broadcast(map, &me, Some(id));
        self.send_ground_items(id);
        self.send_doors(id, map);
        self.send_speeds(id, map);
        self.faction_map_statuses(id);
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
        let now = self.now();
        self.player_mut(id).walk.reset(now);
    }

    /// Tells everyone on the map how this player looks (equipment, death...).
    pub(crate) fn send_look(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let (body, head, weapon, shield, helmet) = self.look_of(id);
        self.broadcast(p.map, &S2C::Look { id, body, head, weapon, shield, helmet }, None);
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

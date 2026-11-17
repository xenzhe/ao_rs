//! Damage, healing, deaths and experience.
use crate::world::World;
use ao_protocol::{Tone, S2C};
use ao_rules::progression::add_exp;
use std::time::{Duration, Instant};

impl World {
    pub(crate) fn damage_npc(&mut self, map: u16, nid: u32, amount: i32, attacker: u32) {
        let n = self.maps.get_mut(&map).unwrap().npcs.get_mut(&nid).unwrap();
        n.hp -= amount;
        let (name, hp, max_hp) = (n.name.clone(), n.hp, n.max_hp);
        self.console(attacker, format!("Le has causado {amount} de dano a {name}."), Tone::Combat);
        if hp <= 0 {
            self.kill_npc(map, nid, attacker);
        } else {
            self.broadcast(map, &S2C::Hp { id: nid, hp, max_hp }, None);
        }
    }

    fn kill_npc(&mut self, map: u16, nid: u32, killer: u32) {
        let n = self.maps.get_mut(&map).unwrap().npcs.get_mut(&nid).unwrap();
        let def = self.data.creature(n.def).cloned();
        let respawn = def.as_ref().map_or(8, |d| d.respawn_secs);
        let reward = def.as_ref().map_or(0, |d| d.exp);
        n.dead_until = Some(Instant::now() + Duration::from_secs(respawn));
        let name = n.name.clone();
        self.broadcast(map, &S2C::Despawn { id: nid }, None);
        self.console(killer, format!("Has matado a {name}! Ganas {reward} de experiencia."), Tone::Good);
        self.give_exp(killer, reward);
    }

    pub(crate) fn give_exp(&mut self, id: u32, amount: u32) {
        let b = self.data.balance.clone();
        let p = self.player_mut(id);
        let r = add_exp(&b, p.level, p.exp, amount);
        p.level = r.level;
        p.exp = r.exp;
        if r.levels_gained > 0 {
            p.max_hp += b.level_hp * r.levels_gained as i32;
            p.max_mana += b.level_mana * r.levels_gained as i32;
            p.hp = p.max_hp;
            p.mana = p.max_mana;
            let (map, hp, max_hp, level) = (p.map, p.hp, p.max_hp, p.level);
            self.console(id, format!("Has subido al nivel {level}!"), Tone::Good);
            self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
        }
        self.send_stats(id);
    }

    /// Returns the amount actually healed.
    pub(crate) fn heal_player(&mut self, id: u32, amount: i32) -> i32 {
        let p = self.player_mut(id);
        let healed = amount.min(p.max_hp - p.hp).max(0);
        p.hp += healed;
        let (map, hp, max_hp) = (p.map, p.hp, p.max_hp);
        self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
        self.send_stats(id);
        healed
    }

    pub(crate) fn damage_player(&mut self, id: u32, dmg: i32, attacker_name: &str) {
        let p = self.player_mut(id);
        p.hp -= dmg;
        if p.hp > 0 {
            let (map, hp, max_hp) = (p.map, p.hp, p.max_hp);
            self.console(id, format!("{attacker_name} te ha pegado por {dmg}."), Tone::Combat);
            self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
        } else {
            p.hp = p.max_hp;
            p.mana = p.max_mana;
            self.console(id, format!("{attacker_name} te ha matado. Reapareces en la ciudad."), Tone::Warn);
            let (map, x, y) = (self.data.balance.start_map, self.data.balance.start_x, self.data.balance.start_y);
            self.teleport(id, map, x, y);
            let p = self.player(id).unwrap();
            let (map, hp, max_hp) = (p.map, p.hp, p.max_hp);
            self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
        }
        self.send_stats(id);
    }
}

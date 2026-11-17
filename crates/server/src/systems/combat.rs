//! Damage, healing, deaths and experience. OWNER: combat system (P1, Opus A).
//! Ports SistemaCombate.bas (NpcAtacaUser, NpcImpacto, NpcDamage, GetExpForUser) and the
//! death/resurrection parts of Modulo_UsUaRiOs.bas (UserDie, RevivirUsuario,
//! DoDamageOrHeal). Pure formulas live in `ao_rules::combat`.
use crate::world::World;
use ao_formats::dat::balance::{Class, ClassMods, Skill};
use ao_formats::dat::obj::ObjData;
use ao_protocol::{status, Tone, S2C};
use ao_rules::combat as rules;

/// What combat needs from a creature: official npcs.dat values when its definition is
/// found, `data/creatures.toml` otherwise.
#[derive(Clone, Debug)]
pub(crate) struct NpcCombat {
    pub name: String,
    pub attack_power: i64,
    pub evasion_power: i64,
    pub defense: i32,
    pub min_hit: i32,
    pub max_hit: i32,
    pub exp: u32,
    pub attackable: bool,
}

/// A player as the combat formulas see it.
#[derive(Clone)]
pub(crate) struct Fighter {
    pub class: Class,
    pub mods: ClassMods,
    pub level: u32,
    pub skills: [u8; crate::world::stats::NUM_SKILLS],
    pub strength: u8,
    pub agility: u8,
    pub weapon: Option<u16>,
    /// Ammunition slot, obj and amount.
    pub ammo: Option<(u8, u16, u32)>,
    pub armor: Option<u16>,
    pub shield: Option<u16>,
    pub helmet: Option<u16>,
    pub tool: Option<u16>,
    pub meditating: bool,
}

impl Fighter {
    pub fn skill(&self, s: Skill) -> u8 {
        self.skills.get(s.id() as usize - 1).copied().unwrap_or(0)
    }
}

/// Who dealt the damage.
#[derive(Clone, Copy)]
pub(crate) enum Source<'a> {
    Player(u32),
    Npc(u16, u32),
    Named(&'a str),
}

impl World {
    // ------------------------------------------------------------ hooks

    /// Hook (contract): the NPC system decided this creature attacks this player (its
    /// own attack interval already checked). Port of `NpcAtacaUser` + `NpcImpacto` +
    /// `NpcDamage` (SistemaCombate.bas).
    #[allow(dead_code)]
    pub(crate) fn npc_attacks_player(&mut self, map: u16, nid: u32, pid: u32) {
        let Some(npc) = self.npc_combat(map, nid) else { return };
        let Some(p) = self.player(pid) else { return };
        if p.map != map || self.is_dead(pid) {
            return;
        }
        let Some(f) = self.fighter(pid) else { return };
        let ao = self.ao.clone();
        let obj = |o: Option<u16>| o.and_then(|o| ao.objs.get(o as u32));

        // NpcImpacto
        let mut evasion = rules::evasion_power(f.skill(Skill::Tactics), f.agility, f.level, f.mods.evasion, 0);
        let shield = obj(f.shield);
        if let Some(s) = shield {
            evasion += rules::shield_evasion(f.skill(Skill::ShieldDefense), f.mods.shield, s.percentage);
        }
        let hit = self.chance(rules::npc_hit_chance(npc.attack_power, evasion));
        let mut blocked = false;
        if !hit && shield.is_some_and(|s| s.percentage > 0) {
            if let Some(c) = rules::npc_block_chance(f.skill(Skill::ShieldDefense), f.skill(Skill::Tactics)) {
                blocked = self.chance(c);
            }
        }

        // NpcDamage
        let mut damage = 0;
        if hit {
            let mut absorbed = 0;
            if self.roll(1, 6) == rules::BODY_HEAD {
                if let Some(h) = obj(f.helmet) {
                    absorbed += self.roll(h.min_def, h.max_def);
                }
            } else {
                for o in [obj(f.armor), shield].into_iter().flatten() {
                    absorbed += self.roll(o.min_def, o.max_def);
                }
            }
            damage = (self.roll(npc.min_hit, npc.max_hit) - absorbed).max(0);
        }
        self.broadcast(map, &S2C::Attack { attacker: nid, target: pid, hit, damage, critical: false }, None);
        if blocked {
            self.console(pid, "Has rechazado el ataque con el escudo.", Tone::Combat);
        }
        if hit {
            self.hurt_player(pid, damage, Source::Npc(map, nid));
        } else if !blocked {
            self.console(pid, format!("{} te ataco y fallo.", npc.name), Tone::Combat);
        }
    }

    /// Hook (contract): a priest (NPC system) or a spell brings a dead player back with
    /// full health (`RevivirUsuario`; magic can lower hp/mana afterwards for the spell).
    #[allow(dead_code)]
    pub(crate) fn resurrect(&mut self, id: u32) {
        if !self.is_dead(id) {
            return;
        }
        let p = self.player_mut(id);
        p.combat.dead = false;
        p.hp = p.max_hp;
        if let Some((body, head)) = p.combat.alive_look.take() {
            p.body = body;
            p.head = head;
        }
        let (map, hp, max_hp) = (p.map, p.hp, p.max_hp);
        self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
        self.broadcast(map, &S2C::Status { id, flags: 0 }, None);
        self.send_look(id);
        self.send_stats(id);
        self.console(id, "Has sido resucitado.", Tone::Good);
    }

    /// A ghost (or a character at 0 hp) cannot attack, cast, use items or trade.
    pub fn is_dead(&self, id: u32) -> bool {
        self.player(id).is_some_and(|p| p.combat.dead || p.hp <= 0)
    }

    // ------------------------------------------------------------ players

    pub(crate) fn fighter(&self, id: u32) -> Option<Fighter> {
        let p = self.player(id)?;
        let class = Class::from_id(p.class as i32).unwrap_or(Class::Mage);
        let obj = |slot: Option<u8>| slot.and_then(|s| p.inv.get(s)).map(|i| i.obj);
        let ammo = p.inv.equipped.ammo.and_then(|s| p.inv.get(s).map(|i| (s, i.obj, i.amount)));
        Some(Fighter {
            class,
            mods: self.ao.balance.class(class).clone(),
            level: p.level,
            skills: p.stats.skills,
            strength: p.stats.attributes[0],
            agility: p.stats.attributes[1],
            weapon: obj(p.inv.equipped.weapon),
            ammo,
            armor: obj(p.inv.equipped.armor),
            shield: obj(p.inv.equipped.shield),
            helmet: obj(p.inv.equipped.helmet),
            tool: obj(p.inv.equipped.tool),
            meditating: p.status.meditating,
        })
    }

    /// Damage from anything to a player (kept for magic and the creature AI).
    pub(crate) fn damage_player(&mut self, id: u32, dmg: i32, attacker_name: &str) {
        self.hurt_player(id, dmg, Source::Named(attacker_name));
    }

    /// `UserMod.DoDamageOrHeal` with a negative amount. Returns true if it died.
    pub(crate) fn hurt_player(&mut self, id: u32, dmg: i32, source: Source) -> bool {
        if self.player(id).is_none() || self.is_dead(id) {
            return false;
        }
        let name = match source {
            Source::Player(a) => self.player(a).map_or_else(String::new, |p| p.name.clone()),
            Source::Npc(map, nid) => self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).map_or_else(String::new, |n| n.name.clone()),
            Source::Named(n) => n.to_string(),
        };
        let p = self.player_mut(id);
        p.hp = (p.hp - dmg.max(0)).max(0);
        let (map, hp, max_hp) = (p.map, p.hp, p.max_hp);
        self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
        if hp > 0 {
            self.console(id, format!("{name} te ha pegado por {dmg}."), Tone::Combat);
            self.send_stats(id);
            return false;
        }
        match source {
            Source::Player(a) => {
                let victim = self.player(id).map_or_else(String::new, |p| p.name.clone());
                self.console(a, format!("Has matado a {victim}!"), Tone::Good);
                self.console(id, format!("{name} te ha matado!"), Tone::Warn);
            }
            _ => self.console(id, format!("{name} te ha matado!"), Tone::Warn),
        }
        self.user_die(id);
        true
    }

    /// `UserDie` (Modulo_UsUaRiOs.bas): ghost body, no head, 0 hp and stamina. What the
    /// original does with the inventory (TirarTodosLosItems, Desequipar) belongs to the
    /// inventory system.
    pub(crate) fn user_die(&mut self, id: u32) {
        let p = self.player_mut(id);
        p.hp = 0;
        p.stats.sta = 0;
        p.combat.dead = true;
        p.status.meditating = false;
        if p.combat.alive_look.is_none() {
            p.combat.alive_look = Some((p.body, p.head));
        }
        p.body = rules::DEAD_BODY;
        p.head = 0;
        let map = p.map;
        let safe = self.maps.get(&map).is_some_and(|m| m.map.safe);
        self.broadcast(map, &S2C::Status { id, flags: status::DEAD }, None);
        self.send_look(id);
        self.send_vitals(id);
        self.send_stats(id);
        if !safe {
            self.console(id, "Escribe /HOGAR si deseas regresar rapido a tu hogar.", Tone::Info);
        }
    }

    /// Returns the amount actually healed (nothing for ghosts).
    pub(crate) fn heal_player(&mut self, id: u32, amount: i32) -> i32 {
        if self.is_dead(id) {
            return 0;
        }
        let p = self.player_mut(id);
        let healed = amount.min(p.max_hp - p.hp).max(0);
        p.hp += healed;
        let (map, hp, max_hp) = (p.map, p.hp, p.max_hp);
        self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
        self.send_stats(id);
        healed
    }

    pub(crate) fn send_vitals(&self, id: u32) {
        if let Some(p) = self.player(id) {
            self.send(id, &S2C::Vitals { sta: p.stats.sta, max_sta: p.stats.max_sta, food: p.stats.food, water: p.stats.water, gold: p.inv.gold });
        }
    }

    // ------------------------------------------------------------ creatures

    /// Combat values of a creature. Its official definition is the npcs.dat entry with the
    /// same name (creatures.toml has no npcs.dat id yet); without one, creatures.toml
    /// damage and experience with no defense, attack power or evasion.
    pub(crate) fn npc_combat(&self, map: u16, nid: u32) -> Option<NpcCombat> {
        let n = self.maps.get(&map)?.npcs.get(&nid)?;
        if !n.alive() {
            return None;
        }
        if let Some((_, d)) = self.ao.npcs.iter().find(|(_, d)| d.name.eq_ignore_ascii_case(&n.name)) {
            return Some(NpcCombat {
                name: n.name.clone(),
                attack_power: d.attack_power as i64,
                evasion_power: d.evasion_power as i64,
                defense: d.defense,
                min_hit: d.min_hit,
                max_hit: d.max_hit,
                exp: d.exp.max(0) as u32,
                attackable: d.attackable != 0,
            });
        }
        let def = self.data.creature(n.def);
        Some(NpcCombat {
            name: n.name.clone(),
            attack_power: 0,
            evasion_power: 0,
            defense: 0,
            min_hit: def.map_or(1, |d| d.damage[0]),
            max_hit: def.map_or(1, |d| d.damage[1]),
            exp: def.map_or(0, |d| d.exp),
            attackable: true,
        })
    }

    /// Damage to a creature from a player (melee, arrows, spells). Gives experience for
    /// the share of life taken (GetExpForUser) and kills it at 0. Returns true if killed.
    pub(crate) fn damage_npc(&mut self, map: u16, nid: u32, amount: i32, attacker: u32) -> bool {
        let exp_total = self.npc_combat(map, nid).map_or(0, |c| c.exp);
        let Some(n) = self.maps.get_mut(&map).and_then(|m| m.npcs.get_mut(&nid)) else { return false };
        if !n.alive() {
            return false;
        }
        let before = n.hp;
        n.hp = (n.hp - amount.max(0)).max(0);
        let (name, hp, max_hp) = (n.name.clone(), n.hp, n.max_hp);
        self.console(attacker, format!("Le has causado {amount} de dano a {name}."), Tone::Combat);
        let exp = rules::exp_for_damage(exp_total, max_hp, before, hp);
        if exp > 0 && self.player(attacker).is_some() {
            self.give_exp(attacker, exp);
        }
        if hp <= 0 {
            self.kill_npc(map, nid, attacker);
            true
        } else {
            self.broadcast(map, &S2C::Hp { id: nid, hp, max_hp }, None);
            false
        }
    }

    fn kill_npc(&mut self, map: u16, nid: u32, killer: u32) {
        let now = self.now();
        let n = self.maps.get_mut(&map).unwrap().npcs.get_mut(&nid).unwrap();
        let respawn = self.data.creature(n.def).map_or(8, |d| d.respawn_secs);
        n.dead_until = Some(now.after_secs(respawn));
        let name = n.name.clone();
        self.broadcast(map, &S2C::Despawn { id: nid }, None);
        self.console(killer, format!("Has matado a {name}!"), Tone::Good);
        self.drop_npc_loot(map, nid, killer);
    }

    // ------------------------------------------------------------ experience

    /// Adds experience with the official curve (Balance.dat `[EXP]`, `CheckUserLevel`).
    /// The gains of each level are the progression system's (`on_level_up`).
    pub(crate) fn give_exp(&mut self, id: u32, amount: u32) {
        let ao = self.ao.clone();
        let p = self.player_mut(id);
        if rules::exp_to_level_up(&ao.balance, p.level).is_none() {
            return;
        }
        let r = rules::add_exp(&ao.balance, p.level, p.exp, amount);
        p.level = r.level;
        p.exp = r.exp;
        if r.levels_gained > 0 {
            let level = r.level;
            self.console(id, format!("Has subido al nivel {level}!"), Tone::Good);
            self.on_level_up(id, r.levels_gained);
        }
        self.send_stats(id);
    }

    // ------------------------------------------------------------ randomness

    /// `RandomNumber(lo, hi)`, tolerating `hi < lo` (bad data).
    pub(crate) fn roll(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            lo
        } else {
            self.rng.range(lo, hi)
        }
    }

    /// `RandomNumber(1, 100) <= pct`.
    pub(crate) fn chance(&mut self, pct: i64) -> bool {
        self.roll(1, 100) as i64 <= pct
    }

    /// VB `Rnd`: uniform in [0, 1).
    pub(crate) fn rnd(&mut self) -> f64 {
        self.rng.next_u32() as f64 / (u32::MAX as f64 + 1.0)
    }

    /// Official object data by id.
    pub(crate) fn obj(&self, obj: Option<u16>) -> Option<&ObjData> {
        obj.and_then(|o| self.ao.objs.get(o as u32))
    }
}

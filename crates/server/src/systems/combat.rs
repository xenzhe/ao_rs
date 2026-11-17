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
        if let Source::Player(a) = source {
            // UsuarioAtacadoPorUsuario: every hit, not only the killing one.
            self.faction_on_player_attacked(a, id);
        }
        // A hit ends the automated work, like the original.
        self.work_cancel(id);
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
        if let Source::Player(a) = source {
            self.faction_on_player_killed(a, id);
        }
        let killer = if let Source::Player(a) = source { Some(a) } else { None };
        self.event_on_player_killed(killer, id);
        true
    }

    /// `UserDie` (Modulo_UsUaRiOs.bas): ghost body, no head, 0 hp and stamina. What the
    /// original does with the inventory (TirarTodosLosItems, Desequipar) belongs to the
    /// inventory system.
    pub(crate) fn user_die(&mut self, id: u32) {
        // UserDie order: drop/unequip first (armor off = naked body), so the look kept for
        // the resurrection is the naked one, then the ghost.
        self.close_bank(id);
        self.close_trades(id);
        if !self.event_keeps_items_on_death(id) {
            self.drop_inventory_on_death(id);
        }
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

    // ------------------------------------------------------------ creatures

    /// Combat values of a live NPC from its npcs.dat entry (`Npc.def`). `attackable` is the
    /// live flag, which the NPC system owns.
    pub(crate) fn npc_combat(&self, map: u16, nid: u32) -> Option<NpcCombat> {
        let n = self.maps.get(&map)?.npcs.get(&nid)?;
        if !n.alive() {
            return None;
        }
        let d = self.ao.npcs.get(n.def as u32)?;
        Some(NpcCombat {
            name: n.name.clone(),
            attack_power: d.attack_power as i64,
            evasion_power: d.evasion_power as i64,
            defense: d.defense,
            min_hit: d.min_hit,
            max_hit: d.max_hit,
            exp: d.exp.max(0) as u32,
            attackable: n.attackable,
        })
    }

    /// Damage to a creature from a player (melee, arrows, spells). Gives experience for
    /// the share of life taken (GetExpForUser) and kills it at 0. Returns true if killed.
    pub(crate) fn damage_npc(&mut self, map: u16, nid: u32, amount: i32, attacker: u32) -> bool {
        let Some(c) = self.npc_combat(map, nid) else { return false };
        if !c.attackable {
            return false;
        }
        let exp_total = c.exp;
        if self.player(attacker).is_some() {
            let number = self.maps[&map].npcs[&nid].def;
            if let Some(why) = self.faction_npc_attack_refusal(attacker, number) {
                self.console(attacker, why, Tone::Warn);
                return false;
            }
            self.faction_on_npc_attacked(attacker, number);
            self.npc_attacked_by(map, nid, attacker);
        }
        let Some(n) = self.maps.get_mut(&map).and_then(|m| m.npcs.get_mut(&nid)) else { return false };
        let before = n.hp;
        n.hp = (n.hp - amount.max(0)).max(0);
        let (name, hp, max_hp) = (n.name.clone(), n.hp, n.max_hp);
        self.console(attacker, format!("Le has causado {amount} de dano a {name}."), Tone::Combat);
        // CalcularDarExp: share of life taken, then `ExpMult`.
        let exp = rules::exp_for_damage(exp_total, max_hp, before, hp) * self.cfg.world.exp_mult.max(1);
        if exp > 0 && self.player(attacker).is_some() {
            self.share_exp(attacker, exp, map);
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
        // The NPC system takes it off its tile and schedules the respawn (tick_respawns).
        let n = self.maps.get_mut(&map).unwrap().npcs.get_mut(&nid).unwrap();
        n.dead_until = Some(now);
        let (name, number) = (n.name.clone(), n.def);
        self.broadcast(map, &S2C::Despawn { id: nid }, None);
        self.console(killer, format!("Has matado a {name}!"), Tone::Good);
        self.drop_npc_loot(map, nid, killer);
        if self.player(killer).is_some() {
            self.quest_on_npc_killed(killer, number, map);
            self.faction_on_npc_killed(killer, number);
            self.guild_on_npc_killed(killer, number);
        }
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

/// World-level tests for what bots cannot set up yet (equipment, skills, direct hooks).
#[cfg(test)]
mod tests {
    use crate::clock::{Clock, GameTime, VirtualClock};
    use crate::db::{CharacterRecord, CharacterState};
    use crate::world::inventory::InvItem;
    use crate::world::{Session, World};
    use crate::Settings;
    use ao_formats::dat::obj::ObjType;
    use ao_rules::combat::{BOW_INTERVAL_MS, DEAD_BODY};
    use ao_rules::Dir;

    const MAP: u16 = 2;

    fn world() -> (World, VirtualClock) {
        let mut s = Settings::defaults();
        s.data.balance.creatures_per_map = 0;
        let clock = VirtualClock::new();
        clock.set(GameTime(10_000));
        s.clock = Clock::Virtual(clock.clone());
        let mut w = World::new(s);
        assert!(w.ensure_map(MAP));
        (w, clock)
    }

    fn join(w: &mut World, id: u32, class: u8, x: i32, y: i32) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        std::mem::forget(rx);
        w.sessions.insert(id, Session::new(tx, "127.0.0.1:9".parse().unwrap()));
        let r = CharacterRecord {
            id: 0,
            account_id: 0,
            name: format!("P{id}"),
            race: 1,
            class,
            gender: 1,
            body: 1,
            head: 7,
            level: 1,
            exp: 0,
            hp: 50,
            max_hp: 50,
            mana: 0,
            max_mana: 0,
            map: MAP,
            x,
            y,
        };
        assert!(w.spawn_player(id, &r, &CharacterState::default(), None));
        let p = w.player_mut(id);
        p.stats.sta = 1000;
        p.stats.max_sta = 1000;
        // Test fighters: seguro off (citizens log in with it on).
        p.faction.safe = false;
    }

    fn wolf(w: &mut World, nid: u32, x: i32, y: i32, hp: i32) {
        // Lobo (npcs.dat 500), static and harmless so only the test's actions matter.
        let mut npc = w.make_npc(500, x, y).expect("Lobo in npcs.dat");
        npc.hp = hp;
        npc.max_hp = hp;
        npc.hostile = false;
        npc.movement = ao_formats::dat::npcs::Movement::Static;
        w.maps.get_mut(&MAP).unwrap().add_npc(nid, npc);
    }

    fn first_obj(w: &World, f: impl Fn(&ao_formats::dat::obj::ObjData) -> bool) -> u16 {
        w.ao.objs.iter().find(|(_, o)| f(o)).map(|(id, _)| id as u16).expect("official object")
    }

    fn give(w: &mut World, id: u32, slot: u8, obj: u16, amount: u32) {
        w.player_mut(id).inv.slots[slot as usize - 1] = Some(InvItem { obj, amount });
    }

    #[test]
    fn bow_needs_matching_arrows_and_spends_them() {
        let (mut w, clock) = world();
        let (px, py) = w.free_tile_near(MAP, 50, 50, 3);
        join(&mut w, 1, 8, px, py);
        let (px, py) = (w.player(1).unwrap().x, w.player(1).unwrap().y);
        let (wx, wy) = w.free_tile_near(MAP, px + 3, py, 3);
        wolf(&mut w, 900_001, wx, wy, 100_000);
        let bow = first_obj(&w, |o| o.projectile == 1 && o.ammo > 0 && o.max_hit > 0);
        let ammo_kind = w.ao.objs.get(bow as u32).unwrap().ammo;
        let arrow = first_obj(&w, |o| o.obj_type == ObjType::Arrows && o.subtype == ammo_kind);
        give(&mut w, 1, 1, bow, 1);
        w.player_mut(1).inv.equipped.weapon = Some(1);

        // Bow but no arrows: refused before any timer or stamina.
        w.on_shoot(1, wx, wy, 1);
        assert!(w.player(1).unwrap().combat.last_shot.is_none());
        assert_eq!(w.instance(MAP).npcs[&900_001].hp, 100_000);

        give(&mut w, 1, 2, arrow, 2);
        w.player_mut(1).inv.equipped.ammo = Some(2);
        w.on_shoot(1, wx, wy, 2);
        assert_eq!(w.player(1).unwrap().inv.get(2).unwrap().amount, 1, "one arrow per shot");
        // Within IntervaloFlechasCazadores: ignored, nothing spent.
        w.on_shoot(1, wx, wy, 3);
        assert_eq!(w.player(1).unwrap().inv.get(2).unwrap().amount, 1);
        clock.set(GameTime(w.now().0 + BOW_INTERVAL_MS));
        w.on_shoot(1, wx, wy, 4);
        let p = w.player(1).unwrap();
        assert!(p.inv.get(2).is_none() && p.inv.equipped.ammo.is_none(), "the last arrow empties and unequips the slot");
    }

    #[test]
    fn npc_hits_respect_armor_and_kill_into_a_ghost() {
        let (mut w, _clock) = world();
        join(&mut w, 1, 3, 50, 50);
        let (x, y) = (w.player(1).unwrap().x, w.player(1).unwrap().y);
        wolf(&mut w, 900_001, x + 1, y, 60);
        let armor = first_obj(&w, |o| o.obj_type == ObjType::Armor && o.min_def >= 5 && o.max_def >= o.min_def);
        give(&mut w, 1, 1, armor, 1);
        w.player_mut(1).inv.equipped.armor = Some(1);
        let (min_def, max_def) = { let a = w.ao.objs.get(armor as u32).unwrap(); (a.min_def, a.max_def) };
        let npc = w.npc_combat(MAP, 900_001).unwrap();
        assert_eq!((npc.min_hit, npc.max_hit, npc.attack_power), (6, 12, 20), "npcs.dat Lobo");

        let p = w.player_mut(1);
        (p.hp, p.max_hp) = (1_000_000, 1_000_000);
        let (mut hits, mut absorbed) = (0, 0);
        for _ in 0..400 {
            let before = w.player(1).unwrap().hp;
            w.npc_attacks_player(MAP, 900_001, 1);
            let d = before - w.player(1).unwrap().hp;
            assert!((0..=npc.max_hit).contains(&d), "damage {d}");
            if d < npc.min_hit {
                absorbed += 1;
            }
            if d > 0 {
                hits += 1;
            }
        }
        assert!(hits > 0, "some bites land");
        assert!(absorbed > 0, "armor {min_def}..{max_def} absorbs part of 6..12");

        w.player_mut(1).hp = 1;
        for _ in 0..200 {
            if w.is_dead(1) {
                break;
            }
            w.npc_attacks_player(MAP, 900_001, 1);
        }
        // An armor of min_def >= 6 could absorb every bite: then use a naked victim.
        if !w.is_dead(1) {
            w.player_mut(1).inv.equipped.armor = None;
            while !w.is_dead(1) {
                w.npc_attacks_player(MAP, 900_001, 1);
            }
        }
        let p = w.player(1).unwrap();
        assert_eq!((p.hp, p.body, p.head, p.combat.dead), (0, DEAD_BODY, 0, true));
        let hp = p.hp;
        w.npc_attacks_player(MAP, 900_001, 1);
        assert_eq!(w.player(1).unwrap().hp, hp, "ghosts are not attacked");
        assert_eq!(w.heal_player(1, 10), 0, "nor healed");

        w.resurrect(1);
        let p = w.player(1).unwrap();
        // UserDie unequipped the armor before the ghost: back to life naked (official).
        let naked = ao_rules::character::naked_body(p.race, p.gender);
        assert_eq!((p.hp, p.body, p.head, p.combat.dead), (p.max_hp, naked, 7, false));
        assert!(!w.is_dead(1));
        w.resurrect(1);
        assert_eq!(w.player(1).unwrap().body, naked, "resurrecting the living does nothing");
    }

    #[test]
    fn safe_trigger_blocks_pvp() {
        let (mut w, _clock) = world();
        join(&mut w, 1, 3, 50, 50);
        let (x, y) = (w.player(1).unwrap().x, w.player(1).unwrap().y);
        let (bx, by) = (x + 1, y);
        join(&mut w, 2, 3, bx, by);
        let p2 = w.player(2).unwrap();
        assert_eq!((p2.x, p2.y), (bx, by), "second player right of the first");
        w.player_mut(1).dir = Dir::Right;
        for k in 0..20 {
            w.player_mut(1).combat.last_attack = None;
            w.on_melee(1, k + 1);
        }
        let hp = w.player(2).unwrap().hp;
        assert!(hp < 50, "control: on a plain tile the warrior hurts");
        let m = &mut w.maps.get_mut(&MAP).unwrap().map;
        let i = ((by - 1) * 100 + (bx - 1)) as usize;
        m.triggers[i] = ao_rules::combat::TRIGGER_SAFE_ZONE;
        assert_eq!(m.trigger(bx, by), ao_rules::combat::TRIGGER_SAFE_ZONE);
        for k in 20..40 {
            w.player_mut(1).combat.last_attack = None;
            w.on_melee(1, k + 1);
        }
        assert_eq!(w.player(2).unwrap().hp, hp, "nobody fights on a ZonaSegura tile");
    }
}

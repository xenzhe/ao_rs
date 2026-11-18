//! Melee and ranged attacks of players. OWNER: combat system (P1, Opus A).
//! Ports SistemaCombate.bas (UsuarioAtaca, UserAttackPosition, UsuarioAtacaNpc,
//! UserImpactoNpc, UserDamageNpc, UsuarioAtacaUsuario, UsuarioImpacto, UserDamageToUser,
//! PuedeAtacar, ThrowProjectileToTarget, ConsumeAmunition) and the checks of
//! Protocol.bas (HandleAttack, HandleWorkLeftClick `Case e_Skill.Proyectiles`).
use super::combat::{Fighter, Source};
use super::creatures::Vs;
use super::eot::Who;
use crate::world::World;
use ao_formats::dat::balance::{Class, Skill};
use ao_formats::dat::obj::{ObjData, ObjType};
use ao_protocol::{Tone, S2C};
use ao_rules::combat as rules;
use ao_rules::{tile_distance, walk, VIEW_TILES_H, VIEW_TILES_W};

/// What was attacked on a tile.
enum Target {
    Player(u32),
    Npc(u32),
}

impl World {
    /// `HandleAttack` + `UsuarioAtaca`: hits whatever is on the tile in front.
    pub(crate) fn on_melee(&mut self, id: u32, seq: u32) {
        self.reveal(id);
        if !self.player_mut(id).seq.accept(seq) {
            return self.strike(id, "paquete repetido (replay)");
        }
        if self.is_dead(id) {
            return self.console(id, "Estas muerto!!", Tone::Warn);
        }
        let Some(f) = self.fighter(id) else { return };
        if self.obj(f.weapon).is_some_and(|w| w.projectile == 1 && w.ammo > 0) {
            return self.console(id, "No podes usar asi esta arma.", Tone::Warn);
        }
        if f.tool.is_some() {
            return self.console(id, "Para atacar debes desequipar la herramienta.", Tone::Warn);
        }
        // IntervaloPermiteUsarArcos (no update) and IntervaloPermiteAtacar: spam is ignored.
        let now = self.now();
        let c = &self.player(id).unwrap().combat;
        let bow_ok = c.last_shot.is_none_or(|t| now.0 >= t.0 + rules::BOW_INTERVAL_MS);
        let attack_ok = c.last_attack.is_none_or(|t| now.0 >= t.0 + rules::ATTACK_INTERVAL_MS);
        if !bow_ok || !attack_ok {
            return;
        }
        self.player_mut(id).combat.last_attack = Some(now);
        if !self.spend_attack_stamina(id) {
            return;
        }
        // TargetWillAttackPosition: Golpes Rápidos queues its extra blows.
        self.eot_will_attack(id);
        self.attack_front(id);
    }

    /// `UserAttackPosition`: the blow itself on the tile in front (also the extra blows
    /// of `MultipleAttacks`, which skip the interval and the stamina).
    pub(crate) fn attack_front(&mut self, id: u32) {
        if self.is_dead(id) {
            return;
        }
        let p = self.player_mut(id);
        p.status.meditating = false;
        let (dx, dy) = p.dir.delta();
        let (map, tx, ty, dir) = (p.map, p.x + dx, p.y + dy, p.dir);

        // UserAttackPosition
        let m = &self.instance(map).map;
        let blocked = !walk::in_bounds(tx, ty) || m.blocked(tx, ty) & dir.block_bit() != 0;
        let target = if blocked { None } else { self.target_at(map, tx, ty) };
        match target {
            Some(Target::Player(v)) => {
                self.user_attacks_user(id, v, false);
            }
            Some(Target::Npc(nid)) => {
                self.user_attacks_npc(id, map, nid, false);
            }
            None => self.broadcast(map, &S2C::Attack { attacker: id, target: 0, hit: false, damage: 0, critical: false }, None),
        }
    }

    /// `HandleWorkLeftClick` with projectiles: shoots the tile (tx, ty).
    pub(crate) fn on_shoot(&mut self, id: u32, tx: i32, ty: i32, seq: u32) {
        if !self.player_mut(id).seq.accept(seq) {
            return self.strike(id, "paquete repetido (replay)");
        }
        let (map, px, py) = {
            let p = self.player(id).unwrap();
            (p.map, p.x, p.y)
        };
        if !walk::in_bounds(tx, ty) || (tx - px).abs() > VIEW_TILES_W / 2 || (ty - py).abs() > VIEW_TILES_H / 2 {
            return self.strike(id, "disparo fuera de la vista");
        }
        if self.is_dead(id) {
            return self.console(id, "Estas muerto!!", Tone::Warn);
        }
        let now = self.now();
        let c = &self.player(id).unwrap().combat;
        if c.last_shot.is_some_and(|t| now.0 < t.0 + rules::BOW_INTERVAL_MS) {
            return;
        }
        let Some(f) = self.fighter(id) else { return };
        let Some(weapon) = self.obj(f.weapon).filter(|w| w.projectile == 1).cloned() else {
            return self.console(id, "No tenes un arma de proyectiles equipada.", Tone::Warn);
        };
        if weapon.ammo > 0 && !self.has_ammo_for(&f, &weapon) {
            return self.console(id, "No tenes municiones.", Tone::Warn);
        }
        let c = &mut self.player_mut(id).combat;
        c.last_shot = Some(now);
        c.last_attack = Some(now);
        if !self.spend_attack_stamina(id) {
            return;
        }
        if (tx, ty) == (px, py) {
            return self.console(id, "No podes atacarte a vos mismo!", Tone::Warn);
        }
        let attacked = match self.target_at(map, tx, ty) {
            Some(Target::Player(v)) => self.user_attacks_user(id, v, true),
            Some(Target::Npc(nid)) => self.user_attacks_npc(id, map, nid, true),
            None => false,
        };
        if attacked && weapon.ammo > 0 {
            self.consume_ammo(id);
        }
    }

    fn target_at(&self, map: u16, x: i32, y: i32) -> Option<Target> {
        if let Some(v) = self.player_at(map, x, y) {
            return Some(Target::Player(v));
        }
        self.instance(map).npc_at(x, y).map(Target::Npc)
    }

    /// Ammunition equipped, of the weapon's kind and not empty (Protocol.bas checks).
    fn has_ammo_for(&self, f: &Fighter, weapon: &ObjData) -> bool {
        let Some((_, obj, amount)) = f.ammo else { return false };
        self.obj(Some(obj)).is_some_and(|a| a.obj_type == ObjType::Arrows && a.subtype == weapon.ammo) && amount >= 1
    }

    /// Stamina for a swing (UsuarioAtaca: needs 10, costs 1..10).
    fn spend_attack_stamina(&mut self, id: u32) -> bool {
        if self.player(id).unwrap().stats.sta < rules::MIN_ATTACK_STAMINA {
            self.console(id, "Estas muy cansado para luchar.", Tone::Warn);
            return false;
        }
        let (lo, hi) = rules::ATTACK_STAMINA_COST;
        let cost = self.roll(lo, hi);
        let p = self.player_mut(id);
        p.stats.sta = (p.stats.sta - cost).max(0);
        self.send_vitals(id);
        true
    }

    /// `ConsumeAmunition`: one less in the equipped ammunition slot, unequipped when empty.
    /// Touches the inventory piece directly; the inventory system can take it over.
    fn consume_ammo(&mut self, id: u32) {
        let p = self.player_mut(id);
        let Some(slot) = p.inv.equipped.ammo else { return };
        let i = slot as usize - 1;
        let Some(item) = p.inv.slots.get_mut(i).and_then(|s| s.as_mut()) else { return };
        item.amount = item.amount.saturating_sub(1);
        let (obj, amount) = (item.obj, item.amount);
        if amount == 0 {
            p.inv.slots[i] = None;
            p.inv.equipped.ammo = None;
        }
        let msg = if amount == 0 { S2C::InventorySlot { slot, obj: 0, amount: 0, equipped: false } } else { S2C::InventorySlot { slot, obj, amount, equipped: true } };
        self.send(id, &msg);
    }

    /// Weapon plus ammunition damage roll and its maximum (GetUserDamageWithItem).
    fn weapon_roll(&mut self, f: &Fighter, vs: Vs) -> (i32, i32) {
        let vs_npc = vs != Vs::User;
        let Some(w) = self.obj(f.weapon).cloned() else { return (0, 0) };
        // TSAO CalcularDaño: the dragon slayer's fixed rolls (systems/creatures.rs).
        if let Some(r) = self.special_weapon_roll(f.weapon, vs) {
            return r;
        }
        let (lo, hi) = rules::hit_range(&w, vs_npc);
        let (mut roll, mut max) = (self.roll(lo, hi), hi);
        if w.projectile > 0 && w.ammo > 0 {
            if let Some(a) = self.obj(f.ammo.map(|a| a.1)).cloned().filter(|a| a.obj_type == ObjType::Arrows) {
                let (lo, hi) = rules::hit_range(&a, vs_npc);
                roll += self.roll(lo, hi);
                max += hi;
            }
        }
        (roll, max)
    }

    /// `GetUserDamage`.
    fn base_damage(&mut self, f: &Fighter, vs: Vs) -> i64 {
        let (lo, hi) = rules::user_hit_range(&f.mods, f.level);
        if f.weapon.is_none() && self.tsao_rules() {
            // TSAO CalcularDaño with bare hands: CInt(MaxHIT / 5).
            return rules::vb_round(hi as f64 / 5.0);
        }
        let (weapon_roll, max_weapon) = self.weapon_roll(f, vs);
        let user_roll = self.roll(lo, hi);
        let modifier = rules::class_damage_modifier(&f.mods, self.obj(f.weapon));
        rules::user_damage(weapon_roll, max_weapon, f.strength, user_roll, modifier)
    }

    /// `AttackPower` with the skill the weapon uses (UserImpactoNpc / UsuarioImpacto).
    fn attack_power_of(&self, f: &Fighter) -> i64 {
        let w = self.obj(f.weapon);
        let skill = rules::weapon_skill(w);
        if self.tsao_rules() {
            return ao_rules::tsao_combat::attack_power(f.skill(skill), f.agility, f.level, rules::attack_skill_modifier(&f.mods, skill));
        }
        rules::attack_power(f.skill(skill), f.agility, f.level, rules::attack_skill_modifier(&f.mods, skill), rules::weapon_hit_bonus(&f.mods, w))
    }

    /// `GetArmorPenetration` (armor_penetration_feature is on): flat roll plus a share of
    /// the target's armor.
    fn armor_penetration(&mut self, f: &Fighter, target_armor: i32) -> i32 {
        let Some(w) = self.obj(f.weapon).cloned() else { return 0 };
        if !rules::FEATURE_ARMOR_PENETRATION {
            return 0;
        }
        let chance = rules::clamp_chance(f.skill(Skill::Weapons) as f64 * self.ao.balance.backstab.ignore_armor_chance as f64);
        if self.roll(1, 100) > chance {
            return 0;
        }
        let (a, b) = (w.min_armor_penetration_flat.max(0), w.max_armor_penetration_flat.max(0));
        let flat = self.roll(a.min(b), a.max(b));
        flat + rules::armor_penetration_share(w.armor_penetration_percent, target_armor)
    }

    fn weapon_extra_chance(&self, f: &Fighter) -> i32 {
        self.obj(f.weapon).map_or(0, |w| w.extra_crit_and_stab_chance)
    }

    /// `GetStabbingChanceBase`.
    fn stab_chance(&self, f: &Fighter) -> i32 {
        let b = &self.ao.balance.backstab;
        let per = match f.class {
            Class::Assassin => b.assassin_stabbing_chance,
            Class::Bard => b.bard_stabbing_chance,
            Class::Hunter => b.hunter_stabbing_chance,
            _ => b.generic_stabbing_chance,
        };
        rules::stab_chance(f.skill(Skill::Stab), per, self.weapon_extra_chance(f))
    }

    fn critical_chance(&self, f: &Fighter) -> i32 {
        rules::critical_chance(f.skill(Skill::Wrestling), self.ao.balance.backstab.bandit_critical_hit_chance, self.weapon_extra_chance(f))
    }

    // ------------------------------------------------------------ against creatures

    /// `UsuarioAtacaNpc` + `UserImpactoNpc` + `UserDamageNpc`. Returns true if it attacked.
    /// `SubirSkillDeArmaActual`: projectiles, knuckles/no weapon (wrestling) or weapons.
    fn raise_weapon_skill(&mut self, id: u32) {
        let weapon = self.player(id).and_then(|p| p.inv.equipped.weapon.and_then(|s| p.inv.get(s))).and_then(|it| self.ao.objs.get(it.obj as u32));
        let skill = match weapon {
            Some(w) if w.projectile > 0 => Skill::Projectiles,
            Some(w) if w.weapon_type == Some(ao_formats::dat::balance::WeaponType::Knuckle) => Skill::Wrestling,
            Some(_) => Skill::Weapons,
            None => Skill::Wrestling,
        };
        self.raise_skill(id, skill);
    }

    fn user_attacks_npc(&mut self, id: u32, map: u16, nid: u32, _ranged: bool) -> bool {
        let Some(npc) = self.npc_combat(map, nid) else { return false };
        if !npc.attackable {
            self.console(id, "No podes atacar a este NPC.", Tone::Warn);
            return false;
        }
        if self.npc_owner_refuses(id, map, nid) {
            return false;
        }
        let Some(f) = self.fighter(id) else { return false };
        // EOT modifiers (GetHitBonus, GetEvasionBonus, GetDefenseBonus, GetLinearDamageBonus,
        // GetPhysicalDamageModifier, GetPhysicDamageReduction).
        let (um, nm) = (self.modifiers(Who::User(id)), self.modifiers(Who::Npc(map, nid)));
        let attack = self.attack_power_of(&f) + um.hit as i64;
        let evasion = npc.evasion_power + nm.evasion as i64;
        let chance = if self.tsao_rules() { ao_rules::tsao_combat::hit_chance(attack, evasion) } else { rules::user_hit_chance(attack, evasion, 0) };
        let hit = self.chance(chance);
        if !hit {
            self.broadcast(map, &S2C::Attack { attacker: id, target: nid, hit: false, damage: 0, critical: false }, None);
            return true;
        }
        self.raise_weapon_skill(id);
        let vs = self.melee_target_kind(map, nid);
        let base = self.base_damage(&f, vs) + um.linear_bonus(true) as i64;
        let npc_def = npc.defense + nm.defense;
        let def = (npc_def - self.armor_penetration(&f, npc_def)).max(0) as i64;
        let mut damage = ao_rules::eot::scale(ao_rules::eot::scale(base - def, um.physical_factor(true)), nm.physical_taken()).max(0);
        // TSAO UserDañoNpc: pets x1.5, paladin bonus, 2-in-5 critical x2 (profile keys).
        let mut tsao_crit = false;
        if self.tsao_rules() {
            (damage, tsao_crit) = self.tsao_melee_vs_npc(id, map, nid, damage);
        }
        // healers_and_tanks: warriors heal a share of what they deal.
        if rules::FEATURE_HEALERS_AND_TANKS && f.class == Class::Warrior && damage > 0 {
            let max_hp = self.player(id).unwrap().max_hp as i64;
            let heal = rules::vb_round(damage as f64 * self.ao.balance.extra.warrior_life_steal_on_hit_multiplier as f64).min(max_hp);
            self.heal_player(id, heal as i32);
        }
        let w = self.obj(f.weapon).cloned();
        let mut extra = 0;
        if self.tsao_rules() {
            // TSAO DoApuñalar on a creature: x2 of the hit (creatures.rs).
            extra = self.tsao_stab(&f, damage, None);
        } else if rules::can_critical(f.class == Class::Bandit, w.as_ref()) {
            if self.roll(1, 100) <= self.critical_chance(&f) {
                extra = rules::vb_round(base as f64 * rules::NPC_CRITICAL_FACTOR);
            }
        } else if rules::can_stab(f.class == Class::Assassin, f.skill(Skill::Stab), w.as_ref()) && self.roll(1, 100) <= self.stab_chance(&f) {
            let (lo, hi) = (f.mods.stab_npc_min, f.mods.stab_npc_max);
            let r = self.rnd();
            extra = rules::vb_round(damage as f64 * (r * (hi - lo) + lo));
            self.raise_skill(id, Skill::Stab);
        }
        let total = (damage + extra.max(0)) as i32;
        self.broadcast(map, &S2C::Attack { attacker: id, target: nid, hit: true, damage: total, critical: extra > 0 || tsao_crit }, None);
        self.damage_npc(map, nid, total, id);
        // TargetDidHit: on-hit effects (Filo de Toxina, Desgarro...).
        self.eot_on_hit(id, Who::Npc(map, nid));
        true
    }

    // ------------------------------------------------------------ against players

    /// `PuedeAtacar` without factions, guilds, groups or GMs (not ported yet).
    fn can_attack_user(&self, a: u32, v: u32) -> bool {
        if self.is_dead(a) {
            self.console(a, "Estas muerto!!", Tone::Warn);
            return false;
        }
        if self.is_dead(v) {
            self.console(a, "No podes atacar a un espiritu.", Tone::Warn);
            return false;
        }
        let (pa, pv) = (self.player(a).unwrap(), self.player(v).unwrap());
        let m = &self.instance(pv.map).map;
        let (ta, tv) = (m.trigger(pa.x, pa.y), m.trigger(pv.x, pv.y));
        // TriggerZonaPelea / PeleaSegura
        if ta == rules::TRIGGER_FIGHT_ZONE || tv == rules::TRIGGER_FIGHT_ZONE {
            return ta == tv;
        }
        if m.safe {
            self.console(a, "Esta es una zona segura, aqui no podes atacar otros usuarios.", Tone::Warn);
            return false;
        }
        if ta == rules::TRIGGER_SAFE_ZONE || tv == rules::TRIGGER_SAFE_ZONE {
            self.console(a, "No podes pelear aqui.", Tone::Warn);
            return false;
        }
        if let Some(why) = self.faction_attack_refusal(a, v) {
            self.console(a, why, Tone::Warn);
            return false;
        }
        true
    }

    /// `UsuarioAtacaUsuario` + `UsuarioImpacto` + `UserDamageToUser`. True if it attacked.
    fn user_attacks_user(&mut self, a: u32, v: u32, ranged: bool) -> bool {
        if !self.can_attack_user(a, v) {
            return false;
        }
        let (pa, pv) = (self.player(a).unwrap(), self.player(v).unwrap());
        if tile_distance((pa.x, pa.y), (pv.x, pv.y)) > rules::MAX_BOW_DISTANCE {
            self.console(a, "Estas demasiado lejos.", Tone::Warn);
            return false;
        }
        let (map, attacker_name, attacker_dir, victim_dir, near) = (pv.map, pa.name.clone(), pa.dir, pv.dir, tile_distance((pa.x, pa.y), (pv.x, pv.y)) <= 1);
        let (Some(fa), Some(fv)) = (self.fighter(a), self.fighter(v)) else { return false };

        // UsuarioImpacto
        let (am, vm) = (self.modifiers(Who::User(a)), self.modifiers(Who::User(v)));
        let attack = self.attack_power_of(&fa) + am.hit as i64;
        let mut evasion = rules::evasion_power(fv.skill(Skill::Tactics), fv.agility, fv.level, fv.mods.evasion, 0) + vm.evasion as i64;
        let mut block = 0;
        if let Some(s) = self.obj(fv.shield).cloned() {
            if s.percentage > 0 {
                evasion += rules::shield_evasion(fv.skill(Skill::ShieldDefense), fv.mods.shield, s.percentage);
            }
            block = rules::user_block_chance(s.percentage, fv.skill(Skill::ShieldDefense), fv.skill(Skill::Tactics));
        }
        let weapon_mod = match self.obj(fa.weapon) {
            Some(w) if rules::FEATURE_IMPROVED_HIT_CHANCE => {
                if ranged {
                    w.improved_ranged_hit_chance
                } else {
                    w.improved_melee_hit_chance
                }
            }
            _ => 0,
        };
        let mut chance = if self.tsao_rules() { ao_rules::tsao_combat::hit_chance(attack, self.tsao_evasion(&fv)) } else { rules::user_hit_chance(attack, evasion, weapon_mod) };
        if fv.meditating {
            chance = rules::meditating_hit_chance(chance);
        }
        if !self.chance(chance) {
            self.broadcast(map, &S2C::Attack { attacker: a, target: v, hit: false, damage: 0, critical: false }, None);
            if self.chance(block) {
                self.console(a, "El usuario rechazo el ataque con su escudo.", Tone::Combat);
                self.console(v, "Has rechazado el ataque con el escudo.", Tone::Combat);
                self.raise_skill(v, Skill::ShieldDefense);
            } else {
                self.console(v, format!("{attacker_name} te ataco y fallo!"), Tone::Combat);
                self.console(a, "Has fallado el golpe!", Tone::Combat);
            }
            return true;
        }

        self.raise_weapon_skill(a);
        // UserDamageToUser
        let base = self.base_damage(&fa, Vs::User) + am.linear_bonus(false) as i64;
        // Body part 1..8; past the torso it is re-rolled among legs/arms/torso (only the
        // console text of the original depends on which one).
        let place = self.roll(1, 8);
        let mut def = 0;
        let obj = |w: &World, o: Option<u16>| w.obj(o).cloned();
        if place == rules::BODY_HEAD {
            if let Some(h) = obj(self, fv.helmet) {
                def += self.roll(h.min_def, h.max_def);
            }
        } else {
            if place > rules::BODY_TORSO {
                self.roll(rules::BODY_LEFT_LEG, rules::BODY_TORSO);
            }
            for o in [obj(self, fv.armor), obj(self, fv.shield)].into_iter().flatten() {
                def += self.roll(o.min_def, o.max_def);
            }
        }
        let def = def + vm.defense;
        let def = (def - self.armor_penetration(&fa, def)).max(0) as i64;
        let mut damage = ao_rules::eot::scale(ao_rules::eot::scale(base - def, am.physical_factor(false)), vm.physical_taken()).max(0);
        if self.tsao_rules() {
            // TSAO "BALANCEO": class percents and the flat class-versus-class bonus.
            let ranged = self.obj(fa.weapon).is_some_and(|w| w.projectile > 0);
            damage = self.pvp_balance().physical(damage, fa.class.spanish_name(), fv.class.spanish_name(), ranged).max(1);
        }
        let w = self.obj(fa.weapon).cloned();
        // GetBackHitBonusChanceAgainstUsers: same heading and adjacent.
        let back = if attacker_dir == victim_dir && near { self.ao.balance.backstab.extra_backstab_chance as f64 } else { 0.0 };
        let mut bonus = 0;
        if self.tsao_rules() {
            // TSAO DoApuñalar on a player: x1.5 of the hit; assassins from behind always.
            bonus = self.tsao_stab(&fa, damage, Some(attacker_dir == victim_dir));
            // The gauntlet (`Inmoviliza`): paralyses, always on a blow to the head.
            self.tsao_gauntlet(a, v, place == rules::BODY_HEAD);
        } else if rules::can_critical(fa.class == Class::Bandit, w.as_ref()) {
            let c = rules::clamp_chance(self.critical_chance(&fa) as f64 + back);
            if self.roll(1, 100) <= c {
                bonus = rules::vb_round(damage as f64 * self.ao.balance.backstab.critical_hit_dmg_modifier as f64);
            }
        } else if rules::can_stab(fa.class == Class::Assassin, fa.skill(Skill::Stab), w.as_ref()) {
            let c = rules::clamp_chance(self.stab_chance(&fa) as f64 + back);
            if self.roll(1, 100) <= c {
                bonus = rules::vb_round(damage as f64 * fa.mods.stab);
                self.raise_skill(a, Skill::Stab);
            }
        }
        let mut total = damage;
        if bonus > 0 {
            total += bonus;
            // A full-health victim dies only from a bonus hit worth its whole life.
            let pv = self.player(v).unwrap();
            if pv.hp == pv.max_hp && total >= pv.max_hp as i64 {
                total = pv.hp as i64;
            }
        }
        let total = total as i32;
        self.broadcast(map, &S2C::Attack { attacker: a, target: v, hit: true, damage: total, critical: bonus > 0 }, None);
        let victim = self.player(v).unwrap().name.clone();
        self.console(a, format!("Le has pegado a {victim} por {total}."), Tone::Combat);
        self.hurt_player(v, total, Source::Player(a));
        true
    }
}

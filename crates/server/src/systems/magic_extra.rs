//! The rest of `modHechizos.bas` and the special shots of `SistemaCombate.bas`:
//! `HandlePhysicalSkill` (Flecha Cargada, the creatures' cannon), `HandleHechizoTerreno`
//! (`HechizoMaterializacion`, `HechizoTerrenoEstado`, `MultiShot` with
//! `ThrowArrowToTargetDir` / `ThrowArrowToTile`), `HandlePetSpell`, and the stun of
//! `UserDañoNpc` / `UserDañoEspecial` (`StunNPc`, `StunPlayer`).
//!
//! Notes on the VB:
//! - `Hechizos.dat` has a `Tipo=12` (Detectar Invisibilidad) and a `Tipo=13` (Orbe de
//!   Luz) that `e_TipoHechizo` does not know: `HandleHechizoTerreno` does nothing with them
//!   and costs nothing, and so does this server.
//! - `Mimetiza` is read from `Hechizos.dat` but no AO20 routine uses it any more (only the
//!   leftovers of `flags.Mimetizado`): Mimetizar has no effect in AO20 and none here.
//! - `HandlePhysicalSkill` never consumes the arrow of Flecha Cargada; neither does this.
use super::combat::Source;
use super::creatures::Vs;
use super::eot::Who;
use crate::world::inventory::InvItem;
use crate::world::World;
use ao_formats::csm::BLOCK_ALL;
use ao_formats::dat::obj::ObjType;
use ao_formats::dat::spells::{Spell, SpellEffects as Fx, SpellType};
use ao_protocol::{Tone, S2C};

/// `e_SkillType`.
const SKILL_PUSHING_ARROW: i32 = 1;
const SKILL_CANNON: i32 = 2;
/// `MultiShot`: arrows needed, how far each one flies and the fan of angles.
const MULTISHOT_ARROWS: u32 = 5;
const MULTISHOT_DISTANCE: i32 = 10;
const MULTISHOT_ANGLES: [f64; 5] = [0.0, -15.0, -30.0, 15.0, 30.0];
/// `HechizoTerrenoEstado`: the square that loses its invisibility.
const REVEAL_RADIUS: i32 = 11;
/// FX of a stun (`PrepareMessageCreateFX(.., 142, ..)`).
const STUN_FX: u16 = 142;
/// `Subtipo` of stun ammunition.
const STUN_AMMO_SUBTYPE: i32 = 2;

impl World {
    // ------------------------------------------------------------ physical skills

    /// `HandlePhysicalSkill` for a player: the pushing arrow does half a ranged hit
    /// (weapon plus arrow) without a hit roll; the effect (EOT11, the push) follows in
    /// `cast_on_user` / `cast_on_npc` when the target survives.
    pub(crate) fn physical_skill(&mut self, id: u32, s: &Spell, target: Who) -> bool {
        match s.skill_type {
            SKILL_PUSHING_ARROW => {
                let Some(f) = self.fighter(id) else { return false };
                if f.ammo.is_none() {
                    self.console(id, "No tenés municiones.", Tone::Warn);
                    return false;
                }
                let vs = match target {
                    Who::User(_) => Vs::User,
                    Who::Npc(m, n) => self.melee_target_kind(m, n),
                };
                let dmg = (self.base_damage(&f, vs) / 2).max(0);
                let dmg = ao_rules::eot::scale(ao_rules::eot::scale(dmg, self.attack_mods(Who::User(id), target).physical_factor(vs != Vs::User)), self.modifiers(target).physical_taken()) as i32;
                match target {
                    Who::User(t) => {
                        self.broadcast(self.player(t).map_or(0, |p| p.map), &S2C::Attack { attacker: id, target: t, hit: true, damage: dmg, critical: false }, None);
                        self.hurt_player(t, dmg, Source::Player(id));
                    }
                    Who::Npc(m, n) => {
                        self.broadcast(m, &S2C::Attack { attacker: id, target: n, hit: true, damage: dmg, critical: false }, None);
                        self.npc_attacked_by(m, n, id);
                        self.damage_npc(m, n, dmg, id);
                    }
                }
                true
            }
            SKILL_CANNON => {
                // Debug.Assert "User cannot use this spell".
                self.console(id, "Ese hechizo es sólo para criaturas.", Tone::Warn);
                false
            }
            _ => {
                self.console(id, "Ese hechizo todavía no está disponible.", Tone::Warn);
                false
            }
        }
    }

    /// `HandlePhysicalSkill` from a creature on player `pid`.
    pub(crate) fn npc_physical_skill(&mut self, map: u16, nid: u32, pid: u32, s: &Spell) {
        let Some(p) = self.player(pid) else { return };
        let (px, py) = (p.x, p.y);
        let eot = s.eot_id.clamp(0, u16::MAX as i32) as u16;
        match s.skill_type {
            SKILL_PUSHING_ARROW => {
                let def = self.instance(map).npcs[&nid].def;
                let (lo, hi) = self.ao.npcs.get(def as u32).map_or((0, 0), |d| (d.min_hit, d.max_hit));
                let dmg = self.rng.range(lo.min(hi), lo.max(hi));
                let dmg = ao_rules::eot::scale(ao_rules::eot::scale(dmg as i64, self.modifiers(Who::Npc(map, nid)).physical_factor(false)), self.modifiers(Who::User(pid)).physical_taken()) as i32;
                if !self.hurt_player(pid, dmg, Source::Npc(map, nid)) && eot > 0 {
                    self.apply_eot(Who::Npc(map, nid), Who::User(pid), eot, false);
                }
            }
            SKILL_CANNON => {
                if s.wav > 0 {
                    let n = &self.instance(map).npcs[&nid];
                    let (x, y) = (n.x, n.y);
                    self.broadcast(map, &S2C::Sound { wav: s.wav as u16, x, y }, None);
                }
                self.create_delayed_blast(Who::Npc(map, nid), map, px, py, eot, 0);
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ terrain

    /// `HandleHechizoTerreno` for the types that are not summons or areas.
    pub(crate) fn cast_on_terrain(&mut self, id: u32, s: &Spell, tx: i32, ty: i32) -> bool {
        match s.spell_type {
            SpellType::Materialize => self.materialize(id, s, tx, ty),
            SpellType::MultiShoot => self.multishot(id, tx, ty),
            SpellType::Status if s.effects.contains(Fx::REMOVE_INVISIBILITY) => {
                self.terrain_reveal(id, tx, ty);
                true
            }
            // Tipo 12 / 13 and the rest: HandleHechizoTerreno has no case, nothing happens.
            _ => false,
        }
    }

    /// `HechizoMaterializacion`: `MaterializaCant` of `MaterializaObj` on a free tile.
    fn materialize(&mut self, id: u32, s: &Spell, tx: i32, ty: i32) -> bool {
        let map = self.player(id).unwrap().map;
        let inst = self.instance(map);
        if !ao_rules::walk::in_bounds(tx, ty) || inst.ground.at(tx, ty).is_some() || inst.map.blocked(tx, ty) & BLOCK_ALL != 0 {
            self.console(id, "No hay espacio en el suelo.", Tone::Warn);
            return false;
        }
        if s.materialize_obj <= 0 || s.materialize_amount <= 0 || self.ao.objs.get(s.materialize_obj as u32).is_none() {
            return false;
        }
        self.make_obj(map, tx, ty, InvItem { obj: s.materialize_obj as u16, amount: s.materialize_amount as u32 });
        true
    }

    /// `HechizoTerrenoEstado` with RemoveInvisibility: the invisible within 11 tiles of the
    /// tile become visible.
    fn terrain_reveal(&mut self, id: u32, tx: i32, ty: i32) {
        let map = self.player(id).unwrap().map;
        let now = self.now();
        let found: Vec<u32> = self
            .instance(map)
            .players
            .iter()
            .copied()
            .filter(|p| self.player(*p).is_some_and(|p| (p.x - tx).abs() <= REVEAL_RADIUS && (p.y - ty).abs() <= REVEAL_RADIUS && p.status.invisible_until.is_some_and(|u| now < u)))
            .collect();
        for p in found {
            self.player_mut(p).status.invisible_until = None;
            self.refresh_status(p);
            self.console(p, "Tu invisibilidad ya no tiene efecto.", Tone::Combat);
        }
    }

    /// `MultiShot`: five arrows in a fan (0, -15, -30, +15, +30 degrees), each one with
    /// `-MultiShotReduction` damage, flying up to 10 tiles to the first player or
    /// creature that may be attacked. Needs 5 arrows of the bow's kind.
    fn multishot(&mut self, id: u32, tx: i32, ty: i32) -> bool {
        let Some(f) = self.fighter(id) else { return false };
        let weapon = self.ao.objs.get(f.weapon.unwrap_or(0) as u32).cloned();
        let ok = match (&weapon, f.ammo) {
            (Some(w), Some((_, obj, _))) => self.ao.objs.get(obj as u32).is_some_and(|a| a.obj_type == ObjType::Arrows && a.subtype == w.ammo),
            _ => false,
        };
        if !ok {
            self.console(id, "Necesitás flechas equipadas para ese arco.", Tone::Combat);
            return false;
        }
        if f.ammo.is_some_and(|(_, _, n)| n < MULTISHOT_ARROWS) {
            self.console(id, "No tenés suficientes municiones.", Tone::Combat);
            return false;
        }
        let p = self.player(id).unwrap();
        let (map, px, py) = (p.map, p.x, p.y);
        let (dx, dy) = ((tx - px) as f64, (ty - py) as f64);
        if dx == 0.0 && dy == 0.0 {
            self.console(id, "No podés atacarte a vos mismo.", Tone::Combat);
            return false;
        }
        let len = (dx * dx + dy * dy).sqrt();
        let dir = (dx / len, dy / len);
        self.player_mut(id).status.multishot = true;
        for a in MULTISHOT_ANGLES {
            let r = a.to_radians();
            let d = (dir.0 * r.cos() - dir.1 * r.sin(), dir.0 * r.sin() + dir.1 * r.cos());
            if self.player(id).is_none_or(|p| p.inv.equipped.ammo.is_none()) {
                break;
            }
            self.throw_arrow_dir(id, map, (px, py), d);
        }
        if self.player(id).is_some() {
            self.player_mut(id).status.multishot = false;
        }
        true
    }

    /// `ThrowArrowToTargetDir` + `ThrowArrowToTile`: walls do not stop the arrow; it hits
    /// the first one that may be attacked, else it is lost at the end (one arrow either way).
    fn throw_arrow_dir(&mut self, id: u32, map: u16, from: (i32, i32), d: (f64, f64)) {
        let mut cur = from;
        for step in 1..=MULTISHOT_DISTANCE {
            let point = (d.0 * step as f64 + from.0 as f64, d.1 * step as f64 + from.1 as f64);
            let (sx, sy) = super::eot_extra::step_towards(point.0 - cur.0 as f64, point.1 - cur.1 as f64);
            cur = (cur.0 + sx, cur.1 + sy);
            if !ao_rules::walk::in_bounds(cur.0, cur.1) {
                break;
            }
            if let Some(v) = self.player_at(map, cur.0, cur.1).filter(|v| *v != id) {
                if self.spell_can_hit_user(id, v).is_ok() && self.user_attacks_user(id, v, true) {
                    self.consume_ammo(id);
                    return;
                }
            } else if let Some(n) = self.instance(map).npc_at(cur.0, cur.1) {
                if self.user_attacks_npc(id, map, n, true) {
                    self.consume_ammo(id);
                    return;
                }
            }
        }
        self.consume_ammo(id);
    }

    // ------------------------------------------------------------ pets

    /// `HandlePetSpell`: the spell's effect on every pet of the caster (stops at the first
    /// that already has it and may not be overridden).
    pub(crate) fn cast_on_pets(&mut self, id: u32, s: &Spell) -> bool {
        let pets = self.pets_of(id);
        let eot = s.eot_id.clamp(0, u16::MAX as i32) as u16;
        if pets.is_empty() {
            self.console(id, "No tenés mascotas.", Tone::Warn);
            return false;
        }
        if eot == 0 {
            return false;
        }
        if !super::eot::eot_works(&self.eots(), eot) {
            self.console(id, "Ese hechizo todavía no está disponible.", Tone::Warn);
            return false;
        }
        let map = self.player(id).unwrap().map;
        let override_ = self.eots().get(&eot).is_some_and(|d| d.override_);
        for nid in pets {
            let who = Who::Npc(map, nid);
            if self.find_eot(who, eot, crate::world::spellbook::EotCaster::User(id)).is_some() && !override_ {
                break;
            }
            self.apply_eot(Who::User(id), who, eot, false);
        }
        true
    }

    // ------------------------------------------------------------ stun

    /// `StunNPc` / `StunPlayer` after a ranged hit with stun ammunition (`Subtipo` 2,
    /// chance `Porcentaje`).
    pub(crate) fn try_ranged_stun(&mut self, attacker: u32, target: Who) {
        let Some(f) = self.fighter(attacker) else { return };
        let Some(w) = f.weapon.and_then(|o| self.ao.objs.get(o as u32)) else { return };
        let obj = if w.ammo == 0 { f.weapon } else { f.ammo.map(|a| a.1) };
        let Some(o) = obj.and_then(|o| self.ao.objs.get(o as u32)) else { return };
        if o.subtype != STUN_AMMO_SUBTYPE {
            return;
        }
        let chance = o.percentage;
        if self.rng.range(1, 100) >= chance {
            return;
        }
        let now = self.now();
        let stun = &self.ao.balance.stun;
        let (player_ms, npc_ms, immune_ms) = (stun.player_stun_time.max(0) as u64, stun.npc_stun_time.max(0) as u64, stun.player_immune_time.max(0) as u64);
        match target {
            Who::Npc(m, n) => {
                let def = self.maps.get(&m).and_then(|i| i.npcs.get(&n)).map_or(0, |n| n.def);
                if self.ao.npcs.get(def as u32).is_none_or(|d| d.affected_by_paralysis != 0) {
                    return;
                }
                let npc = self.npc_mut(m, n);
                npc.status.stunned_until = Some(now.after_ms(npc_ms));
                npc.path.clear();
                self.broadcast(m, &S2C::Fx { id: n, fx: STUN_FX, loops: 0 }, None);
            }
            Who::User(v) => {
                if self.player(v).is_none() || self.cc_immune(target) || !self.can_move(v) {
                    return;
                }
                let p = self.player(v).unwrap();
                // Anti-chain: a new stun only once the old one plus PlayerInmuneTime passed.
                if p.status.stun_until.is_some_and(|t| now < t.after_ms(immune_ms)) {
                    return;
                }
                let map = p.map;
                self.player_mut(v).status.stun_until = Some(now.after_ms(player_ms));
                self.broadcast(map, &S2C::Fx { id: v, fx: STUN_FX, loops: 0 }, None);
                self.console(v, "¡Has sido aturdido!", Tone::Combat);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::eot::tests::{T, MAP};

    #[test]
    fn materialize_drink_puts_it_on_a_free_tile_only() {
        let mut t = T::new();
        t.join(1, 1, 50, 50);
        t.learn(1, 44);
        let mana = t.mana(1);
        t.cast(1, 44, 52, 50);
        assert_eq!(t.w.instance(MAP).ground.at(52, 50).map(|i| (i.obj, i.amount)), Some((43, 1)));
        assert_eq!(t.mana(1), mana - 500);
        // The tile is taken now: refused and free.
        t.drain(1);
        t.cast(1, 44, 52, 50);
        assert!(t.said(1, "No hay espacio"));
        assert_eq!(t.mana(1), mana - 500);
    }

    #[test]
    fn arrow_dance_shoots_five_arrows_in_a_fan() {
        let mut t = T::new();
        t.join(1, 4, 50, 50);
        t.learn(1, 250);
        let p = t.w.player_mut(1);
        p.inv.slots[0] = Some(InvItem { obj: 479, amount: 1 });
        p.inv.slots[1] = Some(InvItem { obj: 480, amount: 20 });
        p.inv.equipped.weapon = Some(1);
        p.inv.equipped.ammo = Some(2);
        t.dummy(900_001, 54, 50);
        t.cast(1, 250, 54, 50);
        assert_eq!(t.w.player(1).unwrap().inv.get(2).unwrap().amount, 15, "one arrow per shot");
        assert!(!t.w.player(1).unwrap().status.multishot, "the reduction is gone");
        // Fewer than five arrows: refused.
        t.w.player_mut(1).inv.slots[1] = Some(InvItem { obj: 480, amount: 4 });
        t.drain(1);
        t.cast(1, 250, 54, 50);
        assert!(t.said(1, "suficientes municiones"));
        assert_eq!(t.w.player(1).unwrap().inv.get(2).unwrap().amount, 4);
    }

    #[test]
    fn pet_spell_buffs_every_pet() {
        let mut t = T::new();
        t.join(1, 1, 50, 50);
        t.learn(1, 273);
        t.drain(1);
        t.cast(1, 273, 50, 50);
        assert!(t.said(1, "No tenés mascotas"));
        t.dummy(900_001, 51, 50);
        t.dummy(900_002, 52, 50);
        for n in [900_001, 900_002] {
            t.w.npc_mut(MAP, n).pet.owner = Some(1);
        }
        t.cast(1, 273, 50, 50);
        for n in [900_001, 900_002] {
            assert!(t.w.has_eot(Who::Npc(MAP, n), 48), "Mejorar Ataque de Mascotas on {n}");
            assert_eq!(t.w.modifiers(Who::Npc(MAP, n)).linear, 20);
        }
    }

    #[test]
    fn stunned_creatures_neither_move_nor_attack() {
        let mut t = T::new();
        t.join(1, 4, 50, 50);
        t.dummy(900_001, 54, 50);
        let now = t.w.now();
        t.w.npc_mut(MAP, 900_001).status.stunned_until = Some(now.after_ms(10_000));
        assert!(t.w.npc_rooted(MAP, 900_001) && t.w.npc_paralyzed(MAP, 900_001));
        assert!(!t.w.npc_move(MAP, 900_001, ao_rules::Dir::Left));
        t.advance(10_100);
        assert!(!t.w.npc_rooted(MAP, 900_001));
    }
}

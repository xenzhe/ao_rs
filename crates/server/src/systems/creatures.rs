//! Creatures beyond the plain AI: fights between creatures, paralysis on creatures, and
//! what Tierras Sagradas did by NPC or object number (now data, see tools/tsao/convert.py):
//! - SistemaCombate.bas `NpcAtacaNpc` / `NpcImpactoNpc` / `NpcDañoNpc` and
//!   modHechizos.bas `NpcLanzaSpellSobreNpc`: a creature hits or casts on another;
//! - modHechizos.bas `HechizoEstadoNPC` + General.bas `EfectoParalisisNpc`: paralysis and
//!   immobilisation of creatures;
//! - modHechizos.bas `HechizoPropNPC`: critical spells, `x1.3` on pets, `x1.4`, creatures that
//!   ignore a spell (`AfectaRelampago` -> `ImmuneSpells`);
//! - SistemaCombate.bas `CalcularDaño` / `UserDañoNpc`: the dragon slayer, critical blows,
//!   `x1.5` on pets, the paladin bonus, losing the slayer when the dragon dies;
//! - CalcularDarExp / NPCTirarOro / NPC_TIRAR_ITEMS: the tunics of mastery, wealth and luck.
//!
//! Fixes over TSAO:
//! - `HechizoEstadoNPC` paralysed the creature with EVERY status spell (the `AfectaParalisis`
//!   block had no `If Paraliza` guard): here only paralysis and immobilisation do;
//! - `NpcDañoNpc` gave the owner no experience for a pet's blows (only for its spells): here
//!   both count the same.
use crate::clock::GameTime;
use crate::world::World;
use ao_formats::dat::npcs::NpcType;
use ao_formats::dat::obj::SpecialObj;
use ao_formats::dat::spells::{Spell, SpellEffects as Fx};
use ao_protocol::{Tone, S2C};
use ao_rules::combat as rules;

/// Paralysis state of a creature.
#[derive(Clone, Debug, Default)]
pub struct NpcStatus {
    /// Paralysed: no AI at all.
    pub paralyzed_until: Option<GameTime>,
    /// Immobilised: fights but cannot step.
    pub immobilized_until: Option<GameTime>,
    /// `Contadores.StunEndTime` (`StunNPc`): neither moves nor attacks.
    pub stunned_until: Option<GameTime>,
    /// Effects over time on the creature (systems/eot.rs).
    pub eots: Vec<crate::world::spellbook::ActiveEot>,
    /// `pathFindingInfo.OrbitDirection` (+1/-1, 0 = not chosen) and when it is chosen
    /// again (systems/npc_tactics.rs).
    pub orbit_dir: i8,
    pub orbit_until: GameTime,
    /// `Spells(i).lastUse` + `Cd`: when each spell slot may be cast again.
    pub spell_ready: Vec<GameTime>,
    /// `flags.AttackedFirstBy` / owner of a creature (`GetOwnedBy`, `CanAttackNotOwner`):
    /// the player who hit it first and until when it stays theirs.
    pub owner: Option<(u32, GameTime)>,
}

/// Who a weapon hits (dragon slayer: dragons and players differ).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Vs {
    User,
    Npc,
    Dragon,
}

impl World {
    fn profile(&self) -> &'static ao_formats::profile::Profile {
        ao_formats::profile::for_recursos(&self.recursos)
    }

    // ------------------------------------------------------------ paralysis

    pub(crate) fn npc_paralyzed(&self, map: u16, nid: u32) -> bool {
        let now = self.now();
        self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).is_some_and(|n| [n.status.paralyzed_until, n.status.stunned_until].iter().any(|t| t.is_some_and(|t| now < t)))
    }

    /// Paralysed or immobilised: may not take a step.
    pub(crate) fn npc_rooted(&self, map: u16, nid: u32) -> bool {
        let now = self.now();
        self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).is_some_and(|n| [n.status.paralyzed_until, n.status.immobilized_until, n.status.stunned_until].iter().any(|t| t.is_some_and(|t| now < t)))
    }

    /// `HechizoEstadoNPC` (the part with a creature effect): paralyse / immobilise unless the
    /// creature is immune (`AfectaParalisis=1`); `RemoverParalisis` on a creature is refused.
    /// Returns true when the spell took effect (the caller then pays it).
    pub(crate) fn status_on_npc(&mut self, caster: u32, s: &Spell, map: u16, nid: u32) -> bool {
        let fx = s.effects;
        let Some(n) = self.maps.get(&map).and_then(|m| m.npcs.get(&nid)) else { return false };
        let Some(d) = self.ao.npcs.get(n.def as u32) else { return false };
        if fx.contains(Fx::REMOVE_PARALYSIS) {
            self.console(caster, "Ese hechizo no tiene efecto sobre las criaturas.", Tone::Warn);
            return false;
        }
        if !(fx.contains(Fx::PARALYZE) || fx.contains(Fx::IMMOBILIZE)) {
            self.console(caster, "Ese hechizo todavía no afecta a las criaturas.", Tone::Warn);
            return false;
        }
        if !n.attackable {
            self.console(caster, "No puedes atacar a esa criatura.", Tone::Warn);
            return false;
        }
        if let Some(why) = self.faction_npc_attack_refusal(caster, n.def) {
            self.console(caster, why, Tone::Warn);
            return false;
        }
        if d.affected_by_paralysis != 0 {
            self.console(caster, "La criatura es inmune a ese hechizo.", Tone::Warn);
            return false;
        }
        let now = self.now();
        let secs = self.profile().npc_combat.paralysis_secs.unwrap_or(s.duration.max(1) as u32) as u64;
        let until = Some(now.after_secs(secs));
        let n = self.npc_mut(map, nid);
        if fx.contains(Fx::PARALYZE) {
            n.status.paralyzed_until = until;
        } else {
            n.status.immobilized_until = until;
        }
        n.path.clear();
        self.npc_attacked_by(map, nid, caster);
        self.pets_assist(caster, map, nid);
        true
    }

    /// `EfectoParalisisNpc`: the time ran out.
    pub(crate) fn tick_npc_status(&mut self, map: u16, now: GameTime) {
        let Some(inst) = self.maps.get_mut(&map) else { return };
        for n in inst.npcs.values_mut() {
            for t in [&mut n.status.paralyzed_until, &mut n.status.immobilized_until] {
                if t.is_some_and(|t| now >= t) {
                    *t = None;
                }
            }
        }
        self.tick_npc_eots(map, now);
    }

    // ------------------------------------------------------------ creature against creature

    /// `NpcAtacaNpc` + `NpcImpactoNpc` + `NpcDañoNpc`: `a` swings at `v` (both on `map`).
    pub(crate) fn npc_attacks_npc(&mut self, map: u16, a: u32, v: u32) {
        let (Some(att), Some(vic)) = (self.npc_combat(map, a), self.npc_combat(map, v)) else { return };
        if !vic.attackable {
            return;
        }
        let hit = self.chance(ao_rules::tsao_combat::hit_chance(att.attack_power, vic.evasion_power));
        let damage = if hit { self.roll(att.min_hit, att.max_hit).max(0) } else { 0 };
        self.broadcast(map, &S2C::Attack { attacker: a, target: v, hit, damage, critical: false }, None);
        if hit {
            self.npc_damaged_by_npc(map, v, damage, a);
        }
    }

    /// `NpcLanzaSpellSobreNpc`: only harmful spells (`SubeHP=2`); the first spell of `a`.
    pub(crate) fn npc_casts_on_npc(&mut self, map: u16, a: u32, v: u32) {
        let Some(def) = self.maps.get(&map).and_then(|m| m.npcs.get(&a)).map(|n| n.def) else { return };
        let Some(spell) = self.ao.npcs.get(def as u32).and_then(|d| d.spells.first()).map(|s| s.spell) else { return };
        let Some(s) = self.ao.spells.get(spell as u32).cloned() else { return };
        if !s.effects.contains(Fx::DO_DAMAGE) {
            return;
        }
        let Some((tx, ty)) = self.maps.get(&map).and_then(|m| m.npcs.get(&v)).filter(|n| n.alive() && n.attackable).map(|n| (n.x, n.y)) else { return };
        let damage = self.roll(s.hp.min.min(s.hp.max), s.hp.max.max(s.hp.min)).max(0);
        self.broadcast(map, &S2C::Effect { spell: spell as u16, from: a, tx, ty }, None);
        if s.fx > 0 {
            self.broadcast(map, &S2C::Fx { id: v, fx: s.fx as u16, loops: s.loops.clamp(0, 255) as u8 }, None);
        }
        self.npc_damaged_by_npc(map, v, damage, a);
    }

    /// Damage from creature `a` to creature `v`. A pet's owner gets the experience and the
    /// kill (loot, quests), like `MuereNpc(Victima, MaestroUser)`.
    pub(crate) fn npc_damaged_by_npc(&mut self, map: u16, v: u32, damage: i32, a: u32) {
        let owner = self.maps.get(&map).and_then(|m| m.npcs.get(&a)).and_then(|n| n.pet.owner).filter(|o| self.player(*o).is_some());
        let Some(mut c) = self.npc_combat(map, v) else { return };
        if owner.is_some_and(|o| self.pet_gives_no_exp(map, v, o)) {
            c.exp = 0;
        }
        let Some(n) = self.maps.get_mut(&map).and_then(|m| m.npcs.get_mut(&v)) else { return };
        let before = n.hp;
        n.hp = (n.hp - damage.max(0)).max(0);
        let (hp, max_hp, name) = (n.hp, n.max_hp, n.name.clone());
        if let Some(o) = owner {
            let pet = self.maps[&map].npcs[&a].name.clone();
            self.console(o, format!("{pet} le ha causado {damage} de daño a {name}."), Tone::Combat);
            let exp = rules::exp_for_damage(c.exp, max_hp, before, hp) * self.cfg.world.exp_mult.max(1);
            let exp = self.exp_with_items(o, exp);
            if exp > 0 {
                let lvl = self.npc_level(map, v);
                self.share_exp(o, exp, map, lvl);
            }
        }
        if hp <= 0 {
            self.kill_npc(map, v, owner.unwrap_or(0));
            self.pet_follow(map, a);
        } else {
            self.broadcast(map, &S2C::Hp { id: v, hp, max_hp }, None);
        }
    }

    // ------------------------------------------------------------ special objects

    /// The `SpecialObj` of an equipped piece.
    fn special_of(&self, obj: Option<u16>) -> Option<&SpecialObj> {
        self.obj(obj).map(|o| &o.special)
    }

    fn worn_armor(&self, id: u32) -> Option<u16> {
        let p = self.player(id)?;
        p.inv.equipped.armor.and_then(|s| p.inv.get(s)).map(|i| i.obj)
    }

    /// CalcularDarExp: a tunic of mastery (`ExpBonusPct`) adds its share.
    pub(crate) fn exp_with_items(&self, id: u32, exp: u32) -> u32 {
        let pct = self.special_of(self.worn_armor(id)).map_or(0, |s| s.exp_bonus_pct);
        exp + (exp as u64 * pct.max(0) as u64 / 100) as u32
    }

    /// NPCTirarOro: a tunic of wealth (`GoldBonusPct`) adds its share.
    pub(crate) fn gold_with_items(&self, id: u32, gold: u32) -> u32 {
        let pct = self.special_of(self.worn_armor(id)).map_or(0, |s| s.gold_bonus_pct);
        gold.saturating_add((gold as u64 * pct.max(0) as u64 / 100) as u32)
    }

    /// NPC_TIRAR_ITEMS: a tunic of luck (`DropLuck`) adds to each drop's chance in 200.
    pub(crate) fn drop_luck(&self, id: u32) -> i32 {
        self.special_of(self.worn_armor(id)).map_or(0, |s| s.drop_luck.max(0))
    }

    /// `CalcularDaño` for weapons with a fixed roll: the dragon slayer against a dragon
    /// (`DragonHit`, `DragonHitMax`) or a player (`UserHit`). None = the normal roll.
    pub(crate) fn special_weapon_roll(&mut self, weapon: Option<u16>, vs: Vs) -> Option<(i32, i32)> {
        let s = self.special_of(weapon)?.clone();
        match vs {
            Vs::Dragon => s.dragon_hit.map(|(lo, hi)| (self.roll(lo, hi), s.dragon_hit_max.max(hi))),
            Vs::User if s.user_hit > 0 => Some((s.user_hit, s.user_hit)),
            _ => None,
        }
    }

    /// What a player's blow hits: a dragon counts apart for the dragon slayer.
    pub(crate) fn melee_target_kind(&self, map: u16, nid: u32) -> Vs {
        match self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).map(|n| n.kind) {
            Some(NpcType::Dragon) => Vs::Dragon,
            _ => Vs::Npc,
        }
    }

    /// `UserDañoNpc` (TSAO profile keys): pets take `MeleeVsPetPct`, classes add
    /// `NpcFlatBonus`, then `NpcCriticalPct` doubles (`NpcCriticalMult`). Returns the damage
    /// and whether it was critical.
    pub(crate) fn tsao_melee_vs_npc(&mut self, id: u32, map: u16, nid: u32, damage: i64) -> (i64, bool) {
        let r = self.profile().npc_combat.clone();
        let mut d = damage;
        if self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).is_some_and(|n| n.pet.owner.is_some()) {
            d = d * r.melee_vs_pet_pct as i64 / 100;
        }
        if let Some(c) = self.player(id).and_then(|p| ao_formats::dat::balance::Class::from_id(p.class as i32)) {
            d += r.flat_bonus_for(c.spanish_name()) as i64;
        }
        let d = d.max(0);
        if r.critical_pct > 0 && self.chance(r.critical_pct as i64) {
            self.console(id, "¡Golpe crítico!", Tone::Combat);
            return (d * r.critical_mult.max(1) as i64, true);
        }
        (d, false)
    }

    /// `HechizoPropNPC` damage with the TSAO profile: `Daño * 3.1`, critical, staff, pets
    /// `SpellVsPetPct`, `SpellVsNpcPct`, then the weapon's `DañoMagico`. None when the
    /// creature ignores this spell (`ImmuneSpells`).
    pub(crate) fn tsao_spell_vs_npc(&mut self, id: u32, spell_id: u16, s: &Spell, map: u16, nid: u32, roll: i32) -> Option<i32> {
        use ao_formats::dat::balance::Class;
        let n = self.maps.get(&map)?.npcs.get(&nid)?;
        let (def, pet) = (n.def, n.pet.owner.is_some());
        if self.ao.npcs.get(def as u32).is_some_and(|d| d.special.immune_spells.contains(&(spell_id as i32))) {
            self.console(id, "La criatura es inmune a ese hechizo.", Tone::Warn);
            return None;
        }
        let r = self.profile().npc_combat.clone();
        let f = self.fighter(id)?;
        let weapon = f.weapon.and_then(|o| self.ao.objs.get(o as u32)).cloned();
        let mut d = ao_rules::tsao_combat::spell_base(roll);
        if r.critical_pct > 0 && self.chance(r.critical_pct as i64) {
            self.console(id, "¡Hechizo crítico!", Tone::Combat);
            d *= r.critical_mult.max(1) as i64;
        }
        if s.staff_affected {
            d = ao_rules::tsao_combat::staff_scaled(d, f.class == Class::Mage, f.class == Class::Druid, weapon.as_ref().map(|w| w.staff_damage_bonus));
        }
        if pet {
            d = d * r.spell_vs_pet_pct as i64 / 100;
        }
        d = d * r.spell_vs_npc_pct as i64 / 100;
        if let Some((lo, hi)) = weapon.map(|w| w.tsao_magic_damage).filter(|(lo, _)| *lo > 0) {
            d += self.roll(lo, hi) as i64;
        }
        Some(d.clamp(0, i32::MAX as i64) as i32)
    }

    /// UserDañoNpc: killing a dragon costs the weapon that has `LostOnDragonKill`.
    pub(crate) fn creature_on_killed(&mut self, map: u16, kind: NpcType, killer: u32) {
        let _ = map;
        if kind != NpcType::Dragon {
            return;
        }
        let Some(slot) = self.player(killer).and_then(|p| p.inv.equipped.weapon) else { return };
        let Some(obj) = self.player(killer).and_then(|p| p.inv.get(slot)).map(|i| i.obj) else { return };
        if self.special_of(Some(obj)).is_some_and(|s| s.lost_on_dragon_kill) {
            let name = self.obj(Some(obj)).map_or(String::new(), |o| o.name.clone());
            self.unequip(killer, slot);
            self.remove_from_slot(killer, slot, 1);
            self.console(killer, format!("¡{name} se ha quebrado al matar al dragón!"), Tone::Warn);
        }
    }

    /// Magic defence of a helmet or ring with a class table (`MagicDefByClass`), as
    /// `calcularDefCasco` / `calcularDefAnillo`.
    pub(crate) fn class_magic_def(&self, obj: Option<u16>, class: &str) -> Option<i32> {
        self.special_of(obj).and_then(|s| s.magic_def_for(class))
    }

    /// The ring of immunity (`ParalysisResist`): 1 in N paralyses do nothing.
    pub(crate) fn resists_paralysis(&mut self, id: u32, from_npc: bool) -> bool {
        let Some(p) = self.player(id) else { return false };
        let tool = p.inv.equipped.tool.and_then(|s| p.inv.get(s)).map(|i| i.obj);
        let n = self.special_of(tool).map_or(0, |s| if from_npc { s.paralysis_resist_npcs } else { s.paralysis_resist_users });
        n > 0 && self.roll(1, n) == 1
    }

    /// `NpcLanzaSpellSobreUser`: a creature's spell loses `RandomNumber(DefensaMagicaMin,
    /// DefensaMagicaMax)` for each of helmet, armour and ring (plain ranges, no class table).
    pub(crate) fn tsao_npc_spell_absorb(&mut self, pid: u32) -> i32 {
        let Some(f) = self.fighter(pid) else { return 0 };
        let mut total = 0;
        for o in [f.helmet, f.armor, f.tool] {
            if let Some((lo, hi)) = self.obj(o).map(|d| d.tsao_magic_def).filter(|(a, b)| *a > 0 || *b > 0) {
                total += self.roll(lo, hi);
            }
        }
        total
    }

    /// TSAO `NpcDaño` absorption: a blow to the head (1 in 6) meets the helmet, any other the
    /// armour (no shield); when something absorbed, at least 1 still gets through.
    pub(crate) fn tsao_npc_blow(&mut self, pid: u32, roll: i32) -> i32 {
        let Some(f) = self.fighter(pid) else { return roll };
        let piece = if self.roll(1, 6) == rules::BODY_HEAD { f.helmet } else { f.armor };
        match self.obj(piece).map(|o| (o.min_def, o.max_def)) {
            Some((lo, hi)) => (roll - self.roll(lo, hi)).max(1),
            None => roll.max(0),
        }
    }

    /// TSAO `PuedeApuñalar` + `DoApuñalar` (Trabajo.bas): with a stabbing weapon and 10 skill
    /// points (assassins with any), a roll of `0..Suerte` under 15 stabs (Suerte 200 at 0-10
    /// points down to 100 at 100; assassins 0..95, and always from behind a player). Returns
    /// the extra damage: x1.5 of the hit on players, x2 on creatures.
    pub(crate) fn tsao_stab(&mut self, f: &super::combat::Fighter, hit: i64, vs_user_same_heading: Option<bool>) -> i64 {
        use ao_formats::dat::balance::{Class, Skill};
        let skill = f.skill(Skill::Stab);
        let assassin = f.class == Class::Assassin;
        if !rules::can_stab(assassin, skill, self.obj(f.weapon)) {
            return 0;
        }
        let res = if assassin { self.roll(0, 95) } else { self.roll(0, ao_rules::tsao_combat::stab_luck(skill)) };
        let res = if assassin && vs_user_same_heading == Some(true) { 1 } else { res };
        if res >= 15 {
            return 0;
        }
        match vs_user_same_heading {
            Some(_) => (hit as f64 * 1.5) as i64,
            None => hit * 2,
        }
    }

    /// TSAO UsuarioAtacaUsuario: a weapon with `Inmoviliza` paralyses the victim
    /// `probInmov`% of the hits (always on the head) for `ImmobilizeSecs`.
    pub(crate) fn tsao_gauntlet(&mut self, attacker: u32, victim: u32, head: bool) {
        let Some(w) = self.fighter(attacker).and_then(|f| f.weapon) else { return };
        let Some(s) = self.special_of(Some(w)).cloned().filter(|s| s.immobilize_pct > 0) else { return };
        if !(head || self.chance(s.immobilize_pct as i64)) || self.resists_paralysis(victim, false) {
            return;
        }
        let now = self.now();
        let name = self.player(attacker).map_or(String::new(), |p| p.name.clone());
        self.player_mut(victim).status.paralyzed_until = Some(now.after_secs(s.immobilize_secs.max(1) as u64));
        let map = self.player(victim).unwrap().map;
        if let Some(&fx) = self.profile().fx.get("paralyze") {
            self.broadcast(map, &S2C::Fx { id: victim, fx, loops: 0 }, None);
        }
        self.console(victim, format!("{name} te ha inmovilizado con su manopla."), Tone::Combat);
        self.refresh_status(victim);
    }

    /// The magic lute (`SpellDamagePct` on the tool slot) on spells against players.
    pub(crate) fn spell_damage_pct(&self, id: u32) -> i32 {
        let Some(p) = self.player(id) else { return 0 };
        let tool = p.inv.equipped.tool.and_then(|s| p.inv.get(s)).map(|i| i.obj);
        self.special_of(tool).map_or(0, |s| s.spell_damage_pct)
    }
}

/// The special objects and creature rules on the TSAO data (`AO_TSAO`, skipped without it).
#[cfg(test)]
mod tests {
    use crate::systems::pets::tests::tsao;
    use crate::world::inventory::InvItem;
    use ao_rules::Dir;

    const MAP: u16 = 1;
    const SLAYER: u16 = 1053;
    const WARRIOR: u8 = 3;
    /// Cría del Dragón Senagoso (NpcType 6, 15000 hp, DEF 40).
    const DRAGON: u16 = 971;
    const WOLF: u16 = 501;

    fn wield(t: &mut super::super::pets::tests::T, id: u32, slot: u8, obj: u16) {
        t.w.player_mut(id).inv.slots[slot as usize - 1] = Some(InvItem { obj, amount: 1 });
        t.w.on_equip_item(id, slot);
    }

    #[test]
    fn dragon_slayer_kills_dragons_and_breaks_but_tickles_others() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Matadragon");
        t.w.player_mut(1).class = WARRIOR;
        wield(&mut t, 1, 1, SLAYER);
        assert_eq!(t.w.player(1).unwrap().inv.equipped.weapon, Some(1), "warriors may wield it");
        let p = t.w.player_mut(1);
        p.stats.skills[3] = 100; // Armas
        let (x, y) = (p.x, p.y);
        p.dir = Dir::Right;
        let dragon = t.creature(MAP, DRAGON, x + 1, y);
        let n = t.w.npc_mut(MAP, dragon);
        (n.movement, n.hostile) = (ao_formats::dat::npcs::Movement::Static, false);
        t.w.maps.get_mut(&MAP).unwrap().move_npc(dragon, x + 1, y);
        let mut hits = Vec::new();
        let mut seq = 1;
        while t.npc(MAP, dragon).is_some() && seq < 2000 {
            let before = t.npc(MAP, dragon).unwrap().hp;
            t.w.player_mut(1).combat.last_attack = None;
            t.w.player_mut(1).stats.sta = 1000;
            t.w.on_melee(1, seq);
            seq += 1;
            if let Some(n) = t.npc(MAP, dragon) {
                if n.hp < before {
                    hits.push(before - n.hp);
                }
            }
        }
        assert!(t.npc(MAP, dragon).is_none(), "the dragon died in {seq} swings");
        // 3 x 220..225 + userroll, minus DEF 40: never a feeble hit (MinHIT=MaxHIT=1 otherwise).
        assert!(hits.iter().all(|h| *h >= 600), "{hits:?}");
        let p = t.w.player(1).unwrap();
        assert!(p.inv.equipped.weapon.is_none() && p.inv.get(1).is_none(), "the slayer broke on the dragon");

        // Against a wolf the same sword is a stick (MaxHIT 1).
        wield(&mut t, 1, 1, SLAYER);
        let wolf = t.creature(MAP, WOLF, x + 1, y);
        t.w.maps.get_mut(&MAP).unwrap().move_npc(wolf, x + 1, y);
        let hp0 = t.npc(MAP, wolf).unwrap().max_hp;
        for s in 0..30 {
            t.w.player_mut(1).combat.last_attack = None;
            t.w.on_melee(1, 5000 + s);
        }
        if let Some(n) = t.npc(MAP, wolf) {
            assert!(hp0 - n.hp < 600, "no dragon damage on a wolf");
        }
    }

    #[test]
    fn tunics_add_experience_and_gold() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Maestro");
        let plain = t.w.exp_with_items(1, 1000);
        // 1455 Túnica de la Maestria: +25% exp and drop luck; 1456 de la Riqueza: +25% gold.
        wield(&mut t, 1, 2, 1455);
        assert_eq!(t.w.player(1).unwrap().inv.equipped.armor, Some(2));
        assert_eq!((plain, t.w.exp_with_items(1, 1000), t.w.gold_with_items(1, 1000), t.w.drop_luck(1)), (1000, 1250, 1000, 3));
        wield(&mut t, 1, 3, 1456);
        assert_eq!((t.w.exp_with_items(1, 1000), t.w.gold_with_items(1, 1000)), (1000, 1250));
    }

    #[test]
    fn tiara_and_ring_defend_by_class_and_the_ring_resists_paralysis() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Inmune");
        assert_eq!(t.w.class_magic_def(Some(1035), "Paladín"), Some(17));
        assert_eq!(t.w.class_magic_def(Some(1035), "Mago"), Some(26));
        assert_eq!(t.w.class_magic_def(Some(1540), "Guerrero"), Some(11));
        assert_eq!(t.w.class_magic_def(Some(1), "Mago"), None);
        wield(&mut t, 1, 4, 1540);
        let resisted = (0..2000).filter(|_| t.w.resists_paralysis(1, false)).count();
        assert!((120..290).contains(&resisted), "about 1 in 10: {resisted}");
        let resisted = (0..2000).filter(|_| t.w.resists_paralysis(1, true)).count();
        assert!((170..340).contains(&resisted), "about 1 in 8: {resisted}");
    }

    #[test]
    fn spells_on_creatures_crit_and_hit_pets_harder() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Critico");
        let wolf = t.creature(MAP, WOLF, 52, 50);
        let s = t.w.ao.spells.get(1).unwrap().clone();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            seen.insert(t.w.tsao_spell_vs_npc(1, 1, &s, MAP, wolf, 10).unwrap());
        }
        // 10 * 3.1 = 31; x1.4 = 43; a critical doubles before: 86 (no staff, mage 70%: 21/43... ).
        assert_eq!(seen.len(), 2, "plain and critical: {seen:?}");
        let (lo, hi) = (*seen.iter().next().unwrap(), *seen.iter().last().unwrap());
        assert!((hi as f64 / lo as f64 - 2.0).abs() < 0.1, "{lo} {hi}");
        // On someone's pet x1.3 more.
        t.w.npc_mut(MAP, wolf).pet.owner = Some(99);
        let pet: std::collections::BTreeSet<i32> = (0..200).map(|_| t.w.tsao_spell_vs_npc(1, 1, &s, MAP, wolf, 10).unwrap()).collect();
        assert!(*pet.iter().next().unwrap() > lo, "{pet:?} vs {lo}");
        // Relámpago on an elemental: immune.
        let fire = t.creature(MAP, 93, 48, 50);
        let r = t.w.ao.spells.get(12).unwrap().clone();
        assert_eq!(t.w.tsao_spell_vs_npc(1, 12, &r, MAP, fire, 10), None);
    }

    #[test]
    fn blows_on_creatures_crit_pets_take_more_and_paladins_add() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Paladin");
        let wolf = t.creature(MAP, WOLF, 52, 50);
        let seen: std::collections::BTreeSet<i64> = (0..300).map(|_| t.w.tsao_melee_vs_npc(1, MAP, wolf, 100).0).collect();
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), vec![100, 200], "40% critical x2");
        t.w.npc_mut(MAP, wolf).pet.owner = Some(99);
        t.w.player_mut(1).class = 7; // Paladín
        let seen: std::collections::BTreeSet<i64> = (0..300).map(|_| t.w.tsao_melee_vs_npc(1, MAP, wolf, 100).0).collect();
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), vec![195, 390], "pet x1.5 + 45");
    }

    #[test]
    fn creature_blows_meet_helmet_or_armour_and_leave_at_least_one() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Blindado");
        assert_eq!(t.w.tsao_npc_blow(1, 7), 7, "naked: the whole roll");
        let armor = t.w.ao.objs.iter().find(|(_, o)| o.obj_type == ao_formats::dat::obj::ObjType::Armor && o.min_def >= 20).map(|(i, _)| i as u16).unwrap();
        let helmet = t.w.ao.objs.iter().find(|(_, o)| o.obj_type == ao_formats::dat::obj::ObjType::Helmet && o.min_def >= 20).map(|(i, _)| i as u16).unwrap();
        t.w.player_mut(1).class = WARRIOR;
        let eq = &mut t.w.player_mut(1).inv;
        eq.slots[1] = Some(InvItem { obj: armor, amount: 1 });
        eq.slots[2] = Some(InvItem { obj: helmet, amount: 1 });
        (eq.equipped.armor, eq.equipped.helmet) = (Some(2), Some(3));
        assert!((0..100).all(|_| t.w.tsao_npc_blow(1, 7) == 1), "absorbed but 1 gets through");
    }

    #[test]
    fn stabs_and_the_gauntlet() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Asesino");
        t.join(2, "Victima");
        t.w.player_mut(1).class = 4; // Asesino
        let dagger = t.w.ao.objs.iter().find(|(_, o)| o.stabs == 1 && o.obj_type == ao_formats::dat::obj::ObjType::Weapon).map(|(i, _)| i as u16).unwrap();
        wield(&mut t, 1, 1, dagger);
        let f = t.w.fighter(1).unwrap();
        assert!((0..50).all(|_| t.w.tsao_stab(&f, 100, Some(true)) == 150), "assassins always stab from behind for x1.5");
        let on_npc = (0..2000).filter(|_| t.w.tsao_stab(&f, 100, None) == 200).count();
        assert!((200..450).contains(&on_npc), "15 in 96 on creatures, x2: {on_npc}");
        // Manopla con Púas (Inmoviliza, probInmov 12): a blow to the head always paralyses.
        t.w.player_mut(1).class = WARRIOR;
        wield(&mut t, 1, 5, 1543);
        assert!(!t.w.player(2).unwrap().status.paralyzed_until.is_some_and(|u| t.w.now() < u));
        t.w.tsao_gauntlet(1, 2, true);
        assert!(t.w.player(2).unwrap().status.paralyzed_until.is_some_and(|u| t.w.now() < u));
    }

    #[test]
    fn tsao_chasers_come_for_the_player() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Cebo");
        let p = t.w.player_mut(1);
        (p.hp, p.max_hp) = (1_000_000, 1_000_000);
        let (x, y) = (p.x, p.y);
        // The baby dragon had TSAO Movement=3 (NpcMaloAtacaUsersBuenos): it used to stand.
        let d = t.creature(MAP, DRAGON, x + 6, y);
        let start = { let n = t.npc(MAP, d).unwrap(); (n.x, n.y) };
        let far = (start.0 - x).abs() + (start.1 - y).abs();
        t.run(8000);
        let n = t.npc(MAP, d).unwrap();
        assert!((n.x - x).abs() + (n.y - y).abs() < far, "it came closer from {start:?} to {:?}", (n.x, n.y));
    }
}

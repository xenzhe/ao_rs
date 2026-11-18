//! Spells, status effects, effects over time and meditation, ported from the official
//! AO20 server (`modHechizos.bas`, `EffectsOverTime.bas` + `UpdateHpOverTime.cls`,
//! `General.bas` status counters, `Trabajo.bas::DoMeditar`, `Protocol.bas::HandleMeditate`).
//! Every check happens here: the client only names the spell (a Hechizos.dat id it knows)
//! and the tile. Pure formulas live in `ao_rules::magic`.
//!
//! Wired from outside this file (see the P1 report):
//! - `magic_on_enter(id)` from session when a character enters the world;
//! - `tick_magic(now)` from `World::tick`;
//! - `can_move(id)` must be checked by movement before a step;
//! - `stop_meditating(id)` should be called by melee/ranged attacks (also detected here).
use crate::clock::GameTime;
use crate::world::spellbook::{ActiveEot, MAX_SPELL_SLOTS};
use crate::world::World;
use ao_formats::dat::spells::{Spell, SpellEffects as Fx, SpellRequirements as Req, SpellTarget, SpellType, TargetEffect};
use ao_protocol::{status, Tone, S2C};
use ao_rules::magic::{self as rules, MagicItem};
use ao_rules::spells::{SpellDef, SpellEffect, SpellTarget as DefTarget};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// TEMPORARY (development login only): spells given to throwaway characters with an
/// empty book so the client can try magic before character creation hands them out.
/// Dardo Mágico, Curar Heridas Leves, Curar Veneno, Paralizar, Inmovilizar, Remover
/// Parálisis, Invisibilidad Menor, Resucitar.
pub const DEV_KIT: [u16; 8] = [1, 12, 11, 25, 26, 27, 24, 14];
/// TEMPORARY (development login only): Magic skill, mana and stamina raised so every
/// spell of [`DEV_KIT`] passes `PuedeLanzar`.
pub const DEV_MAGIC_SKILL: u8 = 100;
pub const DEV_MIN_MANA: i32 = 1000;
pub const DEV_MIN_STA: i32 = 500;

/// `e_Skill` indexes (0-based) read by magic.
const SKILL_MAGIC: usize = 0;
const SKILL_MEDITATE: usize = 4;
/// `e_Trigger`: ZonaSegura, ZONAPELEA.
const TRIGGER_SAFE: u16 = 4;
const TRIGGER_ARENA: u16 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    User(u32),
    Npc(u32),
}

/// `RandomNumber` with the bounds in any order (EffectsOverTime.dat has min > max).
fn roll(w: &mut World, a: i32, b: i32) -> i32 {
    w.rng.range(a.min(b), a.max(b))
}

// ---------------------------------------------------------------- EffectsOverTime.dat

/// The part of an `[EOTn]` section magic runs (types 1 eModifyHealth, 9 eModifyMana).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EotDef {
    pub kind: i32,
    pub power_min: i32,
    pub power_max: i32,
    pub ticks: u32,
    pub tick_ms: u64,
    pub tick_fx: u16,
    pub override_: bool,
}

/// Minimal reader of `Dat/EffectsOverTime.dat` (`EffectsOverTime.bas::LoadEffectsOverTime`).
/// TODO(integrator): move to `ao-formats` with the other Dat readers.
pub fn parse_eots(text: &str) -> HashMap<u16, EotDef> {
    let mut out = HashMap::new();
    let mut cur: Option<(u16, EotDef)> = None;
    for line in text.lines() {
        let line = line.split('\'').next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix('[') {
            if let Some((id, def)) = cur.take() {
                out.insert(id, def);
            }
            let name = rest.split(']').next().unwrap_or("");
            cur = name.strip_prefix("EOT").and_then(|n| n.parse().ok()).map(|id| (id, EotDef::default()));
            continue;
        }
        let (Some((_, def)), Some((k, v))) = (cur.as_mut(), line.split_once('=')) else { continue };
        let v = v.trim();
        let n = v.parse::<f64>().unwrap_or(0.0);
        match k.trim().to_ascii_lowercase().as_str() {
            "type" => def.kind = n as i32,
            "tickpowermin" => def.power_min = n as i32,
            "tickpowermax" => def.power_max = n as i32,
            "ticks" => def.ticks = n.max(0.0) as u32,
            "ticktime" => def.tick_ms = n.max(0.0) as u64,
            "tickfx" => def.tick_fx = n.max(0.0) as u16,
            "override" => def.override_ = n != 0.0,
            _ => {}
        }
    }
    if let Some((id, def)) = cur {
        out.insert(id, def);
    }
    out
}

type EotTable = Arc<HashMap<u16, EotDef>>;

fn eot_table(recursos: &Path) -> EotTable {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, EotTable>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    cache
        .entry(recursos.to_path_buf())
        .or_insert_with(|| {
            // Windows-1252 file: only ASCII keys matter, so a lossy read is enough.
            let bytes = std::fs::read(recursos.join("Dat").join("EffectsOverTime.dat")).unwrap_or_default();
            Arc::new(parse_eots(&String::from_utf8_lossy(&bytes)))
        })
        .clone()
}

// ---------------------------------------------------------------- client spell book

/// What the prototype client shows for an official spell. `effect` is a simple
/// heuristic (the client has only three animations): healing or positive spells -> heal,
/// expensive (>= 100 mana) or strong (max HP >= 40) -> lightning, the rest -> fireball.
/// `cooldown_ms` includes the global cast interval so the client never sends a cast the
/// server would refuse; `range` is the vertical vision range (the tighter one).
pub fn client_spell_def(id: u16, s: &Spell) -> SpellDef {
    let positive = s.effects.contains(Fx::DO_HEAL) || s.target_effect == TargetEffect::Positive;
    let effect = if positive {
        SpellEffect::Heal
    } else if s.mana_required >= 100 || s.hp.max >= 40 {
        SpellEffect::Lightning
    } else {
        SpellEffect::Fireball
    };
    SpellDef {
        id,
        name: s.name.clone(),
        mana: s.mana_required,
        range: rules::VIEW_RANGE_Y,
        cooldown_ms: rules::CAST_INTERVAL_MS.max(s.cooldown.max(0) as u64 * 1000),
        power: (s.hp.min + s.hp.max) / 2,
        target: if positive { DefTarget::Ally } else { DefTarget::Enemy },
        effect,
    }
}

impl World {
    // ------------------------------------------------------------ entering / book

    /// Called once when a character enters the world (session): dev kit, spell book,
    /// slots and the status of everyone already on the map.
    pub(crate) fn magic_on_enter(&mut self, id: u32) {
        let Some(p) = self.player(id) else { return };
        if p.char_id.is_none() && p.spells.is_empty() {
            let p = self.player_mut(id);
            for (i, s) in DEV_KIT.iter().enumerate() {
                p.spells.slots[i] = *s;
            }
            p.stats.skills[SKILL_MAGIC] = p.stats.skills[SKILL_MAGIC].max(DEV_MAGIC_SKILL);
            p.max_mana = p.max_mana.max(DEV_MIN_MANA);
            p.mana = p.max_mana;
            p.stats.max_sta = p.stats.max_sta.max(DEV_MIN_STA);
            p.stats.sta = p.stats.max_sta;
        }
        self.send_spellbook(id);
        let map = self.player(id).unwrap().map;
        let others: Vec<u32> = self.instance(map).players.iter().copied().filter(|o| *o != id).collect();
        for o in others {
            let flags = self.status_flags(o);
            if flags != 0 {
                self.send(id, &S2C::Status { id: o, flags });
            }
        }
        self.player_mut(id).status.sent_flags = u32::MAX;
        self.refresh_status(id);
        self.send_stats(id);
        self.send_vitals(id);
    }

    /// `S2C::SpellBook` (definitions the prototype client uses) and `S2C::SpellSlots`.
    pub(crate) fn send_spellbook(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let mut seen = Vec::new();
        let defs = p
            .spells
            .slots
            .iter()
            .filter(|s| **s != 0)
            .filter(|s| if seen.contains(*s) { false } else { seen.push(**s); true })
            .filter_map(|s| self.ao.spells.get(*s as u32).map(|d| client_spell_def(*s, d)))
            .collect();
        self.send(id, &S2C::SpellBook { spells: defs });
        self.send(id, &S2C::SpellSlots { slots: p.spells.slots.clone() });
    }

    /// Hook (contract): the inventory system calls it when a spell scroll is used.
    /// `AgregarHechizo`: obj.dat `HechizoIndex` names the spell. Returns true if learned
    /// (the caller then consumes the scroll).
    #[allow(dead_code)]
    pub(crate) fn learn_spell_from_item(&mut self, id: u32, obj: u16) -> bool {
        let Some(spell) = self.ao.objs.get(obj as u32).map(|o| o.spell_index) else { return false };
        if spell <= 0 || self.ao.spells.get(spell as u32).is_none() || self.player(id).is_none() {
            return false;
        }
        let spell = spell as u16;
        if !self.class_may_use_spell(id, spell) {
            self.console(id, "Tu clase no puede aprender ese hechizo.", Tone::Warn);
            return false;
        }
        if self.player(id).unwrap().spells.knows(spell) {
            self.console(id, "Ya tenés ese hechizo.", Tone::Warn);
            return false;
        }
        if self.player_mut(id).spells.add(spell).is_none() {
            self.console(id, "No tenés espacio para más hechizos.", Tone::Warn);
            return false;
        }
        let name = self.ao.spells.get(spell as u32).unwrap().name.clone();
        self.console(id, format!("Has aprendido el hechizo {name}."), Tone::Good);
        self.send_spellbook(id);
        true
    }

    // ------------------------------------------------------------ status

    /// Movement must ask this before every step (`UserMod.CanMove`): paralysed or
    /// immobilised players cannot walk.
    #[allow(dead_code)]
    pub(crate) fn can_move(&self, id: u32) -> bool {
        let Some(p) = self.player(id) else { return false };
        let now = self.now();
        let on = |t: Option<GameTime>| t.is_some_and(|t| now < t);
        !(on(p.status.paralyzed_until) || on(p.status.immobilized_until))
    }

    /// Current `S2C::Status` bits of a player.
    pub(crate) fn status_flags(&self, id: u32) -> u32 {
        let Some(p) = self.player(id) else { return 0 };
        let now = self.now();
        let on = |t: Option<GameTime>| t.is_some_and(|t| now < t);
        let s = &p.status;
        let mut f = 0;
        let mut set = |c: bool, bit: u32| {
            if c {
                f |= bit;
            }
        };
        set(p.combat.dead || p.hp <= 0, status::DEAD);
        set(on(s.paralyzed_until), status::PARALYZED);
        set(on(s.invisible_until), status::INVISIBLE);
        set(s.poisoned, status::POISONED);
        set(s.meditating, status::MEDITATING);
        set(on(s.immobilized_until), status::IMMOBILIZED);
        set(on(s.blind_until), status::BLIND);
        set(on(s.dumb_until), status::DUMB);
        f
    }

    /// Broadcasts `S2C::Status` to the map if the flags changed since the last time.
    pub(crate) fn refresh_status(&mut self, id: u32) {
        let flags = self.status_flags(id);
        let Some(p) = self.player(id) else { return };
        // After a map change the new map must learn the flags too (if any are set).
        let moved = p.status.sent_map != p.map && flags != 0;
        if p.status.sent_flags == flags && !moved {
            return;
        }
        let map = p.map;
        let st = &mut self.player_mut(id).status;
        st.sent_flags = flags;
        st.sent_map = map;
        self.broadcast(map, &S2C::Status { id, flags }, None);
    }


    // ------------------------------------------------------------ world tick

    /// Called by `World::tick`: status expiry (`PasarSegundo`), poison (`EfectoVeneno`),
    /// effects over time and meditation (`DoMeditar`).
    pub(crate) fn tick_magic(&mut self, now: GameTime) {
        for id in self.player_ids() {
            let Some(p) = self.player(id) else { continue };
            if p.combat.dead || p.hp <= 0 {
                // UserDie clears states; the DEAD bit still reaches the map.
                let st = &mut self.player_mut(id).status;
                if st.poisoned || st.meditating || !st.eots.is_empty() || st.paralyzed_until.is_some() || st.immobilized_until.is_some() || st.invisible_until.is_some() {
                    let sent = (st.sent_flags, st.sent_map);
                    *st = Default::default();
                    (st.sent_flags, st.sent_map) = sent;
                }
                self.refresh_status(id);
                continue;
            }
            self.expire_status(id, now);
            self.tick_poison(id, now);
            self.tick_eots(id, now);
            self.tick_meditation(id, now);
            if self.player(id).is_some() {
                self.refresh_status(id);
            }
        }
    }

    fn expire_status(&mut self, id: u32, now: GameTime) {
        let p = self.player_mut(id);
        let s = &mut p.status;
        let mut msgs: Vec<&str> = Vec::new();
        let mut take = |t: &mut Option<GameTime>, msg: &'static str| {
            if t.is_some_and(|t| now >= t) {
                *t = None;
                msgs.push(msg);
            }
        };
        take(&mut s.paralyzed_until, "Ya no estás paralizado.");
        take(&mut s.immobilized_until, "Ya no estás inmovilizado.");
        take(&mut s.invisible_until, "Has vuelto a ser visible.");
        take(&mut s.blind_until, "Ya no estás ciego.");
        take(&mut s.dumb_until, "Ya no estás estúpido.");
        if s.paralysis_immune_until.is_some_and(|t| now >= t) {
            s.paralysis_immune_until = None;
        }
        for m in msgs {
            self.console(id, m, Tone::Info);
        }
    }

    fn tick_poison(&mut self, id: u32, now: GameTime) {
        let p = self.player(id).unwrap();
        if !p.status.poisoned || now < p.status.poison_next {
            return;
        }
        let max_hp = p.max_hp;
        let r = self.rng.range(3, 5);
        let dmg = rules::poison_damage(r, max_hp);
        self.player_mut(id).status.poison_next = now.after_ms(rules::POISON_PERIOD_MS);
        self.console(id, "Estás envenenado, si no te curas morirás.", Tone::Combat);
        self.damage_player(id, dmg, "El veneno");
    }

    fn tick_eots(&mut self, id: u32, now: GameTime) {
        if self.player(id).is_none_or(|p| p.status.eots.is_empty()) {
            return;
        }
        let table = eot_table(&self.recursos.clone());
        let mut i = 0;
        while let Some(e) = self.player(id).and_then(|p| p.status.eots.get(i).cloned()) {
            let Some(def) = table.get(&e.eot).cloned() else {
                self.player_mut(id).status.eots.remove(i);
                continue;
            };
            if e.ticks_left == 0 {
                self.player_mut(id).status.eots.remove(i);
                continue;
            }
            if now < e.next_tick {
                i += 1;
                continue;
            }
            {
                let eot = &mut self.player_mut(id).status.eots[i];
                eot.ticks_left -= 1;
                eot.next_tick = now.after_ms(def.tick_ms.max(1));
            }
            let amount = roll(self, def.power_min, def.power_max);
            let map = self.player(id).unwrap().map;
            if def.tick_fx > 0 {
                self.broadcast(map, &S2C::Fx { id, fx: def.tick_fx, loops: 0 }, None);
            }
            match def.kind {
                1 if amount >= 0 => {
                    self.heal_player(id, amount);
                }
                1 => {
                    let src = self.player(e.source).map_or_else(|| "Un efecto".to_string(), |p| p.name.clone());
                    self.damage_player(id, -amount, &src);
                    if self.player(id).is_none_or(|p| p.status.eots.len() <= i) {
                        return;
                    }
                }
                9 => {
                    let p = self.player_mut(id);
                    p.mana = (p.mana + amount).clamp(0, p.max_mana);
                    self.send_stats(id);
                }
                _ => {}
            }
            i += 1;
        }
    }

    // ------------------------------------------------------------ meditation

    /// `HandleMeditate`: toggles meditation.
    pub(crate) fn on_meditate(&mut self, id: u32) {
        let p = self.player(id).unwrap();
        if p.status.meditating {
            return self.stop_meditating(id);
        }
        if p.max_mana <= 0 || !rules::can_meditate(p.class) {
            return self.console(id, "Tu clase no usa maná: no puede meditar.", Tone::Warn);
        }
        if p.mana >= p.max_mana {
            return self.console(id, "Tu maná ya está completo.", Tone::Info);
        }
        if !rules::can_meditate(p.class) {
            return self.console(id, "Tu clase no puede meditar.", Tone::Warn);
        }
        if p.combat.dead || p.hp <= 0 {
            return self.console(id, "¡¡Estás muerto!!", Tone::Warn);
        }
        let now = self.now();
        let fx = ao_formats::profile::for_recursos(&self.recursos).meditate_fx(p.level).unwrap_or_else(|| rules::meditation_fx(p.level, false));
        let (map, x, y) = (p.map, p.x, p.y);
        let anchor = (map, x, y, p.combat.last_attack.unwrap_or(GameTime::ZERO));
        let s = &mut self.player_mut(id).status;
        s.meditating = true;
        s.meditate_fx = fx;
        s.meditate_next = now.after_ms(rules::MEDITATE_WARMUP_MS);
        s.meditate_anchor = anchor;
        self.broadcast(map, &S2C::Fx { id, fx, loops: 255 }, None);
        self.console(id, "Comenzás a meditar.", Tone::Info);
        self.refresh_status(id);
    }

    /// Stops meditation (moving, attacking, casting, full mana). Public for other
    /// systems; magic also notices moves and attacks by itself on its tick.
    pub(crate) fn stop_meditating(&mut self, id: u32) {
        let Some(p) = self.player(id) else { return };
        if !p.status.meditating {
            return;
        }
        let map = p.map;
        let s = &mut self.player_mut(id).status;
        s.meditating = false;
        s.meditate_fx = 0;
        self.broadcast(map, &S2C::Fx { id, fx: 0, loops: 0 }, None);
        self.console(id, "Dejas de meditar.", Tone::Info);
        self.refresh_status(id);
    }

    fn tick_meditation(&mut self, id: u32, now: GameTime) {
        let p = self.player(id).unwrap();
        if !p.status.meditating {
            return;
        }
        // Moving or attacking breaks meditation (VB: HandleWalk / attacks clear Meditando).
        if p.status.meditate_anchor != (p.map, p.x, p.y, p.combat.last_attack.unwrap_or(GameTime::ZERO)) {
            return self.stop_meditating(id);
        }
        if now < p.status.meditate_next {
            return;
        }
        let e = &self.ao.balance.extra;
        let gain = rules::meditation_mana(p.max_mana, p.stats.skills[SKILL_MEDITATE] as i32, e.mana_recovery_percent, e.recovery_mana, e.mana_per_skill_multiplier);
        let p = self.player_mut(id);
        p.status.meditate_next = now.after_ms(rules::MEDITATE_PULSE_MS);
        let full = p.mana + gain >= p.max_mana;
        p.mana = (p.mana + gain).min(p.max_mana);
        self.send_stats(id);
        if full {
            self.stop_meditating(id);
        }
    }

    // ------------------------------------------------------------ casting

    pub(crate) fn on_cast(&mut self, id: u32, spell_id: u16, tx: i32, ty: i32, seq: u32) {
        if !self.player_mut(id).seq.accept(seq) {
            return self.strike(id, "paquete repetido (replay)");
        }
        let Some(spell) = self.ao.spells.get(spell_id as u32).cloned() else {
            return self.strike(id, "hechizo inexistente");
        };
        let now = self.now();
        let p = self.player(id).unwrap();
        if !p.spells.knows(spell_id) {
            return self.strike(id, "hechizo que no conoce");
        }
        let (dx, dy) = ((tx - p.x).abs(), (ty - p.y).abs());
        if dx > rules::CHEAT_RANGE_X || dy > rules::CHEAT_RANGE_Y {
            return self.strike(id, "hechizo fuera de rango");
        }
        if p.combat.dead || p.hp <= 0 {
            return self.console(id, "¡¡Estás muerto!!", Tone::Warn);
        }
        if dx > rules::VIEW_RANGE_X || dy > rules::VIEW_RANGE_Y {
            return self.console(id, "Estás demasiado lejos.", Tone::Warn);
        }
        if now < p.spells.next_cast {
            // The client mirrors the interval per spell (SpellDef.cooldown_ms): repeating
            // the same one early is a modified client; mixing two is just a fast hand.
            if p.spells.cooldowns.get(&spell_id).is_some_and(|t| now < *t) && spell.cooldown <= 0 {
                self.console(id, "Debes esperar para volver a lanzar un hechizo.", Tone::Warn);
                return self.strike(id, "hechizo antes del intervalo");
            }
            return self.console(id, "Debes esperar para volver a lanzar un hechizo.", Tone::Warn);
        }
        // HandleWorkLeftClick: meditation ends, then IntervaloPermiteLanzarSpell updates
        // the timer whether or not the spell is finally cast.
        self.stop_meditating(id);
        self.player_mut(id).spells.next_cast = now.after_ms(rules::CAST_INTERVAL_MS);
        let Some(target) = self.resolve_target(id, &spell, tx, ty) else { return };
        if !self.can_cast(id, spell_id, &spell, target) {
            return;
        }
        let ok = match target {
            Target::User(t) => self.cast_on_user(id, spell_id, &spell, t),
            Target::Npc(n) => self.cast_on_npc(id, &spell, n),
        };
        if ok {
            self.after_cast(id, spell_id, &spell, target);
        }
    }

    /// `LanzarHechizo`: what is on the tile, by `Hechizos.Target`.
    fn resolve_target(&mut self, id: u32, spell: &Spell, tx: i32, ty: i32) -> Option<Target> {
        let map = self.player(id).unwrap().map;
        if spell.auto_cast != 0 {
            return Some(Target::User(id));
        }
        let user = self.player_at(map, tx, ty);
        let npc = self.instance(map).npc_at(tx, ty);
        let (t, msg) = match spell.target {
            SpellTarget::Users => (user.map(Target::User), "Este hechizo actúa sólo sobre usuarios."),
            SpellTarget::Npcs => (npc.map(Target::Npc), "Este hechizo sólo afecta a los npcs."),
            SpellTarget::UsersAndNpcs => (user.map(Target::User).or(npc.map(Target::Npc)), "Target inválido."),
            // Summons, area, terrain and pet spells (HechizoInvocacion, HechizoSobreArea,
            // HandleHechizoTerreno, HandlePetSpell) need NPC ownership: not in P1.
            _ => (None, "Ese hechizo todavía no está disponible."),
        };
        if t.is_none() {
            self.console(id, msg, Tone::Warn);
        }
        t
    }

    fn is_safe_for_fight(&self, a: u32, b: u32) -> bool {
        let (Some(pa), Some(pb)) = (self.player(a), self.player(b)) else { return true };
        let m = &self.instance(pa.map).map;
        if m.trigger(pa.x, pa.y) == TRIGGER_ARENA && m.trigger(pb.x, pb.y) == TRIGGER_ARENA {
            return false;
        }
        m.safe || m.trigger(pa.x, pa.y) == TRIGGER_SAFE || m.trigger(pb.x, pb.y) == TRIGGER_SAFE
    }

    /// `PuedeAtacar` / `CanAttackUser` for a harmful spell (factions, guilds, groups and
    /// the citizen "seguro" are not in P1: everyone may fight outside safe zones).
    fn spell_can_hit_user(&self, caster: u32, target: u32) -> Result<(), &'static str> {
        let t = self.player(target).unwrap();
        if t.combat.dead || t.hp <= 0 {
            return Err("No podés atacar a un espíritu.");
        }
        if caster == target {
            return Err("No podés atacarte a vos mismo.");
        }
        if self.is_safe_for_fight(caster, target) {
            return Err("Esta es una zona segura, aquí no podés atacar otros usuarios.");
        }
        match self.faction_attack_refusal(caster, target) {
            Some(why) => Err(why),
            None => Ok(()),
        }
    }

    /// TSAO `ExclusivoClase`: checked when learning AND casting. The VB cast check joined
    /// the two class conditions with `And`, so it never refused anyone.
    fn class_may_use_spell(&self, id: u32, spell: u16) -> bool {
        let (Some(p), Some(s)) = (self.player(id), self.ao.spells.get(spell as u32)) else { return false };
        s.exclusive_classes.is_empty() || s.exclusive_classes.iter().any(|c| *c as u8 == p.class)
    }

    /// `PuedeLanzar` (the parts that exist in P1).
    fn can_cast(&mut self, id: u32, spell_id: u16, s: &Spell, target: Target) -> bool {
        let now = self.now();
        let refuse = |w: &World, msg: String| {
            w.console(id, msg, Tone::Warn);
            false
        };
        if !self.class_may_use_spell(id, spell_id) {
            return refuse(self, "Tu clase no puede lanzar ese hechizo.".into());
        }
        let p = self.player(id).unwrap();
        if s.require_transform > 0 {
            return refuse(self, "Ese hechizo requiere una transformación.".into());
        }
        if s.needs_obj > 0 {
            let has = |o: i32| p.inv.slots.iter().flatten().any(|it| it.obj as i32 == o);
            if !has(s.needs_obj) && !has(s.needs_obj2) {
                let name = self.ao.objs.get(s.needs_obj as u32).map_or("objeto".to_string(), |o| o.name.clone());
                return refuse(self, format!("Necesitas un {name} para lanzar el hechizo."));
            }
        }
        if let Target::User(t) = target {
            if s.target_effect == TargetEffect::Negative {
                if let Err(m) = self.spell_can_hit_user(id, t) {
                    return refuse(self, m.into());
                }
            }
        }
        if s.cooldown > 0 {
            if let Some(t) = p.spells.cooldowns.get(&spell_id).filter(|t| now < **t) {
                let secs = t.since(now).div_ceil(1000);
                return refuse(self, format!("Debes esperar {secs} segundos para volver a tirar este hechizo."));
            }
        }
        if (p.stats.skills[SKILL_MAGIC] as i32) < s.min_skill {
            return refuse(self, format!("No tienes suficientes puntos de magia para lanzar este hechizo, necesitas {} puntos.", s.min_skill));
        }
        if s.max_level_castable > 0 && p.level as i32 > s.max_level_castable {
            return refuse(self, format!("Para lanzar este hechizo debes ser nivel {} o inferior.", s.max_level_castable));
        }
        if p.hp < s.required_hp {
            return refuse(self, format!("No tienes suficiente vida. Necesitas {} puntos de vida.", s.required_hp));
        }
        if p.mana < s.mana_required {
            return refuse(self, "No tienes suficiente maná.".into());
        }
        if p.stats.sta < rules::stamina_cost(s.stamina_required, s.stamina_percent_required, p.stats.max_sta) {
            return refuse(self, "Estás muy cansado para lanzar este hechizo.".into());
        }
        let eq = &p.inv.equipped;
        let r = s.requirements;
        if r.contains(Req::WEAPON) && eq.weapon.is_none()
            || r.contains(Req::SHIELD) && eq.shield.is_none()
            || r.contains(Req::ARMOR) && eq.armor.is_none()
            || r.contains(Req::HELM) && eq.helmet.is_none()
        {
            return refuse(self, "No tenés el equipo necesario para lanzar este hechizo.".into());
        }
        true
    }

    /// `HandleHechizoUsuario` minus the costs (paid in `after_cast`).
    fn cast_on_user(&mut self, id: u32, spell_id: u16, s: &Spell, t: u32) -> bool {
        if s.eot_id > 0 {
            let table = eot_table(&self.recursos.clone());
            let def = table.get(&(s.eot_id as u16)).cloned();
            let has = self.player(t).unwrap().status.eots.iter().any(|e| e.eot == s.eot_id as u16);
            if has && !def.as_ref().is_some_and(|d| d.override_) {
                self.console(id, "El objetivo ya está afectado por ese efecto.", Tone::Warn);
                return false;
            }
        }
        let ok = match s.spell_type {
            SpellType::Status => self.status_on_user(id, s, t),
            SpellType::Properties => self.props_on_user(id, s, t),
            // HechizoCombinados applies both families.
            SpellType::Combined => {
                let a = self.status_on_user(id, s, t);
                let b = self.player(t).is_some() && self.props_on_user(id, s, t);
                a || b
            }
            _ => {
                self.console(id, "Ese hechizo todavía no está disponible.", Tone::Warn);
                false
            }
        };
        if ok && s.eot_id > 0 && self.player(t).is_some_and(|p| !p.combat.dead && p.hp > 0) {
            self.start_eot(id, t, s.eot_id as u16);
        }
        let _ = spell_id;
        ok
    }

    fn start_eot(&mut self, source: u32, t: u32, eot: u16) {
        let table = eot_table(&self.recursos.clone());
        let Some(def) = table.get(&eot) else { return };
        if !matches!(def.kind, 1 | 9) {
            // Stat modifiers, taunts, transforms... (other IBaseEffectOverTime classes).
            return;
        }
        let now = self.now();
        let e = ActiveEot { eot, source, ticks_left: def.ticks, next_tick: now.after_ms(def.tick_ms.max(1)) };
        let eots = &mut self.player_mut(t).status.eots;
        if let Some(old) = eots.iter_mut().find(|x| x.eot == eot) {
            *old = e;
        } else {
            eots.push(e);
        }
    }

    /// `HechizoEstadoUsuario` (the effects present in P1).
    fn status_on_user(&mut self, id: u32, s: &Spell, t: u32) -> bool {
        let now = self.now();
        let fx = s.effects;
        let mut ok = false;
        let warn = |w: &World, m: &str| w.console(id, m, Tone::Warn);
        let target_dead = |w: &World| w.player(t).is_none_or(|p| p.combat.dead || p.hp <= 0);

        if fx.contains(Fx::INVISIBILITY) {
            if target_dead(self) {
                warn(self, "¡El usuario está muerto!");
                return false;
            }
            if self.player(t).unwrap().status.invisible_until.is_some_and(|u| now < u) {
                warn(self, "El objetivo ya es invisible.");
                return false;
            }
            let secs = roll(self, s.duration - 4, s.duration + 1).max(1) as u64;
            self.player_mut(t).status.invisible_until = Some(now.after_secs(secs));
            ok = true;
        }
        if s.poison > 0 {
            if let Err(m) = self.spell_can_hit_user(id, t) {
                warn(self, m);
                return false;
            }
            let st = &mut self.player_mut(t).status;
            if !st.poisoned {
                st.poisoned = true;
                // EfectoVeneno counts up from Counters.Veneno = Duration to IntervaloVeneno.
                let left = rules::POISON_INTERVAL_TICKS.saturating_sub(s.duration.max(0) as u64) + 1;
                st.poison_next = now.after_ms(left * rules::GAME_EVENTS_MS);
                self.console(t, "Has sido envenenado.", Tone::Combat);
            }
            ok = true;
        }
        if fx.contains(Fx::CURE_POISON) {
            if target_dead(self) {
                warn(self, "¡El usuario está muerto!");
                return false;
            }
            if !self.player(t).unwrap().status.poisoned {
                warn(self, "El usuario no está envenenado.");
                return false;
            }
            self.player_mut(t).status.poisoned = false;
            self.console(t, "Te has curado del envenenamiento.", Tone::Good);
            ok = true;
        }
        if fx.contains(Fx::PARALYZE) || fx.contains(Fx::IMMOBILIZE) {
            let para = fx.contains(Fx::PARALYZE);
            if id == t {
                return false;
            }
            if self.player(t).unwrap().status.paralysis_immune_until.is_some_and(|u| now < u) {
                return false;
            }
            if !self.can_move(t) {
                warn(self, "No podés inmovilizar un objetivo que no puede moverse.");
                return false;
            }
            if let Err(m) = self.spell_can_hit_user(id, t) {
                warn(self, m);
                return false;
            }
            let class = self.player(t).unwrap().class;
            let until = Some(now.after_secs(rules::paralysis_secs(s.duration, class).max(0) as u64));
            let st = &mut self.player_mut(t).status;
            if para {
                st.paralyzed_until = until;
            } else {
                st.immobilized_until = until;
            }
            self.console(t, if para { "Has sido paralizado." } else { "Has sido inmovilizado." }, Tone::Combat);
            ok = true;
        }
        if fx.contains(Fx::REMOVE_PARALYSIS) {
            let st = &self.player(t).unwrap().status;
            let (p, i) = (st.paralyzed_until.is_some_and(|u| now < u), st.immobilized_until.is_some_and(|u| now < u));
            if !p && !i {
                warn(self, "El usuario no está paralizado.");
                return false;
            }
            let class = self.player(t).unwrap().class;
            let st = &mut self.player_mut(t).status;
            st.paralyzed_until = None;
            st.immobilized_until = None;
            if matches!(class, rules::class::WARRIOR | rules::class::THIEF | rules::class::PIRATE) {
                st.paralysis_immune_until = Some(now.after_secs(4));
            }
            self.console(t, "Ya puedes moverte.", Tone::Good);
            ok = true;
        }
        if fx.contains(Fx::REMOVE_DUMB) && self.player(t).unwrap().status.dumb_until.is_some_and(|u| now < u) {
            self.player_mut(t).status.dumb_until = None;
            ok = true;
        }
        if fx.contains(Fx::RESURRECT) {
            if !target_dead(self) {
                return false;
            }
            let p = self.player(id).unwrap();
            if p.class != rules::class::CLERIC {
                let eq = p.inv.equipped;
                let revive = [eq.weapon, eq.ring, eq.amulet]
                    .into_iter()
                    .flatten()
                    .filter_map(|slot| p.inv.get(slot))
                    .any(|it| self.ao.objs.get(it.obj as u32).is_some_and(|o| o.revive));
                if !revive {
                    warn(self, "Necesitás un objeto con mayor poder mágico para poder revivir.");
                    return false;
                }
            }
            if p.stats.sta < p.stats.max_sta {
                warn(self, "Deberás tener la barra de energía llena para poder resucitar.");
                return false;
            }
            self.resurrect(t);
            ok = true;
        }
        if fx.contains(Fx::BLINDNESS) || fx.contains(Fx::DUMB) {
            let blind = fx.contains(Fx::BLINDNESS);
            if id == t {
                return false;
            }
            if let Err(m) = self.spell_can_hit_user(id, t) {
                warn(self, m);
                return false;
            }
            let until = Some(now.after_secs(s.duration.max(0) as u64));
            let st = &mut self.player_mut(t).status;
            if blind {
                st.blind_until = until;
            } else if st.dumb_until.is_none_or(|u| now >= u) {
                st.dumb_until = until;
            }
            ok = true;
        }
        ok
    }

    /// Equipped items that add magic damage (weapon, amulet, ring), obj.dat values.
    fn magic_items(&self, id: u32) -> Vec<MagicItem> {
        let p = self.player(id).unwrap();
        let eq = p.inv.equipped;
        [eq.weapon, eq.amulet, eq.ring]
            .into_iter()
            .flatten()
            .filter_map(|slot| p.inv.get(slot))
            .filter_map(|it| self.ao.objs.get(it.obj as u32))
            .map(|o| MagicItem { damage_bonus: o.magic_damage_bonus, absolute_bonus: o.magic_absolute_bonus, penetration: o.magic_penetration })
            .collect()
    }

    /// `HechizoPropUsuario`: HP, mana and stamina (hunger, thirst and timed attribute
    /// buffs are not used by any spell this server can cast yet).
    fn props_on_user(&mut self, id: u32, s: &Spell, t: u32) -> bool {
        let mut ok = false;
        let caster_name = self.player(id).unwrap().name.clone();
        if s.effects.contains(Fx::DO_HEAL) {
            let tp = self.player(t).unwrap();
            if tp.combat.dead || tp.hp <= 0 {
                self.console(id, "¡¡Estás muerto!!", Tone::Warn);
                return false;
            }
            if tp.hp >= tp.max_hp {
                self.console(id, "El usuario no tiene heridas.", Tone::Warn);
                return false;
            }
            let amount = roll(self, s.hp.min, s.hp.max);
            let healed = self.heal_player(t, amount);
            if id != t {
                let tname = self.player(t).unwrap().name.clone();
                self.console(id, format!("Le has restaurado {healed} puntos de vida a {tname}."), Tone::Good);
                self.console(t, format!("{caster_name} te ha restaurado {healed} puntos de vida."), Tone::Good);
            } else {
                self.console(id, format!("Te has restaurado {healed} puntos de vida."), Tone::Good);
            }
            ok = true;
        } else if s.effects.contains(Fx::DO_DAMAGE) {
            if let Err(m) = self.spell_can_hit_user(id, t) {
                self.console(id, m, Tone::Warn);
                return false;
            }
            let r = roll(self, s.hp.min, s.hp.max);
            let level = self.player(id).unwrap().level;
            let dmg = if self.tsao_rules() {
                self.tsao_spell_damage(id, s.staff_affected, r, Some(t))
            } else {
                let (dmg, pen) = rules::spell_damage(r, level, &self.magic_items(id));
                // GetUserMR (magic resistance from items/class) belongs to combat/inventory: 0.
                rules::damage_vs_user(dmg, 0, pen, s.anti_magic_resistance != 0)
            };
            let tname = self.player(t).unwrap().name.clone();
            self.console(id, format!("Le has quitado {dmg} puntos de vida a {tname}."), Tone::Combat);
            // As a player hit (not a named source): faction and kill credit apply to spells too.
            self.hurt_player(t, dmg, super::combat::Source::Player(id));
            ok = true;
        }
        if self.player(t).is_none() {
            return ok;
        }
        for (change, is_mana) in [(s.mana, true), (s.stamina, false)] {
            if change.mode != 1 && change.mode != 2 {
                continue;
            }
            if change.mode == 2 {
                if let Err(m) = self.spell_can_hit_user(id, t) {
                    self.console(id, m, Tone::Warn);
                    continue;
                }
            }
            let amount = roll(self, change.min, change.max) * if change.mode == 1 { 1 } else { -1 };
            let p = self.player_mut(t);
            if is_mana {
                p.mana = (p.mana + amount).clamp(0, p.max_mana);
            } else {
                p.stats.sta = (p.stats.sta + amount).clamp(0, p.stats.max_sta);
            }
            self.send_stats(t);
            self.send_vitals(t);
            ok = true;
        }
        ok
    }

    /// `HandleHechizoNPC` -> `HechizoPropNPC`. Status spells on creatures (paralysis...)
    /// need fields on `Npc` (NPC system): not in P1.
    fn cast_on_npc(&mut self, id: u32, s: &Spell, nid: u32) -> bool {
        let map = self.player(id).unwrap().map;
        let Some(n) = self.instance(map).npcs.get(&nid) else { return false };
        let (hp, max_hp, name) = (n.hp, n.max_hp, n.name.clone());
        match s.spell_type {
            SpellType::Properties | SpellType::Combined => {}
            _ => {
                self.console(id, "Ese hechizo todavía no afecta a las criaturas.", Tone::Warn);
                return false;
            }
        }
        if s.effects.contains(Fx::DO_HEAL) {
            if hp >= max_hp {
                self.console(id, "La criatura no tiene heridas que curar, el hechizo no tiene efecto.", Tone::Warn);
                return false;
            }
            let amount = roll(self, s.hp.min, s.hp.max).min(max_hp - hp);
            let n = self.maps.get_mut(&map).unwrap().npcs.get_mut(&nid).unwrap();
            n.hp += amount;
            let (hp, max_hp) = (n.hp, n.max_hp);
            self.broadcast(map, &S2C::Hp { id: nid, hp, max_hp }, None);
            self.console(id, format!("Le has restaurado {amount} puntos de vida a la criatura."), Tone::Good);
            return true;
        }
        if s.effects.contains(Fx::DO_DAMAGE) {
            let r = roll(self, s.hp.min, s.hp.max);
            let level = self.player(id).unwrap().level;
            let dmg = if self.tsao_rules() {
                self.tsao_spell_damage(id, s.staff_affected, r, None)
            } else {
                let (dmg, pen) = rules::spell_damage(r, level, &self.magic_items(id));
                // npcs.dat MagicResistance/MagicDef/DefM are not on `Npc` yet (NPC system).
                let skill = self.player(id).unwrap().stats.skills[SKILL_MAGIC] as i32;
                rules::damage_vs_npc(dmg, Default::default(), skill, pen, s.anti_magic_resistance != 0)
            };
            let _ = name;
            self.damage_npc(map, nid, dmg, id);
            return true;
        }
        self.console(id, "Ese hechizo todavía no afecta a las criaturas.", Tone::Warn);
        false
    }

    /// Costs and presentation of a successful cast (`HandleHechizoUsuario`/`NPC` tail,
    /// `InfoHechizo`, `LanzarHechizo` cooldown).
    fn after_cast(&mut self, id: u32, spell_id: u16, s: &Spell, target: Target) {
        let now = self.now();
        let caster_map_safe = {
            let p = self.player(id).unwrap();
            self.instance(p.map).map.safe
        };
        let resurrect_cost = match target {
            Target::User(t) if s.effects.contains(Fx::RESURRECT) && !caster_map_safe => {
                let lvl = self.player(t).map_or(1, |p| p.level);
                Some(rules::resurrect_hp_cost(lvl, self.player(id).unwrap().hp))
            }
            _ => None,
        };
        let (map, px, py) = {
            let p = self.player_mut(id);
            p.mana = (p.mana - s.mana_required).max(0);
            if s.required_hp > 0 {
                p.hp = (p.hp - s.required_hp).max(1);
            }
            if let Some(c) = resurrect_cost {
                p.hp = (p.hp - c).max(1);
            }
            let cost = rules::stamina_cost(s.stamina_required, s.stamina_percent_required, p.stats.max_sta);
            p.stats.sta = (p.stats.sta - cost).max(0);
            // Spells without CoolDown: remember the interval per spell, the way the
            // client mirrors it once it sees the Effect (repeating it early = cheat).
            let until = if s.cooldown > 0 { now.after_secs(s.cooldown as u64) } else { now.after_ms(rules::CAST_INTERVAL_MS) };
            p.spells.cooldowns.insert(spell_id, until);
            (p.map, p.x, p.y)
        };
        let (tid, tx, ty) = match target {
            Target::User(t) => self.player(t).map_or((t, px, py), |p| (t, p.x, p.y)),
            Target::Npc(n) => self.instance(map).npcs.get(&n).map_or((n, px, py), |n2| (n, n2.x, n2.y)),
        };
        self.broadcast(map, &S2C::Effect { spell: spell_id, from: id, tx, ty }, None);
        if s.fx > 0 {
            self.broadcast(map, &S2C::Fx { id: tid, fx: s.fx as u16, loops: s.loops.clamp(0, 255) as u8 }, None);
        }
        if let Some(p) = self.player(id) {
            let (hp, max_hp) = (p.hp, p.max_hp);
            self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
        }
        self.send_stats(id);
        self.send_vitals(id);
        if let Target::User(t) = target {
            if self.player(t).is_some() {
                self.refresh_status(t);
            }
        }
        // TODO(progression): SubirSkill(Magia) unless the spell is a physical skill.
    }

    /// Hook (contract): the NPC system wants creature `nid` on `map` to cast Hechizos.dat
    /// `spell` on player `pid` (`NpcLanzaSpellSobreUser`). The AI decides when (interval
    /// and range); magic applies the effect.
    #[allow(dead_code)]
    pub(crate) fn npc_cast_spell(&mut self, map: u16, nid: u32, pid: u32, spell: u16) {
        let Some(s) = self.ao.spells.get(spell as u32).cloned() else { return };
        let now = self.now();
        let Some(name) = self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).filter(|n| n.alive()).map(|n| n.name.clone()) else { return };
        let Some(p) = self.player(pid) else { return };
        if p.map != map || p.combat.dead || p.hp <= 0 || p.status.invisible_until.is_some_and(|u| now < u) {
            return;
        }
        let (px, py) = (p.x, p.y);
        self.broadcast(map, &S2C::Effect { spell, from: nid, tx: px, ty: py }, None);
        if s.fx > 0 {
            self.broadcast(map, &S2C::Fx { id: pid, fx: s.fx as u16, loops: s.loops.clamp(0, 255) as u8 }, None);
        }
        if s.effects.contains(Fx::DO_HEAL) {
            let amount = roll(self, s.hp.min, s.hp.max);
            if amount > 0 {
                self.heal_player(pid, amount);
            }
        } else if s.effects.contains(Fx::DO_DAMAGE) {
            // Npc MagicBonus and GetUserMRForNpc are not modelled yet: base roll.
            let dmg = roll(self, s.hp.min, s.hp.max).max(0);
            self.damage_player(pid, dmg, &name);
        }
        if self.player(pid).is_none_or(|p| p.combat.dead || p.hp <= 0) {
            return;
        }
        if s.eot_id > 0 {
            self.start_eot(0, pid, s.eot_id as u16);
        }
        let change = s.mana;
        if change.mode == 1 || change.mode == 2 {
            let amount = roll(self, change.min, change.max) * if change.mode == 1 { 1 } else { -1 };
            let p = self.player_mut(pid);
            p.mana = (p.mana + amount).clamp(0, p.max_mana);
            self.send_stats(pid);
        }
        let fx = s.effects;
        // NpcLanzaSpellSobreUser: creatures paralyse/immobilise for Duration / 2.
        let half = Some(now.after_secs(rules::vb_round(s.duration as f64 / 2.0).max(0) as u64));
        let st = &mut self.player_mut(pid).status;
        if fx.contains(Fx::PARALYZE) && st.paralyzed_until.is_none_or(|u| now >= u) {
            st.paralyzed_until = half;
        }
        if fx.contains(Fx::IMMOBILIZE) && st.immobilized_until.is_none_or(|u| now >= u) {
            st.immobilized_until = half;
        }
        if fx.contains(Fx::REMOVE_PARALYSIS) {
            st.paralyzed_until = None;
            st.immobilized_until = None;
        }
        if s.poison > 0 && !st.poisoned {
            st.poisoned = true;
            st.poison_next = now.after_ms((rules::POISON_INTERVAL_TICKS.saturating_sub(s.duration.max(0) as u64) + 1) * rules::GAME_EVENTS_MS);
        }
        if fx.contains(Fx::BLINDNESS) {
            st.blind_until = Some(now.after_secs(s.duration.max(0) as u64));
        }
        if fx.contains(Fx::DUMB) {
            st.dumb_until = Some(now.after_secs(s.duration.max(0) as u64));
        }
        self.refresh_status(pid);
    }
}

const _: () = assert!(MAX_SPELL_SLOTS == 40);

#[cfg(test)]
mod tests {
    //! Hooks and states that bots cannot reach yet (scrolls, NPC casts, movement gate),
    //! driving the world directly.
    use super::*;
    use crate::net::Event;
    use crate::Settings;
    use ao_protocol::{FrameReader, C2S, PROTOCOL_VERSION};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    struct Conn {
        rx: UnboundedReceiver<Vec<u8>>,
    }

    impl Conn {
        fn drain(&mut self) -> Vec<S2C> {
            let mut r = FrameReader::default();
            while let Ok(b) = self.rx.try_recv() {
                r.push(&b);
            }
            let mut out = Vec::new();
            while let Some(Ok(m)) = r.next::<S2C>(1 << 24) {
                out.push(m);
            }
            out
        }
    }

    fn world(start: (u16, i32, i32)) -> World {
        let mut s = Settings::defaults();
        s.start = Some(start);
        s.clock = crate::clock::Clock::Virtual(crate::clock::VirtualClock::new());
        let mut w = World::new(s);
        w.ensure_map(start.0);
        w
    }

    fn set_time(w: &World, ms: u64) {
        let crate::clock::Clock::Virtual(c) = &w.clock else { panic!("virtual clock") };
        c.set(GameTime(ms));
    }

    fn login(w: &mut World, id: u32, name: &str) -> Conn {
        let (tx, rx) = unbounded_channel();
        w.on_event(Event::Connected { id, addr: "127.0.0.1:1".parse().unwrap(), tx });
        w.handle(id, C2S::Login { name: name.into(), version: PROTOCOL_VERSION });
        assert!(w.player(id).is_some(), "dev login");
        // Test casters: seguro off (citizens log in with it on).
        w.player_mut(id).faction.safe = false;
        Conn { rx }
    }

    #[test]
    fn eot_file_parses() {
        let t = parse_eots("[INIT]\nEffectCount=2\n[EOT1] 'x\nType=1\nTickPowerMin=8\nTickPowerMax=12\nTicks=10\nTickTime=3000\nTickFX=144\nOverride=1\n[EOT2]\nType=1\nTickPowerMin=-15 'c\n");
        assert_eq!(t[&1], EotDef { kind: 1, power_min: 8, power_max: 12, ticks: 10, tick_ms: 3000, tick_fx: 144, override_: true });
        assert_eq!(t[&2].power_min, -15);
    }

    #[test]
    fn dev_kit_spells_pass_puede_lanzar() {
        let mut w = world((2, 50, 50));
        let mut c = login(&mut w, 1, "Probador");
        let _other = login(&mut w, 2, "Blanco");
        let sent = c.drain();
        assert!(sent.iter().any(|m| matches!(m, S2C::SpellSlots { slots } if slots[..DEV_KIT.len()] == DEV_KIT)));
        assert!(sent.iter().any(|m| matches!(m, S2C::SpellBook { spells } if spells.len() == DEV_KIT.len())));
        for s in DEV_KIT {
            let spell = w.ao.spells.get(s as u32).unwrap().clone();
            let target = if spell.target_effect == TargetEffect::Negative { 2 } else { 1 };
            assert!(w.can_cast(1, s, &spell, Target::User(target)), "{}", spell.name);
        }
    }

    #[test]
    fn scroll_teaches_its_spell_once() {
        let mut w = world((1, 50, 50));
        let mut c = login(&mut w, 1, "Lector");
        let (obj, spell) = w.ao.objs.iter().find(|(_, o)| o.spell_index > 0 && !DEV_KIT.contains(&(o.spell_index as u16)) && w.ao.spells.get(o.spell_index as u32).is_some()).map(|(i, o)| (i as u16, o.spell_index as u16)).expect("a scroll in obj.dat");
        c.drain();
        assert!(w.learn_spell_from_item(1, obj));
        assert!(w.player(1).unwrap().spells.knows(spell));
        assert!(c.drain().iter().any(|m| matches!(m, S2C::SpellSlots { slots } if slots.contains(&spell))));
        assert!(!w.learn_spell_from_item(1, obj), "already known: the scroll is not consumed");
        assert!(!w.learn_spell_from_item(1, 1), "obj 1 teaches nothing");
    }

    #[test]
    fn paralysis_blocks_movement_until_it_expires() {
        let mut w = world((2, 50, 50));
        let _a = login(&mut w, 1, "Mago");
        let _b = login(&mut w, 2, "Victima");
        let paralizar = w.ao.spells.get(25).unwrap().clone();
        assert!(w.can_move(2));
        assert!(w.status_on_user(1, &paralizar, 2));
        assert!(!w.can_move(2));
        let secs = paralizar.duration as u64;
        set_time(&w, secs * 1000 - 1);
        w.tick_magic(w.now());
        assert!(!w.can_move(2), "still paralysed just before Duration");
        set_time(&w, secs * 1000);
        w.tick_magic(w.now());
        assert!(w.can_move(2));
    }

    #[test]
    fn npc_cast_damages_and_paralyses() {
        let mut w = world((2, 50, 50));
        let _a = login(&mut w, 1, "Presa");
        let nid = *w.instance(2).npcs.keys().next().expect("creatures on map 2");
        let hp = w.player(1).unwrap().hp;
        w.npc_cast_spell(2, nid, 1, 1);
        assert!(w.player(1).unwrap().hp < hp, "Dardo Mágico hurts");
        w.npc_cast_spell(2, nid, 1, 25);
        assert!(!w.can_move(1), "a creature's Paralizar works for Duration / 2");
    }
}

//! Movement speed (`ActualizarVelocidadDeUsuario`) and mounts (`DoMontar`).
//!
//! Speed multiplies the walking pace: 1 normal, 1.4 for ghosts, the ship's or saddle's
//! `Velocidad` (times the rider level's `JineteLevelNSpeed`), the armour's own `Velocidad`.
//! The server paces walking with it (`WalkLimiter`) and tells the clients, which shorten
//! the step interval and the walk animation the same way.
use crate::clock::GameTime;
use crate::world::World;
use ao_formats::dat::obj::ObjType;
use ao_protocol::{Tone, S2C};

/// `VelocidadNormal` / `VelocidadMuerto` (Declares.bas).
pub const SPEED_NORMAL: f32 = 1.0;
pub const SPEED_DEAD: f32 = 1.4;
/// `JineteLevel1Speed` .. `JineteLevel9Speed` (Configuracion.ini).
pub const RIDER_LEVEL_SPEED: [f32; 9] = [1.05, 1.10, 1.15, 1.20, 1.25, 1.30, 1.35, 1.40, 1.45];

impl World {
    /// `ActualizarVelocidadDeUsuario`.
    pub(crate) fn compute_speed(&self, id: u32) -> f32 {
        let Some(p) = self.player(id) else { return SPEED_NORMAL };
        if self.is_dead(id) {
            return SPEED_DEAD;
        }
        let obj = |slot: Option<u8>| slot.and_then(|s| p.inv.get(s)).and_then(|it| self.ao.objs.get(it.obj as u32));
        let mut item = 1.0f32;
        let mut rider = 1.0f32;
        if let Some(ship) = p.status.sailing.and_then(|o| self.ao.objs.get(o as u32)) {
            item = positive(ship.speed);
        }
        if let Some(saddle) = obj(p.mounted) {
            item = positive(saddle.speed);
            let level = p.rider_level.clamp(1, 9) as usize;
            rider = RIDER_LEVEL_SPEED[level - 1];
        }
        if let Some(armor) = obj(p.inv.equipped.armor) {
            if armor.speed > 0.0 && (armor.speed - 1.0).abs() > f32::EPSILON {
                item *= armor.speed;
            }
        }
        // `max(0, 1 + Modifiers.MovementSpeed)` of the active effects over time.
        let effects = (1.0 + self.modifiers(super::eot::Who::User(id)).speed).max(0.0);
        (SPEED_NORMAL * item * rider * p.status.speed_spell.unwrap_or(1.0) * effects).max(0.1)
    }

    /// Recomputes `id`'s speed; on a change, paces its walking and tells the map.
    pub(crate) fn update_speed(&mut self, id: u32) {
        let speed = self.compute_speed(id);
        let Some(p) = self.player(id) else { return };
        if (p.speed - speed).abs() < 1e-4 {
            return;
        }
        let map = p.map;
        let p = self.player_mut(id);
        p.speed = speed;
        p.walk.set_step_ms(ao_rules::step_ms(speed));
        self.broadcast(map, &S2C::Speed { id, speed }, None);
    }

    /// Every few ticks: speeds follow deaths, boats, mounts, armour and spells.
    pub(crate) fn tick_speed(&mut self, now: GameTime) {
        if now < self.next_speed_check {
            return;
        }
        self.next_speed_check = now.after_ms(200);
        for id in self.player_ids() {
            // A ghost, or a saddle no longer in its slot, is off the mount.
            let gone = self.player(id).and_then(|p| p.mounted.map(|s| p.inv.get(s).and_then(|it| self.ao.objs.get(it.obj as u32)).is_none_or(|o| o.obj_type != ObjType::Saddles)));
            if self.is_dead(id) && self.player(id).is_some_and(|p| p.mounted.is_some()) || gone == Some(true) {
                self.dismount(id);
            }
            self.update_speed(id);
        }
    }

    /// Speeds other than normal on the map, for a player who just arrived.
    pub(crate) fn send_speeds(&self, id: u32, map: u16) {
        for pid in &self.instance(map).players {
            if let Some(p) = self.player(*pid).filter(|p| (p.speed - SPEED_NORMAL).abs() > 1e-4) {
                self.send(id, &S2C::Speed { id: *pid, speed: p.speed });
            }
        }
    }

    /// `otSaddles` -> `DoMontar`: mounts (or dismounts) with the saddle in `slot`.
    pub(crate) fn use_saddle(&mut self, id: u32, slot: u8) {
        let ao = self.ao.clone();
        let Some(p) = self.player(id) else { return };
        let Some(saddle) = p.inv.get(slot).and_then(|it| ao.objs.get(it.obj as u32)).filter(|o| o.obj_type == ObjType::Saddles) else { return };
        if self.is_dead(id) || p.status.sailing.is_some() {
            return;
        }
        if p.mounted.is_some() {
            return self.dismount(id);
        }
        if self.instance(p.map).map.zone.eq_ignore_ascii_case("DUNGEON") {
            return self.console(id, "No podés montar en un calabozo.", Tone::Warn);
        }
        if p.stats.timers.combat_secs > 0 {
            return self.console(id, "No podés montar estando en combate.", Tone::Warn);
        }
        // Faction saddles (`Real` / `Caos`): status 3 royal army, 2 chaos legion.
        let status = self.faction_status(id) as i32;
        if (saddle.royal != 0 && status != 3) || (saddle.chaos != 0 && status != 2) {
            return self.console(id, "Esta montura no es para tu facción.", Tone::Warn);
        }
        // Mounting reveals the hidden and the invisible, and ends meditation.
        self.reveal(id);
        // ObtenerRopaje: `RazaBajos` for gnomes and dwarves, `RazaAltos` for the rest.
        let short = matches!(self.player(id).map(|p| p.race), Some(4 | 5));
        let body = if short { saddle.clothing_short } else { saddle.clothing_tall };
        let body = if body > 0 { body } else { saddle.clothing };
        let p = self.player_mut(id);
        p.status.invisible_until = None;
        p.mounted = Some(slot);
        if body > 0 {
            p.body = body as u16;
        }
        self.stop_meditating(id);
        self.send_look(id);
        self.refresh_status(id);
        self.update_speed(id);
        self.console(id, "Has montado.", Tone::Good);
    }

    /// Back on foot: the armour's body (or the naked one) again.
    pub(crate) fn dismount(&mut self, id: u32) {
        let Some(p) = self.player(id) else { return };
        if p.mounted.is_none() {
            return;
        }
        let prof = ao_formats::profile::for_recursos(&self.recursos);
        let ao = self.ao.clone();
        let p = self.player_mut(id);
        p.mounted = None;
        if !p.combat.dead {
            let armor = p.inv.equipped.armor.and_then(|s| p.inv.get(s)).and_then(|i| ao.objs.get(i.obj as u32)).map(|o| o.clothing.max(0) as u16).filter(|b| *b > 0);
            p.body = armor.unwrap_or_else(|| prof.naked_body(p.race, p.gender));
        }
        self.send_look(id);
        self.update_speed(id);
        self.console(id, "Has desmontado.", Tone::Info);
    }
}

fn positive(v: f32) -> f32 {
    if v > 0.0 { v } else { 1.0 }
}

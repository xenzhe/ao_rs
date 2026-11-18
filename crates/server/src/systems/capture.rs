//! Capture the flag: port of `ModCaptura.bas`, `clsCaptura.cls` and
//! `clsCapturaParticipante.cls`, started by `HandleIniciarCaptura`
//! (`/CREAREVENTO CAPTURA participantes rondas nivel_min nivel_max precio`).
//!
//! Inscriptions last 90 s (`/PARTICIPAR` from a safe zone, paying the fee; once extended
//! by 30 s if fewer than half came). Players are dealt into blue (map 275) and red (277)
//! by level; each round starts after 60 s in the waiting room (278). Whoever stays 7
//! seconds near their own stand holding the enemy banner wins the round; the dead wait in
//! the room 5 s + 2 s per death. Best of N rounds; the winners get back their fee plus
//! one loser's fee.
//!
//! VB defects fixed: a disconnected player kept the enemy banner in the inventory (it is
//! dropped where they were); an odd number of players cancelled the event without giving
//! the fees back; the loser's fees were lost when a team had fewer players than the other
//! (only the collected gold is paid out now); `muereUsuario` revived without moving the
//! dead out of the fight when they had no team.
use crate::clock::GameTime;
use crate::world::inventory::InvItem;
use crate::world::World;
use ao_protocol::{Tone, S2C};
use ao_rules::events::capture as c;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureState {
    Signup,
    RoundStart,
    Playing,
    Finished,
}

/// `clsCapturaParticipante` (+ the `jugando_captura*` flags).
#[derive(Debug, Clone)]
pub struct Participant {
    pub name: String,
    pub id: Option<u32>,
    pub back: (u16, i32, i32),
    /// 1 blue, 2 red (0 before teams are dealt).
    pub team: u8,
    pub deaths: u32,
    /// Seconds left in the waiting room after dying.
    pub timer: i32,
}

/// `clsCaptura`.
#[derive(Debug)]
pub struct Capture {
    pub participants: Vec<Participant>,
    pub rounds: u32,
    pub min_level: u32,
    pub max_level: u32,
    pub max: usize,
    pub wait: i32,
    pub round_wait: i32,
    pub fee: u32,
    pub pot: u64,
    pub round: u32,
    pub retries: u32,
    pub state: CaptureState,
    pub flag_count: [i32; 2],
    pub wins: [u32; 2],
    pub next_sec: GameTime,
}

impl World {
    pub(crate) fn capture_has(&self, id: u32) -> bool {
        self.capture.as_ref().is_some_and(|cap| cap.participants.iter().any(|p| p.id == Some(id)))
    }

    pub(crate) fn capture_team(&self, id: u32) -> Option<u8> {
        self.capture.as_ref()?.participants.iter().find(|p| p.id == Some(id)).map(|p| p.team).filter(|t| *t > 0)
    }

    /// The line `/SALAS` shows while inscriptions are open.
    pub(crate) fn capture_open(&self) -> Option<String> {
        let cap = self.capture.as_ref().filter(|cap| cap.state == CaptureState::Signup)?;
        Some(format!("Captura de la bandera: niveles {}-{}, {}/{} inscriptos, inscripción {} oro. /PARTICIPAR", cap.min_level, cap.max_level, cap.participants.len(), cap.max, cap.fee))
    }

    fn capture_say(&self, text: impl Into<String>) {
        let text = text.into();
        if let Some(cap) = &self.capture {
            for id in cap.participants.iter().filter_map(|p| p.id) {
                self.console(id, text.clone(), Tone::Info);
            }
        }
    }

    /// `HandleIniciarCaptura` + `inicializar`.
    pub(crate) fn capture_start(&mut self, max: u32, rounds: u32, min_level: u32, max_level: u32, fee: i64) -> Result<(), &'static str> {
        if self.capture.is_some() {
            return Err("Ya hay un evento de captura de bandera en curso.");
        }
        if fee < 0 {
            return Err("El precio de inscripción no puede ser negativo.");
        }
        if max > 48 {
            return Err("La cantidad de participantes no podrá ser mayor que 48.");
        }
        if !(1..=47).contains(&min_level) {
            return Err("El nivel mínimo debe estar entre 1 y 47.");
        }
        if min_level > max_level {
            return Err("El nivel mínimo debe ser menor al máximo.");
        }
        for m in [c::WAIT_MAP, c::TEAM_MAP[0], c::TEAM_MAP[1]] {
            if !self.ensure_map(m) {
                return Err("Faltan los mapas de la captura.");
            }
        }
        let now = self.now();
        self.capture = Some(Capture {
            participants: Vec::new(),
            rounds: rounds.max(1),
            min_level,
            max_level,
            max: max.max(2) as usize,
            wait: c::SIGNUP_SECS,
            round_wait: 0,
            fee: fee as u32,
            pot: 0,
            round: 0,
            retries: 0,
            state: CaptureState::Signup,
            flag_count: [0; 2],
            wins: [0; 2],
            next_sec: now.after_ms(1000),
        });
        self.f6_say_all(format!(
            "Eventos» Se ha iniciado un Captura de la bandera de {max} participantes, el precio de inscripción es de {fee}, el nivel mínimo para ingresar es {min_level} y el máximo es {max_level}, para participar envía /PARTICIPAR Las inscripciones se cerrarán en 3 minutos."
        ));
        Ok(())
    }

    /// `inscribirse`. Returns false when there is no capture accepting players.
    pub(crate) fn capture_join(&mut self, id: u32) -> bool {
        let Some(cap) = &self.capture else { return false };
        let why = (|| {
            let p = self.player(id)?;
            if cap.state != CaptureState::Signup {
                return Some("Ya se ha cerrado la inscripción para el evento.".to_string());
            }
            if p.inv.gold < cap.fee {
                return Some("No tienes oro suficiente para ingresar al evento.".into());
            }
            if cap.participants.iter().any(|x| x.id == Some(id)) {
                return Some("Ya estás anotado en el evento.".into());
            }
            if p.level < cap.min_level {
                return Some(format!("Para ingresar al evento deberás ser al menos nivel {}.", cap.min_level));
            }
            if p.level > cap.max_level {
                return Some(format!("Para ingresar al evento tu nivel no puede ser mayor a {}.", cap.max_level));
            }
            if !self.maps.get(&p.map).is_some_and(|m| m.map.safe) {
                return Some("Solamente podrás ingresar al evento en zona segura.".into());
            }
            if cap.participants.len() >= cap.max {
                return Some("Los cupos ya están completos.".into());
            }
            if p.gm.jail_ms > 0 {
                return Some("No puedes anotarte al evento mientras estás encarcelado.".into());
            }
            if self.is_dead(id) {
                return Some("No puedes anotarte a un evento estando muerto.".into());
            }
            if p.status.sailing.is_some() {
                return Some("No puedes ingresar al evento mientras estás navegando.".into());
            }
            if p.event.room.is_some() || p.event.in_tournament || self.lobby_seat_of(id).is_some() {
                return Some("Ya estas en un evento, debes terminar para poder participar en otro.".into());
            }
            None
        })();
        if let Some(why) = why {
            self.console(id, why, Tone::Warn);
            return true;
        }
        let fee = cap.fee;
        if !self.f6_take_gold(id, fee) {
            return true;
        }
        let p = self.player(id).unwrap();
        let part = Participant { name: p.name.clone(), id: Some(id), back: (p.map, p.x, p.y), team: 0, deaths: 0, timer: 0 };
        let cap = self.capture.as_mut().unwrap();
        cap.pot += fee as u64;
        cap.participants.push(part);
        let full = cap.participants.len() >= cap.max;
        self.capture_to_room(id);
        if full {
            self.capture_begin();
        }
        true
    }

    fn capture_to_room(&mut self, id: u32) {
        let (x1, y1, x2, y2) = c::WAIT_BOX;
        let (x, y) = (self.rng.range(x1, x2), self.rng.range(y1, y2));
        self.f6_warp(id, c::WAIT_MAP, x, y);
    }

    fn capture_to_base(&mut self, id: u32, team: u8) {
        if team == 0 {
            return;
        }
        let t = team as usize - 1;
        let (x, y) = c::TEAM_BASE[t];
        self.f6_warp(id, c::TEAM_MAP[t], x, y);
    }

    /// Every world tick: `PasarSegundo` once per second.
    pub(crate) fn tick_capture(&mut self, now: GameTime) {
        let Some(cap) = &mut self.capture else { return };
        if now < cap.next_sec {
            return;
        }
        cap.next_sec = now.after_ms(1000);
        match cap.state {
            CaptureState::Signup => {
                cap.wait -= 1;
                let w = cap.wait;
                if w % 30 == 0 && w > 5 {
                    self.f6_say_all(format!("Eventos» En {w} segundos se cierran las inscripciones para el evento captura de bandera, escribe /PARTICIPAR para anotarte."));
                } else if w <= 5 && w > 0 {
                    self.f6_say_all(format!("Eventos» Cerramos inscripciones en {w}."));
                } else if w <= 0 {
                    let cap = self.capture.as_mut().unwrap();
                    if cap.participants.len() * 2 < cap.max {
                        cap.retries += 1;
                        if cap.retries >= 2 {
                            self.capture_finish(Some(-2));
                        } else {
                            cap.wait = 30;
                            self.f6_say_all("Eventos» Se extendieron las inscripciones por falta de participantes.");
                        }
                    } else {
                        self.capture_begin();
                    }
                }
            }
            CaptureState::RoundStart => {
                cap.round_wait -= 1;
                let (w, r) = (cap.round_wait, cap.round);
                if w % 30 == 0 && w > 5 {
                    self.capture_say(format!("Eventos» En {w} segundos comienza la ronda {r}."));
                } else if w <= 5 && w > 0 {
                    self.capture_say(format!("Eventos» La ronda comienza en {w}."));
                } else if w <= 0 {
                    self.capture_say("Eventos» ¡¡¡Yaaa!!!");
                    let list: Vec<(u32, u8)> = self.capture.as_ref().unwrap().participants.iter().filter_map(|p| p.id.map(|i| (i, p.team))).collect();
                    for (id, team) in list {
                        self.capture_to_base(id, team);
                    }
                    self.capture.as_mut().unwrap().state = CaptureState::Playing;
                }
            }
            CaptureState::Playing => self.capture_second(),
            CaptureState::Finished => {}
        }
    }

    /// The `EnJuego` part of `PasarSegundo`.
    fn capture_second(&mut self) {
        let list: Vec<(usize, u32, u8)> = self.capture.as_ref().unwrap().participants.iter().enumerate().filter_map(|(i, p)| p.id.map(|id| (i, id, p.team))).collect();
        let mut holding = [false; 2];
        for (i, id, team) in list {
            if team == 0 {
                continue;
            }
            let t = team as usize - 1;
            let Some((map, x, y)) = self.player(id).map(|p| (p.map, p.x, p.y)) else { continue };
            if map == c::TEAM_MAP[t] {
                let enemy_flag = c::FLAG_OBJ[1 - t];
                if c::in_base(t, map, x, y) && self.capture_carries(id, enemy_flag) {
                    holding[t] = true;
                    let cap = self.capture.as_mut().unwrap();
                    cap.flag_count[t] += 1;
                    let n = cap.flag_count[t];
                    if n >= c::FLAG_HOLD_SECS {
                        self.capture_round_won(team);
                        return;
                    }
                    let name = self.name_of(id);
                    self.capture_say(format!("Eventos» ¡Atención! {name} del equipo {} tiene la bandera enemiga en su base, si aguanta {} segundos más ganará la ronda.", c::COLOR[t], c::FLAG_HOLD_SECS - n));
                }
            } else {
                let cap = self.capture.as_mut().unwrap();
                let p = &mut cap.participants[i];
                if p.timer > 0 {
                    p.timer -= 1;
                    if p.timer == 0 {
                        self.capture_to_base(id, team);
                    }
                }
            }
        }
        let cap = self.capture.as_mut().unwrap();
        for t in 0..2 {
            if !holding[t] {
                cap.flag_count[t] = 0;
            }
        }
    }

    fn capture_carries(&self, id: u32, obj: u16) -> bool {
        self.player(id).is_some_and(|p| p.inv.slots.iter().flatten().any(|i| i.obj == obj))
    }

    /// `iniciarCaptura`: teams by level and the first round.
    fn capture_begin(&mut self) {
        let cap = self.capture.as_mut().unwrap();
        cap.wait = 0;
        if cap.participants.len() % 2 != 0 || cap.participants.len() < 2 {
            self.capture_finish(Some(-2));
            return;
        }
        self.f6_say_all("Eventos» El evento captura la bandera ha iniciado.");
        let mut order: Vec<(usize, u32)> = self.capture.as_ref().unwrap().participants.iter().enumerate().map(|(i, p)| (i, p.id.and_then(|id| self.player(id)).map_or(0, |p| p.level))).collect();
        order.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let teams = c::teams(order.len());
        for ((i, _), team) in order.into_iter().zip(teams) {
            let cap = self.capture.as_mut().unwrap();
            cap.participants[i].team = team;
            if let Some(id) = cap.participants[i].id {
                self.console(id, format!("Te ha tocado participar en el equipo {}. Deberás proteger a tus compañeros de equipo y capturar la bandera del equipo contrario para ganar.", c::COLOR[team as usize - 1]), Tone::Info);
            }
        }
        self.capture_round();
    }

    /// `iniciarRonda`.
    fn capture_round(&mut self) {
        let cap = self.capture.as_mut().unwrap();
        cap.state = CaptureState::RoundStart;
        cap.round += 1;
        cap.round_wait = c::ROUND_START_SECS;
        let (r, w) = (cap.round, cap.round_wait);
        self.capture_say(format!("Eventos» La ronda {r} dará comienzo en {w} segundos."));
        let ids: Vec<u32> = self.capture.as_ref().unwrap().participants.iter().filter_map(|p| p.id).collect();
        for p in &mut self.capture.as_mut().unwrap().participants {
            p.deaths = 0;
            p.timer = 0;
        }
        for id in ids {
            self.capture_drop_flag(id, false);
            self.capture_to_room(id);
        }
        self.capture_restore_flag(0);
        self.capture_restore_flag(1);
    }

    /// `restaurarBandera`: the banner back on its stand.
    fn capture_restore_flag(&mut self, t: usize) {
        let (map, (x, y)) = (c::TEAM_MAP[t], c::FLAG_POS[t]);
        if self.maps[&map].ground.at(x, y).is_none() {
            self.make_obj(map, x, y, InvItem { obj: c::FLAG_OBJ[t], amount: 1 });
        }
    }

    /// `tirarBandera`: a carried enemy banner falls where the player is (only while
    /// playing; between rounds it just goes).
    fn capture_drop_flag(&mut self, id: u32, on_floor: bool) {
        let Some(team) = self.capture_team(id) else { return };
        let t = team as usize - 1;
        let obj = c::FLAG_OBJ[1 - t];
        let Some((slot, n)) = self.player(id).and_then(|p| (1..=p.inv.slots.len() as u8).find_map(|s| p.inv.get(s).filter(|i| i.obj == obj).map(|i| (s, i.amount)))) else { return };
        self.remove_from_slot(id, slot, n);
        let playing = self.capture.as_ref().is_some_and(|cap| cap.state == CaptureState::Playing);
        if on_floor && playing {
            let (m, x, y) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
            if let Some((fx, fy)) = self.drop_to_floor(m, x, y, InvItem { obj, amount: 1 }, false) {
                self.capture_say(format!("Eventos» ¡Atención! la bandera del equipo {} se encuentra en ({m},{fx},{fy})", c::COLOR[1 - t]));
            }
        }
    }

    /// HOOK (inventory::on_pick_up), before taking `obj`: `tomaBandera`. False = refused
    /// (a team cannot lift its own banner from its stand).
    pub(crate) fn capture_may_pick(&self, id: u32, obj: u16) -> bool {
        let Some(t) = c::FLAG_OBJ.iter().position(|o| *o == obj) else { return true };
        let Some(team) = self.capture_team(id) else { return true };
        if team as usize - 1 == t {
            let at_stand = self.player(id).is_some_and(|p| p.map == c::TEAM_MAP[t] && (p.x, p.y) == c::FLAG_POS[t]);
            return !at_stand;
        }
        true
    }

    /// HOOK (inventory::on_pick_up), after taking `obj`: `quitarBandera`. The own banner
    /// picked from the floor goes back to its stand; the enemy one is announced.
    pub(crate) fn capture_picked(&mut self, id: u32, obj: u16) {
        let Some(t) = c::FLAG_OBJ.iter().position(|o| *o == obj) else { return };
        let Some(team) = self.capture_team(id) else { return };
        let mine = team as usize - 1;
        let name = self.name_of(id);
        if mine != t {
            self.capture_say(format!("Eventos» ¡Atención! {name} del equipo {} ha tomado la bandera del equipo {}.", c::COLOR[mine], c::COLOR[t]));
        } else {
            if let Some((slot, n)) = self.player(id).and_then(|p| (1..=p.inv.slots.len() as u8).find_map(|s| p.inv.get(s).filter(|i| i.obj == obj).map(|i| (s, i.amount)))) {
                self.remove_from_slot(id, slot, n);
            }
            self.capture_restore_flag(t);
            self.capture_say(format!("Eventos» ¡Atención! La bandera del equipo {} ha sido recuperada.", c::COLOR[t]));
        }
    }

    /// HOOK (inventory::on_drop_item), after dropping `obj`: `tiraBandera`.
    pub(crate) fn capture_dropped(&mut self, id: u32, obj: u16) {
        let Some(t) = c::FLAG_OBJ.iter().position(|o| *o == obj) else { return };
        if !self.capture_has(id) {
            return;
        }
        if let Some(cap) = &mut self.capture {
            cap.flag_count[1 - t] = 0;
        }
        let (m, x, y) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
        self.capture_say(format!("Eventos» ¡Atención! la bandera del equipo {} se encuentra en ({m},{x},{y})", c::COLOR[t]));
    }

    /// HOOK (combat): `muereUsuario`.
    pub(crate) fn capture_on_death(&mut self, id: u32) {
        let Some(team) = self.capture_team(id) else { return };
        if self.capture.as_ref().is_none_or(|cap| cap.state != CaptureState::Playing) {
            return;
        }
        let name = self.name_of(id);
        self.capture_say(format!("Eventos» {name} del equipo {} ha muerto en batalla.", c::COLOR[team as usize - 1]));
        self.capture_drop_flag(id, true);
        let cap = self.capture.as_mut().unwrap();
        let p = cap.participants.iter_mut().find(|p| p.id == Some(id)).unwrap();
        p.deaths += 1;
        p.timer = c::respawn_secs(p.deaths);
        let t = p.timer;
        self.console(id, format!("Volverás a tu base en {t} segundos."), Tone::Info);
        self.capture_to_room(id);
        self.resurrect(id);
    }

    /// `finalizarRonda`.
    fn capture_round_won(&mut self, team: u8) {
        let t = team as usize - 1;
        let cap = self.capture.as_mut().unwrap();
        cap.flag_count = [0; 2];
        cap.wins[t] += 1;
        let (r, wins, need) = (cap.round, cap.wins[t], c::rounds_to_win(cap.rounds));
        self.capture_say(format!("Eventos» ¡El equipo {} ha ganado la ronda {r}!", c::COLOR[t]));
        for id in self.capture.as_ref().unwrap().participants.iter().filter_map(|p| p.id).collect::<Vec<_>>() {
            if let Some((x, y)) = self.player(id).map(|p| (p.x, p.y)) {
                self.send(id, &S2C::Sound { wav: 44, x, y });
            }
        }
        if wins >= need {
            self.capture_finish(Some(team as i32));
        } else {
            self.capture_round();
        }
    }

    /// `finalizarCaptura`: None = cancelled by a GM (no refunds, as in the VB), Some(-2) =
    /// not enough players (refunds), Some(0) = draw, Some(team) = winners.
    pub(crate) fn capture_finish(&mut self, result: Option<i32>) {
        let Some(cap) = &mut self.capture else { return };
        cap.state = CaptureState::Finished;
        let fee = cap.fee as u64;
        let list: Vec<(u32, u8)> = cap.participants.iter().filter_map(|p| p.id.map(|i| (i, p.team))).collect();
        match result {
            Some(-2) => {
                self.f6_say_all("Eventos» Se ha cancelado el evento de captura la bandera por falta de participantes.");
                self.f6_say_all("Eventos» El evento de captura la bandera ha sido cancelado. Se te ha devuelto el precio de inscripción.");
                for (id, _) in &list {
                    self.f6_give_gold(*id, fee);
                }
            }
            Some(0) => self.f6_say_all("Eventos» ¡El evento de captura la bandera ha terminado en empate!"),
            Some(team) if team > 0 => {
                let mut pot = self.capture.as_ref().unwrap().pot;
                for (id, t) in &list {
                    if *t as i32 == team {
                        let prize = (fee * 2).min(pot);
                        pot -= prize;
                        self.f6_give_gold(*id, prize);
                        self.console(*id, format!("¡Has recuperado tu inscripción y ganado {fee} monedas de oro adicionales por ganar el evento!"), Tone::Good);
                    }
                }
                self.f6_say_all(format!("Eventos> ¡El evento de captura la bandera ha terminado! Ganador el equipo {}.", c::COLOR[team as usize - 1]));
            }
            _ => self.f6_say_all("Eventos» El evento ha sido cancelado."),
        }
        let parts = self.capture.as_ref().unwrap().participants.clone();
        for p in parts {
            if let Some(id) = p.id {
                self.capture_leave(id, p.back);
            }
        }
        for t in 0..2 {
            let (map, (x, y)) = (c::TEAM_MAP[t], c::FLAG_POS[t]);
            if let Some(g) = self.maps.get(&map).and_then(|m| m.ground.at(x, y)) {
                self.erase_obj(map, x, y, g.amount);
            }
        }
        self.capture = None;
    }

    /// `eliminarParticipante`: banner dropped, back where they signed up.
    fn capture_leave(&mut self, id: u32, back: (u16, i32, i32)) {
        self.capture_drop_flag(id, true);
        if self.is_dead(id) {
            self.resurrect(id);
        }
        self.f6_warp(id, back.0, back.1, back.2);
    }

    /// HOOK (event_on_leave): a disconnection takes the player out of the capture.
    pub(crate) fn capture_on_leave(&mut self, id: u32) {
        let Some(back) = self.capture.as_ref().and_then(|cap| cap.participants.iter().find(|p| p.id == Some(id))).map(|p| p.back) else { return };
        self.capture_leave(id, back);
        if let Some(cap) = &mut self.capture {
            cap.participants.retain(|p| p.id != Some(id));
        }
    }
}

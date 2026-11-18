//! `/SALIR` (`HandleQuit` + `Cerrar_Usuario` + `CancelExit`): the character leaves the
//! world. In a safe map (or as a GM) at once; elsewhere after `IntervaloCerrarConexion`
//! (10 s) counted down second by second, cancelled by walking or being hurt. Unlike the
//! official client, which closes the connection, the account stays logged in and goes
//! back to its character list.
use crate::clock::GameTime;
use crate::world::World;
use ao_protocol::{Tone, S2C};

/// `IntervaloCerrarConexion` / `IntervaloCerrarConexionEnDungeon` (intervalos.ini).
pub const QUIT_SECS: u64 = 10;

impl World {
    pub(crate) fn begin_quit(&mut self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let now = self.now();
        if p.status.paralyzed_until.is_some_and(|u| now < u) {
            return self.console(id, "No podés salir estando paralizado.", Tone::Warn);
        }
        if p.quitting.is_some() {
            return;
        }
        let map = p.map;
        self.close_trades(id);
        let safe = self.instance(map).map.safe;
        if safe || self.gm_rank(id) != ao_protocol::p4::gm::GmRank::User {
            return self.leave_world(id);
        }
        // Leaving reveals the hidden and the invisible.
        self.reveal(id);
        self.player_mut(id).status.invisible_until = None;
        self.player_mut(id).quitting = Some((now.after_secs(QUIT_SECS), QUIT_SECS));
        self.console(id, format!("Cerrando el juego en {QUIT_SECS} segundos..."), Tone::Info);
    }

    /// `CancelExit`: walking or being hurt stops the countdown.
    pub(crate) fn cancel_quit(&mut self, id: u32) {
        if self.player(id).is_some_and(|p| p.quitting.is_some()) {
            self.player_mut(id).quitting = None;
            self.console(id, "Has cancelado la salida.", Tone::Info);
        }
    }

    /// The countdown, once per second.
    pub(crate) fn tick_quit(&mut self, now: GameTime) {
        let due: Vec<(u32, GameTime, u64)> = self.sessions.iter().filter_map(|(id, s)| s.player.as_ref().and_then(|p| p.quitting).map(|(at, left)| (*id, at, left))).collect();
        for (id, at, left) in due {
            if now >= at {
                self.leave_world(id);
                continue;
            }
            let secs = (at.0.saturating_sub(now.0)).div_ceil(1000);
            if secs < left {
                self.player_mut(id).quitting = Some((at, secs));
                self.console(id, format!("Cerrando el juego en {secs} segundos..."), Tone::Info);
            }
        }
    }

    /// Takes the character out of the world (saved), keeping the account logged in.
    pub(crate) fn leave_world(&mut self, id: u32) {
        if self.player(id).is_none() {
            return;
        }
        self.social_on_leave(id);
        self.pets_on_leave(id);
        self.close_bank(id);
        let Some(p) = self.sessions.get_mut(&id).and_then(|s| s.player.take()) else { return };
        tracing::info!(id, name = %p.name, "vuelve a la seleccion de personajes");
        self.save_player(&p);
        if let Some(inst) = self.maps.get_mut(&p.map) {
            inst.players.remove(&id);
        }
        self.broadcast(p.map, &S2C::Despawn { id }, None);
        self.broadcast_all(&S2C::Console { text: format!("{} se desconecto.", p.name), color: Tone::Info }, None);
        self.send(id, &S2C::LeftWorld);
        if let Some(acc) = self.sessions.get(&id).and_then(|s| s.account.as_ref()).map(|a| a.id) {
            self.send_character_list_pub(id, acc);
        } else {
            // Development logins have no account: back to the login.
            self.close(id);
        }
    }
}

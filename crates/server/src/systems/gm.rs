//! Gm system: game masters: commands, roles, penalties (jail, silence, ban), logs. OWNER: Opus C (docs/P4_CONTRATO.md).
//!
//! Ported from `Protocol_GmCommands.bas` (handlers), `Admin.bas` (Encarcelar, PurgarPenas,
//! BanPJ, DoAdminInvisible), `Penas.bas` (SavePenaDatabase, /PENAS) and `Logging.bas`
//! (LogGM, LogBanByName), with `e_PlayerType` (Declares.bas) as the rank.
//!
//! **Ranks** live in the database (`gm_role`), keyed by character name like Server.ini
//! `[Admines]`/`[Dioses]`/`[SemiDioses]`/`[Consejeros]`. They are only read on the server;
//! the client is told its own rank (`GmS2C::Rank`) just to show the panel.
//!
//! **First admin**: env var `AO_GM_SEED="Nombre:admin,Otro:dios"`, applied once when the
//! world starts using the database (upsert, idempotent). After that, an admin promotes
//! others in game with `/ROL nick@rango` (and can drop the env var).
//!
//! **Commands** (`/X` typed in the chat or sent by the panel). Arguments split by `@`
//! like the VB client when there is one, else by spaces. Minimum rank, VB equivalent:
//!
//! | Command | Rank | VB |
//! |---|---|---|
//! | `/GMSG texto` (mensaje a los GMs) | Consejero | HandleGMMessage |
//! | `/RMSG texto` (anuncio global) | Consejero | HandleServerMessage |
//! | `/DONDE nick` | Consejero | HandleWhere |
//! | `/PENAS nick` (historial) | Consejero | HandlePunishments |
//! | `/LOG [nick]` (registro de GMs) | Consejero | (Logging.bas, consultable) |
//! | `/SILENCIAR nick@minutos` (0 = quitar) | Consejero | HandleSilence |
//! | `/ADVERTENCIA nick@motivo` | Consejero | HandleWarnUser |
//! | `/ECHAR nick` | Consejero | HandleKick |
//! | `/TELEP YO mapa x y` / `/TELEP nick mapa x y` | Consejero (YO) / SemiDios | HandleWarpChar |
//! | `/IRA nick` | SemiDios | HandleGoToChar |
//! | `/SUM nick` | SemiDios | HandleSummonChar |
//! | `/INFO nick` | SemiDios | HandleRequestCharInfo |
//! | `/REVIVIR nick` | SemiDios | HandleReviveChar |
//! | `/EJECUTAR nick` | SemiDios | HandleExecute |
//! | `/CARCEL nick@motivo@minutos` (0 = liberar) | SemiDios | HandleJail / Encarcelar |
//! | `/BAN nick@motivo` | SemiDios | HandleBanChar / BanPJ |
//! | `/INVISIBLE` | SemiDios | HandleInvisible / DoAdminInvisible |
//! | `/ACC npc` (sin respawn) / `/RACC npc` | SemiDios | HandleCreateNPC / ...WithRespawn |
//! | `/MATA` (NPC más cercano, sin respawn) | SemiDios | HandleKillNPCNoRespawn |
//! | `/UNBAN nick` | Dios | HandleUnbanChar |
//! | `/BANCUENTA nick@motivo` / `/UNBANCUENTA nick` | Dios | HandleBanCuenta / HandleUnBanCuenta |
//! | `/BANIP nick-o-ip@motivo` / `/UNBANIP ip` | Dios | HandleBanIP / HandleUnbanIP |
//! | `/MOD nick ORO cantidad` | Dios | HandleEditChar (solo ORO) |
//! | `/CI objeto [cantidad]` | Admin | HandleCreateItem |
//! | `/DAR nick@objeto@cantidad[@motivo]` | Admin | HandleGiveItem |
//! | `/QUITAR nick@objeto@cantidad` | Admin | (nuevo) |
//! | `/ROL nick@rango` | Admin | (nuevo: sustituye editar Server.ini) |
//! | `/APAGAR minutos` (0 = cancelar) | Admin | (nuevo: el VB lo hace desde el formulario) |
//!
//! Punitive commands only work on targets of a lower rank (CompararPrivilegios). A player
//! without rank gets no answer at all: the line goes on as any other `/` line (as if the
//! commands did not exist). A GM without enough rank gets the VB "Comando deshabilitado
//! para tu cargo".
use crate::clock::{Clock, GameTime};
use crate::security::sanitize_chat;
use crate::world::entities::Npc;
use crate::world::gm::{Shutdown, FREEDOM, JAIL, MAX_JAIL_MINUTES, MAX_SILENCE_MINUTES, PANEL_LOG_ROWS, SEED_ENV};
use crate::world::inventory::InvItem;
use crate::world::World;
use ao_formats::dat::obj::ObjType;
use ao_protocol::p4::gm::{GmPlayerRow, GmRank};
use ao_protocol::{GmC2S, GmS2C, Tone, S2C};
use ao_rules::items::MAX_STACK;
use ao_rules::{Dir, MAX_CHAT};
use std::net::IpAddr;
use tracing::{info, warn};

const DENIED: &str = "Servidor » Comando deshabilitado para tu cargo.";
const NOT_ONLINE: &str = "El usuario no está online.";
const HIGHER: &str = "No puedes hacer eso a un administrador de mayor o igual rango.";
const MS_PER_MIN: u64 = 60_000;

/// Splits the arguments like the VB client: by `@` if there is one, else by spaces.
fn args(rest: &str) -> Vec<String> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Vec::new();
    }
    if rest.contains('@') {
        rest.split('@').map(|s| s.trim().replace('+', " ")).collect()
    } else {
        rest.split_whitespace().map(str::to_string).collect()
    }
}

/// `AO_GM_SEED`: `Name:rank,Name:rank`.
pub fn parse_seed(s: &str) -> Vec<(String, GmRank)> {
    s.split(',')
        .filter_map(|e| {
            let (n, r) = e.split_once(':')?;
            let n = n.trim();
            (!n.is_empty()).then_some(())?;
            Some((n.to_string(), GmRank::parse(r)?))
        })
        .collect()
}

fn mins(ms: u64) -> u64 {
    ms.div_ceil(MS_PER_MIN)
}

impl World {
    // ------------------------------------------------------------ hooks

    pub(crate) fn on_gm(&mut self, id: u32, msg: GmC2S) {
        if !self.gm_rank(id).is_gm() {
            // Nobody answers a non GM: not even "you are not allowed".
            return;
        }
        match msg {
            GmC2S::Refresh => self.gm_send_panel(id),
            GmC2S::Command { line } => {
                // Same cleaning as the chat (control characters, length).
                let Some(line) = sanitize_chat(&line, MAX_CHAT) else { return };
                if !line.starts_with('/') || !self.gm_command(id, &line) {
                    self.console(id, "Comando de GM desconocido.", Tone::Warn);
                }
                if self.player(id).is_some() {
                    self.gm_send_panel(id);
                }
            }
        }
    }

    /// HOOK (chat): a `/` line. Returns true if it was a GM command (handled, not said).
    /// Runs before the development commands (`/dar`, `/ir`).
    pub(crate) fn gm_command(&mut self, id: u32, line: &str) -> bool {
        let rank = self.gm_rank(id);
        if !rank.is_gm() {
            return false;
        }
        let (cmd, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let cmd = cmd.to_uppercase();
        let a = args(rest);
        let need = match cmd.as_str() {
            "/GMSG" | "/RMSG" | "/DONDE" | "/PENAS" | "/LOG" | "/SILENCIAR" | "/ADVERTENCIA" | "/ECHAR" | "/TELEP" => GmRank::Consejero,
            "/IRA" | "/SUM" | "/INFO" | "/REVIVIR" | "/EJECUTAR" | "/CARCEL" | "/BAN" | "/INVISIBLE" | "/ACC" | "/RACC" | "/MATA" => GmRank::SemiDios,
            "/UNBAN" | "/BANCUENTA" | "/UNBANCUENTA" | "/BANIP" | "/UNBANIP" | "/MOD" => GmRank::Dios,
            "/CI" | "/QUITAR" | "/ROL" | "/APAGAR" => GmRank::Admin,
            // `/dar` without `@` is the development command (`/dar obj cant`).
            "/DAR" if rest.contains('@') => GmRank::Admin,
            _ => return false,
        };
        // Warping someone else needs SemiDios (a Consejero may only move himself).
        let need = if cmd == "/TELEP" && !a.first().is_some_and(|n| n.eq_ignore_ascii_case("YO")) { GmRank::SemiDios } else { need };
        if rank < need {
            self.console(id, DENIED, Tone::Warn);
            return true;
        }
        let me = self.player(id).map(|p| p.name.clone()).unwrap_or_default();
        info!(gm = %me, %line, "comando de GM");
        match cmd.as_str() {
            "/GMSG" => self.gm_msg(id, &me, rest.trim()),
            "/RMSG" => self.gm_announce(id, &me, rest.trim()),
            "/DONDE" => self.gm_where(id, &a),
            "/PENAS" => self.gm_show_penalties(id, &a),
            "/LOG" => self.gm_show_log(id, &a),
            "/SILENCIAR" => self.gm_silence(id, &me, &a),
            "/ADVERTENCIA" => self.gm_warn(id, &me, &a),
            "/ECHAR" => self.gm_kick(id, &me, &a),
            "/TELEP" => self.gm_warp(id, &me, &a),
            "/IRA" => self.gm_goto(id, &me, &a),
            "/SUM" => self.gm_summon(id, &me, &a),
            "/INFO" => self.gm_info(id, &a),
            "/REVIVIR" => self.gm_revive(id, &me, &a),
            "/EJECUTAR" => self.gm_execute(id, &me, &a),
            "/CARCEL" => self.gm_jail(id, &me, &a),
            "/BAN" => self.gm_ban_char(id, &me, &a),
            "/INVISIBLE" => self.gm_invisible(id, &me),
            "/ACC" => self.gm_create_npc(id, &me, &a, false),
            "/RACC" => self.gm_create_npc(id, &me, &a, true),
            "/MATA" => self.gm_kill_npc(id, &me),
            "/UNBAN" => self.gm_unban_char(id, &me, &a),
            "/BANCUENTA" => self.gm_ban_account(id, &me, &a, true),
            "/UNBANCUENTA" => self.gm_ban_account(id, &me, &a, false),
            "/BANIP" => self.gm_ban_ip(id, &me, &a),
            "/UNBANIP" => self.gm_unban_ip(id, &me, &a),
            "/MOD" => self.gm_edit(id, &me, &a),
            "/CI" => self.gm_create_item(id, &me, &a),
            "/DAR" => self.gm_give(id, &me, &a),
            "/QUITAR" => self.gm_take(id, &me, &a),
            "/ROL" => self.gm_set_role(id, &me, &a),
            "/APAGAR" => self.gm_shutdown(id, &me, &a),
            _ => unreachable!(),
        }
        true
    }

    /// HOOK (chat, channels): silenced players cannot talk (`flags.Silenciado`).
    pub(crate) fn gm_is_silenced(&self, id: u32) -> bool {
        self.player(id).is_some_and(|p| p.gm.silence_ms > 0)
    }

    /// For the login (integrator): why this character / address cannot enter, if banned.
    /// Account bans are already refused by `Db::verify_account` (`account.banned`).
    /// `gm_on_enter` applies the same check as a safety net (kicks right after entering).
    #[allow(dead_code)]
    pub(crate) fn gm_login_refusal(&self, character: &str, ip: Option<IpAddr>) -> Option<String> {
        if self.gms.shut_down {
            return Some("El servidor se está apagando.".into());
        }
        if let Ok(Some(reason)) = self.db.gm_ban_reason(character) {
            return Some(format!("Este personaje está baneado: {reason}"));
        }
        let gm = self.offline_rank(character).is_gm();
        if let (false, Some(ip)) = (gm, ip) {
            if let Ok(Some(reason)) = self.db.gm_ip_ban_reason(&ip.to_string()) {
                return Some(format!("Tu dirección está baneada: {reason}"));
            }
        }
        None
    }

    /// Every world tick: sentences count down, the shutdown countdown, `/ACC` NPCs.
    pub(crate) fn tick_gm(&mut self, now: GameTime) {
        let dt = self.gms.last_tick.map_or(0, |t| now.since(t));
        self.gms.last_tick = Some(now);
        for (id, reason) in std::mem::take(&mut self.gms.refusals) {
            if self.player(id).is_some_and(|p| p.gm.refused) {
                self.kick(id, &reason);
            }
        }
        if dt > 0 {
            self.tick_sentences(dt);
        }
        self.tick_no_respawn();
        self.tick_shutdown(now);
    }

    /// When the player enters the world (after everything else is sent).
    pub(crate) fn gm_on_enter(&mut self, id: u32) {
        self.gm_seed();
        let Some(p) = self.player(id) else { return };
        let name = p.name.clone();
        let ip = self.sessions.get(&id).map(|s| s.addr.ip());
        let rank = self.db.gm_rank(&name, p.char_id, self.cfg.dev_login).unwrap_or_default();
        if let Some(reason) = self.gm_login_refusal(&name, ip) {
            warn!(id, %name, %reason, "entrada rechazada por GM");
            self.player_mut(id).gm.refused = true;
            self.gms.refusals.push((id, reason));
            return;
        }
        let (jail_ms, silence_ms) = self.db.gm_sentence(&name).unwrap_or((0, 0));
        let p = self.player_mut(id);
        p.gm.rank = rank;
        p.gm.jail_ms = jail_ms;
        p.gm.silence_ms = silence_ms;
        let map = p.map;
        if rank.is_gm() {
            self.send(id, &S2C::Gm(GmS2C::Rank { rank }));
            self.console(id, format!("Rango: {}. F10 abre el panel de GM.", rank.label()), Tone::Good);
        }
        if jail_ms > 0 {
            if map != JAIL.0 {
                self.teleport(id, JAIL.0, JAIL.1, JAIL.2);
            }
            self.console(id, format!("Estás encarcelado. Te quedan {} minutos.", mins(jail_ms)), Tone::Warn);
        }
        if silence_ms > 0 {
            self.console(id, format!("Estás silenciado. Te quedan {} minutos.", mins(silence_ms)), Tone::Warn);
        }
    }

    /// Right before the player leaves the world, still present.
    pub(crate) fn gm_on_leave(&mut self, id: u32) {
        let Some(p) = self.player(id) else { return };
        if !p.gm.refused {
            let (name, j, s) = (p.name.clone(), p.gm.jail_ms, p.gm.silence_ms);
            if let Err(e) = self.db.gm_save_sentence(&name, j, s) {
                warn!(?e, "no se pudo guardar la pena");
            }
        }
    }

    // ------------------------------------------------------------ helpers

    pub(crate) fn gm_rank(&self, id: u32) -> GmRank {
        self.player(id).map(|p| p.gm.rank).unwrap_or_default()
    }

    /// Applies `AO_GM_SEED` once. Each name is bound to the character that has it NOW: a
    /// name with no character is skipped (whoever registered it later would be admin),
    /// except with `dev_login`, where it is an unbound row for development logins.
    fn gm_seed(&mut self) {
        if self.gms.seeded {
            return;
        }
        self.gms.seeded = true;
        let Ok(seed) = std::env::var(SEED_ENV) else { return };
        for (name, rank) in parse_seed(&seed) {
            let char_id = self.db.gm_char_id(&name).ok().flatten();
            if char_id.is_none() && !self.cfg.dev_login {
                warn!(%name, "{SEED_ENV}: ese personaje no existe todavía; créalo y reinicia");
                continue;
            }
            if self.db.gm_rank(&name, char_id, true).ok() != Some(rank) {
                info!(%name, rank = rank.label(), "rango de GM desde {SEED_ENV}");
                let _ = self.db.gm_set_rank(&name, char_id, rank, SEED_ENV);
                let _ = self.db.gm_log(SEED_ENV, "rol", &name, rank.label());
            }
        }
    }

    fn online(&self, name: &str) -> Option<u32> {
        self.player_ids().into_iter().find(|i| self.player(*i).is_some_and(|p| p.name.eq_ignore_ascii_case(name)))
    }

    /// Stored rank of a character that is not online (its live character id, if any).
    fn offline_rank(&self, name: &str) -> GmRank {
        let char_id = self.db.gm_char_id(name).ok().flatten();
        self.db.gm_rank(name, char_id, self.cfg.dev_login).unwrap_or_default()
    }

    /// Rank of a character, online or not.
    fn rank_of(&self, name: &str) -> GmRank {
        match self.online(name) {
            Some(t) => self.gm_rank(t),
            None => self.offline_rank(name),
        }
    }

    /// The target online, of a lower rank than the actor. Says why not otherwise.
    fn target(&self, id: u32, a: &[String], usage: &str) -> Option<u32> {
        let Some(name) = a.first().filter(|n| !n.is_empty()) else {
            self.console(id, usage, Tone::Info);
            return None;
        };
        let Some(t) = self.online(name) else {
            self.console(id, NOT_ONLINE, Tone::Warn);
            return None;
        };
        if t != id && self.gm_rank(t) >= self.gm_rank(id) {
            self.console(id, HIGHER, Tone::Warn);
            return None;
        }
        Some(t)
    }

    fn name_of(&self, id: u32) -> String {
        self.player(id).map(|p| p.name.clone()).unwrap_or_default()
    }

    fn log_gm(&self, gm: &str, action: &str, target: &str, detail: &str) {
        if let Err(e) = self.db.gm_log(gm, action, target, detail) {
            warn!(?e, "no se pudo escribir el registro de GM");
        }
    }

    fn penalty(&self, target: &str, gm: &str, kind: &str, minutes: i64, reason: &str) {
        if let Err(e) = self.db.gm_add_penalty(target, gm, kind, minutes, reason) {
            warn!(?e, "no se pudo guardar la pena");
        }
    }

    /// `SendData(ToGM)`.
    fn to_gms(&self, text: &str) {
        let msg = S2C::Console { text: text.to_string(), color: Tone::Info };
        for pid in self.player_ids() {
            if self.gm_rank(pid).is_gm() {
                self.send(pid, &msg);
            }
        }
    }

    fn persist_sentence(&self, id: u32) {
        if let Some(p) = self.player(id) {
            let _ = self.db.gm_save_sentence(&p.name, p.gm.jail_ms, p.gm.silence_ms);
        }
    }

    pub(crate) fn gm_send_panel(&self, id: u32) {
        let players = self
            .player_ids()
            .into_iter()
            .filter_map(|pid| {
                let p = self.player(pid)?;
                Some(GmPlayerRow {
                    id: pid,
                    name: p.name.clone(),
                    level: p.level,
                    map: p.map,
                    x: p.x,
                    y: p.y,
                    rank: p.gm.rank,
                    dead: p.combat.dead || p.hp <= 0,
                    jail_secs: (p.gm.jail_ms / 1000) as u32,
                    silence_secs: (p.gm.silence_ms / 1000) as u32,
                })
            })
            .collect();
        self.send(id, &S2C::Gm(GmS2C::Online { players }));
        let entries = self.db.gm_recent_log(PANEL_LOG_ROWS, None).unwrap_or_default();
        self.send(id, &S2C::Gm(GmS2C::Log { entries }));
    }

    // ------------------------------------------------------------ ticks

    /// PurgarPenas (jail only counts while alive) and the silence timer.
    fn tick_sentences(&mut self, dt: u64) {
        for pid in self.player_ids() {
            let dead = self.is_dead(pid);
            let p = self.player_mut(pid);
            let (jail_before, silence_before) = (p.gm.jail_ms, p.gm.silence_ms);
            if jail_before > 0 && !dead {
                p.gm.jail_ms = jail_before.saturating_sub(dt);
            }
            if silence_before > 0 {
                p.gm.silence_ms = silence_before.saturating_sub(dt);
            }
            let (jail, silence, map) = (p.gm.jail_ms, p.gm.silence_ms, p.map);
            if jail_before > 0 && jail == 0 {
                self.teleport(pid, FREEDOM.0, FREEDOM.1, FREEDOM.2);
                self.console(pid, "Has sido liberado.", Tone::Good);
                self.persist_sentence(pid);
            } else if jail > 0 && map != JAIL.0 {
                // No escaping (/HOGAR, a teleport...): back to the cell.
                self.teleport(pid, JAIL.0, JAIL.1, JAIL.2);
            }
            if silence_before > 0 && silence == 0 {
                self.console(pid, "Has sido desilenciado.", Tone::Good);
                self.persist_sentence(pid);
            }
        }
    }

    fn tick_no_respawn(&mut self) {
        if self.gms.no_respawn.is_empty() {
            return;
        }
        let list: Vec<(u16, u32)> = self.gms.no_respawn.iter().copied().collect();
        for (map, nid) in list {
            let state = self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).map(|n| n.alive());
            match state {
                Some(true) => {}
                Some(false) => {
                    self.maps.get_mut(&map).unwrap().remove_npc(nid);
                    self.broadcast(map, &S2C::Despawn { id: nid }, None);
                    self.gms.no_respawn.remove(&(map, nid));
                }
                None => {
                    self.gms.no_respawn.remove(&(map, nid));
                }
            }
        }
    }

    fn tick_shutdown(&mut self, now: GameTime) {
        let Some(s) = &self.gms.shutdown else { return };
        if now >= s.at {
            let by = s.by.clone();
            self.gms.shutdown = None;
            self.gms.shut_down = true;
            warn!(%by, "servidor apagado por GM");
            for pid in self.player_ids() {
                self.kick(pid, "El servidor se ha apagado. Vuelve más tarde.");
            }
            // Everyone was saved by the kicks (synchronous SQLite). With the real clock the
            // process ends here; under a virtual clock (tests) it only refuses logins.
            if matches!(self.clock, Clock::Real(_)) {
                info!("apagado completo: saliendo del proceso");
                std::process::exit(0);
            }
            return;
        }
        if now >= s.next_notice {
            let left = s.at.since(now);
            let next = if left > 60_000 { 60_000 } else if left > 10_000 { left - 10_000 } else { left };
            let text = if left >= 60_000 { format!("Servidor » Apagado en {} minutos.", mins(left)) } else { format!("Servidor » Apagado en {} segundos.", left.div_ceil(1000)) };
            self.gms.shutdown.as_mut().unwrap().next_notice = now.after_ms(next.max(1000));
            self.broadcast_all(&S2C::Console { text, color: Tone::Warn }, None);
        }
    }

    // ------------------------------------------------------------ messages

    fn gm_msg(&mut self, id: u32, me: &str, text: &str) {
        if text.is_empty() {
            return self.console(id, "Uso: /GMSG texto", Tone::Info);
        }
        self.to_gms(&format!("{me} (GM) » {text}"));
        self.log_gm(me, "gmsg", "", text);
    }

    fn gm_announce(&mut self, id: u32, me: &str, text: &str) {
        if text.is_empty() {
            return self.console(id, "Uso: /RMSG texto", Tone::Info);
        }
        self.broadcast_all(&S2C::Console { text: format!("Servidor » {text}"), color: Tone::Good }, None);
        self.log_gm(me, "anuncio", "", text);
    }

    fn gm_where(&mut self, id: u32, a: &[String]) {
        let Some(name) = a.first() else { return self.console(id, "Uso: /DONDE nick", Tone::Info) };
        match self.online(name).and_then(|t| self.player(t)) {
            Some(p) => self.console(id, format!("Ubicación de {}: mapa {}, {} {}.", p.name, p.map, p.x, p.y), Tone::Info),
            None => self.console(id, NOT_ONLINE, Tone::Warn),
        }
    }

    fn gm_show_penalties(&mut self, id: u32, a: &[String]) {
        let Some(name) = a.first() else { return self.console(id, "Uso: /PENAS nick", Tone::Info) };
        let rows = self.db.gm_penalties(name).unwrap_or_default();
        if rows.is_empty() {
            return self.console(id, format!("{name} no tiene penas."), Tone::Info);
        }
        self.console(id, format!("Penas de {name} ({}):", rows.len()), Tone::Info);
        for (i, r) in rows.iter().enumerate() {
            let t = if r.minutes > 0 { format!(" {} min", r.minutes) } else { String::new() };
            self.console(id, format!("{}. {} por {}{}: {}", i + 1, r.kind, r.gm, t, r.reason), Tone::Info);
        }
    }

    fn gm_show_log(&mut self, id: u32, a: &[String]) {
        let rows = self.db.gm_recent_log(15, a.first().map(String::as_str)).unwrap_or_default();
        if rows.is_empty() {
            return self.console(id, "El registro está vacío.", Tone::Info);
        }
        for r in rows.iter().rev() {
            self.console(id, format!("[{}] {} {} {} {}", r.at, r.gm, r.action, r.target, r.detail).trim_end().to_string(), Tone::Info);
        }
    }

    // ------------------------------------------------------------ penalties

    /// HandleSilence: online or offline (existing character), lower rank only.
    fn gm_silence(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /SILENCIAR nick@minutos (0 = quitar el silencio)";
        let (Some(name), Some(m)) = (a.first(), a.get(1).and_then(|m| m.parse::<u64>().ok())) else { return self.console(id, USAGE, Tone::Info) };
        if m > MAX_SILENCE_MINUTES {
            return self.console(id, format!("No puedes silenciar por más de {MAX_SILENCE_MINUTES} minutos."), Tone::Warn);
        }
        if self.rank_of(name) >= self.gm_rank(id) {
            return self.console(id, HIGHER, Tone::Warn);
        }
        match self.online(name) {
            Some(t) => {
                let tn = self.name_of(t);
                self.player_mut(t).gm.silence_ms = m * MS_PER_MIN;
                self.persist_sentence(t);
                if m > 0 {
                    self.console(t, "Has sido silenciado por los administradores, no podrás hablar con otros usuarios.", Tone::Warn);
                    self.to_gms(&format!("Administración » {me} ha silenciado a {tn} por {m} minutos."));
                } else {
                    self.console(t, "Has sido desilenciado.", Tone::Good);
                    self.to_gms(&format!("Administración » {me} ha desilenciado a {tn}."));
                }
            }
            None => {
                if !self.db.gm_character_exists(name).unwrap_or(false) {
                    return self.console(id, "El personaje no existe.", Tone::Warn);
                }
                let (j, _) = self.db.gm_sentence(name).unwrap_or((0, 0));
                let _ = self.db.gm_save_sentence(name, j, m * MS_PER_MIN);
                self.to_gms(&format!("Administración » {me} ha {} a {name} (offline).", if m > 0 { "silenciado" } else { "desilenciado" }));
            }
        }
        if m > 0 {
            self.penalty(name, me, "silencio", m as i64, "");
        }
        self.log_gm(me, if m > 0 { "silenciar" } else { "desilenciar" }, name, &format!("{m} min"));
    }

    /// HandleWarnUser: a warning in the history.
    fn gm_warn(&mut self, id: u32, me: &str, a: &[String]) {
        let reason = a.get(1).cloned().unwrap_or_default();
        if reason.is_empty() {
            return self.console(id, "Uso: /ADVERTENCIA nick@motivo", Tone::Info);
        }
        let Some(t) = self.target(id, a, "Uso: /ADVERTENCIA nick@motivo") else { return };
        let tn = self.name_of(t);
        self.console(t, format!("Has sido advertido por {me}: {reason}"), Tone::Warn);
        self.penalty(&tn, me, "advertencia", 0, &reason);
        self.log_gm(me, "advertencia", &tn, &reason);
        self.console(id, format!("{tn} ha sido advertido."), Tone::Good);
    }

    /// HandleKick.
    fn gm_kick(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(t) = self.target(id, a, "Uso: /ECHAR nick") else { return };
        if t == id {
            return self.console(id, "No puedes echarte a ti mismo.", Tone::Warn);
        }
        let tn = self.name_of(t);
        self.kick(t, &format!("{me} te ha echado del juego."));
        self.to_gms(&format!("Administración » {me} echó a {tn}."));
        self.log_gm(me, "echar", &tn, "");
    }

    /// HandleJail / Encarcelar. 0 minutes releases (warp to `[Libertad]`).
    fn gm_jail(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /CARCEL nick@motivo@minutos";
        let (reason, m) = (a.get(1).cloned().unwrap_or_default(), a.get(2).and_then(|m| m.parse::<u64>().ok()));
        let Some(m) = m.filter(|_| !reason.is_empty()) else { return self.console(id, USAGE, Tone::Info) };
        if m > MAX_JAIL_MINUTES {
            return self.console(id, format!("No podés encarcelar por más de {MAX_JAIL_MINUTES} minutos."), Tone::Warn);
        }
        let Some(t) = self.target(id, a, USAGE) else { return };
        if self.gm_rank(t).is_gm() {
            return self.console(id, "No podés encarcelar a administradores.", Tone::Warn);
        }
        let tn = self.name_of(t);
        if m == 0 {
            let was = self.player(t).unwrap().gm.jail_ms > 0;
            self.player_mut(t).gm.jail_ms = 0;
            self.persist_sentence(t);
            if was {
                self.teleport(t, FREEDOM.0, FREEDOM.1, FREEDOM.2);
                self.console(t, "Has sido liberado.", Tone::Good);
            }
            self.log_gm(me, "liberar", &tn, &reason);
            return self.console(id, format!("{tn} ha sido liberado."), Tone::Good);
        }
        self.player_mut(t).gm.jail_ms = m * MS_PER_MIN;
        self.persist_sentence(t);
        self.remove_jail_items(t);
        self.teleport(t, JAIL.0, JAIL.1, JAIL.2);
        self.console(t, format!("{me} te ha encarcelado, deberás permanecer en la cárcel {m} minutos."), Tone::Warn);
        self.penalty(&tn, me, "carcel", m as i64, &reason);
        self.log_gm(me, "carcel", &tn, &format!("{m} min: {reason}"));
        self.to_gms(&format!("Administración » {me} encarceló a {tn} por {m} minutos."));
    }

    /// RemoveCriticalObjectsWhileEnteringJail: fish, wood and minerals are lost.
    fn remove_jail_items(&mut self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let ao = self.ao.clone();
        let slots: Vec<(u8, u32)> = p
            .inv
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let it = (*s)?;
                let o = ao.objs.get(it.obj as u32)?;
                let fish = o.obj_type == ObjType::UseOnce && o.subtype == 1;
                (fish || matches!(o.obj_type, ObjType::Wood | ObjType::Minerals)).then_some(((i + 1) as u8, it.amount))
            })
            .collect();
        for (slot, amount) in slots {
            self.remove_from_slot(id, slot, amount);
        }
    }

    /// HandleBanChar / BanPJ: the character cannot enter again (`gm_ban`).
    fn gm_ban_char(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /BAN nick@motivo";
        let (Some(name), Some(reason)) = (a.first().cloned(), a.get(1).cloned().filter(|r| !r.is_empty())) else { return self.console(id, USAGE, Tone::Info) };
        if self.rank_of(&name) >= self.gm_rank(id) {
            return self.console(id, HIGHER, Tone::Warn);
        }
        let online = self.online(&name);
        if online.is_none() && !self.db.gm_character_exists(&name).unwrap_or(false) {
            return self.console(id, "El personaje no existe.", Tone::Warn);
        }
        let name = online.map(|t| self.name_of(t)).unwrap_or(name);
        let _ = self.db.gm_ban(&name, &reason, me);
        self.penalty(&name, me, "ban", 0, &reason);
        self.log_gm(me, "ban", &name, &reason);
        self.to_gms(&format!("Servidor » {me} ha baneado a {name}: {reason}"));
        if let Some(t) = online {
            self.kick(t, &format!("Has sido baneado: {reason}"));
        }
    }

    fn gm_unban_char(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(name) = a.first() else { return self.console(id, "Uso: /UNBAN nick", Tone::Info) };
        if self.db.gm_unban(name).unwrap_or(false) {
            self.penalty(name, me, "unban", 0, "");
            self.log_gm(me, "unban", name, "");
            self.console(id, format!("{name} ha sido desbaneado."), Tone::Good);
        } else {
            self.console(id, format!("{name} no está baneado."), Tone::Info);
        }
    }

    /// HandleBanCuenta / HandleUnBanCuenta: the whole account (`account.banned`).
    fn gm_ban_account(&mut self, id: u32, me: &str, a: &[String], ban: bool) {
        let usage = if ban { "Uso: /BANCUENTA nick@motivo" } else { "Uso: /UNBANCUENTA nick" };
        let Some(name) = a.first().cloned() else { return self.console(id, usage, Tone::Info) };
        let reason = a.get(1).cloned().unwrap_or_default();
        if ban && reason.is_empty() {
            return self.console(id, usage, Tone::Info);
        }
        if ban && self.rank_of(&name) >= self.gm_rank(id) {
            return self.console(id, HIGHER, Tone::Warn);
        }
        let online = self.online(&name);
        let account = match online {
            Some(t) => self.sessions.get(&t).and_then(|s| s.account.as_ref()).map(|acc| acc.id),
            None => self.db.gm_account_of(&name).ok().flatten(),
        };
        let Some(account) = account else { return self.console(id, "Ese personaje no tiene cuenta.", Tone::Warn) };
        let _ = self.db.gm_set_account_banned(account, ban);
        let kind = if ban { "bancuenta" } else { "unbancuenta" };
        self.penalty(&name, me, kind, 0, &reason);
        self.log_gm(me, kind, &name, &reason);
        self.console(id, format!("La cuenta de {name} ha sido {}.", if ban { "baneada" } else { "desbaneada" }), Tone::Good);
        if ban {
            // Every character of that account that is playing.
            let playing: Vec<u32> = self.player_ids().into_iter().filter(|p| self.sessions.get(p).and_then(|s| s.account.as_ref()).is_some_and(|acc| acc.id == account)).collect();
            for t in playing {
                self.kick(t, &format!("Tu cuenta ha sido baneada: {reason}"));
            }
        }
    }

    /// HandleBanIP: by address, or by the address of an online character.
    fn gm_ban_ip(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /BANIP nick-o-ip@motivo";
        let (Some(who), Some(reason)) = (a.first().cloned(), a.get(1).cloned().filter(|r| !r.is_empty())) else { return self.console(id, USAGE, Tone::Info) };
        let ip: IpAddr = match who.parse() {
            Ok(ip) => ip,
            Err(_) => match self.online(&who) {
                Some(t) if self.gm_rank(t) >= self.gm_rank(id) => return self.console(id, HIGHER, Tone::Warn),
                Some(t) => self.sessions[&t].addr.ip(),
                None => return self.console(id, NOT_ONLINE, Tone::Warn),
            },
        };
        let _ = self.db.gm_ban_ip(&ip.to_string(), &reason, me);
        self.log_gm(me, "banip", &who, &format!("{ip}: {reason}"));
        self.console(id, format!("La IP {ip} ha sido baneada."), Tone::Good);
        // Everyone from there except GMs (who are never IP-banned).
        let from: Vec<u32> = self.player_ids().into_iter().filter(|p| !self.gm_rank(*p).is_gm() && self.sessions[p].addr.ip() == ip).collect();
        for t in from {
            let tn = self.name_of(t);
            self.penalty(&tn, me, "banip", 0, &reason);
            self.kick(t, &format!("Tu dirección ha sido baneada: {reason}"));
        }
    }

    fn gm_unban_ip(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(ip) = a.first().and_then(|s| s.parse::<IpAddr>().ok()) else { return self.console(id, "Uso: /UNBANIP ip", Tone::Info) };
        if self.db.gm_unban_ip(&ip.to_string()).unwrap_or(false) {
            self.log_gm(me, "unbanip", &ip.to_string(), "");
            self.console(id, format!("La IP {ip} ha sido desbaneada."), Tone::Good);
        } else {
            self.console(id, format!("La IP {ip} no está baneada."), Tone::Info);
        }
    }

    // ------------------------------------------------------------ moving people

    fn gm_warp(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /TELEP nick|YO mapa x y";
        let n: Vec<i32> = a.iter().skip(1).filter_map(|s| s.parse().ok()).collect();
        let [map, x, y] = n[..] else { return self.console(id, USAGE, Tone::Info) };
        if map <= 0 || map > u16::MAX as i32 || !(1..=100).contains(&x) || !(1..=100).contains(&y) {
            return self.console(id, "Posición inválida.", Tone::Warn);
        }
        let t = if a[0].eq_ignore_ascii_case("YO") { id } else {
            match self.target(id, a, USAGE) {
                Some(t) => t,
                None => return,
            }
        };
        let tn = self.name_of(t);
        self.teleport(t, map as u16, x, y);
        self.log_gm(me, "telep", &tn, &format!("{map} {x} {y}"));
    }

    fn gm_goto(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(name) = a.first() else { return self.console(id, "Uso: /IRA nick", Tone::Info) };
        let Some(t) = self.online(name) else { return self.console(id, NOT_ONLINE, Tone::Warn) };
        let (map, x, y, tn) = {
            let p = self.player(t).unwrap();
            (p.map, p.x, p.y, p.name.clone())
        };
        self.teleport(id, map, x, y);
        self.log_gm(me, "ira", &tn, &format!("{map} {x} {y}"));
    }

    fn gm_summon(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(t) = self.target(id, a, "Uso: /SUM nick") else { return };
        if t == id {
            return;
        }
        let (map, x, y) = {
            let p = self.player(id).unwrap();
            (p.map, p.x, p.y)
        };
        let tn = self.name_of(t);
        self.teleport(t, map, x, y);
        self.console(t, format!("{me} te ha trasportado."), Tone::Info);
        self.log_gm(me, "sum", &tn, &format!("{map} {x} {y}"));
    }

    // ------------------------------------------------------------ the character

    fn gm_info(&mut self, id: u32, a: &[String]) {
        let Some(name) = a.first() else { return self.console(id, "Uso: /INFO nick", Tone::Info) };
        let Some(t) = self.online(name) else {
            let pen = self.db.gm_penalties(name).map(|v| v.len()).unwrap_or(0);
            let ban = self.db.gm_ban_reason(name).ok().flatten();
            return self.console(id, format!("{name} (offline): rango {}, {pen} penas{}.", self.rank_of(name).label(), ban.map(|r| format!(", baneado: {r}")).unwrap_or_default()), Tone::Info);
        };
        let s = &self.sessions[&t];
        let p = s.player.as_ref().unwrap();
        let account = s.account.as_ref().map(|acc| acc.user.clone()).unwrap_or_else(|| "(sin cuenta)".into());
        let pen = self.db.gm_penalties(&p.name).map(|v| v.len()).unwrap_or(0);
        let lines = [
            format!("Personaje: {} ({}), nivel {}, cuenta {account}, IP {}", p.name, p.gm.rank.label(), p.level, s.addr.ip()),
            format!("Mapa {} ({}, {}), vida {}/{}, maná {}/{}, oro {}, banco {}", p.map, p.x, p.y, p.hp, p.max_hp, p.mana, p.max_mana, p.inv.gold, p.bank.gold),
            format!("Cárcel: {} min, silencio: {} min, penas: {pen}{}", mins(p.gm.jail_ms), mins(p.gm.silence_ms), if p.combat.dead { ", MUERTO" } else { "" }),
        ];
        for l in lines {
            self.console(id, l, Tone::Info);
        }
    }

    fn gm_revive(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(name) = a.first() else { return self.console(id, "Uso: /REVIVIR nick", Tone::Info) };
        let Some(t) = self.online(name) else { return self.console(id, NOT_ONLINE, Tone::Warn) };
        let tn = self.name_of(t);
        if self.is_dead(t) {
            self.resurrect(t);
            self.console(t, format!("{me} te ha resucitado."), Tone::Good);
        } else {
            // HandleReviveChar also heals the living.
            let p = self.player_mut(t);
            p.hp = p.max_hp;
            let (map, hp, max_hp) = (p.map, p.hp, p.max_hp);
            self.broadcast(map, &S2C::Hp { id: t, hp, max_hp }, None);
            self.send_stats(t);
            self.console(t, format!("{me} te ha curado."), Tone::Good);
        }
        self.log_gm(me, "revivir", &tn, "");
    }

    fn gm_execute(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(t) = self.target(id, a, "Uso: /EJECUTAR nick") else { return };
        if t == id || self.is_dead(t) {
            return self.console(id, "No puedes ejecutar a ese personaje.", Tone::Warn);
        }
        let tn = self.name_of(t);
        self.user_die(t);
        self.console(t, format!("{me} te ha ejecutado."), Tone::Warn);
        self.log_gm(me, "ejecutar", &tn, "");
    }

    /// DoAdminInvisible: the spell invisibility without end (NPCs and players stop seeing him).
    fn gm_invisible(&mut self, id: u32, me: &str) {
        let p = self.player_mut(id);
        p.gm.invisible = !p.gm.invisible;
        let on = p.gm.invisible;
        p.status.invisible_until = on.then_some(GameTime(u64::MAX / 2));
        let map = p.map;
        let flags = self.status_flags(id);
        self.broadcast(map, &S2C::Status { id, flags }, None);
        self.console(id, if on { "Ahora eres invisible." } else { "Vuelves a ser visible." }, Tone::Info);
        self.log_gm(me, "invisible", "", if on { "on" } else { "off" });
    }

    fn gm_edit(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /MOD nick ORO cantidad";
        let (Some(name), Some(what), Some(v)) = (a.first(), a.get(1), a.get(2).and_then(|v| v.parse::<u32>().ok())) else { return self.console(id, USAGE, Tone::Info) };
        if !what.eq_ignore_ascii_case("ORO") {
            return self.console(id, USAGE, Tone::Info);
        }
        let Some(t) = self.online(name) else { return self.console(id, NOT_ONLINE, Tone::Warn) };
        let tn = self.name_of(t);
        self.player_mut(t).inv.gold = v;
        self.send_inventory_vitals(t);
        self.log_gm(me, "mod oro", &tn, &v.to_string());
        self.console(id, format!("{tn} tiene ahora {v} monedas de oro."), Tone::Good);
    }

    fn valid_obj(&self, obj: u32, amount: u32) -> bool {
        obj > 0 && obj <= u16::MAX as u32 && (1..=MAX_STACK).contains(&amount) && self.ao.objs.get(obj).is_some()
    }

    fn gm_create_item(&mut self, id: u32, me: &str, a: &[String]) {
        let obj = a.first().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
        let amount = a.get(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
        if !self.valid_obj(obj, amount) {
            return self.console(id, "Uso: /CI objeto [cantidad] (objeto de obj.dat).", Tone::Info);
        }
        if self.add_item(id, InvItem { obj: obj as u16, amount }) {
            self.log_gm(me, "ci", me, &format!("{amount} x {obj}"));
        }
    }

    fn gm_give(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /DAR nick@objeto@cantidad[@motivo]";
        let (Some(name), Some(obj), Some(amount)) = (a.first(), a.get(1).and_then(|s| s.parse::<u32>().ok()), a.get(2).and_then(|s| s.parse::<u32>().ok())) else { return self.console(id, USAGE, Tone::Info) };
        if !self.valid_obj(obj, amount) {
            return self.console(id, USAGE, Tone::Info);
        }
        let Some(t) = self.online(name) else { return self.console(id, NOT_ONLINE, Tone::Warn) };
        let tn = self.name_of(t);
        if self.add_item(t, InvItem { obj: obj as u16, amount }) {
            self.console(t, format!("{me} te ha dado {amount} x objeto {obj}."), Tone::Good);
            self.log_gm(me, "dar", &tn, format!("{amount} x {obj} {}", a.get(3).cloned().unwrap_or_default()).trim_end());
            self.console(id, format!("Diste {amount} x {obj} a {tn}."), Tone::Good);
        } else {
            self.console(id, format!("{tn} no puede cargar más objetos."), Tone::Warn);
        }
    }

    fn gm_take(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /QUITAR nick@objeto@cantidad";
        let (Some(name), Some(obj), Some(mut amount)) = (a.first(), a.get(1).and_then(|s| s.parse::<u16>().ok()), a.get(2).and_then(|s| s.parse::<u32>().ok())) else { return self.console(id, USAGE, Tone::Info) };
        let Some(t) = self.online(name) else { return self.console(id, NOT_ONLINE, Tone::Warn) };
        let tn = self.name_of(t);
        let slots: Vec<(u8, u32)> = self.player(t).unwrap().inv.slots.iter().enumerate().filter_map(|(i, s)| s.filter(|it| it.obj == obj).map(|it| ((i + 1) as u8, it.amount))).collect();
        let mut taken = 0;
        for (slot, have) in slots {
            if amount == 0 {
                break;
            }
            let n = have.min(amount);
            self.remove_from_slot(t, slot, n);
            amount -= n;
            taken += n;
        }
        self.log_gm(me, "quitar", &tn, &format!("{taken} x {obj}"));
        self.console(id, format!("Quitaste {taken} x {obj} a {tn}."), Tone::Good);
    }

    // ------------------------------------------------------------ NPCs

    /// HandleCreateNPC / HandleCreateNPCWithRespawn: next to the GM.
    fn gm_create_npc(&mut self, id: u32, me: &str, a: &[String], respawn: bool) {
        let Some(number) = a.first().and_then(|s| s.parse::<u16>().ok()) else { return self.console(id, "Uso: /ACC npc (número de npcs.dat)", Tone::Info) };
        let ao = self.ao.clone();
        let Some(d) = ao.npcs.get(number as u32).filter(|d| !d.name.is_empty()) else { return self.console(id, "Ese NPC no existe.", Tone::Warn) };
        let (map, gx, gy) = {
            let p = self.player(id).unwrap();
            (p.map, p.x, p.y)
        };
        let (x, y) = self.free_tile_near(map, gx, gy + 1, 2);
        let now = self.now();
        let respawn_secs = self.rng.range(d.respawn_interval_min.max(0), d.respawn_interval_max.max(d.respawn_interval_min).max(0)) as u32;
        let vision = if d.path_vision > 0 { d.path_vision } else { crate::systems::npc_ai::VISION_X };
        let npc = Npc {
            def: number,
            name: d.name.clone(),
            body: d.body.max(0) as u16,
            head: d.head.max(0) as u16,
            x,
            y,
            dir: Dir::Down,
            hp: d.min_hp.max(1),
            max_hp: d.max_hp.max(1),
            home: (x, y),
            next_move: now,
            next_attack: now,
            dead_until: None,
            kind: d.npc_type,
            movement: d.movement,
            hostile: d.hostile == 1,
            attackable: d.attackable == 1,
            old_movement: d.movement,
            old_hostile: d.hostile == 1,
            target: None,
            attacked_by: None,
            vision,
            original_vision: vision,
            path: Vec::new(),
            path_dest: (0, 0),
            next_path_at: GameTime::ZERO,
            next_spell: now,
            walk_step: 0,
            // `/ACC`: a respawn so far away that the NPC system never brings it back before
            // `tick_no_respawn` removes it (a 0 s respawn would revive it in the same tick).
            respawn_secs: if respawn { respawn_secs } else { u32::MAX / 4 },
            respawn_at: None,
        };
        let nid = self.next_npc;
        self.next_npc += 1;
        let msg = npc.spawn_msg(nid);
        self.maps.get_mut(&map).unwrap().add_npc(nid, npc);
        self.broadcast(map, &msg, None);
        if !respawn {
            self.gms.no_respawn.insert((map, nid));
        }
        self.log_gm(me, if respawn { "racc" } else { "acc" }, &d.name, &format!("{number} en {map} {x} {y}"));
    }

    /// HandleKillNPCNoRespawn: the closest live NPC (2 tiles), gone for good.
    fn gm_kill_npc(&mut self, id: u32, me: &str) {
        let (map, x, y) = {
            let p = self.player(id).unwrap();
            (p.map, p.x, p.y)
        };
        let inst = self.instance(map);
        let near = inst
            .npcs
            .iter()
            .filter(|(_, n)| n.alive() && (n.x - x).abs().max((n.y - y).abs()) <= 2)
            .min_by_key(|(nid, n)| ((n.x - x).abs() + (n.y - y).abs(), **nid))
            .map(|(nid, n)| (*nid, n.name.clone()));
        let Some((nid, name)) = near else { return self.console(id, "No hay ningún NPC cerca.", Tone::Info) };
        self.maps.get_mut(&map).unwrap().remove_npc(nid);
        self.gms.no_respawn.remove(&(map, nid));
        self.broadcast(map, &S2C::Despawn { id: nid }, None);
        self.log_gm(me, "mata", &name, &format!("{map} {x} {y}"));
    }

    // ------------------------------------------------------------ administration

    /// Sets a rank (replaces editing Server.ini). Not one's own.
    fn gm_set_role(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /ROL nick@rango (usuario, consejero, semidios, dios, admin)";
        let (Some(name), Some(rank)) = (a.first().cloned(), a.get(1).and_then(|r| GmRank::parse(r))) else { return self.console(id, USAGE, Tone::Info) };
        if name.eq_ignore_ascii_case(me) {
            return self.console(id, "No puedes cambiar tu propio rango.", Tone::Warn);
        }
        if self.rank_of(&name) >= self.gm_rank(id) {
            return self.console(id, HIGHER, Tone::Warn);
        }
        let online = self.online(&name);
        // Bound to that exact character (online: its id; dev logins have none).
        let char_id = match online {
            Some(t) => self.player(t).and_then(|p| p.char_id),
            None => match self.db.gm_char_id(&name).ok().flatten() {
                Some(c) => Some(c),
                None => return self.console(id, "El personaje no existe.", Tone::Warn),
            },
        };
        let name = online.map(|t| self.name_of(t)).unwrap_or(name);
        let _ = self.db.gm_set_rank(&name, char_id, rank, me);
        if let Some(t) = online {
            self.player_mut(t).gm.rank = rank;
            self.send(t, &S2C::Gm(GmS2C::Rank { rank }));
            self.console(t, format!("Tu rango ahora es {}.", rank.label()), Tone::Good);
        }
        self.log_gm(me, "rol", &name, rank.label());
        self.console(id, format!("{name} ahora es {}.", rank.label()), Tone::Good);
    }

    /// Shutdown with notice; 0 cancels.
    fn gm_shutdown(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(m) = a.first().and_then(|s| s.parse::<u64>().ok()).filter(|m| *m <= 60) else { return self.console(id, "Uso: /APAGAR minutos (0 = cancelar, máximo 60)", Tone::Info) };
        if m == 0 {
            if self.gms.shutdown.take().is_some() {
                self.broadcast_all(&S2C::Console { text: "Servidor » Se canceló el apagado.".into(), color: Tone::Good }, None);
                self.log_gm(me, "apagar", "", "cancelado");
            }
            return;
        }
        let now = self.now();
        self.gms.shutdown = Some(Shutdown { at: now.after_ms(m * MS_PER_MIN), by: me.to_string(), next_notice: now });
        self.log_gm(me, "apagar", "", &format!("{m} min"));
        self.tick_shutdown(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_split_like_the_vb_client() {
        assert_eq!(args("Pepe@flood de chat@10"), ["Pepe", "flood de chat", "10"]);
        assert_eq!(args("  Pepe  5 "), ["Pepe", "5"]);
        assert_eq!(args("Juan+Perez@x"), ["Juan Perez", "x"]);
        assert!(args("   ").is_empty());
    }

    /// `/ACC` NPC killed by combat (not `/MATA`): the NPC system's respawn must not bring it
    /// back, even across a long time.
    #[test]
    fn acc_npc_killed_in_combat_never_respawns() {
        use crate::net::Event;
        use crate::Settings;
        use ao_protocol::PROTOCOL_VERSION;
        let mut w = World::new(Settings::defaults());
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        w.on_event(Event::Connected { id: 1, addr: "127.0.0.1:9".parse().unwrap(), tx });
        w.on_event(Event::Frame { id: 1, msg: ao_protocol::C2S::Login { name: "Gema".into(), version: PROTOCOL_VERSION } });
        w.player_mut(1).gm.rank = GmRank::Admin;
        let map = w.player(1).unwrap().map;
        let before: Vec<u32> = w.instance(map).npc_ids().to_vec();
        assert!(w.gm_command(1, "/acc 500"));
        let nid = *w.instance(map).npc_ids().iter().find(|n| !before.contains(n)).expect("npc created");
        // What combat's kill_npc does.
        let now = GameTime(1000);
        w.maps.get_mut(&map).unwrap().npcs.get_mut(&nid).unwrap().dead_until = Some(now);
        for t in 1..=200u64 {
            w.tick(GameTime(1000 + t * 10_000));
        }
        assert!(!w.instance(map).npcs.contains_key(&nid), "the /ACC NPC is gone for good");
        assert!(w.gms.no_respawn.is_empty());
    }

    #[test]
    fn seed_parses_names_and_ranks() {
        assert_eq!(parse_seed("Ana:admin, Beto:Dios,bad,Carla:nada,:dios"), [("Ana".to_string(), GmRank::Admin), ("Beto".to_string(), GmRank::Dios)]);
    }
}

//! Phase 5 GM commands: temporary bans, inspecting players, moderation, the help queue
//! (SOS and reports) and map / world commands. The basic commands, ranks, helpers and the
//! dispatcher are in `systems/gm.rs`; this file only adds handlers.
//!
//! Ported from `Protocol_GmCommands.bas`, `Admin.bas` (BanTemporal, SaveBans, the
//! `EstadoTimer` sweep in frmMain), `ModCola.cls` (`Ayuda`), `TCP.bas` (SendMOTD) and
//! `General.bas` (CuentaRegresivaTimer). Minimum ranks follow the VB masks (RoleMaster is
//! not ported):
//!
//! | Command | Rank | VB |
//! |---|---|---|
//! | `/SHOW SOS` (lista de ayuda) | Consejero | HandleSOSShowList |
//! | `/ATENDER id` | Consejero | HandleSOSRemove |
//! | `/CR segundos` | Consejero | HandleCuentaRegresiva |
//! | `/BANT nick@motivo@días` | SemiDios | HandleBanTemporal (sólo panel en el VB) |
//! | `/STAT`, `/INV`, `/BOV`, `/SKILLS nick` | SemiDios | HandleRequestChar{Stats,Inventory,Bank,Skills} |
//! | `/NICK2IP nick`, `/IP2NICK ip` | SemiDios | HandleNickToIP / HandleIPToNick |
//! | `/ONLINEGM`, `/ONLINEMAP [mapa]` | SemiDios | HandleOnlineGM / HandleOnlineMap |
//! | `/LIMPIAR` | SemiDios | HandleCleanWorld |
//! | `/ESTUPIDO nick`, `/NOESTUPIDO nick` | Dios | HandleMakeDumb / HandleMakeDumbNoMore |
//! | `/BORRARPENA nick@número@texto` | Dios | HandleRemovePunishment |
//! | `/ECHARTODOSPJS` | Dios | HandleKickAllChars |
//! | `/GRABAR` | Dios | HandleSaveChars |
//! | `/BORRAR SOS` | Dios | HandleCleanSOS |
//! | `/CT mapa x y [radio]`, `/DT [x y]` | Dios | HandleTeleportCreate / HandleTeleportDestroy |
//! | `/MODMAPINFO opción valor` | Dios | HandleChangeMapInfo* |
//! | `/MOTDCAMBIA [línea|línea]` | Dios | HandleChangeMOTD / HandleSetMOTD |
//! | `/GLOBAL` | Dios | HandleGlobalOnOff |
//! | `/BAL nick` | Admin | HandleRequestCharGold |
//!
//! Player commands (anyone): `/GM [tipo@]texto` (HandleQuestionGM), `/DENUNCIAR nick`
//! (HandleDenounce), `/MOTD` (HandleRequestMOTD).
//!
//! VB defects fixed here:
//! - BanTemporal did not check the target's rank nor that the character existed, and did
//!   not kick an online target. Banning twice added a second `baneos.dat` entry: the first
//!   to expire unbanned the character. The sweep removed entries inside
//!   `For i = 1 To Baneos.count`, skipping the one after each removal.
//! - CleanWorld only printed "Se han limpiado los items del suelo": nothing was cleaned.
//! - GlobalOnOff announced the change but never flipped `ChatGlobal`.
//! - ChangeMapInfoPK stored `Seguro = 1` for a PK map (inverted) while saying "Seguro: No".
//! - ChangeMapInfoNoMagic / NoInvi only wrote the log; here they set the map restriction.
//! - The help queue lived in memory (lost on restart) and SOSRemove matched the whole text;
//!   here it is in the database, by id, and the author is told when a GM attends it.
//! - MakeDumb had no rank check: a Dios could make an Admin dumb.
use crate::clock::GameTime;
use crate::db::now_unix;
use crate::systems::gm::{ban_left, DENIED, HIGHER, MS_PER_MIN, NOT_ONLINE};
use crate::world::gm::{Countdown, CHAT_HISTORY, COUNTDOWN_AREA, MAX_BAN_DAYS, PANEL_SOS_ROWS, SOS_INTERVAL_MS};
use crate::world::World;
use ao_formats::csm::{RESTRICT_NEWBIE, RESTRICT_NO_CITIZENS, RESTRICT_NO_CRIMINALS, RESTRICT_NO_INVISIBILITY, RESTRICT_NO_MAGIC};
use ao_formats::dat::obj::ObjType;
use ao_protocol::p4::gm::{GmRank, GmSosRow};
use ao_protocol::{Tone, S2C};
use std::net::IpAddr;

/// `FrmGmAyuda` options (`TipoDeConsulta`).
pub const SOS_KINDS: [&str; 7] = ["Consulta regular", "Descargo", "Acusación", "Sugerencia", "Reporte de bug", "Otro", "Queja"];
/// Open requests one player may have at once (the VB had no limit).
pub const MAX_OPEN_SOS: usize = 3;
/// Settings keys (`gm_setting`).
const KEY_MOTD: &str = "motd";
const KEY_GLOBAL_OFF: &str = "global_off";
/// `/CT` teleport objects by `Radio` (HandleTeleportCreate).
const TELEPORT_OBJS: [u16; 3] = [370, 371, 378];
const TERRAINS: [&str; 6] = ["BOSQUE", "NIEVE", "DESIERTO", "CIUDAD", "CAMPO", "DUNGEON"];

fn yes_no(b: bool) -> &'static str {
    if b {
        "Sí"
    } else {
        "No"
    }
}

impl World {
    /// Minimum rank of a phase 5 command (None = not one of them).
    pub(crate) fn gm_mod_rank(cmd: &str) -> Option<GmRank> {
        Some(match cmd {
            "/SHOW" | "/ATENDER" | "/CR" => GmRank::Consejero,
            "/BANT" | "/STAT" | "/INV" | "/BOV" | "/SKILLS" | "/NICK2IP" | "/IP2NICK" | "/ONLINEGM" | "/ONLINEMAP" | "/LIMPIAR" => GmRank::SemiDios,
            "/ESTUPIDO" | "/NOESTUPIDO" | "/BORRARPENA" | "/ECHARTODOSPJS" | "/GRABAR" | "/BORRAR" | "/CT" | "/DT" | "/MODMAPINFO" | "/MOTDCAMBIA" | "/GLOBAL" => GmRank::Dios,
            "/BAL" => GmRank::Admin,
            _ => return None,
        })
    }

    /// Runs a phase 5 command (rank already checked).
    pub(crate) fn gm_mod(&mut self, id: u32, me: &str, cmd: &str, rest: &str, a: &[String]) {
        match cmd {
            "/SHOW" => match a.first().map(|s| s.to_uppercase()).as_deref() {
                Some("SOS") => self.gm_sos_list(id),
                _ => self.console(id, "Uso: /SHOW SOS", Tone::Info),
            },
            "/ATENDER" => self.gm_sos_attend(id, me, a),
            "/CR" => self.gm_countdown(id, me, a),
            "/BANT" => self.gm_ban_temporal(id, me, a),
            "/STAT" => self.gm_stat(id, me, a),
            "/INV" => self.gm_inventory(id, me, a),
            "/BOV" => self.gm_bank(id, me, a),
            "/SKILLS" => self.gm_skills(id, me, a),
            "/BAL" => self.gm_gold(id, me, a),
            "/NICK2IP" => self.gm_nick_to_ip(id, me, a),
            "/IP2NICK" => self.gm_ip_to_nick(id, me, a),
            "/ONLINEGM" => self.gm_online_gm(id),
            "/ONLINEMAP" => self.gm_online_map(id, a),
            "/LIMPIAR" => self.gm_clean_world(id, me),
            "/ESTUPIDO" => self.gm_dumb(id, me, a, true),
            "/NOESTUPIDO" => self.gm_dumb(id, me, a, false),
            "/BORRARPENA" => self.gm_remove_punishment(id, me, a),
            "/ECHARTODOSPJS" => self.gm_kick_all(id, me),
            "/GRABAR" => {
                self.autosave_all();
                self.log_gm(me, "grabar", "", "");
                self.console(id, "Se han guardado todos los personajes.", Tone::Good);
            }
            "/BORRAR" => match a.first().map(|s| s.to_uppercase()).as_deref() {
                Some("SOS") => self.gm_sos_clear(id, me),
                _ => self.console(id, "Uso: /BORRAR SOS", Tone::Info),
            },
            "/CT" => self.gm_teleport_create(id, me, a),
            "/DT" => self.gm_teleport_destroy(id, me, a),
            "/MODMAPINFO" => self.gm_map_info(id, me, a),
            "/MOTDCAMBIA" => self.gm_set_motd(id, me, rest),
            "/GLOBAL" => self.gm_global_toggle(me),
            _ => self.console(id, DENIED, Tone::Warn),
        }
    }

    // ------------------------------------------------------------ hooks

    /// Unix seconds following the game clock (see `GmWorld::unix_base`).
    pub(crate) fn unix_now(&self) -> i64 {
        let secs = (self.now().ms() / 1000) as i64;
        let base = match self.gms.unix_base.get() {
            Some(b) => b,
            None => {
                let b = now_unix() - secs;
                self.gms.unix_base.set(Some(b));
                b
            }
        };
        base + secs + self.gms.unix_skew
    }

    /// Every world tick (from `tick_gm`): ended temporary bans, the `/CR` countdown.
    pub(crate) fn tick_moderation(&mut self, now: GameTime) {
        if now >= self.gms.next_ban_sweep {
            self.gms.next_ban_sweep = now.after_ms(MS_PER_MIN);
            for name in self.db.gm_expire_bans(self.unix_now()).unwrap_or_default() {
                self.to_gms(&format!("Servidor » Se ha concluido la sentencia de ban para {name}."));
                self.log_gm("Servidor", "fin ban", &name, "");
            }
        }
        if let Some(c) = &mut self.gms.countdown {
            if now >= c.next {
                let left = c.left;
                c.left -= 1;
                c.next = c.next.after_ms(1000);
                if c.left == 0 {
                    self.gms.countdown = None;
                }
                let text = if left > 1 { format!("{} segundos...!", left - 1) } else { "¡Ya!".to_string() };
                self.broadcast_all(&S2C::Console { text, color: Tone::Good }, None);
            }
        }
    }

    /// After entering (from `gm_on_enter`): the message of the day (SendMOTD) and, for
    /// GMs, how many help requests wait.
    pub(crate) fn gm_mod_on_enter(&mut self, id: u32) {
        for line in self.motd() {
            self.console(id, line, Tone::Info);
        }
        if self.gm_rank(id).is_gm() {
            let n = self.db.gm_sos_open().map(|v| v.len()).unwrap_or(0);
            if n > 0 {
                self.console(id, format!("Hay {n} pedidos de ayuda sin atender (/SHOW SOS o el panel)."), Tone::Warn);
            }
        }
    }

    /// HOOK (chat): a line the player said, kept for `/DENUNCIAR` (`flags.ChatHistory`).
    pub(crate) fn gm_chat_record(&mut self, id: u32, text: &str) {
        let Some(p) = self.sessions.get_mut(&id).and_then(|s| s.player.as_mut()) else { return };
        let h = &mut p.gm.chat_history;
        h.push_back(text.to_string());
        while h.len() > CHAT_HISTORY {
            h.pop_front();
        }
    }

    /// HOOK (chat): `/GLOBAL` turned the global chat off.
    pub(crate) fn global_chat_off(&mut self) -> bool {
        if self.gms.global_off.is_none() {
            let off = self.db.gm_setting(KEY_GLOBAL_OFF).ok().flatten().is_some_and(|v| v == "1");
            self.gms.global_off = Some(off);
        }
        self.gms.global_off == Some(true)
    }

    fn motd(&mut self) -> Vec<String> {
        if self.gms.motd.is_none() {
            let text = self.db.gm_setting(KEY_MOTD).ok().flatten().unwrap_or_default();
            self.gms.motd = Some(text.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect());
        }
        self.gms.motd.clone().unwrap_or_default()
    }

    /// Open requests for the panel.
    pub(crate) fn sos_rows(&self) -> Vec<GmSosRow> {
        let rows = self.db.gm_sos_open().unwrap_or_default();
        rows.into_iter()
            .take(PANEL_SOS_ROWS)
            .map(|r| GmSosRow { online: self.online(&r.name).is_some(), id: r.id, name: r.name, kind: r.kind, text: r.text, at: r.at })
            .collect()
    }

    // ------------------------------------------------------------ player commands

    /// `/GM`, `/DENUNCIAR`, `/MOTD` (anyone). Returns true if the line was one of them.
    pub(crate) fn sos_player_command(&mut self, id: u32, line: &str) -> bool {
        let (cmd, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        match cmd.to_uppercase().as_str() {
            "/GM" => self.sos_question(id, rest.trim()),
            "/DENUNCIAR" => self.sos_denounce(id, rest.trim()),
            "/MOTD" => {
                let lines = self.motd();
                if lines.is_empty() {
                    self.console(id, "No hay mensaje del día.", Tone::Info);
                }
                for l in lines {
                    self.console(id, l, Tone::Info);
                }
            }
            _ => return false,
        }
        true
    }

    fn open_requests_of(&self, name: &str) -> usize {
        self.db.gm_sos_open().unwrap_or_default().iter().filter(|r| r.name.eq_ignore_ascii_case(name)).count()
    }

    /// HandleQuestionGM: `/GM texto` or `/GM tipo@texto`, one every 5 minutes.
    fn sos_question(&mut self, id: u32, rest: &str) {
        let (kind, text) = match rest.split_once('@') {
            Some((k, t)) => match SOS_KINDS.iter().find(|s| s.eq_ignore_ascii_case(k.trim())) {
                Some(k) => (*k, t.trim()),
                None => return self.console(id, format!("Tipos de consulta: {}.", SOS_KINDS.join(", ")), Tone::Info),
            },
            None => (SOS_KINDS[0], rest),
        };
        if text.is_empty() {
            return self.console(id, "Uso: /GM texto (o /GM tipo@texto). Un administrador te responderá.", Tone::Info);
        }
        let now = self.now();
        let name = self.name_of(id);
        if self.player(id).and_then(|p| p.gm.last_question).is_some_and(|t| now.since(t) < SOS_INTERVAL_MS) {
            return self.console(id, "Solo puedes enviar una consulta cada 5 minutos.", Tone::Warn);
        }
        if self.open_requests_of(&name) >= MAX_OPEN_SOS {
            return self.console(id, "Ya tienes consultas sin atender. Espera a que un administrador las vea.", Tone::Warn);
        }
        let _ = self.db.gm_sos_push(&name, kind, text);
        self.player_mut(id).gm.last_question = Some(now);
        self.to_gms(&format!("Se ha recibido un nuevo mensaje de soporte de {name}."));
        self.console(id, "Tu mensaje fue recibido por el equipo de soporte.", Tone::Good);
    }

    /// HandleDenounce: the last lines the target said go to the queue. The answer is the
    /// same whether the target is online or not (the VB did it so nobody uses it to know
    /// who is connected).
    fn sos_denounce(&mut self, id: u32, nick: &str) {
        const RECEIVED: &str = "Tu denuncia fue recibida por el equipo de soporte.";
        if nick.is_empty() {
            return self.console(id, "Uso: /DENUNCIAR nick", Tone::Info);
        }
        if self.rank_of(nick).is_gm() {
            return self.console(id, "No podés denunciar a un administrador.", Tone::Warn);
        }
        let Some(t) = self.online(nick) else { return self.console(id, RECEIVED, Tone::Good) };
        let me = self.name_of(id);
        if t == id {
            return self.console(id, "No puedes denunciarte a ti mismo.", Tone::Warn);
        }
        let (tn, lines) = {
            let p = self.player(t).unwrap();
            (p.name.clone(), p.gm.chat_history.iter().cloned().collect::<Vec<_>>())
        };
        if lines.is_empty() {
            return self.console(id, "El usuario no ha escrito nada. Recordá que las denuncias inválidas pueden ser motivo de advertencia.", Tone::Warn);
        }
        if self.open_requests_of(&me) >= MAX_OPEN_SOS {
            return self.console(id, "Ya tienes consultas sin atender. Espera a que un administrador las vea.", Tone::Warn);
        }
        let text = format!("[Últimos mensajes de {tn}] {}", lines.join(" | "));
        let _ = self.db.gm_sos_push(&me, &format!("Denuncia a {tn}"), &text);
        self.to_gms(&format!("Se ha recibido una nueva denuncia de parte de {me}."));
        self.console(id, RECEIVED, Tone::Good);
    }

    // ------------------------------------------------------------ help queue (GM side)

    fn gm_sos_list(&mut self, id: u32) {
        let rows = self.sos_rows();
        if rows.is_empty() {
            return self.console(id, "No hay pedidos de ayuda.", Tone::Info);
        }
        self.console(id, format!("Pedidos de ayuda ({}):", rows.len()), Tone::Info);
        for r in rows {
            let on = if r.online { "" } else { " (offline)" };
            self.console(id, format!("#{} {}{} [{}]: {}", r.id, r.name, on, r.kind, r.text), Tone::Info);
        }
    }

    fn gm_sos_attend(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(n) = a.first().and_then(|s| s.trim_start_matches('#').parse::<i64>().ok()) else { return self.console(id, "Uso: /ATENDER número (de /SHOW SOS)", Tone::Info) };
        match self.db.gm_sos_close(n, me).ok().flatten() {
            Some(r) => {
                if let Some(t) = self.online(&r.name) {
                    self.console(t, format!("{me} atendió tu consulta ({}).", r.kind), Tone::Good);
                }
                self.log_gm(me, "atender", &r.name, &format!("#{n} {}", r.kind));
                self.console(id, format!("Consulta #{n} de {} atendida.", r.name), Tone::Good);
            }
            None => self.console(id, "No hay ningún pedido abierto con ese número.", Tone::Warn),
        }
    }

    fn gm_sos_clear(&mut self, id: u32, me: &str) {
        let n = self.db.gm_sos_clear(me).unwrap_or(0);
        self.log_gm(me, "borrar sos", "", &n.to_string());
        self.console(id, format!("Se borraron {n} pedidos de ayuda."), Tone::Good);
    }

    // ------------------------------------------------------------ bans

    /// HandleBanTemporal / Admin.BanTemporal: `/BANT nick@motivo@días`.
    fn gm_ban_temporal(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /BANT nick@motivo@días";
        let (Some(name), Some(reason), Some(days)) = (a.first().cloned(), a.get(1).cloned().filter(|r| !r.is_empty()), a.get(2).and_then(|d| d.parse::<u32>().ok())) else { return self.console(id, USAGE, Tone::Info) };
        if days == 0 || days > MAX_BAN_DAYS {
            return self.console(id, format!("Los días van de 1 a {MAX_BAN_DAYS}."), Tone::Warn);
        }
        if self.rank_of(&name) >= self.gm_rank(id) {
            return self.console(id, HIGHER, Tone::Warn);
        }
        let online = self.online(&name);
        if online.is_none() && !self.db.gm_character_exists(&name).unwrap_or(false) {
            return self.console(id, "El personaje no existe.", Tone::Warn);
        }
        let name = online.map(|t| self.name_of(t)).unwrap_or(name);
        let until = self.unix_now() + days as i64 * 86_400;
        let _ = self.db.gm_ban(&name, &reason, me, Some(until));
        self.penalty(&name, me, "ban temporal", days as i64 * 1440, &reason);
        self.log_gm(me, "ban temporal", &name, &format!("{days} días: {reason}"));
        self.to_gms(&format!("Servidor » {name} fue baneado por {reason} durante los próximos {days} días. La medida fue tomada por: {me}."));
        if let Some(t) = online {
            self.kick(t, &format!("Has sido baneado {}: {reason}", ban_left(until - self.unix_now())));
        }
    }

    // ------------------------------------------------------------ inspecting

    /// The online target of an inspection command (`IsValidUserRef`), or why not.
    fn inspect_target(&self, id: u32, a: &[String], usage: &str) -> Option<u32> {
        let Some(name) = a.first().filter(|n| !n.is_empty()) else {
            self.console(id, usage, Tone::Info);
            return None;
        };
        let t = self.online(name);
        if t.is_none() {
            self.console(id, NOT_ONLINE, Tone::Warn);
        }
        t
    }

    /// SendUserMiniStatsTxt.
    fn gm_stat(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(t) = self.inspect_target(id, a, "Uso: /STAT nick") else { return };
        let p = self.player(t).unwrap();
        let f = &p.faction.rec;
        let guild = p.guild.guild.and_then(|g| self.guilds.list.get(&g)).map(|g| g.name.clone());
        let mut lines = vec![
            format!("Pj: {} (nivel {}, {})", p.name, p.level, crate::systems::event::class_name(p.class)),
            format!("Ciudadanos matados: {}, criminales matados: {}", f.citizens_killed, f.criminals_killed),
            format!("Vida {}/{}, maná {}/{}, experiencia {}", p.hp, p.max_hp, p.mana, p.max_mana, p.exp),
            format!("Pena: {} min, silencio: {} min", crate::systems::gm::mins(p.gm.jail_ms), crate::systems::gm::mins(p.gm.silence_ms)),
            format!("Oro en billetera: {}, oro en banco: {}", p.inv.gold, p.bank.gold),
        ];
        if let Some(g) = guild {
            lines.push(format!("Clan: {g}"));
        }
        let tn = p.name.clone();
        for l in lines {
            self.console(id, l, Tone::Info);
        }
        self.log_gm(me, "stat", &tn, "");
    }

    /// SendUserInvTxt.
    fn gm_inventory(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(t) = self.inspect_target(id, a, "Uso: /INV nick") else { return };
        let p = self.player(t).unwrap();
        let items: Vec<(usize, u16, u32)> = p.inv.slots.iter().enumerate().filter_map(|(i, s)| s.map(|it| (i + 1, it.obj, it.amount))).collect();
        let tn = p.name.clone();
        self.list_items(id, &tn, &items);
        self.log_gm(me, "inv", &tn, "");
    }

    /// SendUserBovedaTxt.
    fn gm_bank(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(t) = self.inspect_target(id, a, "Uso: /BOV nick") else { return };
        let p = self.player(t).unwrap();
        let items: Vec<(usize, u16, u32)> = p.bank.slots.iter().enumerate().filter_map(|(i, s)| s.map(|it| (i + 1, it.obj, it.amount))).collect();
        let tn = p.name.clone();
        self.list_items(id, &format!("{tn} (bóveda)"), &items);
        self.log_gm(me, "bov", &tn, "");
    }

    fn list_items(&self, id: u32, title: &str, items: &[(usize, u16, u32)]) {
        self.console(id, format!("{title}: tiene {} objetos.", items.len()), Tone::Info);
        for (slot, obj, amount) in items {
            let name = self.ao.objs.get(*obj as u32).map(|o| o.name.clone()).unwrap_or_else(|| format!("objeto {obj}"));
            self.console(id, format!("Objeto {slot} {name} ({obj}) Cantidad: {amount}"), Tone::Info);
        }
    }

    /// SendUserSkillsTxt.
    fn gm_skills(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(t) = self.inspect_target(id, a, "Uso: /SKILLS nick") else { return };
        let p = self.player(t).unwrap();
        let tn = p.name.clone();
        let list: Vec<String> = p
            .stats
            .skills
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let n = ao_formats::dat::balance::Skill::from_id(i as i32 + 1).and_then(|s| s.spanish_name()).unwrap_or("?");
                format!("{n} = {v}")
            })
            .collect();
        let free = p.stats.skill_points;
        self.console(id, tn.clone(), Tone::Info);
        for chunk in list.chunks(4) {
            self.console(id, chunk.join(", "), Tone::Info);
        }
        self.console(id, format!("Skills libres: {free}"), Tone::Info);
        self.log_gm(me, "skills", &tn, "");
    }

    /// HandleRequestCharGold (`/BAL`): the bank gold.
    fn gm_gold(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(t) = self.inspect_target(id, a, "Uso: /BAL nick") else { return };
        let (tn, bank) = {
            let p = self.player(t).unwrap();
            (p.name.clone(), p.bank.gold)
        };
        self.console(id, format!("El usuario {tn} tiene {bank} en el banco."), Tone::Info);
        self.log_gm(me, "bal", &tn, "");
    }

    /// NickToIP / IPToNick only show GMs to a Dios or above (`priv`).
    fn sees_ip_of(&self, id: u32, t: u32) -> bool {
        self.gm_rank(id) >= GmRank::Dios || !self.gm_rank(t).is_gm()
    }

    fn names_at_ip(&self, id: u32, ip: IpAddr) -> Vec<String> {
        self.player_ids().into_iter().filter(|p| self.sessions.get(p).is_some_and(|s| s.addr.ip() == ip) && self.sees_ip_of(id, *p)).map(|p| self.name_of(p)).collect()
    }

    fn gm_nick_to_ip(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(name) = a.first().filter(|n| !n.is_empty()) else { return self.console(id, "Uso: /NICK2IP nick", Tone::Info) };
        self.log_gm(me, "nick2ip", name, "");
        let Some(t) = self.online(name).filter(|t| self.sees_ip_of(id, *t)) else { return self.console(id, "No hay ningún personaje con ese nick.", Tone::Warn) };
        let ip = self.sessions[&t].addr.ip();
        let tn = self.name_of(t);
        self.console(id, format!("El ip de {tn} es {ip}"), Tone::Info);
        let list = self.names_at_ip(id, ip).join(", ");
        self.console(id, format!("Los personajes con ip {ip} son: {list}"), Tone::Info);
    }

    fn gm_ip_to_nick(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(ip) = a.first().and_then(|s| s.parse::<IpAddr>().ok()) else { return self.console(id, "Uso: /IP2NICK ip", Tone::Info) };
        self.log_gm(me, "ip2nick", &ip.to_string(), "");
        let list = self.names_at_ip(id, ip).join(", ");
        self.console(id, format!("Los personajes con ip {ip} son: {list}"), Tone::Info);
    }

    /// OnlineGM / OnlineMap list Dioses and Admins only to a Dios or above.
    fn sees_in_lists(&self, id: u32, t: u32) -> bool {
        self.gm_rank(id) >= GmRank::Dios || self.gm_rank(t) <= GmRank::SemiDios
    }

    fn gm_online_gm(&mut self, id: u32) {
        let list: Vec<String> = self.player_ids().into_iter().filter(|p| self.gm_rank(*p).is_gm() && self.sees_in_lists(id, *p)).map(|p| self.name_of(p)).collect();
        if list.is_empty() {
            return self.console(id, "No hay GMs Online.", Tone::Info);
        }
        self.console(id, format!("{}.", list.join(", ")), Tone::Info);
    }

    fn gm_online_map(&mut self, id: u32, a: &[String]) {
        let mine = self.player(id).map(|p| p.map).unwrap_or(0);
        let map = a.first().and_then(|s| s.parse::<u16>().ok()).unwrap_or(mine);
        let list: Vec<String> = self.player_ids().into_iter().filter(|p| self.player(*p).is_some_and(|q| q.map == map) && self.sees_in_lists(id, *p)).map(|p| self.name_of(p)).collect();
        self.console(id, format!("Usuarios en el mapa {map}: {}", list.join(", ")), Tone::Info);
    }

    // ------------------------------------------------------------ moderation

    /// HandleMakeDumb / HandleMakeDumbNoMore: the spell's dumbness, without end.
    fn gm_dumb(&mut self, id: u32, me: &str, a: &[String], on: bool) {
        let usage = if on { "Uso: /ESTUPIDO nick" } else { "Uso: /NOESTUPIDO nick" };
        let Some(t) = self.target(id, a, usage) else { return };
        let tn = self.name_of(t);
        self.player_mut(t).status.dumb_until = on.then_some(GameTime(u64::MAX / 2));
        self.refresh_status(t);
        self.log_gm(me, if on { "estupido" } else { "noestupido" }, &tn, "");
        self.console(id, format!("{tn} {}.", if on { "ahora está estúpido" } else { "ya no está estúpido" }), Tone::Good);
    }

    /// HandleRemovePunishment: replaces the text of penalty n (`/PENAS` numbering).
    fn gm_remove_punishment(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /BORRARPENA nick@número@nuevo texto";
        let (Some(name), Some(n), Some(text)) = (a.first(), a.get(1).and_then(|s| s.parse::<usize>().ok()), a.get(2).filter(|t| !t.is_empty())) else { return self.console(id, USAGE, Tone::Info) };
        if !self.db.gm_character_exists(name).unwrap_or(false) && self.online(name).is_none() {
            return self.console(id, "El personaje no existe.", Tone::Warn);
        }
        let stamp = format!("{me}: <{text}>");
        if self.db.gm_change_penalty(name, n, &stamp).unwrap_or(false) {
            self.log_gm(me, "borrarpena", name, &format!("{n}: {text}"));
            self.console(id, "Pena modificada.", Tone::Good);
        } else {
            self.console(id, format!("{name} no tiene la pena {n}."), Tone::Warn);
        }
    }

    /// HandleKickAllChars / EcharPjsNoPrivilegiados: every plain user out.
    fn gm_kick_all(&mut self, id: u32, me: &str) {
        let users: Vec<u32> = self.player_ids().into_iter().filter(|p| !self.gm_rank(*p).is_gm()).collect();
        for t in &users {
            self.kick(*t, "Un administrador ha echado a todos los personajes.");
        }
        self.log_gm(me, "echartodos", "", &users.len().to_string());
        self.console(id, format!("Se echó a {} personajes.", users.len()), Tone::Good);
    }

    /// HandleCuentaRegresiva: countdown to everyone; the players around the GM wait frozen.
    fn gm_countdown(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(secs) = a.first().and_then(|s| s.parse::<u32>().ok()).filter(|s| (1..=255).contains(s)) else { return self.console(id, "Uso: /CR segundos (1 a 255)", Tone::Info) };
        let now = self.now();
        self.gms.countdown = Some(Countdown { left: secs, next: now.after_ms(1000) });
        self.broadcast_all(&S2C::Console { text: format!("¡Empezando cuenta regresiva desde: {secs} segundos...!"), color: Tone::Good }, None);
        let (map, x, y) = {
            let p = self.player(id).unwrap();
            (p.map, p.x, p.y)
        };
        let around: Vec<u32> = self.instance(map).players.iter().copied().filter(|p| self.player(*p).is_some_and(|q| (q.x - x).abs() <= COUNTDOWN_AREA.0 && (q.y - y).abs() <= COUNTDOWN_AREA.1)).collect();
        let until = now.after_secs(secs as u64);
        for p in around {
            self.player_mut(p).status.immobilized_until = Some(until);
            self.refresh_status(p);
        }
        self.log_gm(me, "cuenta regresiva", "", &secs.to_string());
    }

    // ------------------------------------------------------------ world

    /// HandleCleanWorld: what players dropped disappears now on every map (the official
    /// handler only printed the message; this is what its floor cleanup does).
    fn gm_clean_world(&mut self, id: u32, me: &str) {
        let mut n = 0;
        let maps: Vec<u16> = self.maps.keys().copied().collect();
        for map in maps {
            let tiles: Vec<(i32, i32)> = self.maps[&map].ground.dropped_at.keys().copied().collect();
            for (x, y) in tiles {
                let g = &mut self.maps.get_mut(&map).unwrap().ground;
                g.dropped_at.remove(&(x, y));
                if g.tiles.remove(&(x, y)).is_some() {
                    n += 1;
                    self.broadcast(map, &S2C::GroundItemRemoved { x, y }, None);
                }
            }
        }
        self.log_gm(me, "limpiar", "", &n.to_string());
        self.console(id, format!("Se han limpiado los items del suelo ({n})."), Tone::Good);
    }

    /// HandleTeleportCreate: a teleport on the tile above the GM to `mapa x y`.
    fn gm_teleport_create(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /CT mapa x y [radio]";
        let n: Vec<i32> = a.iter().filter_map(|s| s.parse().ok()).collect();
        let (dest, dx, dy, radius) = match n[..] {
            [m, x, y] => (m, x, y, 0),
            [m, x, y, r, ..] => (m, x, y, r),
            _ => return self.console(id, USAGE, Tone::Info),
        };
        if dest <= 0 || dest > u16::MAX as i32 || !(1..=100).contains(&dx) || !(1..=100).contains(&dy) || !self.ensure_map(dest as u16) {
            return self.console(id, "Posición inválida.", Tone::Warn);
        }
        let dest = dest as u16;
        let (map, x, y) = {
            let p = self.player(id).unwrap();
            (p.map, p.x, p.y - 1)
        };
        if !(1..=100).contains(&y) || self.map_object_at(map, x, y).is_some_and(|o| o != 0) || self.instance(map).ground.at(x, y).is_some() || self.instance(map).map.exit(x, y).is_some() {
            return self.console(id, "No hay lugar para el teleport sobre ti.", Tone::Warn);
        }
        if self.instance(dest).ground.at(dx, dy).is_some() {
            return self.console(id, "Hay un objeto en el piso en ese lugar.", Tone::Warn);
        }
        if self.instance(dest).map.exit(dx, dy).is_some() {
            return self.console(id, "No podés crear un teleport que apunte a la entrada de otro.", Tone::Warn);
        }
        let obj = match radius {
            1 => TELEPORT_OBJS[0],
            2 => TELEPORT_OBJS[1],
            _ => TELEPORT_OBJS[2],
        };
        let inst = self.maps.get_mut(&map).unwrap();
        inst.map.set_exit(x, y, Some((dest, dx, dy)));
        inst.doors.insert((x, y), obj);
        self.gms.teleports.insert((map, x, y));
        self.broadcast(map, &S2C::MapObject { x, y, obj }, None);
        self.log_gm(me, "ct", "", &format!("{map} {x} {y} -> {dest} {dx} {dy}"));
        self.console(id, format!("Teleport creado hacia el mapa {dest} ({dx}, {dy})."), Tone::Good);
    }

    /// HandleTeleportDestroy: the teleport on `x y` (default: the tile above the GM, where
    /// `/CT` puts it). A map exit of the file is removed until the next restart.
    fn gm_teleport_destroy(&mut self, id: u32, me: &str, a: &[String]) {
        let (map, px, py) = {
            let p = self.player(id).unwrap();
            (p.map, p.x, p.y)
        };
        let n: Vec<i32> = a.iter().filter_map(|s| s.parse().ok()).collect();
        let (x, y) = match n[..] {
            [x, y] => (x, y),
            _ => (px, py - 1),
        };
        let obj = self.map_object_at(map, x, y).filter(|o| *o != 0);
        let teleport_obj = obj.is_some_and(|o| self.ao.objs.get(o as u32).is_some_and(|d| d.obj_type == ObjType::Teleport));
        if self.instance(map).map.exit(x, y).is_none() && !teleport_obj {
            return self.console(id, "No hay ningún teleport ahí.", Tone::Info);
        }
        let inst = self.maps.get_mut(&map).unwrap();
        inst.map.set_exit(x, y, None);
        if teleport_obj {
            inst.doors.insert((x, y), 0);
            self.broadcast(map, &S2C::MapObject { x, y, obj: 0 }, None);
        }
        self.gms.teleports.remove(&(map, x, y));
        self.log_gm(me, "dt", "", &format!("{map} {x} {y}"));
        self.console(id, "Teleport destruido.", Tone::Good);
    }

    /// HandleChangeMapInfo*: the GM's current map, until the server restarts.
    fn gm_map_info(&mut self, id: u32, me: &str, a: &[String]) {
        const USAGE: &str = "Uso: /MODMAPINFO PK|RESTRINGIR|MAGIASINEFECTO|INVISINEFECTO|TERRENO|ZONA valor";
        let (Some(opt), Some(val)) = (a.first().map(|s| s.to_uppercase()), a.get(1).map(|s| s.to_uppercase())) else { return self.console(id, USAGE, Tone::Info) };
        let map = self.player(id).unwrap().map;
        let flag = |bit: u32, m: &mut ao_formats::AoMap, on: bool| if on { m.restrict |= bit } else { m.restrict &= !bit };
        let m = &mut self.maps.get_mut(&map).unwrap().map;
        let text = match opt.as_str() {
            "PK" => {
                // PK = 1: not safe (the VB stored it inverted).
                m.safe = val != "1";
                format!("Mapa {map}: Seguro = {}", yes_no(m.safe))
            }
            "RESTRINGIR" => {
                let (bit, label) = match val.as_str() {
                    "NEWBIE" => (RESTRICT_NEWBIE, "Newbie"),
                    "SINMAGIA" => (RESTRICT_NO_MAGIC, "SinMagia"),
                    "SININVI" => (RESTRICT_NO_INVISIBILITY, "SinInvi"),
                    "NOPKS" => (RESTRICT_NO_CRIMINALS, "NoPKs"),
                    "NOCIUD" => (RESTRICT_NO_CITIZENS, "NoCiudadanos"),
                    _ => return self.console(id, "Opciones para restringir: 'NEWBIE', 'SINMAGIA', 'SININVI', 'NOPKS', 'NOCIUD'", Tone::Info),
                };
                let on = m.restrict & bit == 0;
                flag(bit, m, on);
                format!("Mapa {map}: {label} = {}", yes_no(on))
            }
            "MAGIASINEFECTO" => {
                flag(RESTRICT_NO_MAGIC, m, val == "1");
                format!("Mapa {map}: SinMagia = {}", yes_no(val == "1"))
            }
            "INVISINEFECTO" => {
                flag(RESTRICT_NO_INVISIBILITY, m, val == "1");
                format!("Mapa {map}: SinInvi = {}", yes_no(val == "1"))
            }
            "TERRENO" | "ZONA" => {
                if !TERRAINS.contains(&val.as_str()) {
                    return self.console(id, "Opciones: 'BOSQUE', 'NIEVE', 'DESIERTO', 'CIUDAD', 'CAMPO', 'DUNGEON'", Tone::Info);
                }
                if opt == "TERRENO" {
                    m.terrain = val.clone();
                    format!("Mapa {map} Terreno: {val}")
                } else {
                    m.zone = val.clone();
                    format!("Mapa {map} Zona: {val}")
                }
            }
            _ => return self.console(id, USAGE, Tone::Info),
        };
        self.log_gm(me, "modmapinfo", &map.to_string(), &format!("{opt} {val}"));
        self.console(id, text, Tone::Good);
    }

    /// HandleChangeMOTD (no text: shows the current one to edit) / HandleSetMOTD (`|`
    /// separates lines, `-` clears it).
    fn gm_set_motd(&mut self, id: u32, me: &str, rest: &str) {
        if rest.is_empty() {
            let cur = self.motd().join("|");
            return self.console(id, format!("MOTD actual: {}. Uso: /MOTDCAMBIA línea|línea (- para borrarlo)", if cur.is_empty() { "(vacío)" } else { &cur }), Tone::Info);
        }
        let text = if rest == "-" { String::new() } else { rest.split('|').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n") };
        let _ = self.db.gm_set_setting(KEY_MOTD, &text);
        self.gms.motd = None;
        self.log_gm(me, "motd", "", &text.replace('\n', " | "));
        self.console(id, "Se ha cambiado el MOTD con éxito.", Tone::Good);
    }

    /// HandleGlobalOnOff, which in the VB never flipped the setting.
    fn gm_global_toggle(&mut self, me: &str) {
        let off = !self.global_chat_off();
        self.gms.global_off = Some(off);
        let _ = self.db.gm_set_setting(KEY_GLOBAL_OFF, if off { "1" } else { "0" });
        self.log_gm(me, "global", "", if off { "off" } else { "on" });
        let text = if off { "Servidor » Chat general deshabilitado." } else { "Servidor » Chat general habilitado. Usa el canal global para escribir." };
        self.broadcast_all(&S2C::Console { text: text.into(), color: Tone::Warn }, None);
    }
}

#[cfg(test)]
mod tests {
    use crate::clock::GameTime;
    use crate::net::Event;
    use crate::world::World;
    use crate::Settings;
    use ao_protocol::p4::gm::GmRank;
    use ao_protocol::PROTOCOL_VERSION;

    fn world_with(names: &[&str]) -> World {
        let mut w = World::new(Settings::defaults());
        for (i, n) in names.iter().enumerate() {
            let id = i as u32 + 1;
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            std::mem::forget(rx);
            w.on_event(Event::Connected { id, addr: "127.0.0.1:9".parse().unwrap(), tx });
            w.on_event(Event::Frame { id, msg: ao_protocol::C2S::Login { name: n.to_string(), version: PROTOCOL_VERSION }, at: None });
        }
        w
    }

    /// A temporary ban keeps the character out until its day passes, then the sweep lifts
    /// it (the virtual clock cannot tick a whole day, so the unix clock is skewed).
    #[test]
    fn a_temporary_ban_ends_by_itself() {
        let mut w = world_with(&["Semi", "Beto"]);
        w.player_mut(1).gm.rank = GmRank::SemiDios;
        // Dev-login characters do not exist in the database: use an offline name with GM rows.
        w.db.gm_ban("Malo", "flood", "Semi", Some(w.unix_now() + 86_400)).unwrap();
        assert!(w.gm_login_refusal("Malo", None).is_some_and(|r| r.contains("durante 1 día más")));
        w.gms.unix_skew = 86_400;
        w.tick(GameTime(61_000));
        assert!(w.gm_login_refusal("Malo", None).is_none(), "the ban ended");
        assert!(w.db.gm_recent_log(5, Some("Servidor")).unwrap().iter().any(|r| r.action == "fin ban" && r.target == "Malo"));
    }

    #[test]
    fn countdown_says_every_second_and_ends() {
        let mut w = world_with(&["Conse", "Beto"]);
        w.player_mut(1).gm.rank = GmRank::Consejero;
        let (x, y) = (w.player(1).unwrap().x, w.player(1).unwrap().y);
        w.teleport(2, w.player(1).unwrap().map, x + 1, y);
        assert!(w.gm_command(1, "/cr 3"));
        assert!(!w.can_move(2), "people around wait frozen");
        for t in 1..=4u64 {
            w.tick(GameTime(t * 1000));
        }
        assert!(w.gms.countdown.is_none());
        assert!(w.can_move(2));
    }

    #[test]
    fn teleport_create_and_destroy_change_the_map_exit() {
        let mut w = world_with(&["Dios"]);
        w.player_mut(1).gm.rank = GmRank::Dios;
        let (map, x, y) = { let p = w.player(1).unwrap(); (p.map, p.x, p.y) };
        if w.instance(map).map.exit(x, y - 1).is_some() || w.map_object_at(map, x, y - 1).is_some() {
            return; // the default start tile has something above: nothing to test here
        }
        assert!(w.gm_command(1, &format!("/ct {map} 30 30 1")));
        let e = w.instance(map).map.exit(x, y - 1).expect("exit created");
        assert_eq!((e.map, e.dest_x, e.dest_y), (map, 30, 30));
        assert_eq!(w.map_object_at(map, x, y - 1), Some(370));
        assert!(w.gm_command(1, "/dt"));
        assert!(w.instance(map).map.exit(x, y - 1).is_none());
        assert!(w.gms.teleports.is_empty());
    }
}

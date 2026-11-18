//! Phase 6 GM commands (events and the handlers of `Protocol_GmCommands.bas` still
//! missing after phase 5) and the event commands of players. The dispatcher and the rank
//! helpers are in `systems/gm.rs`.
//!
//! | Command | Rank | VB |
//! |---|---|---|
//! | `/TELEPLOC` (al último tile clicado) | Consejero | HandleWarpMeToTarget |
//! | `/SLOT nick@slot` | Consejero | HandleCheckSlot |
//! | `/CREAREVENTO LOBBY\|CACERIA\|DEATHMATCH\|CAPTURA ...` | SemiDios | CreateEventCmd -> HandleStartEvent / HandleIniciarCaptura |
//! | `/CREAREVENTO tipo@duración@multi` | Admin | HandleCrearEvento -> ForzarEvento |
//! | `/CONFIGLOBBY ...` | SemiDios | HandleLobbyCommand |
//! | `/CANCELAREVENTO` | SemiDios | HandleCancelarEvento |
//! | `/FINALIZAREVENTO` | SemiDios | HandleFinEvento |
//! | `/SEGUIR` (NPC clicado) | SemiDios | HandleNPCFollow |
//! | `/CC [filtro]`, `/SPAWN n` | SemiDios | HandleSpawnListRequest / HandleSpawnCreature |
//! | `/TALKAS texto` | SemiDios | HandleTalkAsNPC |
//! | `/TRABAJANDO`, `/OCULTANDO` | SemiDios | HandleWorking / HandleHiding |
//! | `/LLUVIA`, `/NIEVE`, `/NIEBLA` | SemiDios | HandleRainToggle / NieveToggle / NieblaToggle |
//! | `/RESETINV` (NPC clicado) | SemiDios | HandleResetNPCInventory |
//! | `/MASSDEST` | SemiDios | HandleDestroyAllItemsInArea |
//! | `/RMATA` (NPC clicado) | SemiDios | HandleKillNPC |
//! | `/CE INVASION BANDER\|CARCEL` | Dios | HandleCreateEvent |
//! | `/EVENTOFACCIONARIO` | Dios | HandleEventoFaccionario |
//! | `/ANAME nick@nuevo` | Dios | HandleAlterName |
//! | `/FORCEMIDI n`, `/FORCEWAV n`, `/FORCEMIDIMAP n [mapa]`, `/FORCEWAVMAP n [mapa x y]` | Dios | HandleForceMIDIAll / WAVEAll / WAVEToMap |
//! | `/SETDESC texto` (usuario clicado) | Dios | HandleSetCharDescription |
//! | `/MASSKILL` | Dios | HandleKillAllNearbyNPCs |
//! | `/GENIO` | Dios | HandleGenio |
//! | `/DOBACKUP` | Dios | HandleDoBackUp |
//! | `/CERRARCLIENTE nick` | Admin | HandleCerrarCliente (only from the VB panel) |
//!
//! Players: `/EVENTO` (HandleEventoInfo), `/SALAS` (the EventMaster list), `/CASTLE
//! ADD|REMOVE nick`, `/UPTIME`, `/AYUDA` (Help.dat), `/EST` (SendUserStatsTxt).
//!
//! VB defects fixed: `HandleStartEvent` (GM path) had no rank check; `HandleGenio` let a
//! Consejero give himself 100 in every skill (Dios here); `HandleCheckSlot` checked the
//! slot against the GM's own inventory size; `HandleNPCFollow` with no NPC clicked did
//! nothing silently; `/LLUVIA` also toggled the snow.
use crate::world::World;
use ao_protocol::p4::gm::GmRank;
use ao_protocol::{Tone, S2C};
use ao_rules::events::EventType;

/// `MinXBorder`/`MinYBorder` - 1: the visible area around the GM.
const AREA: (i32, i32) = (11, 9);

impl World {
    pub(crate) fn gm_event_rank(cmd: &str) -> Option<GmRank> {
        Some(match cmd {
            "/TELEPLOC" | "/SLOT" => GmRank::Consejero,
            "/CREAREVENTO" | "/CONFIGLOBBY" | "/CANCELAREVENTO" | "/FINALIZAREVENTO" | "/SEGUIR" | "/CC" | "/CO" | "/SPAWN" | "/TALKAS" | "/TRABAJANDO" | "/OCULTANDO" | "/LLUVIA" | "/NIEVE" | "/NIEBLA" | "/RESETINV" | "/MASSDEST" | "/RMATA" => GmRank::SemiDios,
            "/CE" | "/EVENTOFACCIONARIO" | "/ANAME" | "/FORCEMIDI" | "/FORCEWAV" | "/FORCEMIDIMAP" | "/FORCEWAVMAP" | "/SETDESC" | "/MASSKILL" | "/GENIO" | "/DOBACKUP" => GmRank::Dios,
            "/CERRARCLIENTE" => GmRank::Admin,
            _ => return None,
        })
    }

    pub(crate) fn gm_event(&mut self, id: u32, me: &str, cmd: &str, rest: &str, a: &[String]) {
        let (map, x, y) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap_or_default();
        let num = |i: usize| a.get(i).and_then(|s| s.trim().parse::<i64>().ok());
        match cmd {
            "/TELEPLOC" => match self.player(id).and_then(|p| p.target_tile) {
                Some((m, tx, ty)) => {
                    self.f6_warp(id, m, tx, ty);
                    self.log_gm(me, "teleploc", "", &format!("{m} {tx} {ty}"));
                }
                None => self.console(id, "Primero haz click en una posición.", Tone::Info),
            },
            "/SLOT" => self.gm_check_slot(id, me, a),
            "/CREAREVENTO" if rest.contains('@') => self.hour_event_force(id, me, a),
            "/CREAREVENTO" => {
                let words: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
                match words.first().and_then(|w| EventType::from_keyword(w)) {
                    Some(EventType::CaptureTheFlag) => {
                        let n = |i: usize| words.get(i).and_then(|s| s.parse::<i64>().ok());
                        let (Some(max), Some(rounds), Some(min), Some(maxl), Some(fee)) = (n(1), n(2), n(3), n(4), n(5)) else {
                            return self.console(id, "Utilice /CREAREVENTO CAPTURA PARTICIPANTES RONDAS NIVEL_MINIMO NIVEL_MAXIMO PRECIO.", Tone::Info);
                        };
                        match self.capture_start(max.max(0) as u32, rounds.max(1) as u32, min.max(0) as u32, maxl.max(0) as u32, fee) {
                            Ok(()) => self.log_gm(me, "crear evento", "", &format!("captura {max} {rounds} {min}-{maxl} {fee}")),
                            Err(e) => self.console(id, e, Tone::Warn),
                        }
                    }
                    Some(kind) => self.lobby_gm_create(id, me, kind, &words),
                    None => self.console(id, "Tipo de evento inválido: LOBBY, CAPTURA, CACERIA o DEATHMATCH (o TIPO@DURACION@MULTIPLICACION).", Tone::Info),
                }
            }
            "/CONFIGLOBBY" => {
                let words: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
                self.lobby_gm_config(id, me, &words);
            }
            "/CANCELAREVENTO" => self.lobby_gm_cancel(id, me),
            "/FINALIZAREVENTO" => {
                if self.wevents.active.is_some() {
                    self.hour_event_end();
                    self.log_gm(me, "fin evento", "", "");
                } else {
                    self.console(id, "No hay ningun evento activo.", Tone::Info);
                }
            }
            "/SEGUIR" => match self.target_npc(id) {
                Some(nid) => {
                    let n = self.maps.get_mut(&map).unwrap().npcs.get_mut(&nid).unwrap();
                    // DoFollow: SigueAmo behind the GM, not hostile, paralysis gone. A
                    // companion here (never counted as a fighting pet).
                    n.target = None;
                    n.hostile = false;
                    n.movement = ao_formats::dat::npcs::Movement::FollowOwner;
                    n.pet.owner = Some(id);
                    n.pet.kind = crate::systems::pets::PetKind::Companion;
                    n.status = Default::default();
                    self.log_gm(me, "seguir", &self.instance(map).npcs[&nid].name.clone(), "");
                }
                None => self.console(id, "Debes hacer click sobre un NPC antes.", Tone::Info),
            },
            "/CC" | "/CO" => self.gm_spawn_list(id, rest),
            "/SPAWN" => self.gm_spawn(id, me, num(0)),
            "/TALKAS" => match self.target_npc(id) {
                Some(nid) if !rest.is_empty() => self.broadcast(map, &S2C::NpcSay { id: nid, text: rest.to_string() }, None),
                _ => self.console(id, "Debes seleccionar el NPC por el que quieres hablar antes de usar este comando.", Tone::Info),
            },
            "/TRABAJANDO" => {
                let list: Vec<String> = self.player_ids().into_iter().filter_map(|i| self.player(i).filter(|p| p.work.active.is_some()).map(|p| p.name.clone())).collect();
                if list.is_empty() {
                    self.console(id, "No hay usuarios trabajando.", Tone::Info);
                } else {
                    self.console(id, format!("Usuarios trabajando: {}", list.join(", ")), Tone::Info);
                }
            }
            "/OCULTANDO" => {
                let list: Vec<String> = self.player_ids().into_iter().filter_map(|i| self.player(i).filter(|p| p.status.hidden).map(|p| p.name.clone())).collect();
                if list.is_empty() {
                    self.console(id, "No hay usuarios ocultandose.", Tone::Info);
                } else {
                    self.console(id, format!("Usuarios ocultandose: {}", list.join(", ")), Tone::Info);
                }
            }
            "/LLUVIA" | "/NIEVE" => {
                // Rain or snow is the same precipitation here (the map's terrain decides).
                let on = !self.weather.precipitation;
                self.weather.precipitation = on;
                if on && self.weather.fog == 0 {
                    self.weather.fog = 20;
                }
                let msg = self.weather.message();
                self.broadcast_all(&msg, None);
                if on && cmd == "/LLUVIA" {
                    self.broadcast_all(&S2C::Thunder, None);
                }
                self.log_gm(me, &cmd[1..].to_lowercase(), "", "");
            }
            "/NIEBLA" => {
                self.weather.fog = if self.weather.fog > 0 { 0 } else { self.rng.range(10, 45) as u8 };
                if self.weather.fog == 0 {
                    self.weather.precipitation = false;
                }
                let msg = self.weather.message();
                self.broadcast_all(&msg, None);
                self.log_gm(me, "niebla", "", "");
            }
            "/RESETINV" => match self.target_npc(id) {
                Some(nid) => {
                    self.trades.stocks.remove(&nid);
                    self.log_gm(me, "resetinv", &self.instance(map).npcs[&nid].name.clone(), "");
                }
                None => self.console(id, "Debes hacer click sobre un NPC antes.", Tone::Info),
            },
            "/MASSDEST" => {
                let tiles: Vec<((i32, i32), u32)> = self.instance(map).ground.tiles.iter().filter(|((tx, ty), _)| (tx - x).abs() <= AREA.0 && (ty - y).abs() <= AREA.1).map(|(k, v)| (*k, v.amount)).collect();
                for ((tx, ty), n) in tiles {
                    self.erase_obj(map, tx, ty, n);
                }
                self.log_gm(me, "massdest", "", &format!("{map} {x} {y}"));
            }
            "/RMATA" => match self.target_npc(id) {
                Some(nid) => {
                    let name = self.instance(map).npcs[&nid].name.clone();
                    self.console(id, format!("RMatas (con posible respawn) a: {name}"), Tone::Info);
                    let now = self.now();
                    let n = self.maps.get_mut(&map).unwrap().npcs.get_mut(&nid).unwrap();
                    n.dead_until = Some(now);
                    self.broadcast(map, &S2C::Despawn { id: nid }, None);
                    self.log_gm(me, "rmata", &name, "");
                }
                None => self.console(id, "Debes hacer click sobre un NPC antes.", Tone::Info),
            },
            "/CE" => self.invasion_gm(id, me, rest),
            "/EVENTOFACCIONARIO" => {
                self.wevents.faction_event = !self.wevents.faction_event;
                let what = if self.wevents.faction_event { "iniciado" } else { "finalizado" };
                for g in self.player_ids() {
                    if self.gm_rank(g) >= GmRank::Dios {
                        self.console(g, format!("Servidor » Se ha {what} el evento faccionario."), Tone::Info);
                    }
                }
            }
            "/ANAME" => self.gm_alter_name(id, me, a),
            "/FORCEMIDI" | "/FORCEMIDIMAP" => {
                let Some(n) = num(0).filter(|n| (0..=u16::MAX as i64).contains(n)) else { return self.console(id, format!("Uso: {cmd} número"), Tone::Info) };
                let msg = S2C::Music { midi: n as u16 };
                if cmd == "/FORCEMIDI" {
                    self.f6_say_all(format!("{me} broadcast musica: {n}"));
                    self.broadcast_all(&msg, None);
                } else {
                    let m = num(1).map_or(map, |m| m as u16);
                    self.broadcast(m, &msg, None);
                }
            }
            "/FORCEWAV" => {
                let Some(n) = num(0).filter(|n| (1..=u16::MAX as i64).contains(n)) else { return self.console(id, "Uso: /FORCEWAV número", Tone::Info) };
                for pid in self.player_ids() {
                    if let Some(p) = self.player(pid) {
                        self.send(pid, &S2C::Sound { wav: n as u16, x: p.x, y: p.y });
                    }
                }
            }
            "/FORCEWAVMAP" => {
                let Some(n) = num(0).filter(|n| (1..=u16::MAX as i64).contains(n)) else { return self.console(id, "Uso: /FORCEWAVMAP número [mapa x y]", Tone::Info) };
                let (m, wx, wy) = match (num(1), num(2), num(3)) {
                    (Some(m), Some(wx), Some(wy)) if (1..=100).contains(&wx) && (1..=100).contains(&wy) && self.ensure_map(m as u16) => (m as u16, wx as i32, wy as i32),
                    _ => (map, x, y),
                };
                self.broadcast(m, &S2C::Sound { wav: n as u16, x: wx, y: wy }, None);
            }
            "/SETDESC" => match self.player(id).and_then(|p| p.target_user).filter(|u| self.player(*u).is_some()) {
                Some(u) => {
                    self.player_mut(u).gm_desc = (!rest.is_empty()).then(|| rest.to_string());
                    self.log_gm(me, "setdesc", &self.name_of(u), rest);
                }
                None => self.console(id, "Haz click sobre un personaje antes.", Tone::Info),
            },
            "/MASSKILL" => {
                let ids: Vec<u32> = self.instance(map).npc_ids().iter().copied().filter(|n| self.instance(map).npcs.get(n).is_some_and(|n| n.alive() && (n.x - x).abs() <= AREA.0 && (n.y - y).abs() <= AREA.1)).collect();
                for nid in ids {
                    self.f6_remove_npc(map, nid);
                }
                self.log_gm(me, "masskill", "", &format!("{map} {x} {y}"));
            }
            "/GENIO" => {
                let p = self.player_mut(id);
                for s in p.stats.skills.iter_mut() {
                    *s = 100;
                }
                self.send_skills(id);
                self.console(id, "Tus skills fueron editados.", Tone::Info);
                self.log_gm(me, "genio", "", "");
            }
            "/DOBACKUP" => {
                self.autosave_all();
                let dir = std::path::PathBuf::from(&self.cfg.ops.backup_dir);
                match crate::ops::backup::backup_into(&self.cfg.database, &dir, crate::db::now_unix()) {
                    Ok(p) => self.console(id, format!("Copia guardada en {}.", p.display()), Tone::Good),
                    Err(e) => self.console(id, format!("No se pudo hacer la copia: {e:?}"), Tone::Warn),
                }
                self.log_gm(me, "dobackup", "", "");
            }
            "/CERRARCLIENTE" => {
                let Some(t) = a.first().and_then(|n| self.id_by_name(n)) else { return self.console(id, "El usuario no esta online.", Tone::Warn) };
                let name = self.name_of(t);
                for g in self.player_ids() {
                    if self.gm_rank(g) >= GmRank::Dios {
                        self.console(g, format!("{me} cerro el cliente de {name}."), Tone::Info);
                    }
                }
                self.log_gm(me, "cerrar cliente", &name, "");
                self.kick(t, "cliente cerrado por un GM");
            }
            _ => {}
        }
    }

    fn target_npc(&self, id: u32) -> Option<u32> {
        let p = self.player(id)?;
        let nid = p.target_npc?;
        self.instance(p.map).npcs.get(&nid).filter(|n| n.alive()).map(|_| nid)
    }

    /// `HandleCheckSlot`.
    fn gm_check_slot(&mut self, id: u32, me: &str, a: &[String]) {
        let (Some(name), Some(slot)) = (a.first(), a.get(1).and_then(|s| s.parse::<u8>().ok())) else { return self.console(id, "Uso: /SLOT nick@slot", Tone::Info) };
        let Some(t) = self.id_by_name(name) else { return self.console(id, "Usuario offline.", Tone::Warn) };
        self.log_gm(me, "slot", name, &slot.to_string());
        let p = self.player(t).unwrap();
        if slot == 0 || slot > p.inv.usable_slots() {
            return self.console(id, "Slot inválido.", Tone::Warn);
        }
        match p.inv.get(slot) {
            Some(it) => {
                let oname = self.ao.objs.get(it.obj as u32).map_or("?", |o| o.name.as_str()).to_string();
                self.console(id, format!(" Objeto {slot}) {oname} Cantidad:{}", it.amount), Tone::Info)
            }
            None => self.console(id, "No hay objeto en el slot seleccionado.", Tone::Info),
        }
    }

    /// `HandleSpawnListRequest` (`WriteSpawnList`): a SemiDios only sees what may be
    /// summoned (`PuedeInvocar`), a Dios everything.
    fn gm_spawn_list(&mut self, id: u32, filter: &str) {
        let all = self.gm_rank(id) >= GmRank::Dios;
        if !all {
            self.console(id, "Servidor » La cantidad de NPCs disponible para tu rango está limitada.", Tone::Info);
        }
        let f = filter.trim().to_lowercase();
        let ao = self.ao.clone();
        let mut shown = 0;
        for (n, d) in ao.npcs.iter() {
            if d.name.is_empty() || !(all || d.can_summon == 1) || (!f.is_empty() && !d.name.to_lowercase().contains(&f)) {
                continue;
            }
            shown += 1;
            if shown > 40 {
                self.console(id, "... (usa /CC filtro para acotar)", Tone::Info);
                break;
            }
            self.console(id, format!("{n}: {}", d.name), Tone::Info);
        }
        if shown == 0 {
            self.console(id, "No hay NPCs que coincidan.", Tone::Info);
        }
    }

    /// `HandleSpawnCreature`.
    fn gm_spawn(&mut self, id: u32, me: &str, n: Option<i64>) {
        let Some(n) = n.filter(|n| (1..=u16::MAX as i64).contains(n)) else { return self.console(id, "Uso: /SPAWN número (ver /CC)", Tone::Info) };
        let ok = self.ao.npcs.get(n as u32).is_some_and(|d| !d.name.is_empty() && (d.can_summon == 1 || self.gm_rank(id) >= GmRank::Dios));
        if !ok {
            return self.console(id, "No puedes invocar esa criatura.", Tone::Warn);
        }
        let (map, x, y) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
        if let Some(nid) = self.f6_spawn_npc(map, n as u16, x, y + 1, true) {
            let name = self.instance(map).npcs[&nid].name.clone();
            self.log_gm(me, "sumoneo", &name, &format!("{map} {x} {y}"));
        }
    }

    /// `HandleAlterName` (+ `ChangeNameDatabase`).
    fn gm_alter_name(&mut self, id: u32, me: &str, a: &[String]) {
        let (Some(old), Some(new)) = (a.first().cloned(), a.get(1).map(|s| s.trim().to_string())) else { return self.console(id, "Usar: /ANAME origen@destino", Tone::Info) };
        if new.is_empty() || new.len() > 30 || !new.chars().all(|c| c.is_alphabetic() || c == ' ') {
            return self.console(id, "Nombre inválido.", Tone::Warn);
        }
        let online = self.id_by_name(&old);
        if online.is_some_and(|t| self.player(t).is_some_and(|p| p.guild.guild.is_some())) || self.db.char_in_guild(&old).unwrap_or(false) {
            return self.console(id, format!("El personaje {old} pertenece a un clan, debe salir antes."), Tone::Warn);
        }
        if online.is_none() && self.db.gm_char_id(&old).ok().flatten().is_none() {
            return self.console(id, format!("El personaje {old} es inexistente."), Tone::Warn);
        }
        if self.db.gm_char_id(&new).ok().flatten().is_some() || self.id_by_name(&new).is_some() {
            return self.console(id, "El nick solicitado ya existe.", Tone::Warn);
        }
        if let Some(t) = online {
            // Out first (saved under the old name), renamed after.
            self.kick(t, "tu nombre fue cambiado por un GM, vuelve a entrar");
        }
        if let Err(e) = self.db.rename_character(&old, &new) {
            return self.console(id, format!("No se pudo cambiar el nombre: {e:?}"), Tone::Warn);
        }
        self.penalty(&new, me, "nombre", 0, &format!("{me}: nombre cambiado de \"{old}\" a \"{new}\"."));
        self.console(id, "Transferencia exitosa.", Tone::Good);
        for g in self.player_ids() {
            if self.gm_rank(g).is_gm() {
                self.console(g, format!("Administración » {me} cambió el nombre del usuario \"{old}\" por \"{new}\"."), Tone::Info);
            }
        }
        self.log_gm(me, "aname", &old, &new);
    }

    // ================================================================== players

    /// Event commands anyone may use. True if the line was one of them.
    pub(crate) fn event_player_command(&mut self, id: u32, line: &str) -> bool {
        let (cmd, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        match cmd.to_uppercase().as_str() {
            "/EVENTO" | "/EVENT" => self.hour_event_info(id),
            "/SALAS" => self.lobby_list(id),
            "/CASTLE" | "/CASTILLO" => self.castle_whitelist_cmd(id, rest),
            "/UPTIME" => {
                let secs = self.started_at.map_or(0, |t| self.now().since(t) / 1000);
                let (d, h, m, s) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
                self.console(id, format!("Server Online: {d} {}, {h} horas, {m} minutos, {s} segundos.", if d == 1 { "día" } else { "días" }), Tone::Info);
            }
            "/AYUDA" | "/HELP" => {
                let ini = std::fs::read(self.recursos.join("Dat").join("Help.dat")).ok().map(|b| ao_rules::events::ini(&b.iter().map(|c| *c as char).collect::<String>())).unwrap_or_default();
                let n = ini.get("init").and_then(|m| m.get("numlines")).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
                for i in 1..=n {
                    if let Some(l) = ini.get("help").and_then(|m| m.get(&format!("line{i}"))) {
                        self.console(id, l.clone(), Tone::Info);
                    }
                }
            }
            "/EST" => {
                let Some(p) = self.player(id) else { return true };
                let s = &p.stats;
                let text = format!(
                    "Estadísticas de {}: nivel {}, experiencia {}, vida {}/{}, maná {}/{}, energía {}/{}, oro {}, hambre {}, sed {}.",
                    p.name, p.level, p.exp, p.hp, p.max_hp, p.mana, p.max_mana, s.sta, s.max_sta, p.inv.gold, s.food, s.water
                );
                self.console(id, text, Tone::Info);
            }
            _ => return false,
        }
        true
    }
}

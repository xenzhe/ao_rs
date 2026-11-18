//! Game master commands of Tierras Sagradas that the AO20 set lacked (TCP_HandleData3.bas).
//! The AO20 ones live in gm.rs; `gm_command` sends these here after checking the rank.
//!
//! Fixes over TSAO: several of them refused to act on one hard-coded character name ("SHAY",
//! a back door); here the rank rules of gm.rs apply to everyone. `/BLOQ` and `/TRIGGER`
//! change the live map only (not the file); the server checks every step against it.
use crate::world::World;
use ao_protocol::{Tone, S2C};

impl World {
    /// Rank needed for the TSAO commands (None = not one of them).
    pub(crate) fn gm_tsao_rank(cmd: &str) -> Option<ao_protocol::p4::gm::GmRank> {
        use ao_protocol::p4::gm::GmRank::*;
        Some(match cmd {
            "/HORA" | "/IRCERCA" | "/SOBJ" | "/NENE" | "/TRIGGER" => Consejero,
            "/KILL" | "/HOME" | "/LIBERAR" | "/DEST" | "/GO" | "/SMSG" => SemiDios,
            "/BLOQ" | "/LMAP" | "/EXP" | "/GLD" | "/HECHIZO" => Dios,
            _ => return None,
        })
    }

    pub(crate) fn gm_tsao(&mut self, id: u32, me: &str, cmd: &str, rest: &str) {
        let a: Vec<&str> = rest.split_whitespace().collect();
        let (map, x, y) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap_or_default();
        match cmd {
            "/HORA" => {
                let t = chrono_now();
                self.broadcast_all(&S2C::Console { text: format!("Hora del servidor: {t}"), color: Tone::Info }, None);
            }
            "/IRCERCA" => {
                let Some(t) = a.first().and_then(|n| self.online_named(n)) else { return self.console(id, "Usuario offline.", Tone::Warn) };
                let (m, tx, ty) = self.player(t).map(|p| (p.map, p.x, p.y)).unwrap();
                self.teleport(id, m, tx + 1, ty);
                self.gm_tsao_log(me, "ircerca", &self.name_of(t), "");
            }
            "/GO" => match a.first().and_then(|s| s.parse::<u16>().ok()) {
                Some(m) => {
                    self.teleport(id, m, 50, 50);
                    self.gm_tsao_log(me, "go", "", &m.to_string());
                }
                None => self.console(id, "Uso: /GO mapa", Tone::Info),
            },
            "/KILL" => {
                let Some(t) = a.first().and_then(|n| self.online_named(n)) else { return self.console(id, "Usuario offline.", Tone::Warn) };
                if t == id || self.gm_rank(t) >= self.gm_rank(id) || self.is_dead(t) {
                    return self.console(id, "No puedes matar a ese personaje.", Tone::Warn);
                }
                self.user_die(t);
                self.console(t, format!("{me} te ha matado."), Tone::Warn);
                self.gm_tsao_log(me, "kill", &self.name_of(t), "");
            }
            "/HOME" | "/LIBERAR" => {
                let Some(t) = a.first().and_then(|n| self.online_named(n)) else { return self.console(id, "Usuario offline.", Tone::Warn) };
                let dest = if cmd == "/HOME" { self.data.balance.home.unwrap_or(self.start) } else { self.data.balance.freedom };
                if cmd == "/LIBERAR" {
                    self.player_mut(t).gm.jail_ms = 0;
                }
                self.teleport(t, dest.0, dest.1, dest.2);
                self.console(id, format!("{} enviado a {} ({}, {}).", self.name_of(t), dest.0, dest.1, dest.2), Tone::Good);
                self.gm_tsao_log(me, &cmd[1..].to_lowercase(), &self.name_of(t), "");
            }
            "/DEST" => {
                let amount = self.instance(map).ground.tiles.get(&(x, y)).map(|i| i.amount);
                match amount {
                    Some(n) => {
                        self.erase_obj(map, x, y, n);
                        self.gm_tsao_log(me, "dest", "", &format!("{map} {x} {y}"));
                    }
                    None => self.console(id, "No hay ningún objeto bajo tus pies.", Tone::Info),
                }
            }
            "/LMAP" => {
                let m = a.first().and_then(|s| s.parse::<u16>().ok()).unwrap_or(map);
                if !self.ensure_map(m) {
                    return self.console(id, "Ese mapa no existe.", Tone::Warn);
                }
                let tiles: Vec<((i32, i32), u32)> = self.instance(m).ground.tiles.iter().map(|(k, v)| (*k, v.amount)).collect();
                for ((tx, ty), n) in &tiles {
                    self.erase_obj(m, *tx, *ty, *n);
                }
                self.console(id, format!("Mapa {m} limpio: {} objetos retirados.", tiles.len()), Tone::Good);
                self.gm_tsao_log(me, "lmap", "", &m.to_string());
            }
            "/BLOQ" => {
                let inst = self.maps.get_mut(&map).unwrap();
                let i = ao_formats::csm::tile(x, y);
                let b = &mut inst.map.blocked[i];
                *b ^= ao_formats::csm::BLOCK_ALL;
                let now_blocked = *b & ao_formats::csm::BLOCK_ALL == ao_formats::csm::BLOCK_ALL;
                self.console(id, if now_blocked { "Casilla bloqueada." } else { "Casilla desbloqueada." }, Tone::Info);
                self.gm_tsao_log(me, "bloq", "", &format!("{map} {x} {y} {now_blocked}"));
            }
            "/TRIGGER" => {
                let inst = self.maps.get_mut(&map).unwrap();
                let i = ao_formats::csm::tile(x, y);
                if let Some(t) = a.first().and_then(|s| s.parse::<u16>().ok()) {
                    inst.map.triggers[i] = t;
                    self.gm_tsao_log(me, "trigger", "", &format!("{map} {x} {y} {t}"));
                }
                let t = self.instance(map).map.triggers[i];
                self.console(id, format!("Trigger en ({x}, {y}): {t}"), Tone::Info);
            }
            "/EXP" | "/GLD" => match a.first().and_then(|s| s.parse::<u32>().ok()).filter(|v| (1..=100).contains(v)) {
                Some(v) => {
                    let what = if cmd == "/EXP" {
                        self.cfg.world.exp_mult = v;
                        "experiencia"
                    } else {
                        self.cfg.world.gold_mult = v;
                        "oro"
                    };
                    self.broadcast_all(&S2C::Console { text: format!("Multiplicador de {what}: x{v}"), color: Tone::Good }, None);
                    self.gm_tsao_log(me, &cmd[1..].to_lowercase(), "", &v.to_string());
                }
                None => self.console(id, format!("Uso: {cmd} 1-100"), Tone::Info),
            },
            "/SMSG" => {
                self.broadcast_all(&S2C::Console { text: format!("» {rest}"), color: Tone::Warn }, None);
                self.gm_tsao_log(me, "smsg", "", rest);
            }
            "/SOBJ" => {
                let q = rest.trim().to_lowercase();
                let hits: Vec<String> = self.ao.objs.iter().filter(|(_, o)| !q.is_empty() && o.name.to_lowercase().contains(&q)).take(20).map(|(i, o)| format!("{i}: {}", o.name)).collect();
                self.console(id, if hits.is_empty() { "Ningún objeto coincide.".into() } else { hits.join(" · ") }, Tone::Info);
            }
            "/NENE" => {
                let m = a.first().and_then(|s| s.parse::<u16>().ok()).unwrap_or(map);
                if !self.ensure_map(m) {
                    return self.console(id, "Ese mapa no existe.", Tone::Warn);
                }
                let mut names: Vec<String> = self.instance(m).npcs.values().filter(|n| n.alive() && n.hostile).map(|n| n.name.clone()).collect();
                names.sort();
                self.console(id, format!("Criaturas hostiles en el mapa {m}: {} — {}", names.len(), names.join(", ")), Tone::Info);
            }
            "/HECHIZO" => {
                let (Some(t), Some(s)) = (a.first().and_then(|n| self.online_named(n)), a.get(1).and_then(|s| s.parse::<u16>().ok())) else {
                    return self.console(id, "Uso: /HECHIZO nick número", Tone::Info);
                };
                if self.ao.spells.get(s as u32).is_none() {
                    return self.console(id, "Ese hechizo no existe.", Tone::Warn);
                }
                if self.player_mut(t).spells.add(s).is_some() {
                    self.send_spellbook(t);
                    self.gm_tsao_log(me, "hechizo", &self.name_of(t), &s.to_string());
                }
            }
            _ => {}
        }
    }

    fn online_named(&self, name: &str) -> Option<u32> {
        self.player_ids().into_iter().find(|i| self.player(*i).is_some_and(|p| p.name.eq_ignore_ascii_case(name)))
    }

    fn gm_tsao_log(&self, gm: &str, action: &str, target: &str, detail: &str) {
        if let Err(e) = self.db.gm_log(gm, action, target, detail) {
            tracing::warn!(?e, "no se pudo escribir el registro de GM");
        }
    }
}

/// `hh:mm:ss` UTC of the server clock.
fn chrono_now() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) % 86_400;
    format!("{:02}:{:02}:{:02} UTC", s / 3600, s / 60 % 60, s % 60)
}

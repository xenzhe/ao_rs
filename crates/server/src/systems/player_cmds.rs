//! Player slash commands of `ProtocolCmdParse.ParseUserCommand` that were reachable only
//! through windows and buttons here, plus the ones that had no equivalent at all:
//! - aliases of existing actions: `/EXIT`, `/SEG`, `/CMSG`, `/GRUPO`, `/SALIRCLAN`,
//!   `/MEDITAR`, `/COMERCIAR`, `/BOVEDA`, `/DEPOSITAR`, `/RETIRAR`, `/RETIRARFACCION`,
//!   `/INFORMACION`, `/RECOMPENSA`, `/CURAR`, `/RESUCITAR`;
//! - new: `/ONLINECLAN`, `/PROMEDIO`, `/BALANCE`, `/QUIETO`, `/ACOMPAÑAR`, `/APOSTAR`
//!   (`HandleGamble`), `/PERDON` + `/DONAR` (`HandleForgive` / `HandleDonateGold` /
//!   `VolverCiudadano`), `/FMSG` `/REALMSG` `/CAOSMSG` (`HandleFactionMessage`),
//!   `/BMSG` (`HandleCouncilMessage`), `/ONLINEREAL` `/ONLINECAOS` (GM).
//!
//! Commands that act on an NPC use the last clicked one (`flags.TargetNPC`).
use crate::security::sanitize_chat;
use crate::world::World;
use ao_formats::dat::npcs::{Movement, NpcType};
use ao_protocol::p4::gm::GmRank;
use ao_protocol::{ChannelC2S, FactionC2S, GuildC2S, Tone, S2C};
use ao_rules::faction as rules;
use ao_rules::MAX_CHAT;

/// `CostoPerdonPorCiudadano` (Configuracion.ini).
pub const FORGIVE_COST_PER_CITIZEN: u32 = 5000;
/// `HandleGamble`: bets from 1 to 5000 coins, 10 % chance of winning.
const GAMBLE_MAX: u32 = 5000;
const GAMBLE_WIN_PERCENT: i32 = 10;

/// The clicked NPC: (npc id, kind, Manhattan distance, pet owner).
struct Target {
    nid: u32,
    kind: NpcType,
    dist: i32,
    owner: Option<u32>,
}

impl World {
    pub(crate) fn extra_player_command(&mut self, id: u32, line: &str) -> bool {
        let mut parts = line.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("").to_lowercase();
        let rest = parts.next().unwrap_or("").trim().to_string();
        match cmd.as_str() {
            "/exit" => self.begin_quit(id),
            "/seg" => self.on_faction(id, FactionC2S::ToggleSafe),
            "/cmsg" => self.say_channel(id, &rest, |text| ChannelC2S::Guild { text }),
            "/grupo" | "/cluster" => self.say_channel(id, &rest, |text| ChannelC2S::Party { text }),
            "/salirclan" | "/exitrclan" => self.on_guild(id, GuildC2S::Leave),
            "/onlineclan" | "/onclan" => self.cmd_online_clan(id),
            "/meditar" | "/meditate" => self.on_meditate(id),
            "/retirarfaccion" => self.on_faction(id, FactionC2S::Leave { npc: self.player(id).and_then(|p| p.target_npc) }),
            "/promedio" | "/average" => self.cmd_average(id),
            "/fmsg" | "/realmsg" | "/caosmsg" => self.cmd_faction_message(id, &rest, false),
            "/bmsg" => self.cmd_faction_message(id, &rest, true),
            "/onlinereal" => self.cmd_online_faction(id, true),
            "/onlinecaos" => self.cmd_online_faction(id, false),
            "/comerciar" | "/trade" => self.cmd_npc(id, |w, id, t| {
                if t.dist > 4 {
                    return w.console(id, "Estas demasiado lejos.", Tone::Info);
                }
                w.open_npc_trade(id, t.nid);
            }),
            "/boveda" | "/bank" => self.cmd_npc(id, |w, id, t| w.cmd_bank_open(id, &t)),
            "/depositar" | "/deposit" => self.cmd_gold_amount(id, &rest, true),
            "/retirar" | "/withdraw" => self.cmd_gold_amount(id, &rest, false),
            "/balance" => self.cmd_npc(id, |w, id, t| {
                if t.kind != NpcType::Banker || t.dist > 3 {
                    return w.console(id, "Primero hacé click en un banquero cercano.", Tone::Info);
                }
                let gold = w.player(id).map_or(0, |p| p.bank.gold);
                w.send(id, &S2C::NpcSay { id: t.nid, text: format!("Tenes {gold} monedas de oro en tu cuenta.") });
            }),
            "/informacion" | "/information" | "/recompensa" | "/reward" => self.cmd_npc(id, |w, id, t| {
                if t.kind != NpcType::Recruiter {
                    return w.console(id, "Primero tenés que seleccionar a un reclutador.", Tone::Info);
                }
                w.faction_npc(id, t.nid, t.kind);
            }),
            "/curar" | "/cure" | "/resucitar" | "/resurrect" => self.cmd_npc(id, |w, id, t| {
                if !matches!(t.kind, NpcType::Reviver | NpcType::NewbieReviver) {
                    return w.console(id, "Primero tenés que seleccionar a un sacerdote.", Tone::Info);
                }
                let Some(p) = w.player(id) else { return };
                let (dead, level) = (p.hp <= 0, p.level);
                w.priest_cmd(id, t.kind, t.dist, dead, level);
            }),
            "/quieto" | "/stay" => self.cmd_pet(id, Movement::Static),
            "/acompañar" | "/acompanar" | "/follow_pet" => self.cmd_pet(id, Movement::FollowOwner),
            "/apostar" | "/gamble" => self.cmd_gamble(id, &rest),
            "/perdon" | "/sorry" => self.cmd_forgive(id, None),
            "/donar" | "/donate" => match rest.parse::<u32>() {
                Ok(n) if n > 0 => self.cmd_forgive(id, Some(n)),
                _ => self.console(id, "Utilice /donar CANTIDAD.", Tone::Info),
            },
            _ => return false,
        }
        true
    }

    fn say_channel(&mut self, id: u32, text: &str, make: impl Fn(String) -> ChannelC2S) {
        if text.is_empty() {
            return self.console(id, "Escribí un mensaje.", Tone::Info);
        }
        self.on_channel(id, make(text.to_string()));
    }

    /// The clicked NPC, if it is still on the player's map.
    fn clicked_npc(&self, id: u32) -> Option<Target> {
        let p = self.player(id)?;
        let nid = p.target_npc?;
        let n = self.maps.get(&p.map)?.npcs.get(&nid)?;
        Some(Target { nid, kind: n.kind, dist: (p.x - n.x).abs() + (p.y - n.y).abs(), owner: n.pet.owner })
    }

    fn cmd_npc(&mut self, id: u32, f: impl FnOnce(&mut World, u32, Target)) {
        if self.is_dead(id) {
            return self.console(id, "¡¡Estás muerto!!", Tone::Warn);
        }
        match self.clicked_npc(id) {
            Some(t) => f(self, id, t),
            None => self.console(id, "Primero tenés que seleccionar un personaje, hacé click izquierdo sobre él.", Tone::Info),
        }
    }

    fn cmd_bank_open(&mut self, id: u32, t: &Target) {
        if t.kind != NpcType::Banker {
            return self.console(id, "Primero hacé click en un banquero.", Tone::Info);
        }
        if t.dist > 6 {
            return self.console(id, "Estas demasiado lejos.", Tone::Info);
        }
        if self.player(id).is_some_and(|p| p.bank.open_with != Some(t.nid)) {
            self.open_bank(id, t.nid);
        }
    }

    /// `/DEPOSITAR` / `/RETIRAR` (`HandleBankDepositGold` / `HandleBankExtractGold`).
    fn cmd_gold_amount(&mut self, id: u32, arg: &str, deposit: bool) {
        let Ok(amount) = arg.parse::<u32>() else {
            return self.console(id, "Cantidad incorrecta.", Tone::Info);
        };
        self.cmd_npc(id, |w, id, t| {
            w.cmd_bank_open(id, &t);
            if w.player(id).is_some_and(|p| p.bank.open_with == Some(t.nid)) {
                if deposit {
                    w.on_bank_deposit_gold(id, amount);
                } else {
                    w.on_bank_withdraw_gold(id, amount);
                }
            }
        });
    }

    /// `HandleGuildOnline`.
    fn cmd_online_clan(&mut self, id: u32) {
        let members = self.guild_members_online(id);
        if self.player(id).and_then(|p| p.guild.guild).is_none() {
            return self.console(id, "No pertenecés a ningún clan.", Tone::Info);
        }
        let names: Vec<String> = members.iter().filter_map(|m| self.player(*m).map(|p| p.name.clone())).collect();
        self.console(id, format!("Compañeros de tu clan conectados: {}", names.join(", ")), Tone::Info);
    }

    /// `HandlePromedio`: expected hit points for class, constitution and level.
    fn cmd_average(&mut self, id: u32) {
        use ao_rules::character::ATTR_CONSTITUTION;
        use ao_rules::progression::{average_hp_per_level, average_max_hp};
        let Some(p) = self.player(id) else { return };
        let Some(class) = ao_formats::dat::balance::Class::from_id(p.class as i32) else { return };
        let con = p.stats.attributes[ATTR_CONSTITUTION];
        let (level, max_hp) = (p.level, p.max_hp);
        let per = average_hp_per_level(&self.ao.balance, class, con);
        let expected = average_max_hp(&self.ao.balance, class, con, level);
        let diff = max_hp - expected;
        self.console(id, format!("Vida esperada: {expected} (promedio {per:.1} por nivel)"), Tone::Info);
        let tone = if diff < 0 { Tone::Warn } else { Tone::Good };
        self.console(id, format!("Vida actual: {max_hp} ({}{})", if diff < 0 { "-" } else { "+" }, diff.abs()), tone);
    }

    /// `HandleFactionMessage` (`/FMSG`): to the own side (army+council or legion+concilio).
    /// `council`: `HandleCouncilMessage` (`/BMSG`), only between council members.
    fn cmd_faction_message(&mut self, id: u32, text: &str, council: bool) {
        let Some(clean) = sanitize_chat(text, MAX_CHAT) else {
            return self.console(id, "Escribí un mensaje.", Tone::Info);
        };
        if self.gm_is_silenced(id) {
            return self.console(id, "Estás silenciado.", Tone::Warn);
        }
        let status = self.faction_status(id);
        let royal = rules::is_royal(status);
        let label = match (council, status) {
            (true, rules::COUNCIL_ROYAL) => "Consejo",
            (true, rules::COUNCIL_CHAOS) => "Concilio",
            (false, rules::COUNCIL_ROYAL) => "Consejo",
            (false, rules::ROYAL) => "Armada",
            (false, rules::COUNCIL_CHAOS) => "Concilio",
            (false, rules::CHAOS) => "Legión",
            _ => return self.console(id, if council { "No pertenecés a ningún consejo." } else { "No pertenecés a ninguna facción." }, Tone::Info),
        };
        let now = self.now();
        let Some(p) = self.sessions.get_mut(&id).and_then(|s| s.player.as_mut()) else { return };
        if !p.chat.allow(now) {
            return self.console(id, "No hagas flood.", Tone::Warn);
        }
        let line = format!("({label}) {}> {clean}", p.name);
        self.gm_chat_record(id, &clean);
        for m in self.player_ids() {
            let s = self.faction_status(m);
            let same = if council { s == status } else if royal { rules::is_royal(s) } else { rules::is_chaos(s) };
            if same || self.gm_rank(m) != GmRank::User {
                self.console(m, line.clone(), Tone::Info);
            }
        }
    }

    /// `HandleOnlineRoyalArmy` / `HandleOnlineChaosLegion` (GMs only).
    fn cmd_online_faction(&mut self, id: u32, royal: bool) {
        if self.gm_rank(id) == GmRank::User {
            return;
        }
        let names: Vec<String> = self
            .player_ids()
            .into_iter()
            .filter(|m| if royal { rules::is_royal(self.faction_status(*m)) } else { rules::is_chaos(self.faction_status(*m)) })
            .filter_map(|m| self.player(m).map(|p| p.name.clone()))
            .collect();
        let who = if royal { "Armadas" } else { "Caos" };
        let text = if names.is_empty() { format!("No hay {who} conectados.") } else { format!("{who} conectados: {}", names.join(", ")) };
        self.console(id, text, Tone::Info);
    }

    /// `HandlePetStand` / `HandlePetFollow` on the clicked own pet, within 10 tiles.
    fn cmd_pet(&mut self, id: u32, movement: Movement) {
        self.cmd_npc(id, |w, id, t| {
            if t.dist > 10 {
                return w.console(id, "Estás demasiado lejos.", Tone::Info);
            }
            if t.owner != Some(id) {
                return;
            }
            let Some(map) = w.player(id).map(|p| p.map) else { return };
            if movement == Movement::FollowOwner {
                w.pet_follow(map, t.nid);
            } else if let Some(n) = w.maps.get_mut(&map).and_then(|m| m.npcs.get_mut(&t.nid)) {
                n.movement = Movement::Static;
                n.target = None;
                n.pet.target_npc = None;
            }
        });
    }

    /// `HandleGamble` with a `Timbero`.
    fn cmd_gamble(&mut self, id: u32, arg: &str) {
        let Ok(amount) = arg.parse::<u32>() else {
            return self.console(id, "Utilice /apostar CANTIDAD.", Tone::Info);
        };
        self.cmd_npc(id, |w, id, t| {
            if t.dist > 10 {
                return w.console(id, "Estás demasiado lejos.", Tone::Info);
            }
            let say = |w: &World, text: String| w.send(id, &S2C::NpcSay { id: t.nid, text });
            if t.kind != NpcType::Gambler {
                return say(w, "No tengo ningún interés en apostar.".into());
            }
            if amount < 1 {
                return say(w, "El mínimo de apuesta es 1 moneda.".into());
            }
            if amount > GAMBLE_MAX {
                return say(w, "El máximo de apuesta es 5.000 monedas.".into());
            }
            if w.player(id).is_some_and(|p| p.inv.gold < amount) {
                return say(w, "No tienes esa cantidad.".into());
            }
            let won = w.rng.range(1, 100) <= GAMBLE_WIN_PERCENT;
            let p = w.player_mut(id);
            if won {
                p.inv.gold = p.inv.gold.saturating_add(amount);
            } else {
                p.inv.gold -= amount;
            }
            w.send_gold_pub(id);
            say(w, if won { format!("¡Felicidades! Has ganado {amount} monedas de oro.") } else { format!("Lo siento, has perdido {amount} monedas de oro.") });
        });
    }

    /// `HandleForgive` (`donation = None`: says the price) and `HandleDonateGold`, then
    /// `VolverCiudadano`: a criminal pays the priest to become a citizen again.
    fn cmd_forgive(&mut self, id: u32, donation: Option<u32>) {
        self.cmd_npc(id, |w, id, t| {
            let newbie = w.player(id).is_some_and(|p| p.level <= w.data.balance.newbie_max_level);
            if !(t.kind == NpcType::Reviver || (t.kind == NpcType::NewbieReviver && newbie)) {
                return w.console(id, "Primero tenés que seleccionar al sacerdote.", Tone::Info);
            }
            if t.dist > 3 {
                return w.console(id, "El sacerdote no puede escucharte, estás demasiado lejos.", Tone::Info);
            }
            let say = |w: &World, text: &str| {
                w.send(id, &S2C::NpcSay { id: t.nid, text: text.to_string() });
                w.console(id, text.to_string(), Tone::Chat);
            };
            let status = w.faction_status(id);
            if rules::is_citizen(status) || rules::is_royal(status) {
                return say(w, "Tu alma ya está libre de pecados, hijo mío.");
            }
            if rules::is_chaos(status) {
                return say(w, "¡¡Dios no te perdonará mientras seas fiel al Demonio!!");
            }
            let gid = w.player(id).and_then(|p| p.guild.guild);
            let dark_guild = gid.and_then(|g| w.guilds.list.get(&g)).is_some_and(|g| {
                matches!(g.alignment, ao_rules::guild::Alignment::Criminal | ao_rules::guild::Alignment::Chaos)
            });
            if dark_guild {
                return say(w, "No podrás ser perdonado perteneciendo a un clan de alineación criminal u oscura.");
            }
            let killed = w.player(id).map_or(0, |p| p.faction.rec.citizens_killed);
            let price = if killed > 0 { killed.saturating_mul(w.cfg.world.gold_mult.max(1)).saturating_mul(FORGIVE_COST_PER_CITIZEN) } else { FORGIVE_COST_PER_CITIZEN / 2 };
            let Some(gold) = donation else {
                let text = if killed > 0 {
                    format!("Has matado a ciudadanos inocentes, Dios no puede perdonarte lo que has hecho. Pero si haces una generosa donación de {price} monedas de oro, tal vez cambie de opinión... (/donar {price})")
                } else {
                    format!("Para volver a ser un ciudadano deberás donar {price} monedas de oro. (/donar {price})")
                };
                return say(w, &text);
            };
            if w.player(id).is_some_and(|p| p.inv.gold < gold) {
                return w.console(id, "No tienes suficiente dinero.", Tone::Warn);
            }
            if gold < price {
                return say(w, "Dios no puede perdonarte si eres una persona avara.");
            }
            w.player_mut(id).inv.gold -= gold;
            w.send_gold_pub(id);
            w.console(id, format!("Has donado {gold} monedas de oro."), Tone::Info);
            say(w, "¡Gracias por tu generosa donación! Con estas palabras, te libero de todo tipo de pecados. ¡Que Dios te acompañe, hijo mío!");
            w.faction_become_citizen(id);
        });
    }
}

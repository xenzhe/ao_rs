//! Guild system. OWNER: Opus A (docs/P2_CONTRATO.md).
//!
//! Ported from the official AO20 server (`modGuilds.bas`, `clsClan.cls` and the guild
//! handlers of `Protocol.bas`). Each handler cites the VB6 routine it follows. Holes of the
//! original that are closed here: rejecting applicants and editing the codex required no
//! leader check (`a_RechazarAspirante`, `ChangeCodexAndDesc`), names had no length limit
//! (`GuildNameValido`) and free text was not capped.
//!
//! Alliances, peace and wars: AO20 keeps `e_RELACIONES_GUILD` and the messages but its
//! handlers answer "Relaciones de clan desactivadas por el momento" (HandleGuildDeclareWar,
//! HandleGuildOfferAlliance, HandleGuildAcceptAlliance...). They are rebuilt here as a
//! leader-only proposal/accept state machine (war is unilateral, as in the handler's dead
//! code), persisted, with no combat effect yet.
use crate::world::guild::Guild;
use crate::world::World;
use ao_protocol::social::guild::{GuildApplicant, GuildInfo, GuildMember, GuildRelation};
use ao_protocol::{GuildC2S, GuildS2C, Tone, S2C};
use ao_rules::guild::{self as rules, Alignment, FactionStatus, Founder, GuildLevels, Relation};
use std::collections::{BTreeMap, HashMap};
use tracing::{info, warn};

const NOT_IN_GUILD: &str = "No perteneces a ningún clan.";
const NOT_LEADER: &str = "No eres el líder de tu clan.";
const NO_DEV: &str = "Los clanes necesitan un personaje de cuenta.";

fn to_proto(r: Relation) -> GuildRelation {
    match r {
        Relation::War => GuildRelation::War,
        Relation::Peace => GuildRelation::Peace,
        Relation::Allied => GuildRelation::Alliance,
    }
}

fn from_proto(r: GuildRelation) -> Relation {
    match r {
        GuildRelation::War => Relation::War,
        GuildRelation::Peace => Relation::Peace,
        GuildRelation::Alliance => Relation::Allied,
    }
}

impl World {
    pub(crate) fn on_guild(&mut self, id: u32, msg: GuildC2S) {
        let now = self.now();
        if !self.player_mut(id).guild.rate.allow(now) {
            // Flood of guild requests: a real client never sends this many.
            return self.strike(id, "flood de clanes");
        }
        self.ensure_guilds();
        if self.player(id).and_then(|p| p.char_id).is_none() {
            return self.console(id, NO_DEV, Tone::Warn);
        }
        match msg {
            GuildC2S::Found { name, description, alignment } => self.guild_found(id, &name, &description, alignment),
            GuildC2S::RequestJoin { guild, application } => self.guild_request_join(id, &guild, &application),
            GuildC2S::Accept { name } => self.guild_accept(id, &name),
            GuildC2S::Reject { name, reason } => self.guild_reject(id, &name, &reason),
            GuildC2S::Kick { name } => self.guild_kick(id, &name),
            GuildC2S::Leave => self.guild_leave(id),
            GuildC2S::UpdateNews { news } => self.guild_update_text(id, Some(news), None),
            GuildC2S::UpdateDescription { description } => self.guild_update_text(id, None, Some(description)),
            GuildC2S::Info => match self.player(id).and_then(|p| p.guild.guild) {
                Some(_) => self.send_guild_info(id),
                None => self.send(id, &S2C::Guild(GuildS2C::NoGuild)),
            },
            GuildC2S::List => {
                // PrepareGuildsList
                let list = self.guilds.list.values().map(|g| (g.name.clone(), g.alignment as u8)).collect();
                self.send(id, &S2C::Guild(GuildS2C::List(list)));
            }
            GuildC2S::Propose { guild, relation, text } => self.guild_propose(id, &guild, from_proto(relation), &text),
            GuildC2S::AcceptProposal { guild } => self.guild_answer_proposal(id, &guild, true),
            GuildC2S::RejectProposal { guild } => self.guild_answer_proposal(id, &guild, false),
            GuildC2S::DeclareWar { guild } => self.guild_declare_war(id, &guild),
        }
    }

    /// HOOK (chat channels): ids of the online members of `id`'s guild, `id` included.
    /// Empty if not in a guild.
    pub(crate) fn guild_members_online(&self, id: u32) -> Vec<u32> {
        match self.player(id).and_then(|p| p.guild.guild) {
            Some(g) => self.guild_online(g),
            None => Vec::new(),
        }
    }

    /// Called when the player enters the world (after everything else is sent).
    /// `m_ConectarMiembroAClan` (from `ConnectUser`) + `SendGuildNews`.
    pub(crate) fn guild_on_enter(&mut self, id: u32) {
        self.ensure_guilds();
        let tags: Vec<(u32, String)> = self.player_ids().into_iter().filter(|o| *o != id).filter_map(|o| self.guild_tag(o).map(|t| (o, t))).collect();
        self.send(id, &S2C::Guild(GuildS2C::Tags(tags)));
        let Some(cid) = self.player(id).and_then(|p| p.char_id) else { return };
        // SaveUserGuildRejectionReason: the answer to an application made while offline.
        if let Ok(Some(reason)) = self.db.take_guild_rejection(cid) {
            self.console(id, reason, Tone::Warn);
        }
        let Some(gid) = self.guilds.of_char(cid) else { return };
        let g = &self.guilds.list[&gid];
        // m_EstadoPermiteEntrar: a member whose faction no longer fits the alignment is
        // expelled (the antifaction expulsion of m_PuedeSalirDeClan with QuienLoEchaUI=-1).
        // The founder is kept so the guild never loses its leader.
        if !rules::status_allows(g.alignment, self.guild_status_of(id)) && g.founder != cid {
            let name = g.name.clone();
            self.guild_remove_member(gid, cid);
            self.console(id, format!("Tu alineación no te permite seguir en el clan {name}. Has sido expulsado."), Tone::Warn);
            self.send(id, &S2C::Guild(GuildS2C::NoGuild));
            return;
        }
        self.player_mut(id).guild.guild = Some(gid);
        self.send_guild_info(id);
        let news = self.guilds.list[&gid].news.clone();
        if !news.is_empty() {
            self.console(id, format!("Noticias del clan: {news}"), Tone::Info);
        }
        self.broadcast_tag(id);
    }

    /// Called right before the player leaves the world (logout or kick), still present.
    /// `m_DesconectarMiembroDelClan`.
    pub(crate) fn guild_on_leave(&mut self, id: u32) {
        if self.player_mut(id).guild.guild.take().is_some() {
            self.broadcast_all(&S2C::Guild(GuildS2C::Tag { id, tag: None }), Some(id));
        }
    }

    /// HOOK for the integrator (combat, after killing a creature): `CheckClanExp` from
    /// `MuereNpc` with `GiveEXPClan * ExpMult`.
    #[allow(dead_code)]
    pub(crate) fn guild_on_npc_killed(&mut self, killer: u32, npc_number: u16) {
        let exp = self.ao.npcs.get(npc_number as u32).map_or(0, |n| n.clan_exp.max(0) as u32);
        let gain = exp.saturating_mul(self.cfg.world.exp_mult.max(1));
        self.guild_add_exp(killer, gain);
    }

    /// `CheckClanExp`: the guild of `id` earns `gain` exp (one level at most per call).
    #[allow(dead_code)]
    pub(crate) fn guild_add_exp(&mut self, id: u32, gain: u32) {
        let Some(gid) = self.player(id).and_then(|p| p.guild.guild) else { return };
        if gain == 0 {
            return;
        }
        let levels = self.guild_levels().clone();
        let g = self.guilds.list.get_mut(&gid).expect("guild");
        if g.level >= levels.max_level {
            return;
        }
        let (level, exp) = levels.add_exp(g.level, g.exp, gain);
        let up = level > g.level;
        (g.level, g.exp) = (level, exp);
        let _ = self.db.save_guild(&self.guilds.list[&gid]);
        self.guild_console(gid, &format!("Clan> El clan ha ganado {gain} puntos de experiencia."), Tone::Good);
        if up {
            self.guild_console(gid, &format!("Clan> El clan ha subido a nivel {level}. Nuevos beneficios disponibles."), Tone::Good);
        }
        self.refresh_guild(gid);
    }

    // ------------------------------------------------------------ actions

    /// `HandleCreateNewGuild` -> `CrearNuevoClan` (`PuedeFundarUnClan`, `GuildNameValido`,
    /// `YaExiste`), then `QuitarObjetos` of the four gems.
    fn guild_found(&mut self, id: u32, name: &str, description: &str, alignment: u8) {
        let Some(a) = Alignment::from_u8(alignment) else {
            return self.strike(id, "alineación de clan inexistente");
        };
        let p = self.player(id).expect("player");
        let cid = p.char_id.expect("account character");
        let f = Founder {
            in_guild: p.guild.guild.is_some() || self.guilds.of_char(cid).is_some(),
            level: p.level,
            leadership: p.stats.skills[rules::SKILL_LEADERSHIP],
            has_gems: rules::FOUNDATION_GEMS.iter().all(|g| p.inv.slots.iter().flatten().any(|s| s.obj == *g)),
            safe_on: self.guild_safe_on(id),
            status: self.guild_status_of(id),
        };
        if let Some(why) = rules::found_refusal(&f, a) {
            return self.console(id, why, Tone::Warn);
        }
        let name = name.to_string();
        if !rules::valid_name(&name) {
            return self.console(id, "Nombre de clan inválido.", Tone::Warn);
        }
        if self.guilds.by_name(&name).is_some() {
            return self.console(id, "Ya existe un clan con ese nombre.", Tone::Warn);
        }
        if self.guilds.list.len() >= rules::MAX_GUILDS {
            return self.console(id, "No hay más slots para fundar clanes. Consulte a un administrador.", Tone::Warn);
        }
        let description = rules::clean_text(description, rules::MAX_DESC_LEN);
        let news = format!("¡Bienvenido a {name}! Clan creado con alineación : {}.", a.name());
        let gid = match self.db.insert_guild(&name, a, cid, &description, &news) {
            Ok(gid) => gid,
            Err(crate::db::DbError::Taken) => return self.console(id, "Ya existe un clan con ese nombre.", Tone::Warn),
            Err(e) => {
                warn!(?e, "no se pudo guardar el clan");
                return self.console(id, "No se pudo fundar el clan.", Tone::Warn);
            }
        };
        let pname = self.player(id).unwrap().name.clone();
        self.guilds.list.insert(
            gid,
            Guild {
                id: gid,
                name: name.clone(),
                alignment: a,
                founder: cid,
                leader: cid,
                description,
                news,
                level: 1,
                exp: 0,
                members: BTreeMap::from([(cid, pname.clone())]),
                applicants: Vec::new(),
                relations: HashMap::new(),
                proposals: BTreeMap::new(),
            },
        );
        // A pending application elsewhere is dropped (the founder now has a guild).
        if let Some(old) = self.guilds.applied_to(cid) {
            self.guilds.list.get_mut(&old).unwrap().applicants.retain(|x| x.0 != cid);
        }
        for gem in rules::FOUNDATION_GEMS {
            let slot = self.player(id).unwrap().inv.slots.iter().position(|s| s.is_some_and(|s| s.obj == gem));
            if let Some(i) = slot {
                self.remove_from_slot(id, (i + 1) as u8, 1);
            }
        }
        self.player_mut(id).guild.guild = Some(gid);
        info!(guild = %name, founder = %pname, "clan fundado");
        self.broadcast_all(&S2C::Console { text: format!("{pname} ha fundado el clan <{name}> de alineación {}.", a.name()), color: Tone::Good }, None);
        self.send_guild_info(id);
        self.broadcast_tag(id);
    }

    /// `HandleGuildRequestMembership` -> `a_NuevoAspirante`.
    fn guild_request_join(&mut self, id: u32, guild: &str, application: &str) {
        let p = self.player(id).expect("player");
        let cid = p.char_id.expect("account character");
        let (pname, level) = (p.name.clone(), p.level);
        if p.guild.guild.is_some() || self.guilds.of_char(cid).is_some() {
            return self.console(id, "Ya perteneces a un clan, debes salir del mismo antes de solicitar ingresar a otro.", Tone::Warn);
        }
        if level <= rules::NEWBIE_MAX_LEVEL {
            return self.console(id, "Los newbies no tienen derecho a entrar a un clan.", Tone::Warn);
        }
        let Some(g) = self.guilds.by_name(guild) else {
            return self.console(id, "Ese clan no existe.", Tone::Warn);
        };
        let gid = g.id;
        if !rules::status_allows(g.alignment, self.guild_status_of(id)) {
            return self.console(id, format!("Tú no podés entrar a un clan de alineación {}.", g.alignment.name()), Tone::Warn);
        }
        if g.applicants.iter().any(|a| a.0 == cid) {
            return self.console(id, "Ya has enviado una solicitud a este clan.", Tone::Warn);
        }
        if g.applicants.len() >= rules::MAX_APPLICANTS {
            return self.console(id, "El clan tiene demasiados aspirantes. Contáctate con un miembro para que procese las solicitudes.", Tone::Warn);
        }
        let gname = g.name.clone();
        // The old application to another guild is withdrawn (ViejoSolicitado).
        if let Some(old) = self.guilds.applied_to(cid) {
            self.guilds.list.get_mut(&old).unwrap().applicants.retain(|x| x.0 != cid);
            self.refresh_guild(old);
        }
        let application = rules::clean_text(application, rules::MAX_APPLICATION_LEN);
        let _ = self.db.set_guild_request(gid, cid, &application);
        self.guilds.list.get_mut(&gid).unwrap().applicants.push((cid, pname.clone(), application));
        self.guild_console(gid, &format!("Clan: [{pname}] ha enviado solicitud para unirse al clan."), Tone::Info);
        self.refresh_guild(gid);
        self.console(id, format!("Tu solicitud ha sido enviada. Espera prontas noticias del líder de {gname}."), Tone::Good);
    }

    /// The guild of `id` if `id` is its leader; otherwise tells why not.
    fn led_guild(&self, id: u32) -> Option<i64> {
        let p = self.player(id)?;
        let Some(gid) = p.guild.guild else {
            self.console(id, NOT_IN_GUILD, Tone::Warn);
            return None;
        };
        if self.guilds.list[&gid].leader != p.char_id? {
            self.console(id, NOT_LEADER, Tone::Warn);
            return None;
        }
        Some(gid)
    }

    /// `HandleGuildAcceptNewMember` -> `a_AceptarAspirante`.
    fn guild_accept(&mut self, id: u32, name: &str) {
        let Some(gid) = self.led_guild(id) else { return };
        let levels = self.guild_levels().clone();
        let g = &self.guilds.list[&gid];
        let price = levels.accept_price(g.level);
        if self.player(id).unwrap().inv.gold < price {
            return self.console(id, format!("Para aceptar un nuevo miembro necesitas {price} monedas de oro."), Tone::Warn);
        }
        let Some((cid, aname, _)) = g.applicants.iter().find(|a| a.1.eq_ignore_ascii_case(name.trim())).cloned() else {
            return self.console(id, "El Pj no es aspirante al clan.", Tone::Warn);
        };
        let (alignment, gname) = (g.alignment, g.name.clone());
        let online = self.char_online(cid);
        let refusal = if self.guilds.of_char(cid).is_some() {
            Some(format!("{aname} ya es parte de otro clan."))
        } else if let Some(o) = online {
            if !rules::status_allows(alignment, self.guild_status_of(o)) {
                Some(format!("{aname} no puede entrar a un clan {}.", alignment.name()))
            } else if alignment == Alignment::Citizen && !self.guild_safe_on(o) {
                Some(format!("{aname} deberá activar el seguro para entrar al clan."))
            } else {
                None
            }
        } else {
            // Offline: the faction is checked again when the character enters the world.
            None
        };
        if let Some(why) = refusal {
            self.guild_drop_applicant(gid, cid);
            self.refresh_guild(gid);
            return self.console(id, why, Tone::Warn);
        }
        let g = &self.guilds.list[&gid];
        if g.members.len() >= levels.members_allowed(g.level) {
            return self.console(id, "La capacidad del clan está completa.", Tone::Warn);
        }
        if let Err(e) = self.db.add_guild_member(gid, cid) {
            warn!(?e, "no se pudo guardar el miembro del clan");
            return self.console(id, "No se pudo aceptar al aspirante.", Tone::Warn);
        }
        self.player_mut(id).inv.gold -= price;
        self.send_inventory_vitals(id);
        let g = self.guilds.list.get_mut(&gid).unwrap();
        g.applicants.retain(|a| a.0 != cid);
        g.members.insert(cid, aname.clone());
        if let Some(o) = online {
            self.player_mut(o).guild.guild = Some(gid);
            self.broadcast_tag(o);
        }
        self.guild_console(gid, &format!("[{aname}] ha sido aceptado como miembro del clan."), Tone::Good);
        self.refresh_guild(gid);
        info!(guild = %gname, member = %aname, "aceptado en el clan");
    }

    /// `HandleGuildRejectNewMember` -> `a_RechazarAspirante` (+ leader check, missing in VB6).
    fn guild_reject(&mut self, id: u32, name: &str, reason: &str) {
        let Some(gid) = self.led_guild(id) else { return };
        let g = &self.guilds.list[&gid];
        let Some((cid, _, _)) = g.applicants.iter().find(|a| a.1.eq_ignore_ascii_case(name.trim())).cloned() else {
            return self.console(id, "El Pj no es aspirante al clan.", Tone::Warn);
        };
        let reason = rules::clean_text(reason, rules::MAX_APPLICATION_LEN);
        let text = format!("Fue rechazada tu solicitud de ingreso a {} : {reason}", g.name);
        self.guild_drop_applicant(gid, cid);
        match self.char_online(cid) {
            Some(o) => self.console(o, text, Tone::Warn),
            None => {
                let _ = self.db.set_guild_rejection(cid, &text);
            }
        }
        self.console(id, "Solicitud rechazada.", Tone::Info);
        self.refresh_guild(gid);
    }

    /// `HandleGuildKickMember` -> `m_EcharMiembroDeClan` / `m_PuedeSalirDeClan`: only the
    /// leader of the member's own guild, never the founder.
    fn guild_kick(&mut self, id: u32, name: &str) {
        let me = self.player(id).and_then(|p| p.char_id).unwrap();
        let target = self.guilds.list.values().find_map(|g| g.members.iter().find(|(_, n)| n.eq_ignore_ascii_case(name.trim())).map(|(c, n)| (g.id, *c, n.clone())));
        let Some((gid, cid, tname)) = target else {
            return self.console(id, "No podés expulsar ese personaje del clan.", Tone::Warn);
        };
        if !self.can_leave(gid, cid, me) {
            return self.console(id, "No podés expulsar ese personaje del clan.", Tone::Warn);
        }
        let online = self.char_online(cid);
        self.guild_remove_member(gid, cid);
        if let Some(o) = online {
            self.console(o, "Has sido expulsado del clan.", Tone::Warn);
        }
        self.guild_console(gid, &format!("{tname} fue expulsado del clan."), Tone::Warn);
        if online != Some(id) {
            self.console(id, format!("{tname} fue expulsado del clan."), Tone::Info);
        }
    }

    /// `HandleGuildLeave` -> `m_EcharMiembroDeClan(UserIndex, .Id)`.
    fn guild_leave(&mut self, id: u32) {
        let p = self.player(id).unwrap();
        let (me, pname) = (p.char_id.unwrap(), p.name.clone());
        match p.guild.guild {
            Some(gid) if self.can_leave(gid, me, me) => {
                self.guild_remove_member(gid, me);
                self.console(id, "Dejas el clan.", Tone::Info);
                self.guild_console(gid, &format!("{pname} deja el clan."), Tone::Info);
            }
            _ => self.console(id, "Tu no puedes salir de ningún clan.", Tone::Warn),
        }
    }

    /// `ActualizarNoticias` / `ChangeCodexAndDesc` (+ leader check, missing in VB6 for the codex).
    fn guild_update_text(&mut self, id: u32, news: Option<String>, description: Option<String>) {
        let Some(gid) = self.led_guild(id) else { return };
        let g = self.guilds.list.get_mut(&gid).unwrap();
        if let Some(n) = news {
            g.news = rules::clean_text(&n, rules::MAX_NEWS_LEN);
        }
        if let Some(d) = description {
            g.description = rules::clean_text(&d, rules::MAX_DESC_LEN);
        }
        let _ = self.db.save_guild(&self.guilds.list[&gid]);
        self.refresh_guild(gid);
    }

    /// Another guild by name for a relation action; refuses the own guild.
    fn other_guild(&self, id: u32, mine: i64, name: &str) -> Option<i64> {
        match self.guilds.by_name(name) {
            None => {
                self.console(id, format!("No existe el clan: {}", name.trim()), Tone::Warn);
                None
            }
            Some(g) if g.id == mine => {
                self.console(id, "No podés hacer eso con tu propio clan.", Tone::Warn);
                None
            }
            Some(g) => Some(g.id),
        }
    }

    fn guild_propose(&mut self, id: u32, guild: &str, rel: Relation, text: &str) {
        let Some(mine) = self.led_guild(id) else { return };
        let Some(other) = self.other_guild(id, mine, guild) else { return };
        let current = self.guilds.list[&mine].relation(other);
        let pointless = match rel {
            Relation::War => return self.guild_declare_war(id, guild),
            Relation::Peace => current != Relation::War,
            Relation::Allied => current == Relation::Allied,
        };
        if pointless {
            return self.console(id, "Esa propuesta no cambia la relación entre los clanes.", Tone::Warn);
        }
        let text = rules::clean_text(text, rules::MAX_APPLICATION_LEN);
        let _ = self.db.set_guild_proposal(mine, other, rel, &text);
        self.guilds.list.get_mut(&other).unwrap().proposals.insert(mine, (rel, text));
        let (mname, oname) = (self.guilds.list[&mine].name.clone(), self.guilds.list[&other].name.clone());
        let what = if rel == Relation::Allied { "alianza" } else { "paz" };
        self.guild_console(other, &format!("El clan {mname} propone una {what} a tu clan."), Tone::Info);
        self.console(id, format!("Propuesta de {what} enviada a {oname}."), Tone::Good);
        self.refresh_guild(other);
    }

    fn guild_answer_proposal(&mut self, id: u32, guild: &str, accept: bool) {
        let Some(mine) = self.led_guild(id) else { return };
        let Some(other) = self.other_guild(id, mine, guild) else { return };
        let Some((rel, _)) = self.guilds.list.get_mut(&mine).unwrap().proposals.remove(&other) else {
            return self.console(id, "Ese clan no te ha propuesto nada.", Tone::Warn);
        };
        let _ = self.db.remove_guild_proposal(other, mine);
        let (mname, oname) = (self.guilds.list[&mine].name.clone(), self.guilds.list[&other].name.clone());
        if accept {
            self.set_relation(mine, other, rel);
            let what = if rel == Relation::Allied { "la alianza" } else { "la paz" };
            self.guild_console(mine, &format!("Tu clan ha firmado {what} con {oname}."), Tone::Good);
            self.guild_console(other, &format!("Tu clan ha firmado {what} con {mname}."), Tone::Good);
        } else {
            self.guild_console(other, &format!("El clan {mname} rechazó la propuesta de tu clan."), Tone::Warn);
            self.console(id, "Propuesta rechazada.", Tone::Info);
        }
        self.refresh_guild(mine);
        self.refresh_guild(other);
    }

    /// `HandleGuildDeclareWar` (the dead code after "desactivadas").
    fn guild_declare_war(&mut self, id: u32, guild: &str) {
        let Some(mine) = self.led_guild(id) else { return };
        let Some(other) = self.other_guild(id, mine, guild) else { return };
        if self.guilds.list[&mine].relation(other) == Relation::War {
            return self.console(id, "Tu clan ya está en guerra con ese clan.", Tone::Warn);
        }
        for (a, b) in [(mine, other), (other, mine)] {
            if self.guilds.list.get_mut(&b).unwrap().proposals.remove(&a).is_some() {
                let _ = self.db.remove_guild_proposal(a, b);
            }
        }
        self.set_relation(mine, other, Relation::War);
        let (mname, oname) = (self.guilds.list[&mine].name.clone(), self.guilds.list[&other].name.clone());
        self.guild_console(mine, &format!("TU CLAN HA ENTRADO EN GUERRA CON {oname}"), Tone::Warn);
        self.guild_console(other, &format!("{mname} LE DECLARA LA GUERRA A TU CLAN"), Tone::Warn);
        self.refresh_guild(mine);
        self.refresh_guild(other);
    }

    // ------------------------------------------------------------ helpers

    /// `LoadGuildsDB` + `LoadGuildsConfig`, once.
    fn ensure_guilds(&mut self) {
        if self.guilds.loaded {
            return;
        }
        self.guilds.loaded = true;
        self.guilds.levels = Some(GuildLevels::load(&self.recursos));
        match self.db.load_guilds() {
            Ok(list) => self.guilds.list = list.into_iter().map(|g| (g.id, g)).collect(),
            Err(e) => warn!(?e, "no se pudieron cargar los clanes"),
        }
    }

    fn guild_levels(&self) -> &GuildLevels {
        self.guilds.levels.as_ref().expect("guild levels loaded")
    }

    /// `Status(UserIndex)`. Factions belong to Opus C (`Player.faction` is still empty):
    /// everyone is a citizen until the integrator connects it.
    fn guild_status_of(&self, id: u32) -> FactionStatus {
        use ao_rules::faction as f;
        match self.faction_status(id) {
            f::CRIMINAL => FactionStatus::Criminal,
            f::CHAOS => FactionStatus::Chaos,
            f::ROYAL => FactionStatus::Armada,
            f::COUNCIL_CHAOS => FactionStatus::Conclave,
            f::COUNCIL_ROYAL => FactionStatus::Council,
            _ => FactionStatus::Citizen,
        }
    }

    /// `flags.Seguro` (faction system).
    fn guild_safe_on(&self, id: u32) -> bool {
        self.player(id).is_some_and(|p| p.faction.safe)
    }

    /// `m_PuedeSalirDeClan`: `by` may take `cid` out of `gid` if `by` is its leader or `cid`
    /// itself, and `cid` is not the founder.
    fn can_leave(&self, gid: i64, cid: i64, by: i64) -> bool {
        let g = &self.guilds.list[&gid];
        (g.leader == by || cid == by) && g.founder != cid
    }

    /// `ExpulsarMiembro` (+ `SetLeader(Fundador)` when the leader goes).
    fn guild_remove_member(&mut self, gid: i64, cid: i64) {
        let g = self.guilds.list.get_mut(&gid).unwrap();
        g.members.remove(&cid);
        let leader_left = g.leader == cid;
        if leader_left {
            g.leader = g.founder;
        }
        let _ = self.db.remove_guild_member(cid);
        if leader_left {
            let _ = self.db.save_guild(&self.guilds.list[&gid]);
        }
        if let Some(o) = self.char_online(cid) {
            self.player_mut(o).guild.guild = None;
            self.send(o, &S2C::Guild(GuildS2C::NoGuild));
            self.broadcast_tag(o);
        }
        self.refresh_guild(gid);
    }

    fn guild_drop_applicant(&mut self, gid: i64, cid: i64) {
        self.guilds.list.get_mut(&gid).unwrap().applicants.retain(|a| a.0 != cid);
        let _ = self.db.remove_guild_request(cid);
    }

    fn set_relation(&mut self, a: i64, b: i64, rel: Relation) {
        for (x, y) in [(a, b), (b, a)] {
            let g = self.guilds.list.get_mut(&x).unwrap();
            if rel == Relation::Peace {
                g.relations.remove(&y);
            } else {
                g.relations.insert(y, rel);
            }
        }
        let _ = self.db.set_guild_relation(a, b, rel);
    }

    fn char_online(&self, cid: i64) -> Option<u32> {
        self.player_ids().into_iter().find(|i| self.player(*i).and_then(|p| p.char_id) == Some(cid))
    }

    /// Online members of `gid` (`m_Iterador_ProximoUserIndex`), sorted.
    fn guild_online(&self, gid: i64) -> Vec<u32> {
        self.player_ids().into_iter().filter(|i| self.player(*i).is_some_and(|p| p.guild.guild == Some(gid))).collect()
    }

    /// `SendData(ToGuildMembers, ...)`.
    fn guild_console(&self, gid: i64, text: &str, tone: Tone) {
        for m in self.guild_online(gid) {
            self.console(m, text, tone);
        }
    }

    fn guild_tag(&self, id: u32) -> Option<String> {
        let gid = self.player(id)?.guild.guild?;
        self.guilds.list.get(&gid).map(|g| g.name.clone())
    }

    /// `RefreshCharStatus`: the `<clan>` under the name, for everyone online.
    fn broadcast_tag(&self, id: u32) {
        self.broadcast_all(&S2C::Guild(GuildS2C::Tag { id, tag: self.guild_tag(id) }), None);
    }

    /// Sends the guild details to every online member.
    fn refresh_guild(&self, gid: i64) {
        for m in self.guild_online(gid) {
            self.send_guild_info(m);
        }
    }

    /// `SendGuildNews` + `SendGuildLeaderInfo` (applicants and proposals only to the leader).
    fn send_guild_info(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let Some(g) = p.guild.guild.and_then(|g| self.guilds.list.get(&g)) else { return };
        let leader = p.char_id == Some(g.leader);
        let online: Vec<i64> = self.guild_online(g.id).into_iter().filter_map(|i| self.player(i).and_then(|p| p.char_id)).collect();
        let levels = self.guild_levels();
        let name_of = |gid: &i64| self.guilds.list.get(gid).map(|x| x.name.clone()).unwrap_or_default();
        let mut relations: Vec<(String, GuildRelation)> = g.relations.iter().map(|(o, r)| (name_of(o), to_proto(*r))).collect();
        relations.sort_by(|a, b| a.0.cmp(&b.0));
        let info = GuildInfo {
            name: g.name.clone(),
            alignment: g.alignment as u8,
            level: g.level,
            exp: g.exp,
            exp_next: levels.exp_required(g.level),
            max_members: levels.members_allowed(g.level) as u16,
            founder: g.members.get(&g.founder).cloned().unwrap_or_default(),
            leader: g.members.get(&g.leader).cloned().unwrap_or_default(),
            description: g.description.clone(),
            news: g.news.clone(),
            members: g.members.iter().map(|(c, n)| GuildMember { name: n.clone(), online: online.contains(c), founder: *c == g.founder, leader: *c == g.leader }).collect(),
            applicants: if leader { g.applicants.iter().map(|(_, n, a)| GuildApplicant { name: n.clone(), application: a.clone() }).collect() } else { Vec::new() },
            relations,
            proposals: if leader { g.proposals.iter().map(|(o, (r, t))| (name_of(o), to_proto(*r), t.clone())).collect() } else { Vec::new() },
        };
        self.send(id, &S2C::Guild(GuildS2C::Info(Box::new(info))));
    }
}

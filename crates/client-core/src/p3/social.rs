//! Client actions and state for guild, party, quests and chat channel actions (`impl Game` blocks, graphics-free and
//! testable with bots). OWNER: Sonnet 3 (docs/P3_CONTRATO.md).
//!
//! - [`parse_chat`]: what the player typed (`/s Name text`, `/g`, `/c`, `/gl`, `/y`, plain
//!   text...) turned into the message to send; [`Game::submit_chat`] sends it.
//! - [`Feed`]: the console lines of `game.console` (map chat, system) and of
//!   `game.local.social.channel.history` (whispers, party, guild, global) merged in the
//!   order they arrived, with a tab filter ([`ConsoleTab`]).
//! - [`InputHistory`]: arrow-up / arrow-down recall of what was typed.
//! - Thin actions for guild, party and quests that check what the server would check first.
use crate::Game;
use ao_protocol::social::channel::ChannelKind;
use ao_protocol::social::guild::GuildRelation;
use ao_protocol::{ChannelC2S, EntityKind, GuildC2S, PartyC2S, QuestC2S, Tone, C2S};
use ao_rules::guild::{self, Alignment, FactionStatus, Founder};
use std::collections::VecDeque;

// ---------------------------------------------------------------- typing

/// What a typed line means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chat {
    /// Map chat (plain text, or any `/` line that is not a channel command: the server
    /// handles development commands such as `/dar`).
    Say(String),
    Whisper { to: String, text: String },
    Yell(String),
    Party(String),
    Guild(String),
    Global(String),
    /// The line is a channel command used wrongly; the text explains how.
    Usage(&'static str),
}

impl Chat {
    /// Short description of where the line will go (shown while typing).
    pub fn label(&self) -> String {
        match self {
            Chat::Say(t) if t.starts_with('/') => "Comando".into(),
            Chat::Say(_) => "Mapa".into(),
            Chat::Whisper { to, .. } => format!("Susurro a {to}"),
            Chat::Yell(_) => "Grito".into(),
            Chat::Party(_) => "Grupo".into(),
            Chat::Guild(_) => "Clan".into(),
            Chat::Global(_) => "Global".into(),
            Chat::Usage(_) => "Uso".into(),
        }
    }
}

/// Splits `rest` into the first word (or a `"quoted name"`) and the remaining text.
fn first_word(rest: &str) -> (&str, &str) {
    let rest = rest.trim_start();
    if let Some(q) = rest.strip_prefix('"') {
        return match q.split_once('"') {
            Some((name, tail)) => (name.trim(), tail.trim()),
            None => (q.trim(), ""),
        };
    }
    match rest.split_once(char::is_whitespace) {
        Some((w, tail)) => (w, tail.trim()),
        None => (rest, ""),
    }
}

/// Interprets a typed line. `reply_to` is the last player who whispered to us (for `/r`).
/// `None` when the line is empty.
pub fn parse_chat(input: &str, reply_to: Option<&str>) -> Option<Chat> {
    let line = input.trim();
    if line.is_empty() {
        return None;
    }
    if !line.starts_with('/') {
        return Some(Chat::Say(line.to_string()));
    }
    let (cmd, rest) = match line.split_once(char::is_whitespace) {
        Some((c, r)) => (c.to_lowercase(), r.trim()),
        None => (line.to_lowercase(), ""),
    };
    let with_text = |usage: &'static str, make: fn(String) -> Chat| if rest.is_empty() { Chat::Usage(usage) } else { make(rest.to_string()) };
    Some(match cmd.as_str() {
        "/s" | "/w" | "/susurro" | "/whisper" | "/msg" => {
            let (to, text) = first_word(rest);
            if to.is_empty() || text.is_empty() {
                Chat::Usage("Uso: /s Nombre texto (susurro; con comillas si el nombre lleva espacios)")
            } else {
                Chat::Whisper { to: to.to_string(), text: text.to_string() }
            }
        }
        "/r" | "/re" | "/responder" => match (reply_to, rest.is_empty()) {
            (_, true) => Chat::Usage("Uso: /r texto (responde al último susurro)"),
            (None, _) => Chat::Usage("Nadie te ha susurrado todavía."),
            (Some(to), false) => Chat::Whisper { to: to.to_string(), text: rest.to_string() },
        },
        "/g" | "/grupo" | "/p" | "/party" => with_text("Uso: /g texto (canal de grupo)", Chat::Party),
        "/c" | "/clan" | "/guild" => with_text("Uso: /c texto (canal de clan)", Chat::Guild),
        "/gl" | "/global" => with_text("Uso: /gl texto (canal global)", Chat::Global),
        "/y" | "/gritar" | "/yell" => with_text("Uso: /y texto (gritar)", Chat::Yell),
        _ => Chat::Say(line.to_string()),
    })
}

/// Arrow-up / arrow-down recall of typed lines.
#[derive(Debug, Default)]
pub struct InputHistory {
    items: Vec<String>,
    cursor: Option<usize>,
    draft: String,
}

impl InputHistory {
    pub const MAX: usize = 50;

    pub fn push(&mut self, line: &str) {
        let line = line.trim();
        if !line.is_empty() && self.items.last().map(String::as_str) != Some(line) {
            self.items.push(line.to_string());
            if self.items.len() > Self::MAX {
                self.items.remove(0);
            }
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        self.cursor = None;
        self.draft.clear();
    }

    /// Arrow up: an older line (`current` is kept as the draft on the first press).
    pub fn up(&mut self, current: &str) -> Option<String> {
        if self.items.is_empty() {
            return None;
        }
        let i = match self.cursor {
            None => {
                self.draft = current.to_string();
                self.items.len() - 1
            }
            Some(i) => i.saturating_sub(1),
        };
        self.cursor = Some(i);
        Some(self.items[i].clone())
    }

    /// Arrow down: a newer line, or the draft after the newest.
    pub fn down(&mut self) -> Option<String> {
        let i = self.cursor?;
        if i + 1 < self.items.len() {
            self.cursor = Some(i + 1);
            Some(self.items[i + 1].clone())
        } else {
            self.cursor = None;
            Some(std::mem::take(&mut self.draft))
        }
    }
}

// ---------------------------------------------------------------- merged console

/// Where a console line came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    /// Chat of the map (and NPC speech).
    Map,
    /// Game messages (combat, warnings, info...).
    System,
    Whisper,
    Yell,
    Party,
    Guild,
    Global,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConsoleTab {
    All,
    Map,
    System,
    Whisper,
    Party,
    Guild,
    Global,
}

impl ConsoleTab {
    pub const ALL: [ConsoleTab; 7] = [ConsoleTab::All, ConsoleTab::Map, ConsoleTab::System, ConsoleTab::Whisper, ConsoleTab::Party, ConsoleTab::Guild, ConsoleTab::Global];

    pub fn label(self) -> &'static str {
        match self {
            ConsoleTab::All => "Todo",
            ConsoleTab::Map => "Mapa",
            ConsoleTab::System => "Sistema",
            ConsoleTab::Whisper => "Susurros",
            ConsoleTab::Party => "Grupo",
            ConsoleTab::Guild => "Clan",
            ConsoleTab::Global => "Global",
        }
    }

    pub fn accepts(self, kind: LineKind) -> bool {
        match self {
            ConsoleTab::All => true,
            ConsoleTab::Map => matches!(kind, LineKind::Map | LineKind::Yell),
            ConsoleTab::System => kind == LineKind::System,
            ConsoleTab::Whisper => kind == LineKind::Whisper,
            ConsoleTab::Party => kind == LineKind::Party,
            ConsoleTab::Guild => kind == LineKind::Guild,
            ConsoleTab::Global => kind == LineKind::Global,
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FeedLine {
    pub kind: LineKind,
    pub text: String,
    pub tone: Tone,
}

/// Most lines kept.
pub const FEED_MAX: usize = 300;

/// The console as one stream. `update` once per frame: it spots the lines added to
/// `game.console` and to the channel history since the last call and appends them in that
/// order. (A line identical to the last one while `game.console` is full, 60 lines, cannot
/// be told apart from "nothing new" and is skipped.)
#[derive(Debug, Default)]
pub struct Feed {
    pub lines: VecDeque<FeedLine>,
    /// Lines ever added per tab (for the unread counters), indexed by `ConsoleTab::index`.
    pub total: [u64; 7],
    seen_console: Vec<String>,
    seen_history: Vec<crate::social::channel::ChannelLine>,
}

/// How many entries at the end of `new` are fresh, given the `old` snapshot of the same
/// bounded queue (old entries may have been dropped from the front).
fn fresh<T: PartialEq>(old: &[T], new: &[T]) -> usize {
    if old.is_empty() {
        return new.len();
    }
    (0..=new.len()).find(|shift| {
        let keep = new.len() - shift;
        keep <= old.len() && new[..keep] == old[old.len() - keep..]
    })
    .unwrap_or(new.len())
}

impl Feed {
    pub fn update(&mut self, game: &Game) {
        let me = game.me().map(|e| e.name.clone()).unwrap_or_else(|| game.name_input.clone());
        let console: Vec<String> = game.console.iter().map(|(t, _)| t.clone()).collect();
        if console != self.seen_console {
            let n = fresh(&self.seen_console, &console);
            for (text, tone) in game.console.iter().skip(game.console.len() - n) {
                let kind = if *tone == Tone::Chat { LineKind::Map } else { LineKind::System };
                self.push(FeedLine { kind, text: text.clone(), tone: *tone });
            }
            self.seen_console = console;
        }
        let hist = &game.local.social.channel.history;
        let unchanged = hist.len() == self.seen_history.len() && hist.back() == self.seen_history.last();
        if !unchanged {
            let now: Vec<_> = hist.iter().cloned().collect();
            let n = fresh(&self.seen_history, &now);
            for l in &now[now.len() - n..] {
                self.push(format_channel_line(l, &me));
            }
            self.seen_history = now;
        }
    }

    fn push(&mut self, l: FeedLine) {
        for t in ConsoleTab::ALL {
            if t.accepts(l.kind) {
                self.total[t.index()] += 1;
            }
        }
        self.lines.push_back(l);
        while self.lines.len() > FEED_MAX {
            self.lines.pop_front();
        }
    }

    /// Lines of a tab, oldest first.
    pub fn tab(&self, tab: ConsoleTab) -> impl DoubleEndedIterator<Item = &FeedLine> {
        self.lines.iter().filter(move |l| tab.accepts(l.kind))
    }
}

/// `[Grupo] Ana: hola`, `[Susurro a Bea] hola`, ...
pub fn format_channel_line(l: &crate::social::channel::ChannelLine, me: &str) -> FeedLine {
    let (kind, text) = match l.channel {
        ChannelKind::Whisper if l.author == me => (LineKind::Whisper, format!("[Susurro a {}] {}", l.target, l.text)),
        ChannelKind::Whisper => (LineKind::Whisper, format!("[Susurro] {}: {}", l.author, l.text)),
        ChannelKind::Yell => (LineKind::Yell, format!("[Grito] {}: {}", l.author, l.text)),
        ChannelKind::Party => (LineKind::Party, format!("[Grupo] {}: {}", l.author, l.text)),
        ChannelKind::Guild => (LineKind::Guild, format!("[Clan] {}: {}", l.author, l.text)),
        ChannelKind::Global => (LineKind::Global, format!("[Global] {}: {}", l.author, l.text)),
    };
    FeedLine { kind, text, tone: Tone::Chat }
}

// ---------------------------------------------------------------- actions

fn faction_status(code: u8) -> FactionStatus {
    match code {
        0 => FactionStatus::Criminal,
        1 => FactionStatus::Citizen,
        2 => FactionStatus::Chaos,
        3 => FactionStatus::Armada,
        4 => FactionStatus::Conclave,
        _ => FactionStatus::Council,
    }
}

impl Game {
    /// Sends what the player typed: channel commands go to their channel, the rest is
    /// said on the map. Returns whether something was sent (a misuse explains itself in
    /// the console).
    pub fn submit_chat(&mut self, input: &str) -> bool {
        let reply = self.local.social.channel.last_whisper_from.clone();
        let Some(chat) = parse_chat(input, reply.as_deref()) else { return false };
        let msg = match chat {
            Chat::Say(text) => C2S::Say { text },
            Chat::Whisper { to, text } => C2S::Channel(ChannelC2S::Whisper { to, text }),
            Chat::Yell(text) => C2S::Channel(ChannelC2S::Yell { text }),
            Chat::Party(text) => C2S::Channel(ChannelC2S::Party { text }),
            Chat::Guild(text) => C2S::Channel(ChannelC2S::Guild { text }),
            Chat::Global(text) => C2S::Channel(ChannelC2S::Global { text }),
            Chat::Usage(help) => {
                self.say(help, Tone::Warn);
                return false;
            }
        };
        self.send(&msg);
        true
    }

    fn refuse(&mut self, why: &str) -> bool {
        self.say(why, Tone::Warn);
        false
    }

    // ---- guild

    pub fn my_guild_name(&self) -> Option<&str> {
        self.local.social.guild.info.as_ref().map(|g| g.name.as_str())
    }

    /// Am I the leader of my guild?
    pub fn is_guild_leader(&self) -> bool {
        let me = self.me().map(|e| e.name.as_str()).unwrap_or(self.name_input.as_str());
        self.local.social.guild.info.as_ref().is_some_and(|g| g.leader == me)
    }

    pub fn guild_list(&mut self) {
        self.send(&C2S::Guild(GuildC2S::List));
    }

    pub fn guild_info(&mut self) {
        self.send(&C2S::Guild(GuildC2S::Info));
    }

    pub fn guild_request_join(&mut self, guild: &str, application: &str) -> bool {
        if guild.trim().is_empty() {
            return self.refuse("Elige un clan.");
        }
        if application.trim().is_empty() {
            return self.refuse("Escribe unas palabras para presentarte al clan.");
        }
        let application = application.trim().chars().take(guild::MAX_APPLICATION_LEN).collect();
        self.send(&C2S::Guild(GuildC2S::RequestJoin { guild: guild.to_string(), application }));
        true
    }

    /// Why this character cannot found a guild of `alignment` now (the official text), or
    /// `None` if it can.
    pub fn found_refusal(&self, alignment: u8) -> Option<&'static str> {
        let Some(a) = Alignment::from_u8(alignment) else { return Some("Alineación desconocida.") };
        let l = &self.local;
        let founder = Founder {
            in_guild: l.social.guild.info.is_some(),
            level: self.stats.level,
            leadership: l.skills.get(guild::SKILL_LEADERSHIP).copied().unwrap_or(0),
            has_gems: guild::FOUNDATION_GEMS.iter().all(|g| l.count_of(*g) >= 1),
            safe_on: l.social.faction.safe,
            status: faction_status(l.social.faction.status),
        };
        guild::found_refusal(&founder, a)
    }

    /// The requirements to found a guild with what the character has: (text, met).
    pub fn found_requirements(&self) -> Vec<(String, bool)> {
        let l = &self.local;
        let leadership = l.skills.get(guild::SKILL_LEADERSHIP).copied().unwrap_or(0);
        let gems = guild::FOUNDATION_GEMS.iter().filter(|g| l.count_of(**g) >= 1).count();
        vec![
            (format!("Nivel {} o más (tienes {})", guild::FOUND_MIN_LEVEL, self.stats.level), self.stats.level >= guild::FOUND_MIN_LEVEL),
            (format!("{} puntos de liderazgo (tienes {leadership})", guild::FOUND_MIN_LEADERSHIP), leadership >= guild::FOUND_MIN_LEADERSHIP),
            (format!("Gemas de Fundación Verde, Roja, Azul y Polar ({gems}/4)"), gems == guild::FOUNDATION_GEMS.len()),
            ("No pertenecer a otro clan".to_string(), l.social.guild.info.is_none()),
        ]
    }

    pub fn guild_found(&mut self, name: &str, description: &str, alignment: u8) -> bool {
        if !guild::valid_name(name) {
            return self.refuse("El nombre del clan solo admite letras y espacios sueltos (hasta 30 letras).");
        }
        if description.trim().is_empty() {
            return self.refuse("Escribe una descripción para el clan.");
        }
        if let Some(why) = self.found_refusal(alignment) {
            return self.refuse(why);
        }
        let description = description.trim().chars().take(guild::MAX_DESC_LEN).collect();
        self.send(&C2S::Guild(GuildC2S::Found { name: name.to_string(), description, alignment }));
        true
    }

    pub fn guild_accept(&mut self, name: &str) {
        self.send(&C2S::Guild(GuildC2S::Accept { name: name.to_string() }));
    }

    pub fn guild_reject(&mut self, name: &str, reason: &str) -> bool {
        if reason.trim().is_empty() {
            return self.refuse("Escribe el motivo del rechazo.");
        }
        self.send(&C2S::Guild(GuildC2S::Reject { name: name.to_string(), reason: reason.trim().to_string() }));
        true
    }

    pub fn guild_kick(&mut self, name: &str) {
        self.send(&C2S::Guild(GuildC2S::Kick { name: name.to_string() }));
    }

    pub fn guild_leave(&mut self) {
        self.send(&C2S::Guild(GuildC2S::Leave));
    }

    pub fn guild_update_news(&mut self, news: &str) {
        let news = news.trim().chars().take(guild::MAX_NEWS_LEN).collect();
        self.send(&C2S::Guild(GuildC2S::UpdateNews { news }));
    }

    pub fn guild_update_description(&mut self, description: &str) -> bool {
        if description.trim().is_empty() {
            return self.refuse("La descripción no puede quedar vacía.");
        }
        let description = description.trim().chars().take(guild::MAX_DESC_LEN).collect();
        self.send(&C2S::Guild(GuildC2S::UpdateDescription { description }));
        true
    }

    pub fn guild_propose(&mut self, guild: &str, relation: GuildRelation, text: &str) -> bool {
        if guild.is_empty() {
            return self.refuse("Elige un clan.");
        }
        self.send(&C2S::Guild(GuildC2S::Propose { guild: guild.to_string(), relation, text: text.trim().to_string() }));
        true
    }

    pub fn guild_answer_proposal(&mut self, guild: &str, accept: bool) {
        let g = guild.to_string();
        self.send(&C2S::Guild(if accept { GuildC2S::AcceptProposal { guild: g } } else { GuildC2S::RejectProposal { guild: g } }));
    }

    pub fn guild_declare_war(&mut self, guild: &str) {
        self.send(&C2S::Guild(GuildC2S::DeclareWar { guild: guild.to_string() }));
    }

    // ---- party

    pub fn is_party_leader(&self) -> bool {
        let p = &self.local.social.party;
        p.in_party() && p.leader == self.my_id
    }

    /// Invites a player by entity id (the server checks range, seguro and the rest).
    pub fn party_invite(&mut self, target: u32) -> bool {
        if target == self.my_id {
            return self.refuse("No puedes invitarte a ti mismo.");
        }
        if self.local.social.party.has(target) {
            return self.refuse("Ya está en tu grupo.");
        }
        self.send(&C2S::Party(PartyC2S::Invite { target }));
        true
    }

    /// The player standing on tile (`x`, `y`), if any (for "invite whoever is clicked").
    pub fn player_at(&self, x: i32, y: i32) -> Option<u32> {
        self.ents.iter().find(|(id, e)| **id != self.my_id && e.kind == EntityKind::Player && e.x == x && e.y == y).map(|(id, _)| *id)
    }

    pub fn party_respond(&mut self, accept: bool) {
        self.send(&C2S::Party(PartyC2S::Respond { accept }));
        // The server answers with the party (or nothing when refusing): drop the notice now.
        self.local.social.party.invitation = None;
    }

    pub fn party_leave(&mut self) {
        self.send(&C2S::Party(PartyC2S::Leave));
    }

    pub fn party_kick(&mut self, member: u32) {
        self.send(&C2S::Party(PartyC2S::Kick { member }));
    }

    pub fn party_toggle_safe(&mut self) {
        self.send(&C2S::Party(PartyC2S::ToggleSafe));
    }

    // ---- quests

    pub fn quest_list(&mut self) {
        self.send(&C2S::Quest(QuestC2S::List));
    }

    pub fn quest_details(&mut self, slot: u8) {
        self.send(&C2S::Quest(QuestC2S::Details { slot }));
    }

    pub fn quest_accept(&mut self, quest: u16) {
        self.send(&C2S::Quest(QuestC2S::Accept { quest }));
    }

    pub fn quest_abandon(&mut self, slot: u8) {
        self.send(&C2S::Quest(QuestC2S::Abandon { slot }));
    }

    /// Slots (1-based) of quests in progress whose details have not arrived.
    pub fn quest_missing_details(&self) -> Vec<(u8, u16)> {
        let q = &self.local.social.quest;
        q.active.iter().enumerate().filter(|(i, id)| q.quest_in(*i as u8 + 1).is_none_or(|d| d.info.id != **id)).map(|(i, id)| (i as u8 + 1, *id)).collect()
    }
}

// ---------------------------------------------------------------- demo (screenshots)

/// Server messages that put the social screens in a known state, for screenshots
/// (`AO_SOCIAL_DEMO=console,guild,party,partyfull,quests,offer`, comma separated). They go
/// through `Game::handle`, exactly as if the server had sent them.
pub fn demo_script(what: &str, me: &str, my_id: u32) -> Vec<ao_protocol::S2C> {
    use ao_protocol::social::guild::{GuildApplicant, GuildInfo, GuildMember};
    use ao_protocol::social::party::PartyMember;
    use ao_protocol::social::quest::{OfferedQuest, QuestInfo};
    use ao_protocol::{ChannelS2C, GuildS2C, PartyS2C, QuestS2C, S2C};
    let chan = |channel, author: &str, target: &str, text: &str| S2C::Channel(ChannelS2C::Message { channel, author: author.into(), target: target.into(), text: text.into() });
    let say = |text: &str, color| S2C::Console { text: text.into(), color };
    let quest = QuestInfo {
        id: 407,
        name: "Una nueva aventura".into(),
        desc: "Los lobos de las afueras de Ullathorpe se han vuelto demasiado atrevidos. Limpia el camino de lobos y serpientes y trae unas manzanas para el herrero.".into(),
        desc_final: "Buen trabajo, ya puedo volver a salir a pasear.".into(),
        required_level: 1,
        limit_level: 12,
        classes: vec![],
        kill: vec![(500, 4), (502, 3)],
        objs: vec![(1, 5)],
        reward_gold: 70,
        reward_exp: 300,
        reward_objs: vec![(38, 5), (1, 10)],
        talk_to: 407,
        pos_map: 1,
        ..QuestInfo::default()
    };
    let mut out = Vec::new();
    for part in what.split(',').map(str::trim) {
        match part {
            "console" => {
                out.push(say("Bienvenido a Argentum Online.", Tone::Info));
                out.push(say("Ana: ¿alguien para el dungeon?", Tone::Chat));
                out.push(chan(ChannelKind::Global, "Ragnar", "", "Vendo espada +2, precio a charlar"));
                out.push(chan(ChannelKind::Party, "Ana", "", "esperadme en la puerta"));
                out.push(chan(ChannelKind::Guild, "Beto", "", "hoy hay reunión de clan a las 21"));
                out.push(say("Le has quitado 14 puntos de vida al lobo.", Tone::Combat));
                out.push(chan(ChannelKind::Whisper, "Bea", me, "¿me vendes el arco?"));
                out.push(chan(ChannelKind::Whisper, me, "Bea", "claro, ahora te lo paso"));
                out.push(chan(ChannelKind::Yell, "Cris", "", "¡Cuidado, un orco!"));
                out.push(say("Has ganado 120 puntos de experiencia.", Tone::Good));
                out.push(chan(ChannelKind::Global, "Dora", "", "Busco grupo para el castillo, nivel 25+"));
            }
            "noguild" => out.push(S2C::Guild(GuildS2C::List(vec![("Hermandad del Alba".into(), 3), ("Legión Carmesí".into(), 2), ("Guardia del Rey".into(), 1), ("Los Errantes".into(), 0)]))),
            "guild" => {
                let member = |name: &str, online, founder, leader| GuildMember { name: name.into(), online, founder, leader };
                out.push(S2C::Guild(GuildS2C::List(vec![("Hermandad del Alba".into(), 3), ("Legión Carmesí".into(), 2), ("Guardia del Rey".into(), 1), ("Los Errantes".into(), 0)])));
                out.push(S2C::Guild(GuildS2C::Info(Box::new(GuildInfo {
                    name: "Hermandad del Alba".into(),
                    alignment: 3,
                    level: 3,
                    exp: 5200,
                    exp_next: 9000,
                    max_members: 25,
                    founder: "Beto".into(),
                    leader: me.into(),
                    description: "Un clan de ciudadanos que defiende las rutas de Ullathorpe y comparte botín a partes iguales. Respeto ante todo.".into(),
                    news: "Reunión de clan el sábado a las 21 hs en la plaza. Se busca clérigo para el dungeon.".into(),
                    members: vec![member(me, true, false, true), member("Beto", true, true, false), member("Ana", true, false, false), member("Cris", false, false, false), member("Dora", false, false, false), member("Eloy", true, false, false)],
                    applicants: vec![GuildApplicant { name: "Fede".into(), application: "Soy clérigo nivel 24 y busco un clan serio.".into() }, GuildApplicant { name: "Gaby".into(), application: "Amiga de Ana, me invitó ella.".into() }],
                    relations: vec![("Legión Carmesí".into(), ao_protocol::social::guild::GuildRelation::War), ("Guardia del Rey".into(), ao_protocol::social::guild::GuildRelation::Alliance)],
                    proposals: vec![("Los Errantes".into(), ao_protocol::social::guild::GuildRelation::Peace, "Cansados de pelear, ¿paz?".into())],
                }))));
            }
            "party" => out.push(S2C::Party(PartyS2C::Invited { from: my_id + 900, name: "Ana".into() })),
            "partyfull" => out.push(S2C::Party(PartyS2C::Members {
                leader: my_id,
                members: vec![PartyMember { id: my_id, name: me.into() }, PartyMember { id: my_id + 900, name: "Ana".into() }, PartyMember { id: my_id + 901, name: "Beto".into() }],
            })),
            "quests" => {
                out.push(S2C::Quest(QuestS2C::List { active: vec![407, 410] }));
                out.push(S2C::Quest(QuestS2C::Details { slot: 1, info: Box::new(quest.clone()), killed: vec![3, 1], have: vec![2], known: vec![], targeted: vec![] }));
                let second = QuestInfo { id: 410, name: "El herrero necesita ayuda".into(), desc: "Trae lingotes al herrero.".into(), kill: vec![], objs: vec![(1, 3)], reward_exp: 120, reward_gold: 30, ..QuestInfo::default() };
                out.push(S2C::Quest(QuestS2C::Details { slot: 2, info: Box::new(second), killed: vec![], have: vec![3], known: vec![], targeted: vec![] }));
            }
            "offer" => {
                let locked = QuestInfo { id: 408, name: "Cazador de serpientes".into(), desc: "Las serpientes del pantano son un peligro.".into(), required_level: 3, required_quest: 407, kill: vec![(502, 10)], reward_exp: 600, reward_gold: 150, ..QuestInfo::default() };
                out.push(S2C::Quest(QuestS2C::Offer { npc: 100_007, quests: vec![OfferedQuest { info: quest.clone(), state: 0 }, OfferedQuest { info: locked, state: 3 }] }));
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::channel::ChannelLine;
    use ao_protocol::social::quest::QuestInfo;
    use ao_protocol::{ChannelS2C, QuestS2C};

    fn p(s: &str) -> Chat {
        parse_chat(s, Some("Ana")).unwrap()
    }

    #[test]
    fn plain_text_is_said_and_empty_is_nothing() {
        assert_eq!(p("hola a todos"), Chat::Say("hola a todos".into()));
        assert_eq!(parse_chat("   ", None), None);
        // Development commands and unknown commands go to the server as chat.
        assert_eq!(p("/dar 500 2"), Chat::Say("/dar 500 2".into()));
        assert_eq!(p("/loquesea"), Chat::Say("/loquesea".into()));
    }

    #[test]
    fn whispers_with_and_without_quotes_and_replies() {
        assert_eq!(p("/s Beto nos vemos"), Chat::Whisper { to: "Beto".into(), text: "nos vemos".into() });
        assert_eq!(p("/W Beto  hola  "), Chat::Whisper { to: "Beto".into(), text: "hola".into() });
        assert_eq!(p("/s \"Beto Gómez\" hola"), Chat::Whisper { to: "Beto Gómez".into(), text: "hola".into() });
        assert!(matches!(p("/s Beto"), Chat::Usage(_)));
        assert!(matches!(p("/s"), Chat::Usage(_)));
        assert_eq!(p("/r gracias"), Chat::Whisper { to: "Ana".into(), text: "gracias".into() });
        assert!(matches!(parse_chat("/r gracias", None).unwrap(), Chat::Usage(_)));
    }

    #[test]
    fn channel_commands() {
        assert_eq!(p("/g vamos"), Chat::Party("vamos".into()));
        assert_eq!(p("/grupo vamos"), Chat::Party("vamos".into()));
        assert_eq!(p("/c reunión"), Chat::Guild("reunión".into()));
        assert_eq!(p("/GL se vende"), Chat::Global("se vende".into()));
        assert_eq!(p("/y ayuda"), Chat::Yell("ayuda".into()));
        assert!(matches!(p("/g"), Chat::Usage(_)));
        assert!(matches!(p("/gl   "), Chat::Usage(_)));
        assert_eq!(p("/g vamos").label(), "Grupo");
        assert_eq!(p("/s Beto x").label(), "Susurro a Beto");
        assert_eq!(p("hola").label(), "Mapa");
    }

    #[test]
    fn history_walks_back_and_forth_and_keeps_the_draft() {
        let mut h = InputHistory::default();
        assert_eq!(h.up("x"), None);
        h.push("uno");
        h.push("dos");
        h.push("dos");
        assert_eq!(h.up("borrador").as_deref(), Some("dos"));
        assert_eq!(h.up("").as_deref(), Some("uno"));
        assert_eq!(h.up("").as_deref(), Some("uno"));
        assert_eq!(h.down().as_deref(), Some("dos"));
        assert_eq!(h.down().as_deref(), Some("borrador"));
        assert_eq!(h.down(), None);
    }

    #[test]
    fn fresh_detects_appends_and_shifts() {
        assert_eq!(fresh::<u8>(&[], &[1, 2]), 2);
        assert_eq!(fresh(&[1, 2], &[1, 2, 3]), 1);
        assert_eq!(fresh(&[1, 2, 3], &[2, 3, 4, 5]), 2);
        assert_eq!(fresh(&[1, 2, 3], &[1, 2, 3]), 0);
        assert_eq!(fresh(&[1, 2, 3], &[7, 8]), 2);
    }

    fn game() -> Game {
        Game::new("Yo".into(), "127.0.0.1:1".into(), std::path::PathBuf::new())
    }

    #[test]
    fn feed_merges_console_and_channels_in_arrival_order() {
        let mut g = game();
        let mut feed = Feed::default();
        g.say("Ana: hola mapa", Tone::Chat);
        g.say("Te has curado.", Tone::Good);
        feed.update(&g);
        g.local.social.channel.apply(&ChannelS2C::Message { channel: ChannelKind::Guild, author: "Beto".into(), target: String::new(), text: "reunión".into() });
        feed.update(&g);
        g.say("Otro mensaje", Tone::Info);
        g.local.social.channel.apply(&ChannelS2C::Message { channel: ChannelKind::Whisper, author: "Yo".into(), target: "Bea".into(), text: "hey".into() });
        feed.update(&g);
        feed.update(&g);
        let all: Vec<_> = feed.tab(ConsoleTab::All).map(|l| l.text.as_str()).collect();
        assert_eq!(all, ["Ana: hola mapa", "Te has curado.", "[Clan] Beto: reunión", "Otro mensaje", "[Susurro a Bea] hey"]);
        assert_eq!(feed.tab(ConsoleTab::Map).count(), 1);
        assert_eq!(feed.tab(ConsoleTab::System).count(), 2);
        assert_eq!(feed.tab(ConsoleTab::Guild).count(), 1);
        assert_eq!(feed.total[ConsoleTab::All.index()], 5);
    }

    #[test]
    fn feed_follows_a_full_console_that_drops_old_lines() {
        let mut g = game();
        let mut feed = Feed::default();
        for i in 0..70 {
            g.say(format!("l{i}"), Tone::Info);
            feed.update(&g);
        }
        assert_eq!(feed.lines.len(), 70);
        assert_eq!(feed.lines.back().unwrap().text, "l69");
        // Several lines in one frame while full.
        for i in 70..74 {
            g.say(format!("l{i}"), Tone::Info);
        }
        feed.update(&g);
        assert_eq!(feed.lines.len(), 74);
        assert_eq!(feed.lines.back().unwrap().text, "l73");
    }

    #[test]
    fn channel_lines_are_worded_by_channel() {
        let l = |channel, author: &str, target: &str| ChannelLine { channel, author: author.into(), target: target.into(), text: "x".into() };
        assert_eq!(format_channel_line(&l(ChannelKind::Whisper, "Ana", "Yo"), "Yo").text, "[Susurro] Ana: x");
        assert_eq!(format_channel_line(&l(ChannelKind::Whisper, "Yo", "Ana"), "Yo").text, "[Susurro a Ana] x");
        assert_eq!(format_channel_line(&l(ChannelKind::Global, "Ana", ""), "Yo").kind, LineKind::Global);
        assert!(ConsoleTab::Map.accepts(LineKind::Yell));
    }

    #[test]
    fn founding_requirements_follow_the_character() {
        let mut g = game();
        g.stats.level = 23;
        g.local.skills[guild::SKILL_LEADERSHIP] = 50;
        assert!(g.found_refusal(0).is_some(), "no gems yet");
        let req = g.found_requirements();
        assert_eq!(req.iter().map(|r| r.1).collect::<Vec<_>>(), [true, true, false, true]);
        for (i, gem) in guild::FOUNDATION_GEMS.iter().enumerate() {
            g.local.inventory[i] = Some(crate::ItemSlot { obj: *gem, amount: 1, equipped: false });
        }
        assert_eq!(g.found_refusal(0), None);
        assert!(g.found_refusal(1).is_some(), "an Armada guild needs the Armada");
        assert!(g.found_refusal(9).is_some());
        assert!(!g.guild_found("Mal--nombre", "x", 0));
    }

    #[test]
    fn quests_missing_details_are_listed_once_known() {
        let mut g = game();
        g.local.social.quest.apply(&QuestS2C::List { active: vec![7, 9] });
        assert_eq!(g.quest_missing_details(), vec![(1, 7), (2, 9)]);
        let info = QuestInfo { id: 7, ..QuestInfo::default() };
        g.local.social.quest.apply(&QuestS2C::Details { slot: 1, info: Box::new(info), killed: vec![], have: vec![], known: vec![], targeted: vec![] });
        assert_eq!(g.quest_missing_details(), vec![(2, 9)]);
    }

    #[test]
    fn the_demo_script_leaves_the_screens_populated() {
        let mut g = game();
        g.my_id = 5;
        for m in demo_script("console,guild,party,quests,offer", "Yo", 5) {
            g.handle(m);
        }
        let s = &g.local.social;
        assert!(g.is_guild_leader() && s.guild.info.is_some() && s.party.invitation.is_some());
        assert_eq!((s.quest.active.len(), s.quest.offer.as_ref().map(|o| o.1.len())), (2, Some(2)));
        assert!(!s.channel.history.is_empty() && g.console.len() >= 3);
    }
}

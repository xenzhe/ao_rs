//! Client actions and state for account flow: log in, new account, character list, create/delete/play (`impl Game` blocks, graphics-free and
//! testable with bots). OWNER: Sonnet 1 (docs/P3_CONTRATO.md).
//!
//! `Game` (state.rs) has no account fields and `handle` only logs `AccountResult` to the
//! console and stores `CharacterList`. So the flow works like this:
//! - the `Game::account_*` / `request_*` methods validate locally (nothing malformed ever
//!   reaches the server, which would count failed logins or kick), empty the console and send;
//! - [`Game::account_response`] reads the answer back from the console (the only thing the
//!   server writes there before entering the world);
//! - [`AccountFlow`] (owned by the UI or a test) watches that every frame and moves
//!   `game.screen` between Login, Characters, Create and Game.
use crate::state::{Game, Screen};
use ao_formats::dat::balance::{Balance, Class, Race};
use ao_protocol::{Tone, C2S, PROTOCOL_VERSION};
use ao_rules::character::{
    initial_attributes, initial_hp, initial_mana, valid_account_name, valid_class, valid_gender, valid_password, valid_race, FEMALE, MAX_CHARACTERS,
};

/// Highest head id the server accepts at creation (`systems/account.rs` `MAX_HEAD`).
pub const MAX_HEAD: u16 = 757;

/// How long a request may wait for its answer (game seconds) before the UI gives up.
pub const REQUEST_TIMEOUT: f64 = 12.0;

// ------------------------------------------------------------------ creation data

/// Valid head ranges by race and gender of the resources in use (AO20 `ValidarCabeza`, or
/// `init/head_ranges.ini`). The server checks the same table.
pub fn head_ranges(race: u8, gender: u8) -> &'static [(u16, u16)] {
    ao_formats::heads::for_recursos(&ao_formats::recursos_dir()).ranges(race, gender)
}

/// Heads a player can pick for `race` + `gender`: the official ranges, capped at what this
/// server accepts ([`MAX_HEAD`]) and filtered by `has_graphics` (the head exists in cabezas.ini).
pub fn valid_heads(race: u8, gender: u8, has_graphics: impl Fn(u16) -> bool) -> Vec<u16> {
    head_ranges(race, gender).iter().flat_map(|&(a, b)| a..=b).filter(|h| *h <= MAX_HEAD && has_graphics(*h)).collect()
}

/// Display name of a race, with its article-free Spanish form.
pub fn race_name(race: u8) -> &'static str {
    ao_rules::character::RACES.iter().find(|(id, _)| *id == race).map(|(_, n)| *n).unwrap_or("?")
}

pub fn class_name(class: u8) -> &'static str {
    ao_rules::character::CLASSES.iter().find(|(id, _)| *id == class).map(|(_, n)| *n).unwrap_or("?")
}

pub fn gender_name(gender: u8) -> &'static str {
    if gender == FEMALE {
        "Mujer"
    } else {
        "Varón"
    }
}

/// Short flavour text of a race. AO20 ships no descriptions (only the attribute modifiers
/// of `Balance.dat`, shown separately), so these are written to match those numbers.
pub fn race_text(race: u8) -> &'static str {
    match race {
        1 => "Los más numerosos de Argentum. Sin grandes virtudes ni defectos, su equilibrio y su capacidad de adaptación les permiten abrazar cualquier camino.",
        2 => "Pueblo antiguo de los bosques. Ágiles, de mente despierta y gran presencia, pagan esa gracia con un cuerpo más frágil que el de otras razas.",
        3 => "Elfos oscuros criados lejos del sol. Fuertes y de gran intelecto, su fama de fríos y traicioneros les cierra más de una puerta.",
        4 => "Pequeños, astutos y brillantes. Nadie los iguala en ingenio ni en agilidad, aunque poco pueden hacer con la fuerza bruta.",
        5 => "Mineros y herreros de la montaña. Robustos y tercos, resisten lo que otros no soportan; la magia, en cambio, no es lo suyo.",
        6 => "Guerreros de piel curtida y mucho aguante. Su fuerza es temida en cualquier campo de batalla; su palabra, menos escuchada.",
        _ => "",
    }
}

/// Short flavour text of a class (same note as [`race_text`]).
pub fn class_text(class: u8) -> &'static str {
    match class {
        1 => "Maestro de las artes arcanas. Su mana es el mayor de todos y sus hechizos arrasan, pero un par de golpes bastan para derribarlo.",
        2 => "Devoto sanador. Cura, resucita y protege a los suyos con magia divina, y sabe defenderse con la maza cuando hace falta.",
        3 => "Combatiente puro. La mejor vida y el mejor manejo de armas y escudos; sin magia, resuelve todo con acero.",
        4 => "Sombra del campo de batalla. Se oculta, golpea por la espalda con daga y apoya su sigilo con unos pocos hechizos.",
        5 => "Trovador de dos filos. Sus laúdes aumentan el mana, su daga hiere en el momento justo y su palabra abre todas las puertas.",
        6 => "Guardián de la naturaleza. Doma criaturas, invoca ayuda del bosque y lanza hechizos de tierra y viento.",
        7 => "Caballero sagrado. Combina armadura pesada y espada con plegarias que lo curan y lo protegen del mal.",
        8 => "Rastreador y arquero. Dispara con precisión desde lejos, se esconde entre los árboles y sobrevive en cualquier terreno.",
        9 => "Hombre de oficio. Talar, minar, pescar y fabricar son su camino; pelea poco, pero sin él nada se construye.",
        10 => "Lobo de mar. Navega con soltura, pelea con espada y sabe cuándo es mejor huir.",
        11 => "Mano ligera. Roba, se oculta y esquiva mejor que casi nadie; prefiere evitar el combate abierto.",
        12 => "Forajido de armas tomar. Mezcla robo, esquiva y un golpe crítico temible; vive al margen de la ley.",
        _ => "",
    }
}

/// One-line role of a class, for the compact class buttons' tooltip.
pub fn class_role(class: u8) -> &'static str {
    match class {
        1 => "Magia ofensiva",
        2 => "Curación y apoyo",
        3 => "Cuerpo a cuerpo",
        4 => "Sigilo y puñaladas",
        5 => "Apoyo y daga",
        6 => "Naturaleza y domar",
        7 => "Guerrero sagrado",
        8 => "Arco y rastreo",
        9 => "Oficios",
        10 => "Combate y mar",
        11 => "Robo y esquiva",
        12 => "Golpes críticos",
        _ => "",
    }
}

/// Attribute names in `e_Atributos` order (Fuerza, Agilidad, Inteligencia, Constitución, Carisma).
pub const ATTRIBUTE_NAMES: [&str; 5] = ["Fuerza", "Agilidad", "Inteligencia", "Constitución", "Carisma"];

/// What a new character of a race and class will start with.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub attributes: [u8; 5],
    /// Attribute modifier of the race (starting value is 18 + this).
    pub race_mods: [i32; 5],
    pub hp: i32,
    pub mana: i32,
    pub class: ClassGauges,
}

/// Class modifiers from `Balance.dat`, for the comparison bars (higher is better).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ClassGauges {
    pub evasion: f32,
    pub melee_attack: f32,
    pub ranged_attack: f32,
    pub melee_damage: f32,
    pub life_per_level: f32,
    pub magic_resistance: f32,
}

/// Highest value of each class gauge among all classes (bars are drawn relative to it).
pub fn class_gauge_max(bal: &Balance) -> ClassGauges {
    let mut m = ClassGauges::default();
    for c in Class::ALL {
        let g = gauges(bal, c);
        m.evasion = m.evasion.max(g.evasion);
        m.melee_attack = m.melee_attack.max(g.melee_attack);
        m.ranged_attack = m.ranged_attack.max(g.ranged_attack);
        m.melee_damage = m.melee_damage.max(g.melee_damage);
        m.life_per_level = m.life_per_level.max(g.life_per_level);
        m.magic_resistance = m.magic_resistance.max(g.magic_resistance);
    }
    m
}

fn gauges(bal: &Balance, class: Class) -> ClassGauges {
    let c = bal.class(class);
    ClassGauges {
        evasion: c.evasion as f32,
        melee_attack: c.melee_attack as f32,
        ranged_attack: c.ranged_attack as f32,
        melee_damage: c.melee_damage as f32,
        life_per_level: c.life as f32,
        magic_resistance: c.magic_resistance as f32,
    }
}

/// Starting attributes, hit points and mana of (`race`, `class`), exactly as the server
/// computes them (`ao_rules::character`). `None` for ids outside the official ranges.
pub fn creation_preview(bal: &Balance, race: u8, class: u8) -> Option<Preview> {
    let (r, c) = (Race::from_id(race as i32)?, Class::from_id(class as i32)?);
    let attributes = initial_attributes(bal, r);
    let m = bal.race(r);
    let prof = ao_formats::profile::for_recursos(&ao_formats::recursos_dir());
    Some(Preview {
        attributes,
        race_mods: [m.strength, m.agility, m.intelligence, m.constitution, m.charisma],
        hp: prof.start_hp.unwrap_or_else(|| initial_hp(&attributes)),
        mana: if prof.start_hp.is_some() { prof.start_mana_for(class, attributes[2]) } else { initial_mana(bal, c, &attributes) },
        class: gauges(bal, c),
    })
}

/// Why a character name cannot be used, in Spanish for the creation form (`Ok` = valid).
/// Same rule as the server (`ao_rules::valid_name`).
pub fn check_character_name(name: &str) -> Result<(), &'static str> {
    let n = name.chars().count();
    if n == 0 {
        Err("Escribe un nombre.")
    } else if n < 3 {
        Err("Mínimo 3 caracteres.")
    } else if n > 16 {
        Err("Máximo 16 caracteres.")
    } else if !ao_rules::valid_name(name) {
        Err("Solo letras, números o _.")
    } else {
        Ok(())
    }
}

/// Why an account name / password cannot be used (`Ok` = valid).
pub fn check_account_name(user: &str) -> Result<(), &'static str> {
    if user.is_empty() {
        Err("Escribe tu usuario.")
    } else if !valid_account_name(user) {
        Err("Usuario: 3 a 20 letras, números o _.")
    } else {
        Ok(())
    }
}

pub fn check_password(pass: &str) -> Result<(), &'static str> {
    if pass.is_empty() {
        Err("Escribe tu contraseña.")
    } else if !valid_password(pass) {
        Err("Contraseña: 6 a 64 caracteres.")
    } else {
        Ok(())
    }
}

// ------------------------------------------------------------------ Game actions

impl Game {
    /// Connects (or reuses the live connection) WITHOUT any login.
    pub fn account_connect(&mut self) -> Result<(), String> {
        if self.net.as_ref().is_some_and(|n| n.alive()) {
            return Ok(());
        }
        self.net = None;
        self.open().map_err(|e| format!("No se pudo conectar a {}: {e}", self.server_addr))
    }

    /// Clears the console so the next entry is the answer to the request about to be sent.
    fn begin_request(&mut self) {
        self.console.clear();
    }

    /// Logs in to an account. `Err` = refused locally (nothing was sent).
    pub fn account_login(&mut self, user: &str, pass: &str) -> Result<(), String> {
        check_account_name(user).map_err(String::from)?;
        check_password(pass).map_err(String::from)?;
        self.account_connect()?;
        self.begin_request();
        self.send(&C2S::AccountLogin { user: user.into(), password: pass.into(), version: PROTOCOL_VERSION });
        Ok(())
    }

    /// Creates an account (the server logs it in and sends its empty character list).
    pub fn account_register(&mut self, user: &str, pass: &str) -> Result<(), String> {
        check_account_name(user).map_err(String::from)?;
        check_password(pass).map_err(String::from)?;
        self.account_connect()?;
        self.begin_request();
        self.send(&C2S::CreateAccount { user: user.into(), password: pass.into(), version: PROTOCOL_VERSION });
        Ok(())
    }

    /// Asks for a new character. The creation screen only offers valid values, but this
    /// refuses anything the server would kick for (a kick would close the connection).
    pub fn request_create_character(&mut self, name: &str, race: u8, class: u8, gender: u8, head: u16) -> Result<(), String> {
        check_character_name(name).map_err(String::from)?;
        if !(valid_race(race) && valid_class(class) && valid_gender(gender)) || !(1..=MAX_HEAD).contains(&head) {
            return Err("Datos de personaje no válidos.".into());
        }
        if !head_ranges(race, gender).iter().any(|&(a, b)| (a..=b).contains(&head)) {
            return Err("Esa cabeza no es de esa raza y género.".into());
        }
        if self.characters.len() >= MAX_CHARACTERS {
            return Err(format!("Ya tienes el máximo de personajes ({MAX_CHARACTERS})."));
        }
        self.begin_request();
        self.send(&C2S::CreateCharacter { name: name.into(), race, class, gender, head });
        Ok(())
    }

    pub fn request_delete_character(&mut self, name: &str) {
        self.begin_request();
        self.send(&C2S::DeleteCharacter { name: name.into() });
    }

    /// Enters the world with a character of the account. The screen changes to
    /// `Screen::Game` when [`AccountFlow::update`] sees the player standing on its map.
    pub fn request_play_character(&mut self, name: &str) {
        self.begin_request();
        self.name_input = name.to_string();
        self.send(&C2S::PlayCharacter { name: name.into() });
    }

    /// Leaves the account: drops the connection and goes back to the login screen.
    pub fn account_logout(&mut self) {
        self.net = None;
        self.characters.clear();
        self.my_id = 0;
        self.map = None;
        self.ents.clear();
        self.console.clear();
        self.screen = Screen::Login;
    }

    /// The server's answer to the last request (`AccountResult`): `(ok, message)`.
    /// `None` while it has not arrived.
    pub fn account_response(&self) -> Option<(bool, String)> {
        self.console.back().map(|(m, tone)| (*tone == Tone::Good, m.clone()))
    }

    /// Standing on a map with its own entity known (the play request succeeded).
    pub fn entered_world(&self) -> bool {
        self.map.is_some() && self.my_id != 0 && self.me().is_some()
    }
}

// ------------------------------------------------------------------ screen flow

#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    Login,
    Register,
    Create,
    Delete,
    Play,
}

impl Pending {
    pub fn label(&self) -> &'static str {
        match self {
            Pending::Login => "Entrando a tu cuenta...",
            Pending::Register => "Creando la cuenta...",
            Pending::Create => "Forjando a tu personaje...",
            Pending::Delete => "Borrando personaje...",
            Pending::Play => "Entrando al mundo...",
        }
    }
}

/// Message for the player: text and whether it is good news.
pub type Notice = (String, bool);

/// Watches the server's answers and changes screens. One per UI (or per test bot).
#[derive(Default)]
pub struct AccountFlow {
    pub pending: Option<Pending>,
    /// Last result to show (error in red, success in green) until the next request.
    pub notice: Option<Notice>,
    started: f64,
}

impl AccountFlow {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// Marks a request as sent (call after a successful `Game::account_*` / `request_*`).
    pub fn begin(&mut self, game: &Game, what: Pending) {
        self.pending = Some(what);
        self.notice = None;
        self.started = game.now;
    }

    /// Runs the request and records it, or records the local refusal as a notice.
    pub fn run(&mut self, game: &mut Game, what: Pending, request: Result<(), String>) {
        match request {
            Ok(()) => self.begin(game, what),
            Err(e) => self.notice = Some((e, false)),
        }
    }

    pub fn login(&mut self, game: &mut Game, user: &str, pass: &str) {
        let r = game.account_login(user, pass);
        self.run(game, Pending::Login, r);
    }

    pub fn register(&mut self, game: &mut Game, user: &str, pass: &str) {
        let r = game.account_register(user, pass);
        self.run(game, Pending::Register, r);
    }

    pub fn create_character(&mut self, game: &mut Game, name: &str, race: u8, class: u8, gender: u8, head: u16) {
        let r = game.request_create_character(name, race, class, gender, head);
        self.run(game, Pending::Create, r);
    }

    pub fn delete_character(&mut self, game: &mut Game, name: &str) {
        game.request_delete_character(name);
        self.begin(game, Pending::Delete);
    }

    pub fn play(&mut self, game: &mut Game, name: &str) {
        game.request_play_character(name);
        self.begin(game, Pending::Play);
    }

    /// Call once per frame (after `Game::poll`). Applies the answer to the pending request.
    pub fn update(&mut self, game: &mut Game) {
        if self.recover_from_kick(game) {
            return;
        }
        let Some(pending) = self.pending.clone() else { return };
        if pending == Pending::Play && game.entered_world() {
            self.pending = None;
            self.notice = None;
            game.screen = Screen::Game;
            return;
        }
        if !matches!(game.screen, Screen::Login | Screen::Characters | Screen::Create) {
            // Kicked (or already in game): the other screens take over.
            self.pending = None;
            return;
        }
        if let Some((ok, msg)) = game.account_response() {
            self.pending = None;
            self.finish(game, &pending, ok, msg);
        } else if game.net.as_ref().is_none_or(|n| !n.alive()) {
            self.pending = None;
            self.connection_lost(game);
        } else if game.now - self.started > REQUEST_TIMEOUT {
            self.pending = None;
            self.notice = Some(("El servidor no responde.".into(), false));
        }
    }

    /// The server kicked us before we ever entered the world (too many wrong passwords,
    /// old client...): `handle` sets `Screen::Kicked`, a dead end. Go back to the login
    /// screen with the reason as a notice. The integrator's frame loop can call this for
    /// `Screen::Kicked` too. Returns true when it changed the screen.
    pub fn recover_from_kick(&mut self, game: &mut Game) -> bool {
        let Screen::Kicked(reason) = &game.screen else { return false };
        if game.entered_world() {
            return false;
        }
        let mut reason = reason.clone();
        if let Some(first) = reason.get(..1) {
            reason = first.to_uppercase() + &reason[1..] + ".";
        }
        game.account_logout();
        self.pending = None;
        self.notice = Some((reason, false));
        true
    }

    fn finish(&mut self, game: &mut Game, what: &Pending, ok: bool, msg: String) {
        game.console.clear();
        self.notice = Some((msg, ok));
        match (what, ok) {
            (Pending::Login, true) => {
                game.screen = Screen::Characters;
                self.notice = None;
            }
            (Pending::Register, true) => {
                // A new account has no characters: straight to creating the first one.
                game.screen = Screen::Create;
                self.notice = None;
            }
            (Pending::Create, true) => game.screen = Screen::Characters,
            // Bad password / name taken / not your character...: stay where we are.
            (Pending::Login | Pending::Register, false) => {
                // The server closes the connection after too many failures or a ban.
                if game.net.as_ref().is_none_or(|n| !n.alive()) {
                    game.net = None;
                }
            }
            _ => {}
        }
    }

    fn connection_lost(&mut self, game: &mut Game) {
        game.net = None;
        if game.screen != Screen::Login {
            game.account_logout();
        }
        self.notice = Some(("Se perdió la conexión con el servidor.".into(), false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ao_rules::character::MALE;

    #[test]
    fn heads_follow_the_official_ranges() {
        let all = |_| true;
        let h = valid_heads(1, MALE, all);
        assert_eq!(h.first(), Some(&1));
        assert!(h.contains(&41) && !h.contains(&42));
        assert!(!h.iter().any(|x| *x > MAX_HEAD), "the server refuses heads above 757");
        assert!(valid_heads(6, FEMALE, all).iter().all(|h| (550..=579).contains(h)));
        assert_eq!(valid_heads(5, MALE, all).len(), 45);
        // Missing graphics are skipped.
        assert_eq!(valid_heads(5, MALE, |h| h % 2 == 0).len(), 23);
        assert!(valid_heads(9, MALE, all).is_empty());
        // Every race/gender has at least one head and all are valid for the server.
        for (race, _) in ao_rules::character::RACES {
            for g in [MALE, FEMALE] {
                assert!(!valid_heads(race, g, all).is_empty(), "race {race} gender {g}");
            }
        }
    }

    #[test]
    fn names_are_checked_like_the_server() {
        assert!(check_character_name("Gandalf").is_ok());
        assert!(check_character_name("ab").is_err());
        assert!(check_character_name("con espacio").is_err());
        assert!(check_character_name("").is_err());
        assert!(check_character_name("DemasiadoLargoNombreYa").is_err());
        assert!(check_account_name("xen_zhe").is_ok() && check_account_name("ab").is_err());
        assert!(check_password("secreta1").is_ok() && check_password("corta").is_err());
    }

    #[test]
    fn preview_matches_the_server_rules() {
        let root = ao_formats::paths::recursos_dir();
        let Ok(bal) = ao_formats::dat::balance::load(&root) else { return };
        let p = creation_preview(&bal.data, 1, 1).unwrap();
        assert_eq!(p.attributes, [19, 19, 18, 20, 18]);
        assert_eq!((p.hp, p.mana), (20, 150));
        assert_eq!(p.race_mods, [1, 1, 0, 2, 0]);
        assert!(creation_preview(&bal.data, 9, 1).is_none());
        let max = class_gauge_max(&bal.data);
        assert!(p.class.melee_damage <= max.melee_damage);
    }

    #[test]
    fn local_validation_sends_nothing() {
        let mut g = Game::new(String::new(), "127.0.0.1:1".into(), std::path::PathBuf::new());
        assert!(g.account_login("ab", "secreta1").is_err());
        assert!(g.account_login("usuario", "corta").is_err());
        assert!(g.request_create_character("Bien", 1, 1, 1, 200).is_err(), "head of another race");
        assert!(g.request_create_character("Bien", 9, 1, 1, 1).is_err());
        assert!(g.net.is_none(), "no connection was even tried");
    }
}

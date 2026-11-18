//! Accounts with several characters, persisted in SQLite.
//!
//! Flow: CreateAccount / AccountLogin -> CharacterList -> CreateCharacter / DeleteCharacter
//! -> PlayCharacter (enters the world) -> on disconnect the character is saved.
use crate::db::{Account, CharacterRecord, DbError, NewCharacter};
use crate::world::entities::Player;
use crate::world::World;
use ao_protocol::{CharacterSummary, C2S, PROTOCOL_VERSION, S2C};
use ao_rules::character::{starting_body, valid_account_name, valid_class, valid_gender, valid_password, valid_race};
use ao_rules::valid_name;
use tracing::{info, warn};

/// Highest head index accepted at creation (cabezas.ini has 757). Per-race head ranges
/// come with the character creation work in P1.
const MAX_HEAD: u16 = 757;

fn summary(c: &CharacterRecord) -> CharacterSummary {
    CharacterSummary { name: c.name.clone(), level: c.level, race: c.race, class: c.class, gender: c.gender, body: c.body, head: c.head, map: c.map }
}

impl World {
    pub(crate) fn on_account(&mut self, id: u32, msg: C2S) {
        if self.player(id).is_some() {
            // Account actions are only valid on the selection screen.
            return self.kick(id, "accion de cuenta durante el juego");
        }
        match msg {
            C2S::CreateAccount { user, password, version } => self.create_account(id, &user, &password, version),
            C2S::AccountLogin { user, password, version } => self.account_login(id, &user, &password, version),
            C2S::CreateCharacter { name, race, class, gender, head } => self.create_character(id, &name, race, class, gender, head),
            C2S::DeleteCharacter { name } => self.delete_character(id, &name),
            C2S::PlayCharacter { name } => self.play_character(id, &name),
            _ => {}
        }
    }

    fn account_of(&self, id: u32) -> Option<Account> {
        self.sessions.get(&id).and_then(|s| s.account.clone())
    }

    fn reply(&self, id: u32, ok: bool, message: impl Into<String>) {
        self.send(id, &S2C::AccountResult { ok, message: message.into() });
    }

    fn send_character_list(&self, id: u32, account_id: i64) {
        match self.db.characters(account_id) {
            Ok(list) => self.send(id, &S2C::CharacterList { characters: list.iter().map(summary).collect() }),
            Err(e) => warn!(?e, "no se pudo listar personajes"),
        }
    }

    fn create_account(&mut self, id: u32, user: &str, password: &str, version: u16) {
        if version != PROTOCOL_VERSION {
            return self.reply(id, false, "Version del cliente desactualizada.");
        }
        if self.account_of(id).is_some() {
            return self.reply(id, false, "Ya hay una cuenta abierta en esta conexion.");
        }
        if !valid_account_name(user) {
            return self.reply(id, false, "Nombre de cuenta invalido (3-20 letras, numeros o _).");
        }
        if !valid_password(password) {
            return self.reply(id, false, "La contrasena debe tener entre 6 y 64 caracteres.");
        }
        match self.db.create_account(user, password) {
            Ok(acc) => {
                info!(id, user = %acc.user, "cuenta creada");
                let acc_id = acc.id;
                self.sessions.get_mut(&id).unwrap().account = Some(acc);
                self.reply(id, true, "Cuenta creada.");
                self.send_character_list(id, acc_id);
            }
            Err(DbError::Taken) => self.reply(id, false, "Ese nombre de cuenta ya existe."),
            Err(e) => {
                warn!(?e, "error creando cuenta");
                self.reply(id, false, "Error del servidor al crear la cuenta.");
            }
        }
    }

    fn account_login(&mut self, id: u32, user: &str, password: &str, version: u16) {
        if version != PROTOCOL_VERSION {
            return self.reply(id, false, "Version del cliente desactualizada.");
        }
        if self.account_of(id).is_some() {
            return self.reply(id, false, "Ya hay una cuenta abierta en esta conexion.");
        }
        let result = if valid_account_name(user) && valid_password(password) { self.db.verify_account(user, password) } else { Err(DbError::WrongPassword) };
        match result {
            Ok(acc) => {
                if self.sessions.values().any(|s| s.account.as_ref().is_some_and(|a| a.id == acc.id)) {
                    return self.reply(id, false, "Esa cuenta ya esta conectada.");
                }
                info!(id, user = %acc.user, "cuenta entra");
                let acc_id = acc.id;
                self.sessions.get_mut(&id).unwrap().account = Some(acc);
                self.reply(id, true, "Bienvenido.");
                self.send_character_list(id, acc_id);
            }
            Err(DbError::Banned) => {
                self.reply(id, false, "Esta cuenta esta suspendida.");
                self.close(id);
            }
            Err(DbError::WrongPassword) => {
                let s = self.sessions.get_mut(&id).unwrap();
                s.failed_logins += 1;
                let failed = s.failed_logins;
                warn!(id, user, failed, "login fallido");
                self.reply(id, false, "Usuario o contrasena incorrectos.");
                if failed >= self.cfg.security.max_failed_logins {
                    self.kick(id, "demasiados intentos de login fallidos");
                }
            }
            Err(e) => {
                warn!(?e, "error de login");
                self.reply(id, false, "Error del servidor.");
            }
        }
    }

    fn create_character(&mut self, id: u32, name: &str, race: u8, class: u8, gender: u8, head: u16) {
        let Some(acc) = self.account_of(id) else {
            return self.kick(id, "crear personaje sin cuenta");
        };
        if !valid_name(name) {
            return self.reply(id, false, "Nombre invalido (3-16 letras, numeros o _).");
        }
        // The head must be one of its race and gender (it used to accept any 1..=757, so a
        // forged packet could pick a head of another race or a GM-only one).
        let head_ok = (1..=MAX_HEAD).contains(&head) && ao_formats::heads::for_recursos(&self.recursos).allows(race, gender, head);
        if !(valid_race(race) && valid_class(class) && valid_gender(gender)) || !head_ok {
            // The client only offers valid options: anything else is a forged packet.
            return self.kick(id, "datos de creacion de personaje invalidos");
        }
        // Official creation (ConnectNewUser): attributes by race, stamina dice roll, start
        // equipment and spells; hit points and mana from constitution/intelligence.
        let Some(race_enum) = ao_formats::dat::balance::Race::from_id(race as i32) else {
            return self.kick(id, "raza invalida");
        };
        let attrs = ao_rules::character::initial_attributes(&self.ao.balance, race_enum);
        let roll = self.rng.range(1, ao_rules::character::stamina_dice(&attrs).max(1));
        let (Some(state), Some(vitals)) = (self.init_new_character_with_roll(race, class, gender, roll), self.new_character_vitals(race, class)) else {
            return self.kick(id, "datos de creacion de personaje invalidos");
        };
        let b = &self.data.balance;
        let new = NewCharacter {
            name,
            race,
            class,
            gender,
            // AO20 starting body, or (profile with naked bodies, e.g. TSAO) the naked body:
            // the starting armour dresses it.
            body: if ao_formats::profile::for_recursos(&self.recursos).naked.is_empty() { starting_body(race, gender) } else { ao_formats::profile::for_recursos(&self.recursos).naked_body(race, gender) },
            head,
            hp: vitals.hp,
            mana: vitals.mana,
            map: b.start_map,
            x: b.start_x,
            y: b.start_y,
        };
        match self.db.create_character(acc.id, &new).and_then(|c| self.db.save_full(&c, &state).map(|_| c)) {
            Ok(c) => {
                info!(id, account = %acc.user, name = %c.name, race, class, "personaje creado");
                self.reply(id, true, format!("{} ha nacido.", c.name));
                self.send_character_list(id, acc.id);
            }
            Err(DbError::Taken) => self.reply(id, false, "Ese nombre ya esta en uso."),
            Err(DbError::TooManyCharacters) => self.reply(id, false, "Ya tienes el maximo de personajes."),
            Err(e) => {
                warn!(?e, "error creando personaje");
                self.reply(id, false, "Error del servidor al crear el personaje.");
            }
        }
    }

    fn delete_character(&mut self, id: u32, name: &str) {
        let Some(acc) = self.account_of(id) else {
            return self.kick(id, "borrar personaje sin cuenta");
        };
        match self.db.delete_character(acc.id, name) {
            Ok(()) => {
                info!(id, account = %acc.user, name, "personaje borrado");
                self.reply(id, true, format!("{name} ha sido borrado."));
                self.send_character_list(id, acc.id);
            }
            Err(DbError::NotFound) => self.reply(id, false, "Ese personaje no es de tu cuenta."),
            Err(e) => {
                warn!(?e, "error borrando personaje");
                self.reply(id, false, "Error del servidor.");
            }
        }
    }

    fn play_character(&mut self, id: u32, name: &str) {
        let Some(acc) = self.account_of(id) else {
            return self.kick(id, "jugar sin cuenta");
        };
        let record = match self.db.load_character(acc.id, name) {
            Ok(r) => r,
            Err(_) => return self.reply(id, false, "Ese personaje no es de tu cuenta."),
        };
        if self.sessions.values().filter_map(|s| s.player.as_ref()).any(|p| p.name.eq_ignore_ascii_case(&record.name)) {
            return self.reply(id, false, "Ese personaje ya esta conectado.");
        }
        // Banned character / IP, or the server shutting down: never enters the world.
        let ip = self.sessions.get(&id).map(|s| s.addr.ip());
        if let Some(why) = self.gm_login_refusal(&record.name, ip) {
            return self.reply(id, false, &why);
        }
        let char_id = record.id;
        let state = match self.db.load_state(char_id) {
            Ok(s) => s,
            Err(e) => {
                warn!(?e, "no se pudo cargar el estado del personaje");
                return self.reply(id, false, "Error del servidor al cargar el personaje.");
            }
        };
        self.spawn_player(id, &record, &state, Some(char_id));
    }

    /// Writes an account character back to the database (logout, and later autosave).
    pub(crate) fn save_player(&self, p: &Player) {
        let Some(char_id) = p.char_id else { return };
        let record = CharacterRecord {
            id: char_id,
            account_id: 0,
            name: p.name.clone(),
            race: p.race,
            class: p.class,
            gender: p.gender,
            body: p.body,
            head: p.head,
            level: p.level,
            exp: p.exp,
            hp: p.hp,
            max_hp: p.max_hp,
            mana: p.mana,
            max_mana: p.max_mana,
            map: p.map,
            x: p.x,
            y: p.y,
        };
        if let Err(e) = self.db.save_full(&record, &super::session::state_of(p)) {
            warn!(?e, name = %p.name, "no se pudo guardar el personaje");
        }
    }
}

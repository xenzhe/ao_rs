//! Account screens as scenes of the world (docs/ESTILO_UI.md §1.8): character selection
//! around a bonfire and character creation as a ritual in front of it. `login.rs` draws the
//! login over the town. The world itself is `scene.rs` (a module of this file).
//!
//! The three screens share one [`AccountUi`] (a thread local: `Game` has no account fields
//! and `main.rs` only gives us `ui`, `game`, `assets` and the renderer).
//!
//! Test and screenshot automation (env vars read here, see [`Auto`]):
//! - `AO_ACCOUNT=user:pass`: logs in by itself (creates the account if it does not exist)
//! - `AO_ACCOUNT_SEED=Name:race:class:gender,...`: creates those characters after logging in
//! - `AO_ACCOUNT_SCREEN=login|characters|create`: screen to capture
//! - `AO_ACCOUNT_SHOT=file.png` (+ `AO_ACCOUNT_SHOT_AT=secs`, default 2.5): capture and exit
//!   (prints the average scene draw time)
//! - `AO_ACCOUNT_SELECT=n`, `AO_ACCOUNT_DELETE=1`: selected slot / open the delete confirmation
//! - `AO_CREATE=race:class:gender[:head]`, `AO_CREATE_NAME=Name`: creation form preset
//! - `AO_LOGIN_FILL=user:pass`, `AO_LOGIN_MODE=login|register|dev`: login form preset
use crate::assets::Assets;
use crate::game::{Game, Screen};
use crate::render::Renderer;
use crate::ui::fx_ui::{ease, key_of, toast, ToastKind};
use crate::ui::theme::{self, FontKind, Surface, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, HP, MANA, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::ui::widgets::{button, button_kind, measure_with, placeholder, text_centered_with, text_field, text_with, tooltip, wrap_lines, Kind};
use crate::ui::Ui;
use ao_formats::dat::balance::Balance;
use ao_protocol::status;
use ao_rules::character::{FEMALE, MALE, MAX_CHARACTERS, RACES, CLASSES, START_SKILL_POINTS};
use ao_rules::Dir;
use client_core::p3::account::{class_name, class_role, class_text, creation_preview, gender_name, race_name, race_text, valid_heads, AccountFlow, Pending, ATTRIBUTE_NAMES};
use client_core::p3::fx::FX_FOREVER;
use macroquad::prelude::*;
use scene::{tile_feet, Act, Look, Scene, Shot, ACTOR_BASE};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

#[path = "scene.rs"]
pub mod scene;

// ------------------------------------------------------------------ the stage (map 1)

/// The camp west of Ullathorpe: bonfire tile.
const FIRE: (i32, i32) = (10, 51);
/// Places around the fire, the best first (the n-th character of the account takes the
/// n-th place): a front row facing the fire and the camera, the sides, a back row.
const RING: [((i32, i32), Dir); MAX_CHARACTERS] = [
    ((10, 49), Dir::Down),
    ((8, 49), Dir::Down),
    ((12, 49), Dir::Down),
    ((7, 51), Dir::Right),
    ((13, 51), Dir::Left),
    ((9, 47), Dir::Down),
    ((11, 47), Dir::Down),
    ((7, 47), Dir::Down),
    ((13, 47), Dir::Down),
    ((15, 49), Dir::Left),
];
/// Place index for the n-th character.
const PROMINENCE: [usize; MAX_CHARACTERS] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
/// Where the new character stands during creation: south of the fire, facing the camera.
const HERO: (i32, i32) = (10, 53);
/// Where the account's characters gather to watch the creation (around the hero).
const WITNESS: [(i32, i32); MAX_CHARACTERS] = [(8, 51), (12, 51), (7, 53), (13, 53), (6, 51), (14, 51), (9, 49), (11, 49), (7, 49), (13, 49)];
/// Camera travelling over the town for the login (tiles).
const TOWN_PATH: [(f32, f32); 6] = [(46.0, 50.0), (53.0, 53.0), (60.0, 59.0), (54.0, 66.0), (42.0, 63.0), (38.0, 54.0)];
/// People strolling around the plaza while the login is open.
const TOWNSFOLK: [((i32, i32), u8, u8, u8); 6] = [((40, 52), 1, 3, MALE), ((46, 53), 2, 1, FEMALE), ((50, 48), 5, 9, MALE), ((36, 48), 3, 4, FEMALE), ((53, 56), 4, 5, MALE), ((32, 53), 1, 8, FEMALE)];

const CHAR_ID: u32 = ACTOR_BASE + 100;
const GHOST_ID: u32 = ACTOR_BASE + 200;
const HERO_ID: u32 = ACTOR_BASE + 300;
const FOLK_ID: u32 = ACTOR_BASE + 400;
const NPC_ID: u32 = ACTOR_BASE + 1000;
/// Meditation (intro + loop) shown around the new character when it is forged.
const RITUAL_FX: u16 = 122;

/// Hours of the scenes.
const HOUR_LOGIN: f32 = 19.9;
const HOUR_CAMP: f32 = 21.2;

// ------------------------------------------------------------------ shared state

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Login,
    Register,
    Dev,
}

/// Creation form.
pub struct Form {
    pub name: String,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub head_i: usize,
    heads: Vec<u16>,
    heads_for: (u8, u8),
    pub facing: Dir,
    pub walk: bool,
    /// Last change (for the transformation flash) and the drag start on the hero.
    changed_at: f64,
    drag_x: Option<f32>,
}

impl Default for Form {
    fn default() -> Form {
        Form { name: String::new(), race: 1, class: 3, gender: MALE, head_i: 0, heads: Vec::new(), heads_for: (0, 0), facing: Dir::Down, walk: false, changed_at: 0.0, drag_x: None }
    }
}

pub struct AccountUi {
    pub flow: AccountFlow,
    // login (used by login.rs)
    pub mode: Mode,
    pub user: String,
    pub pass: String,
    pub pass2: String,
    pub remember: bool,
    pub login_init: bool,
    /// When the mode changed and when the last error arrived (login animations).
    pub mode_at: f64,
    pub notice_at: f64,
    pub scroll_h: f32,
    last_notice: Option<String>,
    // selection
    pub selected: usize,
    select_name: Option<String>,
    confirm_delete: Option<String>,
    confirm_text: String,
    last_click: (usize, f64),
    cast: String,
    map_names: HashMap<u16, String>,
    // creation
    pub form: Form,
    bal: Option<Option<Balance>>,
    kits: Option<HashMap<u8, (u16, u16, u32)>>,
    forging: Option<f64>,
    // plumbing
    last_screen: Option<Screen>,
    /// True during the first frame of a screen (text focus is reset there).
    pub entered: bool,
    auto: Auto,
    pub scene: Scene,
    town_ready: bool,
}

thread_local! {
    static STATE: RefCell<AccountUi> = RefCell::new(AccountUi::new());
}

/// Runs `f` with the shared account state.
pub fn with_state<R>(f: impl FnOnce(&mut AccountUi) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

impl AccountUi {
    fn new() -> AccountUi {
        AccountUi {
            flow: AccountFlow::default(),
            mode: Mode::Login,
            user: String::new(),
            pass: String::new(),
            pass2: String::new(),
            remember: false,
            login_init: false,
            mode_at: 0.0,
            notice_at: -10.0,
            scroll_h: 0.0,
            last_notice: None,
            selected: 0,
            select_name: None,
            confirm_delete: None,
            confirm_text: String::new(),
            last_click: (usize::MAX, 0.0),
            cast: String::new(),
            map_names: HashMap::new(),
            form: Form::default(),
            bal: None,
            kits: None,
            forging: None,
            last_screen: None,
            entered: false,
            auto: Auto::from_env(),
            scene: Scene::new(ao_formats::recursos_dir()),
            town_ready: false,
        }
    }

    fn balance(&mut self, game: &Game) -> Option<&Balance> {
        self.bal.get_or_insert_with(|| ao_formats::dat::balance::load(&game.recursos).ok().map(|l| l.data)).as_ref()
    }

    /// Start of every account screen frame: applies server answers, runs the automation.
    /// Returns false when the screen changed and this frame should not draw the old one.
    pub fn begin(&mut self, game: &mut Game, assets: &Assets) -> bool {
        let before = game.screen.clone();
        self.entered = false;
        let had = self.flow.pending.clone();
        self.flow.update(game);
        if let (Some(p), None) = (had, &self.flow.pending) {
            self.answered(&p, game);
        }
        if game.screen == Screen::Game && self.auto.play.is_some() {
            eprintln!("AUTO: entered the world as {:?} on map {:?}", game.me().map(|e| e.name.clone()), game.map.as_ref().map(|m| m.number));
            std::process::exit(0);
        }
        self.auto_step(game, assets);
        // Errors animate the field they belong to.
        let n = self.flow.notice.as_ref().filter(|n| !n.1).map(|n| n.0.clone());
        if n.is_some() && n != self.last_notice {
            self.notice_at = get_time();
        }
        self.last_notice = n;
        if game.screen != before {
            return false;
        }
        if self.last_screen.as_ref() != Some(&game.screen) {
            self.on_enter(game, assets);
            self.last_screen = Some(game.screen.clone());
            self.entered = true;
        }
        true
    }

    /// A request finished: toasts for the good news.
    fn answered(&mut self, what: &Pending, game: &Game) {
        let ok = self.flow.notice.as_ref().is_none_or(|n| n.1);
        match what {
            Pending::Register if game.screen == Screen::Create => toast("Tu cuenta está lista. ¡Bienvenido!", ToastKind::Good),
            Pending::Create if ok && game.screen == Screen::Characters => {
                if let Some(n) = &self.select_name {
                    toast(format!("{n} se une a la hoguera"), ToastKind::Good);
                }
                self.flow.notice = None;
            }
            Pending::Delete if ok => {
                toast("El personaje partió para siempre", ToastKind::Info);
                self.flow.notice = None;
            }
            _ => {}
        }
    }

    fn on_enter(&mut self, game: &Game, assets: &Assets) {
        match game.screen {
            Screen::Login => {
                // Never keep a password around after leaving the account.
                if self.last_screen.is_some() {
                    self.pass.clear();
                    self.pass2.clear();
                }
                self.scene.travel(&TOWN_PATH, 11.0, 1.0);
                self.scene.hour_goal = HOUR_LOGIN;
                self.scene.follow = 1.2;
            }
            Screen::Characters => {
                self.confirm_delete = None;
                self.confirm_text.clear();
                self.selected = self.selected.min(game.characters.len().saturating_sub(1));
                self.scene.hour_goal = HOUR_CAMP;
                self.scene.follow = 1.8;
                self.scene.shot(camp_shot(None));
            }
            Screen::Create => {
                self.form = Form::default();
                if let Some((r, c, g, h)) = self.auto.create_preset {
                    (self.form.race, self.form.class, self.form.gender) = (r, c, g);
                    self.refresh_heads(assets);
                    self.form.head_i = h.min(self.form.heads.len().saturating_sub(1));
                    self.form.name = self.auto.create_name.clone().unwrap_or_default();
                }
                self.refresh_heads(assets);
                self.forging = None;
                self.scene.hour_goal = HOUR_CAMP;
                self.scene.follow = 1.8;
            }
            _ => {}
        }
        // A fresh process lands straight on the shot (screenshots, auto login).
        if self.last_screen.is_none() && game.screen != Screen::Login {
            let s = match game.screen {
                Screen::Create => hero_shot(),
                _ => camp_shot(None),
            };
            self.scene.jump(s);
        }
    }

    fn refresh_heads(&mut self, assets: &Assets) {
        let f = &mut self.form;
        if f.heads_for != (f.race, f.gender) || f.heads.is_empty() {
            f.heads = valid_heads(f.race, f.gender, |h| assets.db.heads.contains_key(&h));
            f.heads_for = (f.race, f.gender);
            f.head_i = f.head_i.min(f.heads.len().saturating_sub(1));
        }
    }

    // -------------------------------------------------------------- automation

    fn auto_step(&mut self, game: &mut Game, assets: &Assets) {
        if !self.auto.active() || self.flow.busy() {
            return;
        }
        if let Some((m, _)) = self.auto.login_mode {
            if !self.auto.mode_applied {
                self.auto.mode_applied = true;
                self.mode = m;
            }
        }
        if game.screen == Screen::Login {
            if let Some((u, p)) = self.auto.login_fill.clone() {
                if !self.auto.fill_done {
                    self.auto.fill_done = true;
                    (self.user, self.pass) = (u, p);
                    self.login_init = true;
                }
            }
            if self.auto.target.as_deref() == Some("login") {
                return;
            }
            let Some((u, p)) = self.auto.account.clone() else { return };
            if !self.auto.tried_login {
                self.auto.tried_login = true;
                self.flow.login(game, &u, &p);
            } else if !self.auto.tried_register && self.flow.notice.as_ref().is_some_and(|n| !n.1) {
                self.auto.tried_register = true;
                self.flow.register(game, &u, &p);
            }
            return;
        }
        if !matches!(game.screen, Screen::Characters | Screen::Create) {
            return;
        }
        // Seed characters, one at a time.
        let seeds = self.auto.seeds.clone();
        if let Some((name, race, class, gender)) = seeds.iter().find(|s| !game.characters.iter().any(|c| c.name == s.0) && !self.auto.attempted.contains(&s.0)) {
            self.auto.attempted.insert(name.clone());
            let head = valid_heads(*race, *gender, |h| assets.db.heads.contains_key(&h)).get(self.auto.attempted.len() * 3 % 7).copied().unwrap_or(1);
            self.flow.create_character(game, name, *race, *class, *gender, head);
            return;
        }
        let seeded = seeds.iter().all(|s| game.characters.iter().any(|c| c.name == s.0) || self.auto.attempted.contains(&s.0));
        if !seeded {
            return;
        }
        if let Some(name) = self.auto.play.clone() {
            if game.characters.iter().any(|c| c.name == name) && game.screen == Screen::Characters {
                self.flow.play(game, &name);
            }
            return;
        }
        match (self.auto.target.as_deref(), &game.screen) {
            (Some("create"), Screen::Characters) if !self.auto.switched => {
                self.auto.switched = true;
                game.screen = Screen::Create;
            }
            (Some("characters"), Screen::Create) if !self.auto.switched => {
                self.auto.switched = true;
                game.screen = Screen::Characters;
            }
            _ => {}
        }
    }

    /// End of a screen frame: takes the automatic capture when due and exits.
    pub fn end(&mut self, game: &Game, name: &str) {
        let a = &mut self.auto;
        let (Some(path), Some(target)) = (a.shot.clone(), a.target.clone()) else { return };
        let seeded = name == "login" || a.seeds.iter().all(|s| game.characters.iter().any(|c| c.name == s.0) || a.attempted.contains(&s.0));
        let ready = target == name && seeded && !self.flow.busy() && (name != "characters" || a.seeds.is_empty() || game.characters.len() >= a.seeds.len());
        if ready {
            let t0 = *a.reached.get_or_insert(game.now.max(get_time()));
            if game.now.max(get_time()) - t0 >= a.shot_at {
                let (ms, n) = self.scene.perf();
                eprintln!("SCENE: {}x{} draw {ms:.2} ms average over {n} frames (debug build), {} fps", screen_width(), screen_height(), get_fps());
                capture(&path);
                std::process::exit(0);
            }
        } else if game.now.max(get_time()) > 40.0 {
            capture(&path);
            std::process::exit(1);
        }
    }
}

/// Saves the screen as an opaque PNG (the framebuffer's alpha channel is not 1 where
/// translucent panels were drawn, which viewers would blend with their own background).
fn capture(path: &str) {
    let mut img = get_screen_data();
    for px in img.bytes.as_chunks_mut::<4>().0.iter_mut() {
        px[3] = 255;
    }
    img.export_png(path);
}

/// Env-driven automation (kept in this file so the integrator's `devtools` stays untouched).
#[derive(Default)]
struct Auto {
    account: Option<(String, String)>,
    seeds: Vec<(String, u8, u8, u8)>,
    target: Option<String>,
    shot: Option<String>,
    shot_at: f64,
    select: Option<usize>,
    delete_modal: bool,
    play: Option<String>,
    create_preset: Option<(u8, u8, u8, usize)>,
    create_name: Option<String>,
    login_fill: Option<(String, String)>,
    login_mode: Option<(Mode, ())>,
    mode_applied: bool,
    fill_done: bool,
    tried_login: bool,
    tried_register: bool,
    switched: bool,
    select_done: bool,
    attempted: HashSet<String>,
    reached: Option<f64>,
}

impl Auto {
    fn from_env() -> Auto {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let pair = |s: String| s.split_once(':').map(|(a, b)| (a.to_string(), b.to_string()));
        let nums = |s: &str| s.split(':').map(|p| p.parse::<usize>().unwrap_or(1)).collect::<Vec<_>>();
        Auto {
            account: var("AO_ACCOUNT").and_then(pair),
            seeds: var("AO_ACCOUNT_SEED")
                .map(|s| {
                    s.split(',')
                        .filter_map(|e| {
                            let p: Vec<&str> = e.split(':').collect();
                            (p.len() == 4).then(|| (p[0].to_string(), p[1].parse().unwrap_or(1), p[2].parse().unwrap_or(1), p[3].parse().unwrap_or(1)))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            target: var("AO_ACCOUNT_SCREEN"),
            shot: var("AO_ACCOUNT_SHOT"),
            shot_at: var("AO_ACCOUNT_SHOT_AT").and_then(|s| s.parse().ok()).unwrap_or(2.5),
            select: var("AO_ACCOUNT_SELECT").and_then(|s| s.parse().ok()),
            delete_modal: var("AO_ACCOUNT_DELETE").is_some(),
            play: var("AO_ACCOUNT_PLAY"),
            create_preset: var("AO_CREATE").map(|s| {
                let n = nums(&s);
                (n[0] as u8, n.get(1).copied().unwrap_or(3) as u8, n.get(2).copied().unwrap_or(1) as u8, n.get(3).copied().unwrap_or(0))
            }),
            create_name: var("AO_CREATE_NAME"),
            login_fill: var("AO_LOGIN_FILL").and_then(pair),
            login_mode: var("AO_LOGIN_MODE").map(|m| (match m.as_str() { "register" => Mode::Register, "dev" => Mode::Dev, _ => Mode::Login }, ())),
            ..Default::default()
        }
    }

    fn active(&self) -> bool {
        self.account.is_some() || self.play.is_some() || self.login_fill.is_some() || self.login_mode.is_some() || self.target.is_some()
    }
}

// ------------------------------------------------------------------ shared look

/// Dark atmospheric fallback when the scene map cannot be loaded.
pub fn backdrop(u: f32) {
    let (sw, sh) = (screen_width(), screen_height());
    let n = 24;
    for i in 0..n {
        let f = i as f32 / n as f32;
        draw_rectangle(0.0, sh * f, sw, sh / n as f32 + 1.0, Color::new(0.085 - 0.06 * f, 0.058 - 0.04 * f, 0.036 - 0.026 * f, 1.0));
    }
    theme::inner_vignette(Rect::new(0.0, 0.0, sw, sh), 220.0 * u, 0.9);
}

/// Cinematic framing over the scene: darker edges, a bit more at the bottom and top
/// where the UI sits.
pub fn cinematic(u: f32, top: f32, bottom: f32) {
    let (sw, sh) = (screen_width(), screen_height());
    theme::inner_vignette(Rect::new(0.0, 0.0, sw, sh), 180.0 * u, 0.85);
    band(0.0, top, 0.55, true);
    band(sh - bottom, bottom, 0.78, false);
}

/// A vertical gradient band of black: opaque at the screen edge, clear towards the middle.
pub fn band(y: f32, h: f32, strength: f32, from_top: bool) {
    let sw = screen_width();
    let n = 20;
    for i in 0..n {
        let f = i as f32 / n as f32;
        let a = strength * if from_top { (1.0 - f).powi(2) } else { f.powi(2) };
        draw_rectangle(0.0, y + h * f, sw, h / n as f32 + 1.0, Color::new(0.02, 0.015, 0.01, a));
    }
}

/// A horizontal gradient band of black, for a side panel.
fn side_band(x: f32, w: f32, strength: f32) {
    let sh = screen_height();
    let n = 16;
    for i in 0..n {
        let f = i as f32 / n as f32;
        draw_rectangle(x + w * f, 0.0, w / n as f32 + 1.0, sh, Color::new(0.02, 0.015, 0.01, strength * f * f));
    }
}

/// Text with a soft glow (several faint copies around it), for titles over the scene.
#[allow(clippy::too_many_arguments)]
pub fn glow_text(kind: FontKind, s: &str, x: f32, y: f32, size: f32, c: Color, glow: Color, r: f32) {
    for i in 0..8 {
        let a = i as f32 / 8.0 * std::f32::consts::TAU;
        text_with(kind, s, x + a.cos() * r, y + a.sin() * r, size, glow);
    }
    text_with(kind, s, x, y, size, c);
}

#[allow(clippy::too_many_arguments)]
pub fn glow_centered(kind: FontKind, s: &str, cx: f32, y: f32, size: f32, c: Color, glow: Color, r: f32) {
    let w = measure_with(kind, s, size).width;
    glow_text(kind, s, cx - w / 2.0, y, size, c, glow, r);
}

/// Spinner + message over a dimmed screen while a request is in flight.
pub fn busy_overlay(label: &str, u: f32) {
    let (sw, sh) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
    let t = get_time() as f32;
    for i in 0..12 {
        let a = ((t * 2.6 - i as f32 * 0.3).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU).powi(2);
        let ang = i as f32 / 12.0 * std::f32::consts::TAU;
        draw_circle(sw / 2.0 + ang.cos() * 22.0 * u, sh / 2.0 - 16.0 * u + ang.sin() * 22.0 * u, 3.0 * u, Color::new(0.95, 0.74, 0.38, 0.12 + a * 0.85));
    }
    text_centered_with(FontKind::Title, label, sw / 2.0, sh / 2.0 + 40.0 * u, 22.0 * u, GOLD_BRIGHT);
}

/// The server's account answers (plain ASCII) in human, accented Spanish for the screens.
pub fn humane(msg: &str) -> String {
    let known = [
        ("Usuario o contrasena incorrectos.", "Ese usuario o esa contraseña no son correctos."),
        ("Ese nombre de cuenta ya existe.", "Ese nombre de cuenta ya está registrado: prueba con otro."),
        ("Esa cuenta ya esta conectada.", "Esa cuenta ya está dentro del juego."),
        ("Esta cuenta esta suspendida.", "Esta cuenta está suspendida."),
        ("Version del cliente desactualizada.", "Tu cliente está desactualizado: actualízalo para entrar."),
        ("Ya hay una cuenta abierta en esta conexion.", "Ya hay una cuenta abierta en esta conexión."),
        ("Nombre de cuenta invalido (3-20 letras, numeros o _).", "El usuario debe tener de 3 a 20 letras, números o _."),
        ("La contrasena debe tener entre 6 y 64 caracteres.", "La contraseña debe tener entre 6 y 64 caracteres."),
        ("Nombre invalido (3-16 letras, numeros o _).", "El nombre debe tener de 3 a 16 letras, números o _."),
        ("Ese nombre ya esta en uso.", "Ese nombre ya lo lleva otro aventurero."),
        ("Ya tienes el maximo de personajes.", "Ya tienes el máximo de personajes."),
        ("Ese personaje no es de tu cuenta.", "Ese personaje no es de tu cuenta."),
        ("Ese personaje ya esta conectado.", "Ese personaje ya está en el mundo."),
    ];
    known.iter().find(|(k, _)| *k == msg).map(|(_, h)| h.to_string()).unwrap_or_else(|| msg.to_string())
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

/// Section heading N2 (compact): small caps title and a fading gold line.
fn section(title: &str, x: f32, y: f32, w: f32, u: f32) -> f32 {
    let size = 15.0 * u;
    text_with(FontKind::Title, &title.to_uppercase(), x, y + size, size, GOLD_TEXT);
    let tw = measure_with(FontKind::Title, &title.to_uppercase(), size).width + 10.0 * u;
    let ly = y + size * 0.62;
    let n = 10;
    for i in 0..n {
        let f0 = i as f32 / n as f32;
        let f1 = (i + 1) as f32 / n as f32;
        let span = w - tw;
        draw_line(x + tw + span * f0, ly, x + tw + span * f1, ly, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.45 * (1.0 - f0)));
    }
    y + size + 10.0 * u
}

// ------------------------------------------------------------------ staging

/// Camera on the camp: the group between the header and the plaque; with a `focus` (feet
/// of the chosen character, world px) it leans and comes closer to it.
fn camp_shot(focus: Option<Vec2>) -> Shot {
    let base = Scene::base_scale();
    let group = tile_feet(FIRE.0, FIRE.1) - vec2(0.0, 2.6 * 32.0);
    let (at, zoom) = match focus {
        Some(f) => (group.lerp(f - vec2(0.0, 20.0), 0.35), (base + 0.5) / base),
        None => (group, 1.0),
    };
    // That point sits at 44% of the height (the plaque takes the bottom).
    let lift = (0.5 - 0.44) * screen_height() / (base * zoom);
    Shot { at: at + vec2(0.0, lift), zoom }
}

/// Camera on the new character: closer (one whole step of scale), with the hero on the
/// left third and the creation panel on the right.
fn hero_shot() -> Shot {
    let base = Scene::base_scale();
    let zoom = (base + 1.0) / base;
    let s = base * zoom;
    let hero = tile_feet(HERO.0, HERO.1) - vec2(0.0, 22.0);
    let want = vec2(screen_width() * 0.29, screen_height() * 0.54);
    let half = vec2(screen_width(), screen_height()) / 2.0;
    Shot { at: hero - (want - half) / s, zoom }
}

/// The direction from `a` to `b` (tiles), for actors facing something.
fn dir_to(a: (i32, i32), b: (i32, i32)) -> Dir {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    if dx.abs() > dy.abs() {
        if dx > 0 { Dir::Right } else { Dir::Left }
    } else if dy > 0 {
        Dir::Down
    } else {
        Dir::Up
    }
}

/// The body to draw for `body`: some official starting bodies (the orcs' 581/582) have no
/// sprite in this `cuerpos.dat`, so fall back to the race's unarmoured body, then to 1.
fn displayable_body(assets: &Assets, body: u16, race: u8, gender: u8) -> u16 {
    [body, ao_rules::character::naked_body(race, gender), 1].into_iter().find(|b| assets.db.bodies.contains_key(b)).unwrap_or(1)
}

/// Once per process: class kits and the town (every account screen calls it).
pub fn prepare(st: &mut AccountUi, assets: &Assets) {
    if st.kits.is_none() {
        st.kits = Some(class_kits(assets));
    }
    stage_town(st, assets);
}

/// The town for the login: the map's own NPCs at their spawn points and a few people
/// strolling around the plaza (once).
fn stage_town(st: &mut AccountUi, assets: &Assets) {
    if st.town_ready || !st.scene.ready() {
        return;
    }
    st.town_ready = true;
    let npcs = ao_formats::dat::npcs::load(&assets.root).ok().map(|l| l.data);
    let spawns: Vec<(i32, i32, u16)> = st.scene.map().map(|m| m.npcs.iter().map(|n| (n.x, n.y, n.npc)).collect()).unwrap_or_default();
    if let Some(table) = &npcs {
        for (i, (x, y, id)) in spawns.into_iter().enumerate() {
            let Some(n) = table.get(id as u32) else { continue };
            let body = n.body.max(0) as u16;
            if body == 0 || !assets.db.bodies.contains_key(&body) {
                continue;
            }
            let look = Look { body, head: n.head.max(0) as u16, weapon: n.weapon_anim.max(0) as u16, shield: n.shield_anim.max(0) as u16, helmet: n.helmet_anim.max(0) as u16 };
            let dir = match n.heading {
                1 => Dir::Up,
                2 => Dir::Right,
                4 => Dir::Left,
                _ => Dir::Down,
            };
            // Shopkeepers stay behind their counters; the rest walk a little.
            let act = if n.head > 0 && i % 3 != 0 { Act::Wander(2) } else { Act::Idle(dir) };
            st.scene.put(NPC_ID + i as u32, look, (x, y), act);
        }
    }
    for (i, ((x, y), race, class, gender)) in TOWNSFOLK.into_iter().enumerate() {
        let heads = valid_heads(race, gender, |h| assets.db.heads.contains_key(&h));
        let head = heads.get(i * 5 % heads.len().max(1)).copied().unwrap_or(1);
        let (weapon, shield, _) = st.kits.as_ref().and_then(|k| k.get(&class).copied()).unwrap_or((0, 0, 0));
        let body = displayable_body(assets, ao_rules::character::starting_body(race, gender), race, gender);
        st.scene.put(FOLK_ID + i as u32, Look { body, head, weapon, shield, helmet: 0 }, (x, y), Act::Wander(4));
    }
}

/// A representative weapon/shield (animation ids) and an icon (obj GRH) for each class,
/// from obj.dat by name.
fn class_kits(assets: &Assets) -> HashMap<u8, (u16, u16, u32)> {
    fn fold(s: &str) -> String {
        s.to_lowercase().chars().map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            'ñ' => 'n',
            c => c,
        }).collect()
    }
    let wanted: [(u8, &str, Option<&str>); 12] = [
        (1, "vara de cedro", None),
        (2, "maza de guerra", Some("escudo de tortuga")),
        (3, "espada larga", Some("escudo de tortuga")),
        (4, "daga", None),
        (5, "laud elfico", None),
        (6, "flauta elfica", None),
        (7, "espada larga", Some("escudo de hierro")),
        (8, "arco simple", None),
        (9, "piquete de minero", None),
        (10, "sable", None),
        (11, "daga +1", None),
        (12, "cimitarra", None),
    ];
    let mut by_name: HashMap<String, (i32, i32, i32)> = HashMap::new();
    for id in 1..=assets.objs.max_id() {
        if let Some(o) = assets.objs.get(id) {
            by_name.entry(fold(&o.name)).or_insert((o.weapon_anim, o.shield_anim, o.grh));
        }
    }
    let mut out = HashMap::new();
    for (class, weapon, shield) in wanted {
        let w = by_name.get(weapon).copied().unwrap_or((0, 0, 0));
        let s = shield.and_then(|s| by_name.get(s)).map(|s| s.1).unwrap_or(0);
        out.insert(class, (w.0.max(0) as u16, s.max(0) as u16, w.2.max(0) as u32));
    }
    out
}

/// The account's characters around the fire (re-placed when the list changes), the free
/// places, and who faces whom. `hero` = the creation spot is taken (they look at it).
fn stage_camp(st: &mut AccountUi, game: &Game, assets: &Assets, renderer: &mut Renderer, hero: bool) {
    if !st.scene.ready() {
        return;
    }
    st.scene.bonfire(FIRE.0, FIRE.1);
    let cast: String = game.characters.iter().map(|c| format!("{}:{}:{};", c.name, c.body, c.head)).collect();
    if cast != st.cast {
        let fresh = !st.cast.is_empty();
        let before: Vec<String> = st.cast.split(';').filter_map(|e| e.split(':').next().map(String::from)).collect();
        st.cast = cast;
        st.scene.remove_range(CHAR_ID..CHAR_ID + MAX_CHARACTERS as u32);
        for (i, c) in game.characters.iter().enumerate().take(MAX_CHARACTERS) {
            let (spot, dir) = RING[PROMINENCE[i]];
            let body = displayable_body(assets, c.body, c.race, c.gender);
            let id = CHAR_ID + i as u32;
            st.scene.put(id, Look { body, head: c.head, ..Default::default() }, spot, Act::Go { to: spot, face: dir, idle: true });
            if fresh && !before.contains(&c.name) {
                // Just created: it rises where it was forged and walks to its place.
                if let Some(e) = st.scene.ent_mut(id) {
                    e.snap(HERO.0, HERO.1);
                    e.dir = Dir::Up;
                }
                st.scene.burst(renderer, tile_feet(HERO.0, HERO.1) - vec2(0.0, 16.0));
            }
        }
    }
    let n = game.characters.len().min(MAX_CHARACTERS);
    // The first free place has a waiting silhouette (a translucent figure).
    let ghost = (n < MAX_CHARACTERS && !hero).then(|| RING[PROMINENCE[n]]);
    match ghost {
        Some((spot, _)) => {
            // Facing the camera: a recognisable figure, not a side view.
            let body = displayable_body(assets, 1, 1, MALE);
            st.scene.put(GHOST_ID, Look { body, head: 0, ..Default::default() }, spot, Act::Still(Dir::Down));
            if let Some(e) = st.scene.ent_mut(GHOST_ID) {
                e.status = status::DEAD;
            }
        }
        None => st.scene.remove(GHOST_ID),
    }
    // While creating, they gather around the newcomer and watch; otherwise they go back to
    // their places, the selected one looking at the camera and the rest at the fire.
    for i in 0..n {
        let (spot, dir) = RING[PROMINENCE[i]];
        let act = if hero {
            let w = WITNESS[i];
            Act::Go { to: w, face: dir_to(w, HERO), idle: false }
        } else if i == st.selected {
            Act::Go { to: spot, face: Dir::Down, idle: true }
        } else {
            Act::Go { to: spot, face: dir, idle: true }
        };
        let id = CHAR_ID + i as u32;
        let look = st.scene.ent_mut(id).map(|e| Look { body: e.body, head: e.head, ..Default::default() });
        if let Some(look) = look {
            st.scene.put(id, look, spot, act);
            if let Some(e) = st.scene.ent_mut(id) {
                e.status = if st.confirm_delete.as_deref() == game.characters.get(i).map(|c| c.name.as_str()) { status::DEAD } else { 0 };
            }
        }
    }
    if !hero {
        st.scene.remove(HERO_ID);
        st.scene.stop_fx(renderer, HERO_ID);
    }
}

/// Map name for the selector plaque (loaded once per map).
fn map_name(st: &mut AccountUi, recursos: &std::path::Path, map: u16) -> String {
    st.map_names
        .entry(map)
        .or_insert_with(|| ao_formats::AoMap::load(recursos, map).map(|m| m.name).unwrap_or_else(|_| format!("el mapa {map}")))
        .clone()
}

// ------------------------------------------------------------------ character selection

/// Character selection (`Screen::Characters`): the account's characters around a bonfire.
pub fn characters(ui: &mut Ui, game: &mut Game, assets: &mut Assets, renderer: &mut Renderer, dt: f32) {
    with_state(|st| {
        let ok = st.begin(game, assets);
        prepare(st, assets);
        stage_camp(st, game, assets, renderer, false);
        if !st.scene.draw(renderer, assets, dt) {
            backdrop(ui.u);
        }
        if ok && game.screen == Screen::Characters {
            characters_screen(st, ui, game);
        } else {
            ui.focus = None;
        }
        st.scene.draw_black();
        st.end(game, "characters");
    });
}

/// Rect around a figure standing at `feet` (screen), for hovering.
fn figure_rect(feet: Vec2, px: f32) -> Rect {
    Rect::new(feet.x - 15.0 * px, feet.y - 52.0 * px, 30.0 * px, 56.0 * px)
}

fn characters_screen(st: &mut AccountUi, ui: &mut Ui, game: &mut Game) {
    let u = ui.u;
    let (sw, sh) = (screen_width(), screen_height());
    if st.entered {
        ui.focus = None;
    }
    // Pick a character created a moment ago / by the automation.
    if let Some(n) = st.select_name.clone() {
        if let Some(i) = game.characters.iter().position(|c| c.name == n) {
            st.selected = i;
            st.select_name = None;
        }
    }
    if let (Some(i), false) = (st.auto.select, st.auto.select_done) {
        if i < game.characters.len() {
            st.selected = i;
            st.auto.select_done = true;
            if st.auto.delete_modal {
                st.confirm_delete = Some(game.characters[i].name.clone());
            }
        }
    }
    let n = game.characters.len().min(MAX_CHARACTERS);
    st.selected = st.selected.min(n.saturating_sub(1));
    let busy = st.flow.busy();
    let modal = st.confirm_delete.is_some() || busy;
    let mouse: Vec2 = mouse_position().into();
    let px = st.scene.pixel();
    let t = get_time() as f32;

    // ---- the world: who is under the mouse, focus, light, labels
    let mut hovered: Option<usize> = None;
    let mut ghost_hover = false;
    for i in 0..n {
        if let Some(f) = st.scene.feet(CHAR_ID + i as u32) {
            if !modal && figure_rect(st.scene.to_screen(f), px).contains(mouse) {
                hovered = Some(i);
            }
        }
    }
    if let Some(f) = st.scene.feet(GHOST_ID) {
        ghost_hover = !modal && hovered.is_none() && figure_rect(st.scene.to_screen(f), px).contains(mouse);
    }
    let focus = hovered.unwrap_or(st.selected);
    let focus_feet = (n > 0).then(|| st.scene.feet(CHAR_ID + focus as u32)).flatten();
    st.scene.aim(camp_shot(focus_feet));
    st.scene.me = (n > 0).then_some(CHAR_ID + st.selected as u32);
    if let Some(f) = st.scene.feet(CHAR_ID + st.selected as u32).filter(|_| n > 0) {
        st.scene.light(f - vec2(0.0, 18.0), 160.0, Color::new(1.0, 0.86, 0.6, 1.0));
    }
    if let Some(f) = st.scene.feet(GHOST_ID) {
        let pulse = 0.5 + 0.5 * (t * 2.2).sin();
        st.scene.light(f - vec2(0.0, 18.0), 110.0, Color::new(0.55, 0.7, 1.0, 0.5 + 0.4 * pulse));
    }

    cinematic(u, 120.0 * u, 250.0 * u);

    // Names over the heads.
    for (i, c) in game.characters.iter().enumerate().take(n) {
        let Some(f) = st.scene.feet(CHAR_ID + i as u32) else { continue };
        let p = st.scene.to_screen(f);
        let sel = i == st.selected;
        let h = ease(key_of(Rect::new(i as f32 * 7.0 + 9000.0, 0.0, 1.0, 1.0)), sel || hovered == Some(i), 0.12);
        let y = p.y - 56.0 * px - 6.0 * u * h;
        let size = (15.0 + 3.0 * h) * u;
        let col = Color::new(TEXT_DIM.r + (GOLD_BRIGHT.r - TEXT_DIM.r) * h, TEXT_DIM.g + (GOLD_BRIGHT.g - TEXT_DIM.g) * h, TEXT_DIM.b + (GOLD_BRIGHT.b - TEXT_DIM.b) * h, 0.85 + 0.15 * h);
        text_centered_with(FontKind::Title, &c.name, p.x, y, size, col);
        if sel {
            // A small gold diamond bobbing over the chosen one.
            let d = 5.0 * u;
            let c0 = vec2(p.x, y - size - 4.0 * u + (t * 3.0).sin() * 2.0 * u);
            draw_triangle(c0 + vec2(0.0, -d), c0 + vec2(d, 0.0), c0 + vec2(0.0, d), GOLD_BRIGHT);
            draw_triangle(c0 + vec2(0.0, -d), c0 + vec2(-d, 0.0), c0 + vec2(0.0, d), GOLD_BRIGHT);
        }
    }
    let mut create = false;
    if let Some(f) = st.scene.feet(GHOST_ID) {
        let p = st.scene.to_screen(f);
        let h = ease(key_of(Rect::new(9500.0, 0.0, 1.0, 1.0)), ghost_hover, 0.12);
        let a = 0.7 + 0.3 * h + 0.15 * (t * 2.2).sin();
        // A ring of pale light on the ground where the next one will stand.
        let pulse = 0.5 + 0.5 * (t * 2.2).sin();
        for k in 0..3 {
            let grow = 1.0 + k as f32 * 0.18 + pulse * 0.08;
            draw_ellipse_lines(p.x, p.y - 2.0 * px, 13.0 * px * grow, 5.0 * px * grow, 0.0, 1.5, Color::new(0.65, 0.8, 1.0, (0.45 - k as f32 * 0.13) * (0.6 + 0.4 * h)));
        }
        text_centered_with(FontKind::Title, "Un lugar libre", p.x, p.y - 58.0 * px, 15.0 * u, with_alpha(Color::new(0.75, 0.85, 1.0, 1.0), a.clamp(0.0, 1.0)));
        text_centered_with(FontKind::NarrativeItalic, "Haz clic para forjar un personaje", p.x, p.y + 24.0 * u, 15.0 * u, with_alpha(Color::new(0.85, 0.88, 0.95, 1.0), (0.55 + 0.45 * h).clamp(0.0, 1.0)));
        if ghost_hover && is_mouse_button_pressed(MouseButton::Left) {
            create = true;
        }
    }

    // ---- header
    text_centered_with(FontKind::Title, "La hoguera", sw / 2.0, 46.0 * u, 28.0 * u, GOLD_BRIGHT);
    theme::divider(sw / 2.0, 62.0 * u, 260.0 * u, u);
    text_centered_with(FontKind::NarrativeItalic, &format!("{n} de {MAX_CHARACTERS} viajeros esperan junto al fuego"), sw / 2.0, 86.0 * u, 16.0 * u, TEXT_DIM);
    if !st.user.is_empty() {
        text_with(FontKind::Body, "CUENTA", 28.0 * u, 36.0 * u, 13.0 * u, TEXT_FAINT);
        text_with(FontKind::Title, &st.user, 28.0 * u, 58.0 * u, 18.0 * u, TEXT);
    }
    if button_kind(Rect::new(sw - 178.0 * u, 26.0 * u, 150.0 * u, 30.0 * u), "Cerrar sesión", Kind::Ghost, !modal, u) || (!modal && is_key_pressed(KeyCode::Escape)) {
        game.account_logout();
        st.flow.notice = None;
        return;
    }

    // ---- the chosen one (N1) and the actions
    let mut play: Option<String> = None;
    let sel = game.characters.get(st.selected).cloned().filter(|_| n > 0);
    let shown = hovered.and_then(|i| game.characters.get(i).cloned()).or(sel.clone());
    let base = sh - 74.0 * u;
    match &shown {
        Some(c) => {
            glow_centered(FontKind::Title, &c.name, sw / 2.0, base - 112.0 * u, 44.0 * u, GOLD_BRIGHT, Color::new(1.0, 0.6, 0.2, 0.06), 3.0 * u);
            let line = format!("Nivel {}   ·   {} {}   ·   {}", c.level, race_name(c.race), class_name(c.class), gender_name(c.gender));
            text_centered_with(FontKind::Body, &line, sw / 2.0, base - 82.0 * u, 18.0 * u, TEXT);
            let place = map_name(st, &game.recursos, c.map);
            text_centered_with(FontKind::NarrativeItalic, &format!("Descansa en {place}"), sw / 2.0, base - 58.0 * u, 16.0 * u, TEXT_DIM);
        }
        None => {
            glow_centered(FontKind::Title, "Ningún viajero todavía", sw / 2.0, base - 100.0 * u, 36.0 * u, GOLD_BRIGHT, Color::new(1.0, 0.6, 0.2, 0.06), 3.0 * u);
            text_centered_with(FontKind::NarrativeItalic, "El fuego arde para quien se atreva a empezar.", sw / 2.0, base - 66.0 * u, 17.0 * u, TEXT_DIM);
        }
    }
    let bw = 260.0 * u;
    let bh = 46.0 * u;
    let by = base - 30.0 * u;
    if let Some(c) = &sel {
        if button_kind(Rect::new(sw / 2.0 - bw / 2.0, by, bw, bh), &format!("Jugar con {}", c.name), Kind::Primary, !modal, u) {
            play = Some(c.name.clone());
        }
        if button_kind(Rect::new(sw / 2.0 - bw / 2.0 - 170.0 * u, by + 7.0 * u, 150.0 * u, 32.0 * u), "Borrar", Kind::Danger, !modal, u) {
            st.confirm_delete = Some(c.name.clone());
            st.confirm_text.clear();
            ui.focus = Some("del_confirm".into());
        }
        if button_kind(Rect::new(sw / 2.0 + bw / 2.0 + 20.0 * u, by + 7.0 * u, 170.0 * u, 32.0 * u), "Nuevo personaje", Kind::Secondary, !modal && n < MAX_CHARACTERS, u) {
            create = true;
        }
    } else if button_kind(Rect::new(sw / 2.0 - bw / 2.0, by, bw, bh), "Forjar mi primer personaje", Kind::Primary, !modal, u) {
        create = true;
    }
    text_centered_with(FontKind::Body, "Clic o flechas para elegir   ·   Intro o doble clic para jugar   ·   Supr para borrar", sw / 2.0, sh - 14.0 * u, 13.0 * u, TEXT_FAINT);
    if let Some((msg, ok)) = st.flow.notice.clone() {
        text_centered_with(FontKind::Body, &msg, sw / 2.0, base - 140.0 * u - 22.0 * u, 17.0 * u, if ok { GOOD } else { BAD });
    }

    // ---- input: clicks in the world and keyboard
    if let Some(i) = hovered {
        if is_mouse_button_pressed(MouseButton::Left) {
            let double = st.last_click.0 == i && get_time() - st.last_click.1 < 0.4;
            st.selected = i;
            st.last_click = (i, get_time());
            if double {
                play = game.characters.get(i).map(|c| c.name.clone());
            }
        }
    }
    if !modal && n > 0 {
        // Arrows walk the characters from left to right as they stand.
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|i| {
            let ((x, y), _) = RING[PROMINENCE[*i]];
            (x, y)
        });
        let step = |dir: i32, st: &mut AccountUi| {
            let k = order.iter().position(|i| *i == st.selected).unwrap_or(0) as i32;
            st.selected = order[(k + dir).rem_euclid(n as i32) as usize];
        };
        if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::D) {
            step(1, st);
        }
        if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::A) {
            step(-1, st);
        }
        if is_key_pressed(KeyCode::Enter) {
            play = sel.as_ref().map(|c| c.name.clone());
        }
        if is_key_pressed(KeyCode::Delete) {
            st.confirm_delete = sel.as_ref().map(|c| c.name.clone());
            st.confirm_text.clear();
            ui.focus = Some("del_confirm".into());
        }
    }

    if let Some(name) = st.confirm_delete.clone() {
        if let Some(go) = delete_modal(st, ui, &name, u) {
            if go {
                st.flow.delete_character(game, &name);
            }
            st.confirm_delete = None;
            ui.focus = None;
        }
    } else if busy {
        busy_overlay(st.flow.pending.as_ref().map(Pending::label).unwrap_or(""), u);
    }
    if !modal {
        if let Some(c) = hovered.and_then(|i| game.characters.get(i)) {
            tooltip(&[c.name.clone(), format!("~{}", race_text(c.race)), "Doble clic para jugar".into()], u);
        }
    }
    if let Some(name) = play {
        st.flow.play(game, &name);
    } else if create && n < MAX_CHARACTERS {
        game.screen = Screen::Create;
        st.flow.notice = None;
    }
}

/// Delete confirmation: the player must type the character's name. `Some(true)` = delete,
/// `Some(false)` = cancelled, `None` = still open.
fn delete_modal(st: &mut AccountUi, ui: &mut Ui, name: &str, u: f32) -> Option<bool> {
    let (sw, sh) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.35));
    let r = Rect::new(sw / 2.0 - 260.0 * u, sh * 0.58 - 150.0 * u, 520.0 * u, 300.0 * u);
    let close = Rect::new(r.right() - 30.0 * u, r.y + 8.0 * u, 22.0 * u, 22.0 * u);
    let close_hover = close.contains(mouse_position().into());
    ui.theme.window_frame(r, "Borrar personaje", u, close_hover);
    let cx = sw / 2.0;
    glow_centered(FontKind::Title, &format!("¿Despedir a {name}?"), cx, r.y + 64.0 * u, 28.0 * u, Color::new(1.0, 0.62, 0.52, 1.0), Color::new(0.8, 0.1, 0.05, 0.08), 2.0 * u);
    let lines = wrap_lines(FontKind::Narrative, &format!("{name} se perderá para siempre, con todo lo que lleva y todo lo que aprendió. No hay vuelta atrás."), 17.0 * u, r.w - 80.0 * u);
    let mut y = r.y + 100.0 * u;
    for l in lines.iter().take(3) {
        text_centered_with(FontKind::Narrative, l, cx, y, 17.0 * u, TEXT);
        y += 22.0 * u;
    }
    text_centered_with(FontKind::Body, "Escribe su nombre para confirmar", cx, y + 12.0 * u, 15.0 * u, TEXT_DIM);
    let field = Rect::new(r.x + 90.0 * u, y + 22.0 * u, r.w - 180.0 * u, 38.0 * u);
    if ui.focus.is_none() {
        ui.focus = Some("del_confirm".into());
    }
    let enter = text_field(ui, "del_confirm", &mut st.confirm_text, field, 16, false, |c| c.is_ascii_alphanumeric() || c == '_');
    if st.confirm_text.is_empty() && ui.focus.as_deref() != Some("del_confirm") {
        placeholder(field, name, u);
    }
    let matches = st.confirm_text.eq_ignore_ascii_case(name);
    let mut result = None;
    let by = r.bottom() - 58.0 * u;
    if button_kind(Rect::new(r.x + 50.0 * u, by, 180.0 * u, 38.0 * u), "Conservarlo", Kind::Secondary, true, u) || is_key_pressed(KeyCode::Escape) || (close_hover && is_mouse_button_pressed(MouseButton::Left)) {
        result = Some(false);
    }
    if button_kind(Rect::new(r.right() - 230.0 * u, by, 180.0 * u, 38.0 * u), "Borrar para siempre", Kind::Danger, matches, u) || (enter && matches) {
        result = Some(true);
    }
    result
}

// ------------------------------------------------------------------ character creation

/// Character creation (`Screen::Create`): the new character stands in front of the fire.
pub fn create(ui: &mut Ui, game: &mut Game, assets: &mut Assets, renderer: &mut Renderer, dt: f32) {
    with_state(|st| {
        let ok = st.begin(game, assets);
        prepare(st, assets);
        stage_camp(st, game, assets, renderer, true);
        if ok && game.screen == Screen::Create {
            stage_hero(st, assets, renderer);
        }
        if !st.scene.draw(renderer, assets, dt) {
            backdrop(ui.u);
        }
        if ok && game.screen == Screen::Create {
            create_screen(st, ui, game, assets, renderer);
        } else {
            ui.focus = None;
        }
        st.scene.draw_black();
        st.end(game, "create");
    });
}

const NAME_SYLLABLES: [&[&str]; 3] = [
    &["Ael", "Bran", "Cael", "Dor", "El", "Fen", "Gor", "Hal", "Ith", "Kael", "Lor", "Mor", "Nor", "Ora", "Thal", "Ur", "Vor", "Zar", "Ar", "Ey"],
    &["a", "e", "i", "o", "u", "ar", "en", "il", "or", "an", "dri", "la", "ri", "wyn"],
    &["dor", "ric", "wen", "mir", "thas", "nar", "gard", "las", "dil", "mund", "rion", "ssa", "th", "lor"],
];

fn random_name() -> String {
    fn pick<'a>(s: &[&'a str]) -> &'a str {
        s[rand::gen_range(0, s.len())]
    }
    let mut n = format!("{}{}", pick(NAME_SYLLABLES[0]), pick(NAME_SYLLABLES[1]));
    if rand::gen_range(0, 2) == 0 {
        n.push_str(pick(NAME_SYLLABLES[2]));
    }
    n.truncate(16);
    n
}

/// The new character in the world, dressed as the form says (with its class's weapon).
fn stage_hero(st: &mut AccountUi, assets: &Assets, renderer: &mut Renderer) {
    if !st.scene.ready() {
        return;
    }
    st.refresh_heads(assets);
    let f = &st.form;
    let head = f.heads.get(f.head_i).copied().unwrap_or(1);
    let body = displayable_body(assets, ao_rules::character::starting_body(f.race, f.gender), f.race, f.gender);
    let (weapon, shield, _) = st.kits.as_ref().and_then(|k| k.get(&f.class).copied()).unwrap_or((0, 0, 0));
    let act = if f.walk { Act::InPlace(f.facing) } else { Act::Still(f.facing) };
    let fresh = !st.scene.has(HERO_ID);
    st.scene.put(HERO_ID, Look { body, head, weapon, shield, helmet: 0 }, HERO, act);
    if fresh {
        st.scene.jump_if_far(hero_shot());
        st.scene.burst(renderer, tile_feet(HERO.0, HERO.1) - vec2(0.0, 16.0));
    }
    st.scene.aim(hero_shot());
    st.scene.me = Some(HERO_ID);
    let since = (get_time() - f.changed_at) as f32;
    let flash = (1.0 - since / 0.45).clamp(0.0, 1.0);
    let feet = tile_feet(HERO.0, HERO.1);
    st.scene.light(feet - vec2(0.0, 18.0), 110.0 + 90.0 * flash, Color::new(0.85 + 0.15 * flash, 0.82, 0.7, 0.75 + 0.25 * flash));
}

/// Marks a form change: a puff of smoke and a flash on the hero.
fn changed(st: &mut AccountUi, renderer: &mut Renderer) {
    st.form.changed_at = get_time();
    renderer.fx.spawn_puff(tile_feet(HERO.0, HERO.1) - vec2(0.0, 14.0));
}

fn create_screen(st: &mut AccountUi, ui: &mut Ui, game: &mut Game, assets: &mut Assets, renderer: &mut Renderer) {
    let u = ui.u;
    let (sw, sh) = (screen_width(), screen_height());
    if st.entered {
        ui.focus = None;
    }
    let busy = st.flow.busy();
    st.balance(game);
    let bal = st.bal.as_ref().and_then(|b| b.as_ref()).cloned();
    let mouse: Vec2 = mouse_position().into();
    let px = st.scene.pixel();
    let t = get_time() as f32;
    let mut tip: Vec<String> = Vec::new();

    // ---- panel geometry (right side)
    let pw = (sw * 0.40).clamp(470.0 * u, 600.0 * u);
    let panel = Rect::new(sw - pw - 28.0 * u, 52.0 * u, pw, sh - 52.0 * u - 22.0 * u);
    side_band(sw * 0.45, sw * 0.55, 0.55);
    cinematic(u, 90.0 * u, 230.0 * u);

    // ---- the hero in the world: turning, faces, name
    let hero_feet = st.scene.feet(HERO_ID).map(|f| st.scene.to_screen(f)).unwrap_or(vec2(sw * 0.29, sh * 0.6));
    let hero_rect = figure_rect(hero_feet, px);
    let over_hero = !busy && hero_rect.contains(mouse);
    let turn = |d: Dir, cw: bool| -> Dir {
        let order = [Dir::Down, Dir::Left, Dir::Up, Dir::Right];
        let i = order.iter().position(|x| *x == d).unwrap_or(0);
        order[(i + if cw { 1 } else { 3 }) % 4]
    };
    // Turn arrows at the hero's sides.
    let ay = hero_feet.y - 26.0 * px;
    for (side, cw) in [(-1.0f32, false), (1.0, true)] {
        let c = vec2(hero_feet.x + side * 30.0 * px, ay);
        let r = Rect::new(c.x - 16.0 * u, c.y - 16.0 * u, 32.0 * u, 32.0 * u);
        let hover = !busy && r.contains(mouse);
        let h = ease(key_of(r), hover, 0.08);
        draw_circle(c.x, c.y, 15.0 * u, Color::new(0.03, 0.02, 0.015, 0.45 + 0.25 * h));
        draw_circle_lines(c.x, c.y, 15.0 * u, 1.0 + h, with_alpha(GOLD, 0.5 + 0.5 * h));
        let d = 6.0 * u;
        let tip_p = c + vec2(side * d, 0.0);
        draw_triangle(tip_p, c + vec2(-side * d * 0.6, -d), c + vec2(-side * d * 0.6, d), with_alpha(GOLD_BRIGHT, 0.6 + 0.4 * h));
        if hover && is_mouse_button_pressed(MouseButton::Left) {
            st.form.facing = turn(st.form.facing, cw);
        }
    }
    if !busy && ui.focus.as_deref() != Some("create_name") {
        if is_key_pressed(KeyCode::Left) {
            st.form.facing = turn(st.form.facing, false);
        }
        if is_key_pressed(KeyCode::Right) {
            st.form.facing = turn(st.form.facing, true);
        }
    }
    // Drag on the hero to turn it.
    if over_hero && is_mouse_button_pressed(MouseButton::Left) {
        st.form.drag_x = Some(mouse.x);
    }
    if let Some(x0) = st.form.drag_x {
        if !is_mouse_button_down(MouseButton::Left) {
            st.form.drag_x = None;
        } else if (mouse.x - x0).abs() > 36.0 * u {
            st.form.facing = turn(st.form.facing, mouse.x > x0);
            st.form.drag_x = Some(mouse.x);
        }
    }
    // Faces: wheel over the hero, up/down keys, arrows under the name.
    let n_heads = st.form.heads.len().max(1);
    let mut head_step = 0i32;
    let (_, wheel) = mouse_wheel();
    if over_hero && wheel != 0.0 {
        head_step = if wheel > 0.0 { -1 } else { 1 };
    }
    if !busy && is_key_pressed(KeyCode::Up) {
        head_step = -1;
    }
    if !busy && is_key_pressed(KeyCode::Down) {
        head_step = 1;
    }

    // Name and identity under the hero (N1 of the scene).
    let cx = hero_feet.x;
    let name_y = hero_feet.y + 64.0 * u;
    let shown_name = if st.form.name.is_empty() { "Sin nombre" } else { st.form.name.as_str() };
    let name_col = if st.form.name.is_empty() { TEXT_FAINT } else { GOLD_BRIGHT };
    glow_centered(FontKind::Title, shown_name, cx, name_y, 40.0 * u, name_col, Color::new(1.0, 0.6, 0.2, 0.06), 3.0 * u);
    let f = &st.form;
    text_centered_with(FontKind::Body, &format!("{}   ·   {}   ·   {}", race_name(f.race), class_name(f.class), gender_name(f.gender)), cx, name_y + 28.0 * u, 18.0 * u, TEXT);
    // Face selector: ‹ Rostro 3 de 30 ›
    let face_label = format!("Rostro {} de {}", f.head_i + 1, n_heads);
    let fw = measure_with(FontKind::Body, &face_label, 16.0 * u).width;
    let fy = name_y + 56.0 * u;
    text_centered_with(FontKind::Body, &face_label, cx, fy, 16.0 * u, TEXT_DIM);
    for (side, step) in [(-1.0f32, -1i32), (1.0, 1)] {
        let c = vec2(cx + side * (fw / 2.0 + 22.0 * u), fy - 5.0 * u);
        let r = Rect::new(c.x - 13.0 * u, c.y - 13.0 * u, 26.0 * u, 26.0 * u);
        let hover = !busy && r.contains(mouse);
        let h = ease(key_of(r), hover, 0.08);
        let d = 5.0 * u;
        draw_triangle(c + vec2(side * d, 0.0), c + vec2(-side * d * 0.7, -d), c + vec2(-side * d * 0.7, d), with_alpha(GOLD_TEXT, 0.6 + 0.4 * h));
        if hover && is_mouse_button_pressed(MouseButton::Left) {
            head_step = step;
        }
    }
    if head_step != 0 {
        st.form.head_i = (st.form.head_i as i32 + head_step).rem_euclid(n_heads as i32) as usize;
        changed(st, renderer);
    }
    // Walk toggle and hint.
    let wr = Rect::new(cx - 70.0 * u, fy + 14.0 * u, 140.0 * u, 26.0 * u);
    if button_kind(wr, if st.form.walk { "Quieto" } else { "Caminar" }, Kind::Ghost, !busy, u) {
        st.form.walk = !st.form.walk;
    }
    // Lore of the choice, as a quote (left, under everything).
    let f = &st.form;
    let lore_w = (panel.x - 60.0 * u).min(560.0 * u);
    let mut ly = (fy + 74.0 * u).max(sh - 132.0 * u);
    let lore = format!("{} {}", race_text(f.race), class_text(f.class));
    let lines = wrap_lines(FontKind::NarrativeItalic, &lore, 16.0 * u, lore_w);
    let lx = (cx - lore_w / 2.0).max(28.0 * u);
    for l in lines.iter().take(((sh - ly - 10.0 * u) / (20.0 * u)).max(0.0) as usize) {
        text_with(FontKind::NarrativeItalic, l, lx, ly, 16.0 * u, TEXT_DIM);
        ly += 20.0 * u;
    }
    if over_hero {
        tip = vec!["Tu personaje".into(), "Arrastra para girarlo, rueda para cambiar el rostro.".into()];
    }

    // ---- header
    let back_label = if game.characters.is_empty() { "Cerrar sesión" } else { "Volver a la hoguera" };
    if button_kind(Rect::new(24.0 * u, 22.0 * u, 190.0 * u, 30.0 * u), back_label, Kind::Ghost, !busy, u) || (!busy && is_key_pressed(KeyCode::Escape)) {
        if game.characters.is_empty() {
            game.account_logout();
        } else {
            game.screen = Screen::Characters;
            st.flow.notice = None;
        }
        return;
    }

    // ---- the panel (choices and numbers); its X goes back too
    let close = Rect::new(panel.right() - 30.0 * u, panel.y + 8.0 * u, 22.0 * u, 22.0 * u);
    let close_hover = !busy && close.contains(mouse);
    ui.theme.window_frame(panel, "Forja tu destino", u, close_hover);
    if close_hover && is_mouse_button_pressed(MouseButton::Left) {
        if game.characters.is_empty() {
            game.account_logout();
        } else {
            game.screen = Screen::Characters;
            st.flow.notice = None;
        }
        return;
    }
    ui.capture_mouse_if_over(panel);
    let px0 = panel.x + 22.0 * u;
    let pw0 = panel.w - 44.0 * u;
    let mut y = panel.y + 30.0 * u;

    // Name
    y = section("Nombre", px0, y, pw0, u);
    let dice = 40.0 * u;
    let field = Rect::new(px0, y, pw0 - dice - 8.0 * u, 38.0 * u);
    let enter = text_field(ui, "create_name", &mut st.form.name, field, 16, false, |c| c.is_ascii_alphanumeric() || c == '_');
    if st.form.name.is_empty() && ui.focus.as_deref() != Some("create_name") {
        placeholder(field, "¿Cómo te llamarán en Argentum?", u);
    }
    let dr = Rect::new(field.right() + 8.0 * u, y, dice, 38.0 * u);
    if button(dr, "", false, u) && !busy {
        st.form.name = random_name();
    }
    draw_dice(dr, u, t);
    if dr.contains(mouse) {
        tip = vec!["Nombre al azar".into()];
    }
    let check = client_core::p3::account::check_character_name(&st.form.name);
    let (msg, mc) = match (&check, st.form.name.is_empty()) {
        (_, true) => ("Entre 3 y 16 letras, números o guion bajo.".to_string(), TEXT_FAINT),
        (Ok(()), _) => ("Un buen nombre.".to_string(), GOOD),
        (Err(e), _) => (e.to_string(), BAD),
    };
    text_with(FontKind::Body, &msg, px0 + 2.0 * u, y + 56.0 * u, 14.0 * u, mc);
    y += 70.0 * u;

    // Race (+ gender at the right of the heading)
    let gy = y;
    y = section("Raza", px0, y, pw0 - 170.0 * u, u);
    for (i, g) in [MALE, FEMALE].into_iter().enumerate() {
        let r = Rect::new(px0 + pw0 - 160.0 * u + i as f32 * 82.0 * u, gy - 2.0 * u, 78.0 * u, 24.0 * u);
        if button(r, gender_name(g), st.form.gender == g, u) && !busy && st.form.gender != g {
            st.form.gender = g;
            st.form.head_i = 0;
            st.refresh_heads(assets);
            changed(st, renderer);
        }
    }
    let cols = 3usize;
    let gap = 8.0 * u;
    let cw = (pw0 - gap * (cols - 1) as f32) / cols as f32;
    let ch = 38.0 * u;
    for (i, (id, name)) in RACES.into_iter().enumerate() {
        let r = Rect::new(px0 + (i % cols) as f32 * (cw + gap), y + (i / cols) as f32 * (ch + gap), cw, ch);
        let sel = st.form.race == id;
        let (hover, click) = card(r, sel, !busy, u);
        let head = valid_heads(id, st.form.gender, |h| assets.db.heads.contains_key(&h)).first().copied().unwrap_or(1);
        if let Some(g) = assets.db.heads.get(&head).map(|h| h[3]) {
            draw_head_icon(assets, g, Rect::new(r.x + 6.0 * u, r.y + 4.0 * u, 30.0 * u, 30.0 * u));
        }
        text_with(FontKind::Title, name, r.x + 42.0 * u, r.y + r.h / 2.0 + 6.0 * u, 16.0 * u, if sel { GOLD_BRIGHT } else { TEXT });
        if hover {
            tip = vec![name.to_string(), format!("~{}", race_text(id))];
            if let Some(p) = bal.as_ref().and_then(|b| creation_preview(b, id, st.form.class)) {
                let mods: Vec<String> = ATTRIBUTE_NAMES.iter().zip(p.race_mods).filter(|(_, v)| *v != 0).map(|(n, v)| format!("{n} {v:+}")).collect();
                if !mods.is_empty() {
                    tip.push(mods.join("   "));
                }
            }
        }
        if click && !sel {
            st.form.race = id;
            st.form.head_i = 0;
            st.refresh_heads(assets);
            changed(st, renderer);
        }
    }
    y += 2.0 * (ch + gap) + 6.0 * u;

    // Class
    y = section("Clase", px0, y, pw0, u);
    let ch2 = 34.0 * u;
    let kits = st.kits.clone().unwrap_or_default();
    for (i, (id, name)) in CLASSES.iter().enumerate() {
        let r = Rect::new(px0 + (i % cols) as f32 * (cw + gap), y + (i / cols) as f32 * (ch2 + gap * 0.75), cw, ch2);
        let sel = st.form.class == *id;
        let (hover, click) = card(r, sel, !busy, u);
        let icon = kits.get(id).map(|k| k.2).unwrap_or(0);
        if icon > 0 {
            draw_grh_fit(assets, icon, Rect::new(r.x + 5.0 * u, r.y + 3.0 * u, 28.0 * u, 28.0 * u));
        }
        let label = class_label(*id, name);
        text_with(FontKind::Body, label, r.x + 38.0 * u, r.y + r.h / 2.0 + 5.0 * u, 16.0 * u, if sel { GOLD_BRIGHT } else { TEXT });
        if hover {
            tip = vec![label.to_string(), class_role(*id).to_string(), format!("~{}", class_text(*id))];
        }
        if click && !sel {
            st.form.class = *id;
            changed(st, renderer);
        }
    }
    y += 4.0 * (ch2 + gap * 0.75) + 6.0 * u;

    // Numbers: life and mana first, then the attributes.
    y = section("Al nacer", px0, y, pw0, u);
    let f = &st.form;
    if let Some(p) = bal.as_ref().and_then(|b| creation_preview(b, f.race, f.class)) {
        let big = |label: &str, value: String, c: Color, x: f32, y: f32| {
            text_with(FontKind::Title, &value, x, y + 30.0 * u, 36.0 * u, c);
            let w = measure_with(FontKind::Title, &value, 36.0 * u).width;
            text_with(FontKind::Body, label, x + w + 8.0 * u, y + 28.0 * u, 15.0 * u, TEXT_DIM);
        };
        let hp_c = Color::new(0.95, 0.36, 0.30, 1.0);
        let mana_c = if p.mana > 0 { Color::new(0.45, 0.62, 1.0, 1.0) } else { TEXT_FAINT };
        draw_circle(px0 + 7.0 * u, y + 18.0 * u, 6.0 * u, HP);
        big("de vida", p.hp.to_string(), hp_c, px0 + 20.0 * u, y);
        let mx = px0 + pw0 * 0.36;
        draw_circle(mx + 7.0 * u, y + 18.0 * u, 6.0 * u, if p.mana > 0 { MANA } else { TEXT_FAINT });
        big("de maná", p.mana.to_string(), mana_c, mx + 20.0 * u, y);
        let sx = px0 + pw0 * 0.72;
        text_with(FontKind::Title, &START_SKILL_POINTS.to_string(), sx, y + 30.0 * u, 26.0 * u, GOLD_TEXT);
        text_with(FontKind::Body, "habilidades", sx + 34.0 * u, y + 22.0 * u, 13.0 * u, TEXT_DIM);
        text_with(FontKind::Body, "por repartir", sx + 34.0 * u, y + 36.0 * u, 13.0 * u, TEXT_DIM);
        y += 50.0 * u;
        let row = ((panel.bottom() - 74.0 * u - y) / 5.0).clamp(18.0 * u, 24.0 * u);
        for (i, name) in ATTRIBUTE_NAMES.iter().enumerate() {
            let v = p.attributes[i];
            let m = p.race_mods[i];
            text_with(FontKind::Body, name, px0, y + row * 0.72, 15.0 * u, TEXT_DIM);
            let bx = px0 + 112.0 * u;
            let bw = pw0 - 112.0 * u - 74.0 * u;
            let br = Rect::new(bx, y + row * 0.22, bw, row * 0.56);
            theme::surface(br, Surface::Recess, u);
            // 18 points of base and the race's bonus (or the missing part, in red).
            let scale_v = |x: f32| (x - 8.0) / 14.0 * (br.w - 2.0);
            let base_w = scale_v(18.0f32.min(v as f32));
            draw_rectangle(br.x + 1.0, br.y + 1.0, base_w, br.h - 2.0, Color::new(0.62, 0.48, 0.24, 1.0));
            if m > 0 {
                draw_rectangle(br.x + 1.0 + base_w, br.y + 1.0, scale_v(v as f32) - base_w, br.h - 2.0, Color::new(0.42, 0.78, 0.38, 1.0));
            } else if m < 0 {
                draw_rectangle(br.x + 1.0 + scale_v(v as f32), br.y + 1.0, scale_v(18.0) - scale_v(v as f32), br.h - 2.0, Color::new(0.55, 0.16, 0.12, 0.8));
            }
            draw_rectangle(br.x + 1.0, br.y + 1.0, br.w - 2.0, (br.h - 2.0) * 0.4, Color::new(1.0, 1.0, 1.0, 0.07));
            draw_rectangle_lines(br.x, br.y, br.w, br.h, 1.0, GOLD_DIM);
            text_with(FontKind::Title, &v.to_string(), br.right() + 10.0 * u, y + row * 0.76, 18.0 * u, TEXT);
            if m != 0 {
                text_with(FontKind::Body, &format!("{m:+}"), br.right() + 40.0 * u, y + row * 0.74, 15.0 * u, if m > 0 { GOOD } else { BAD });
            }
            y += row;
        }
    } else {
        text_with(FontKind::Body, "No se pudo leer Balance.dat de los recursos.", px0, y + 20.0 * u, 16.0 * u, BAD);
    }

    // The one primary action.
    let name_ok = check.is_ok();
    let br = Rect::new(px0, panel.bottom() - 62.0 * u, pw0, 44.0 * u);
    let label = if name_ok { format!("Dar vida a {}", st.form.name) } else { "Dale un nombre para empezar".to_string() };
    let go = button_kind(br, &label, Kind::Primary, name_ok && !busy, u) || (enter && name_ok && !busy);
    if let Some((msg, ok)) = st.flow.notice.clone() {
        let c = if ok { GOOD } else { BAD };
        let shake = ((get_time() - st.notice_at) as f32 / 0.35).min(1.0);
        let dx = (t * 60.0).sin() * 4.0 * u * (1.0 - shake);
        text_centered_with(FontKind::Body, &msg, br.x + br.w / 2.0 + dx, br.y - 8.0 * u, 15.0 * u, c);
    }
    if go {
        // The ritual: light and the meditation aura, then the request (purely visual: the
        // request leaves at once, the flash and the aura just accompany it).
        st.forging = Some(get_time());
        st.scene.play_fx(renderer, assets, HERO_ID, RITUAL_FX, FX_FOREVER);
        st.scene.burst(renderer, tile_feet(HERO.0, HERO.1) - vec2(0.0, 16.0));
        let f = &st.form;
        let head = f.heads.get(f.head_i).copied().unwrap_or(1);
        let (name, race, class, gender) = (f.name.clone(), f.race, f.class, f.gender);
        st.select_name = Some(name.clone());
        st.flow.create_character(game, &name, race, class, gender, head);
    }
    if let Some(t0) = st.forging {
        let k = (get_time() - t0) as f32;
        let flash = (1.0 - k / 0.6).clamp(0.0, 1.0);
        if flash > 0.0 {
            draw_rectangle(0.0, 0.0, sw, sh, Color::new(1.0, 0.92, 0.75, flash * flash * 0.5));
        }
        if !st.flow.busy() && st.flow.notice.as_ref().is_some_and(|n| !n.1) {
            // Refused (name taken...): the ritual stops.
            st.forging = None;
            st.scene.stop_fx(renderer, HERO_ID);
        }
    }

    if st.flow.busy() {
        busy_overlay(st.flow.pending.as_ref().map(Pending::label).unwrap_or(""), u);
    } else {
        tooltip(&tip, u);
    }
}

/// Short class names that fit the cards (the official ones lack accents).
fn class_label(id: u8, name: &str) -> &str {
    match id {
        2 => "Clérigo",
        7 => "Paladín",
        11 => "Ladrón",
        _ => name,
    }
}

/// A selectable card (race, class): raised surface, gold border when chosen, warm hover.
fn card(r: Rect, selected: bool, enabled: bool, u: f32) -> (bool, bool) {
    let hover = enabled && r.contains(mouse_position().into());
    let h = ease(key_of(r), hover || selected, 0.08);
    if selected {
        draw_rectangle(r.x - 3.0 * u, r.y - 3.0 * u, r.w + 6.0 * u, r.h + 6.0 * u, Color::new(1.0, 0.72, 0.3, 0.12));
    }
    theme::surface(r, if selected { Surface::Raised } else { Surface::Recess }, u);
    if h > 0.0 {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(1.0, 0.82, 0.5, 0.05 * h));
    }
    if selected {
        theme::metal_border(r, 1.5 * u);
    } else {
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.5 + 0.5 * h));
    }
    (hover, hover && is_mouse_button_pressed(MouseButton::Left))
}

/// A die (five pips) drawn on the random-name button.
fn draw_dice(r: Rect, u: f32, t: f32) {
    let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    let hover = r.contains(mouse_position().into());
    let s = 9.0 * u;
    let wob = if hover { (t * 14.0).sin() * 0.12 } else { 0.0 };
    let rot = |p: Vec2| vec2(p.x * wob.cos() - p.y * wob.sin(), p.x * wob.sin() + p.y * wob.cos()) + c;
    let corners = [vec2(-s, -s), vec2(s, -s), vec2(s, s), vec2(-s, s)].map(rot);
    draw_triangle(corners[0], corners[1], corners[2], Color::new(0.88, 0.82, 0.70, 1.0));
    draw_triangle(corners[0], corners[2], corners[3], Color::new(0.88, 0.82, 0.70, 1.0));
    for k in 0..4 {
        draw_line(corners[k].x, corners[k].y, corners[(k + 1) % 4].x, corners[(k + 1) % 4].y, 1.0, theme::INK);
    }
    for p in [vec2(-0.5, -0.5), vec2(0.5, -0.5), vec2(0.0, 0.0), vec2(-0.5, 0.5), vec2(0.5, 0.5)] {
        let q = rot(p * s);
        draw_circle(q.x, q.y, 1.6 * u, theme::INK);
    }
}

/// A GRH (first frame) scaled to fit `r`, centred.
fn draw_grh_fit(assets: &mut Assets, grh: u32, r: Rect) {
    let Some(g) = assets.db.frame(grh, 0.0).cloned() else { return };
    let Some(tex) = assets.texture(g.file) else { return };
    let s = (r.w / g.w).min(r.h / g.h);
    let (w, h) = (g.w * s, g.h * s);
    draw_texture_ex(&tex, (r.x + (r.w - w) / 2.0).round(), (r.y + (r.h - h) / 2.0).round(), WHITE, DrawTextureParams { source: Some(Rect::new(g.sx, g.sy, g.w, g.h)), dest_size: Some(vec2(w, h)), ..Default::default() });
}

/// The head of the official head GRH, scaled into a small square (race card icons).
/// Head sprites are 27x64 with the head somewhere in the middle, so the visible part is
/// measured once per sprite and only that is drawn.
fn draw_head_icon(assets: &mut Assets, grh: u32, r: Rect) {
    thread_local! {
        static BOUNDS: RefCell<HashMap<u32, Option<Rect>>> = RefCell::new(HashMap::new());
    }
    let Some(g) = assets.db.frame(grh, 0.0).cloned() else { return };
    let Some(tex) = assets.texture(g.file) else { return };
    let bounds = BOUNDS.with(|b| {
        *b.borrow_mut().entry(grh).or_insert_with(|| {
            let img = tex.get_texture_data();
            let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
            for y in 0..g.h as u32 {
                for x in 0..g.w as u32 {
                    let (px, py) = (g.sx as u32 + x, g.sy as u32 + y);
                    if px < img.width() as u32 && py < img.height() as u32 && img.get_image_data()[(py as usize) * img.width() + px as usize][3] > 20 {
                        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                    }
                }
            }
            (x0 != u32::MAX).then(|| Rect::new(x0 as f32, y0 as f32, (x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32))
        })
    });
    let Some(b) = bounds else { return };
    let s = (r.w / b.w).min(r.h / b.h);
    let (w, h) = (b.w * s, b.h * s);
    draw_texture_ex(&tex, (r.x + (r.w - w) / 2.0).round(), (r.y + (r.h - h) / 2.0).round(), WHITE, DrawTextureParams { source: Some(Rect::new(g.sx + b.x, g.sy + b.y, b.w, b.h)), dest_size: Some(vec2(w, h)), ..Default::default() });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_places_never_collide() {
        let ring: HashSet<(i32, i32)> = RING.iter().map(|r| r.0).collect();
        let witness: HashSet<(i32, i32)> = WITNESS.iter().copied().collect();
        assert_eq!(ring.len(), MAX_CHARACTERS, "one place per character");
        assert_eq!(witness.len(), MAX_CHARACTERS);
        for p in ring.iter().chain(witness.iter()) {
            assert!(*p != FIRE && *p != HERO, "{p:?} is the fire or the hero");
        }
        let mut seen = HashSet::new();
        assert!(PROMINENCE.iter().all(|i| *i < MAX_CHARACTERS && seen.insert(*i)), "prominence is a permutation");
    }

    #[test]
    fn facing_and_labels() {
        assert!(dir_to((10, 49), (10, 53)) == Dir::Down && dir_to((7, 53), (10, 53)) == Dir::Right);
        assert_eq!(class_label(2, "Clerigo"), "Clérigo");
    }
}

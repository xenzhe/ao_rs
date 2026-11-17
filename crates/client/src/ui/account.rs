//! Account screens: log in / create account, character selection, character creation
//! (preview, races and classes, head choice). OWNER: Sonnet 1 (docs/P3_CONTRATO.md).
//!
//! The three screens share one [`AccountUi`] (a thread local: `Game` has no account fields
//! and `main.rs` only gives us `ui`, `game` and `assets`). `login.rs` draws the login screen
//! with it; this file has the character list, the creation screen and the shared look.
//!
//! Test and screenshot automation (env vars read here, see [`Auto`]):
//! - `AO_ACCOUNT=user:pass`: logs in by itself (creates the account if it does not exist)
//! - `AO_ACCOUNT_SEED=Name:race:class:gender,...`: creates those characters after logging in
//! - `AO_ACCOUNT_SCREEN=login|characters|create`: screen to capture
//! - `AO_ACCOUNT_SHOT=file.png` (+ `AO_ACCOUNT_SHOT_AT=secs`, default 2.5): capture and exit
//! - `AO_ACCOUNT_SELECT=n`, `AO_ACCOUNT_DELETE=1`: selected slot / open the delete confirmation
//! - `AO_CREATE=race:class:gender[:head]`, `AO_CREATE_NAME=Name`: creation form preset
//! - `AO_LOGIN_FILL=user:pass`, `AO_LOGIN_MODE=login|register|dev`: login form preset
use crate::assets::Assets;
use crate::game::{Game, Screen};
use crate::ui::theme::{font, FontKind, BAD, GOLD, GOLD_TEXT, GOOD, MUTED, SLOT, TEXT, WOOD};
use crate::ui::widgets::{bar, measure, panel, text, text_centered, text_field, text_fit, text_with, tooltip};
use crate::ui::Ui;
use ao_formats::dat::balance::Balance;
use ao_protocol::CharacterSummary;
use ao_rules::character::{FEMALE, MALE, MAX_CHARACTERS, RACES, CLASSES, START_SKILL_POINTS};
use client_core::p3::account::{
    class_gauge_max, class_name, class_role, class_text, creation_preview, gender_name, race_name, race_text, valid_heads, AccountFlow, ClassGauges, Pending, Preview,
    ATTRIBUTE_NAMES,
};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;

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
    /// Facing index into [`FACINGS`].
    pub facing: usize,
    pub walk: bool,
    pub spin: bool,
    spin_at: f64,
}

impl Default for Form {
    fn default() -> Form {
        Form { name: String::new(), race: 1, class: 3, gender: MALE, head_i: 0, heads: Vec::new(), heads_for: (0, 0), facing: 0, walk: true, spin: true, spin_at: 0.0 }
    }
}

/// Facings shown by the preview, as AO headings (1 = N, 2 = E, 3 = S, 4 = W) with labels.
const FACINGS: [(usize, &str); 4] = [(3, "Sur"), (4, "Oeste"), (1, "Norte"), (2, "Este")];

pub struct AccountUi {
    pub flow: AccountFlow,
    // login (used by login.rs)
    pub mode: Mode,
    pub user: String,
    pub pass: String,
    pub pass2: String,
    pub remember: bool,
    pub login_init: bool,
    // selection
    pub selected: usize,
    select_name: Option<String>,
    confirm_delete: Option<String>,
    confirm_text: String,
    last_click: (usize, f64),
    // creation
    pub form: Form,
    bal: Option<Option<Balance>>,
    // plumbing
    last_screen: Option<Screen>,
    /// True during the first frame of a screen (text focus is reset there).
    pub entered: bool,
    auto: Auto,
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
            selected: 0,
            select_name: None,
            confirm_delete: None,
            confirm_text: String::new(),
            last_click: (usize::MAX, 0.0),
            form: Form::default(),
            bal: None,
            last_screen: None,
            entered: false,
            auto: Auto::from_env(),
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
        self.flow.update(game);
        if game.screen == Screen::Game && self.auto.play.is_some() {
            eprintln!("AUTO: entered the world as {:?} on map {:?}", game.me().map(|e| e.name.clone()), game.map.as_ref().map(|m| m.number));
            std::process::exit(0);
        }
        self.auto_step(game, assets);
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

    fn on_enter(&mut self, game: &Game, assets: &Assets) {
        match game.screen {
            Screen::Login => {
                // Never keep a password around after leaving the account.
                self.pass.clear();
                self.pass2.clear();
            }
            Screen::Characters => {
                self.confirm_delete = None;
                self.confirm_text.clear();
                self.selected = self.selected.min(game.characters.len().saturating_sub(1));
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
                self.form.spin_at = game.now;
            }
            _ => {}
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
            let head = valid_heads(*race, *gender, |h| assets.db.heads.contains_key(&h)).get(self.auto.attempted.len() % 3).copied().unwrap_or(1);
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
            let t0 = *a.reached.get_or_insert(game.now);
            if game.now - t0 >= a.shot_at {
                capture(&path);
                std::process::exit(0);
            }
        } else if game.now > 40.0 {
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

pub fn title(s: &str, x: f32, y: f32, size: f32, c: Color) {
    text_with(FontKind::Title, s, x, y, size, c);
}

pub fn title_w(s: &str, size: f32) -> f32 {
    measure_text(s, font(FontKind::Title).as_ref(), size as u16, 1.0).width
}

pub fn title_centered(s: &str, cx: f32, y: f32, size: f32, c: Color) {
    title(s, cx - title_w(s, size) / 2.0, y, size, c);
}

/// Title text shrunk until it fits `max_w`, centered.
fn title_fit_centered(s: &str, cx: f32, y: f32, mut size: f32, max_w: f32, c: Color) {
    while size > 10.0 && title_w(s, size) > max_w {
        size -= 1.0;
    }
    title_centered(s, cx, y, size, c);
}

/// A thin gold rule with a diamond in the middle.
pub fn rule(cx: f32, y: f32, w: f32, u: f32) {
    let fade = Color::new(GOLD.r, GOLD.g, GOLD.b, 0.0);
    for i in 0..16 {
        let f = i as f32 / 16.0;
        let c = Color::new(GOLD.r, GOLD.g, GOLD.b, f * f);
        let seg = (w / 2.0 - 12.0 * u) / 16.0;
        draw_line(cx - w / 2.0 + i as f32 * seg, y, cx - w / 2.0 + (i + 1) as f32 * seg, y, 1.5 * u, c);
        draw_line(cx + w / 2.0 - i as f32 * seg, y, cx + w / 2.0 - (i + 1) as f32 * seg, y, 1.5 * u, c);
    }
    let _ = fade;
    draw_poly(cx, y, 4, 5.0 * u, 0.0, GOLD_TEXT);
    draw_poly(cx, y, 4, 2.5 * u, 0.0, WOOD);
}

/// Dark atmospheric background shared by the account screens: gradient, torch glows,
/// rising embers and a vignette.
pub fn backdrop(u: f32) {
    let (sw, sh) = (screen_width(), screen_height());
    let t = get_time() as f32;
    let n = 28;
    for i in 0..n {
        let f = i as f32 / n as f32;
        draw_rectangle(0.0, sh * f, sw, sh / n as f32 + 1.0, Color::new(0.085 - 0.06 * f, 0.058 - 0.04 * f, 0.036 - 0.026 * f, 1.0));
    }
    for (i, x) in [sw * 0.1, sw * 0.9].into_iter().enumerate() {
        let flick = 1.0 + 0.08 * (t * (3.1 + i as f32) + i as f32 * 2.0).sin();
        for k in 0..28 {
            draw_circle(x, sh * 0.32, (340.0 - k as f32 * 11.0) * u * flick, Color::new(1.0, 0.55, 0.18, 0.0085));
        }
    }
    for i in 0..46 {
        let sp = 1.0 + (i % 4) as f32 * 0.6;
        let x = (i as f32 * 97.0 + t * 7.0 * sp + (t * 0.8 + i as f32).sin() * 18.0 * u) % sw;
        let y = sh - (i as f32 * 53.0 + t * 24.0 * sp) % sh;
        let a = 0.55 * (1.0 - y / sh).clamp(0.0, 1.0) + 0.1;
        draw_circle(x, y, (1.0 + (i % 3) as f32 * 0.6) * u, Color::new(1.0, 0.62, 0.22, a));
    }
    for i in 0..48 {
        let a = 0.16 * (1.0 - i as f32 / 48.0).powi(2);
        let o = i as f32 * 3.0 * u;
        draw_rectangle_lines(o, o, sw - 2.0 * o, sh - 2.0 * o, 3.0 * u, Color::new(0.0, 0.0, 0.0, a));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Normal,
    Danger,
}

/// A button with a gradient face. Returns true when clicked this frame (never when disabled).
pub fn fancy_button(r: Rect, label: &str, kind: Kind, enabled: bool, u: f32) -> bool {
    let hover = enabled && r.contains(mouse_position().into());
    let (top, bot, border, ink) = match (kind, enabled) {
        (_, false) => (Color::new(0.12, 0.10, 0.08, 1.0), Color::new(0.09, 0.075, 0.06, 1.0), Color::new(0.30, 0.25, 0.18, 1.0), MUTED),
        (Kind::Primary, _) => (Color::new(0.55, 0.38, 0.12, 1.0), Color::new(0.34, 0.22, 0.07, 1.0), GOLD_TEXT, Color::new(1.0, 0.95, 0.8, 1.0)),
        (Kind::Danger, _) => (Color::new(0.42, 0.13, 0.10, 1.0), Color::new(0.26, 0.08, 0.06, 1.0), Color::new(0.75, 0.32, 0.25, 1.0), Color::new(1.0, 0.88, 0.85, 1.0)),
        (Kind::Normal, _) => (Color::new(0.24, 0.17, 0.10, 1.0), Color::new(0.14, 0.10, 0.065, 1.0), GOLD, TEXT),
    };
    let lift = if hover { 0.08 } else { 0.0 };
    let n = 6;
    for i in 0..n {
        let f = i as f32 / (n - 1) as f32;
        let c = Color::new(top.r + (bot.r - top.r) * f + lift, top.g + (bot.g - top.g) * f + lift, top.b + (bot.b - top.b) * f + lift, 1.0);
        draw_rectangle(r.x, r.y + r.h * i as f32 / n as f32, r.w, r.h / n as f32 + 1.0, c);
    }
    draw_rectangle(r.x, r.y, r.w, 1.5 * u, Color::new(1.0, 0.9, 0.6, if enabled { 0.18 } else { 0.05 }));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, if hover { 2.0 * u } else { 1.5 * u }, border);
    let size = (if kind == Kind::Primary { 21.0 } else { 18.0 }) * u;
    let kind_font = if kind == Kind::Primary { FontKind::Title } else { FontKind::Body };
    let d = measure_text(label, font(kind_font).as_ref(), size as u16, 1.0);
    text_with(kind_font, label, r.x + (r.w - d.width) / 2.0, r.y + r.h / 2.0 + d.height / 2.0 - 1.0 * u, size, ink);
    hover && is_mouse_button_pressed(MouseButton::Left)
}

/// Row background for a selectable list entry; returns (hovered, clicked).
fn list_row(r: Rect, selected: bool, enabled: bool, u: f32) -> (bool, bool) {
    let hover = enabled && r.contains(mouse_position().into());
    let bg = if selected {
        Color::new(0.33, 0.22, 0.09, 1.0)
    } else if hover {
        Color::new(0.20, 0.145, 0.085, 1.0)
    } else {
        Color::new(0.115, 0.085, 0.055, 1.0)
    };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    if selected {
        draw_rectangle(r.x, r.y, 4.0 * u, r.h, GOLD_TEXT);
        draw_rectangle(r.x, r.y, r.w, r.h * 0.4, Color::new(1.0, 0.85, 0.5, 0.07));
    }
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, if selected { GOLD_TEXT } else if hover { GOLD } else { Color::new(0.30, 0.22, 0.12, 1.0) });
    (hover, hover && is_mouse_button_pressed(MouseButton::Left))
}

/// Section heading used by the creation screen.
fn heading(s: &str, x: f32, y: f32, w: f32, u: f32) {
    title(s, x, y, 19.0 * u, GOLD_TEXT);
    draw_line(x, y + 6.0 * u, x + w, y + 6.0 * u, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.55));
}

/// Word-wrapped lines of `s` that fit `max_w` at `size`.
pub fn wrap(s: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let test = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if cur.is_empty() || measure(&test, size).width <= max_w {
            cur = test;
        } else {
            lines.push(std::mem::replace(&mut cur, word.to_string()));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Notice line (red/green) wrapped to two lines at most.
pub fn notice_lines(msg: &str, ok: bool, x: f32, y: f32, w: f32, size: f32, u: f32) {
    let c = if ok { GOOD } else { BAD };
    for (i, l) in wrap(msg, size, w).iter().take(2).enumerate() {
        text(l, x, y + i as f32 * (size + 3.0 * u), size, c);
    }
}

/// Spinner + message over a dimmed screen while a request is in flight.
pub fn busy_overlay(label: &str, u: f32) {
    let (sw, sh) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.55));
    let t = get_time() as f32;
    for i in 0..10 {
        let a = ((t * 3.0 - i as f32 * 0.35).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU).powi(2);
        let ang = i as f32 / 10.0 * std::f32::consts::TAU;
        draw_circle(sw / 2.0 + ang.cos() * 22.0 * u, sh / 2.0 - 16.0 * u + ang.sin() * 22.0 * u, 3.5 * u, Color::new(0.88, 0.7, 0.36, 0.15 + a * 0.85));
    }
    title_centered(label, sw / 2.0, sh / 2.0 + 44.0 * u, 24.0 * u, GOLD_TEXT);
}

// ------------------------------------------------------------------ drawing characters

/// Draws one GRH scaled by `s`, with `(0,0)` of the official 32x32 tile mapped to `origin`
/// minus the tile's bottom-centre (so `origin` is where the feet are).
fn draw_part(assets: &mut Assets, idx: u32, ox: f32, oy: f32, feet: Vec2, s: f32, t_ms: f32) {
    let Some(g) = assets.db.frame(idx, t_ms).cloned() else { return };
    let (ax, ay) = ao_formats::graphics::anchor(&g, ox, oy, true);
    let Some(tex) = assets.texture(g.file) else { return };
    let x = feet.x + (ax - 16.0) * s;
    let y = feet.y + (ay - 32.0) * s;
    draw_texture_ex(&tex, x.round(), y.round(), WHITE, DrawTextureParams { source: Some(Rect::new(g.sx, g.sy, g.w, g.h)), dest_size: Some(vec2(g.w * s, g.h * s)), ..Default::default() });
}

/// Body + head like `render/characters.rs` composes them, at scale `s`, standing with its
/// feet at `feet`. `heading` is the AO heading (1 N, 2 E, 3 S, 4 W); `walk_ms` > 0 animates.
pub fn draw_figure(assets: &mut Assets, body: u16, head: u16, heading: usize, feet: Vec2, s: f32, walk_ms: f32) {
    let Some(b) = assets.db.bodies.get(&body).cloned() else { return };
    draw_ellipse(feet.x, feet.y - 3.0 * s, 11.0 * s, 4.0 * s, 0.0, Color::new(0.0, 0.0, 0.0, 0.35));
    draw_part(assets, b.walk[heading], b.body_offset.0, b.body_offset.1, feet, s, walk_ms);
    if let Some(h) = assets.db.heads.get(&head).map(|h| h[heading]) {
        if h > 0 {
            draw_part(assets, h, b.body_offset.0 + b.head_offset.0, b.body_offset.1 + b.head_offset.1 - 1.0, feet, s, 0.0);
        }
    }
}

// ------------------------------------------------------------------ character selection

/// Character selection (`Screen::Characters`).
pub fn characters(ui: &mut Ui, game: &mut Game, assets: &mut Assets) {
    with_state(|st| characters_screen(st, ui, game, assets));
}

fn characters_screen(st: &mut AccountUi, ui: &mut Ui, game: &mut Game, assets: &mut Assets) {
    let u = ui.u;
    let (sw, sh) = (screen_width(), screen_height());
    backdrop(u);
    if !st.begin(game, assets) {
        ui.focus = None;
        return;
    }
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
    let n = game.characters.len();
    st.selected = st.selected.min(n.saturating_sub(1));
    let busy = st.flow.busy();
    let modal = st.confirm_delete.is_some() || busy;
    let t = get_time() as f32;

    // Header
    title_centered("Elige a tu personaje", sw / 2.0, 54.0 * u, 42.0 * u, GOLD_TEXT);
    rule(sw / 2.0, 70.0 * u, 460.0 * u, u);
    text_centered(&format!("{n} de {MAX_CHARACTERS} personajes en tu cuenta"), sw / 2.0, 92.0 * u, 18.0 * u, MUTED);
    let who = if st.user.is_empty() { String::new() } else { format!("Cuenta: {}", st.user) };
    text(&who, 28.0 * u, 40.0 * u, 18.0 * u, MUTED);
    if fancy_button(Rect::new(sw - 188.0 * u, 20.0 * u, 160.0 * u, 34.0 * u), "Cerrar sesión", Kind::Normal, !modal, u) {
        game.account_logout();
        st.flow.notice = None;
        return;
    }

    // Grid of slots: 5 x 2.
    let cols = 5usize;
    let gap = 14.0 * u;
    let cw = ((sw - 2.0 * 40.0 * u - (cols - 1) as f32 * gap) / cols as f32).min(250.0 * u);
    let ch = (cw * 1.25).min((sh - 270.0 * u) / 2.0 - gap / 2.0);
    let grid_w = cols as f32 * cw + (cols - 1) as f32 * gap;
    let (gx, gy) = ((sw - grid_w) / 2.0, 108.0 * u);
    let mut tip: Vec<String> = Vec::new();
    let mut play: Option<String> = None;
    let mut create = false;
    for slot in 0..MAX_CHARACTERS {
        let r = Rect::new(gx + (slot % cols) as f32 * (cw + gap), gy + (slot / cols) as f32 * (ch + gap), cw, ch);
        if let Some(c) = game.characters.get(slot).cloned() {
            let selected = st.selected == slot;
            let (hover, click) = character_card(assets, r, &c, selected, !modal, u, t);
            if hover {
                tip = vec![c.name.clone(), format!("{} {} - {}", race_name(c.race), class_name(c.class), gender_name(c.gender)), format!("Nivel {}", c.level), format!("Mapa {}", c.map)];
            }
            if click {
                let double = st.last_click.0 == slot && get_time() - st.last_click.1 < 0.4;
                st.selected = slot;
                st.last_click = (slot, get_time());
                if double {
                    play = Some(c.name.clone());
                }
            }
        } else if empty_slot(r, slot == n, !modal, u, t) {
            create = true;
        }
    }

    // Action bar
    let by = gy + 2.0 * ch + gap + 22.0 * u;
    let sel = game.characters.get(st.selected).cloned();
    let bw = 270.0 * u;
    let bh = 48.0 * u;
    let cx = sw / 2.0;
    let play_label = sel.as_ref().map(|c| format!("Jugar con {}", c.name)).unwrap_or_else(|| "Jugar".into());
    if fancy_button(Rect::new(cx - bw / 2.0, by, bw, bh), &play_label, Kind::Primary, !modal && sel.is_some(), u) {
        play = sel.as_ref().map(|c| c.name.clone());
    }
    if fancy_button(Rect::new(cx - bw / 2.0 - 190.0 * u, by + 7.0 * u, 170.0 * u, 34.0 * u), "Borrar personaje", Kind::Danger, !modal && sel.is_some(), u) {
        st.confirm_delete = sel.as_ref().map(|c| c.name.clone());
        st.confirm_text.clear();
        ui.focus = Some("del_confirm".into());
    }
    if fancy_button(Rect::new(cx + bw / 2.0 + 20.0 * u, by + 7.0 * u, 190.0 * u, 34.0 * u), "Crear personaje", Kind::Normal, !modal && n < MAX_CHARACTERS, u) {
        create = true;
    }
    text_centered("Doble clic o Intro para jugar  -  flechas para elegir  -  Supr para borrar", cx, by + bh + 28.0 * u, 16.0 * u, MUTED);
    if let Some((msg, ok)) = st.flow.notice.clone() {
        notice_lines(&msg, ok, cx - 300.0 * u, by + bh + 54.0 * u, 600.0 * u, 18.0 * u, u);
    }

    // Keyboard
    if !modal {
        if n > 0 {
            if is_key_pressed(KeyCode::Right) {
                st.selected = (st.selected + 1) % n;
            }
            if is_key_pressed(KeyCode::Left) {
                st.selected = (st.selected + n - 1) % n;
            }
            if is_key_pressed(KeyCode::Down) {
                st.selected = (st.selected + cols).min(n - 1);
            }
            if is_key_pressed(KeyCode::Up) {
                st.selected = st.selected.saturating_sub(cols);
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
        if is_key_pressed(KeyCode::Escape) {
            game.account_logout();
            return;
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
        tooltip(&tip, u);
    }
    if let Some(name) = play {
        st.flow.play(game, &name);
    } else if create {
        game.screen = Screen::Create;
        st.flow.notice = None;
    }
    st.end(game, "characters");
}

/// One character card. Returns (hovered, clicked).
fn character_card(assets: &mut Assets, r: Rect, c: &CharacterSummary, selected: bool, enabled: bool, u: f32, t: f32) -> (bool, bool) {
    let (hover, click) = {
        let h = enabled && r.contains(mouse_position().into());
        (h, h && is_mouse_button_pressed(MouseButton::Left))
    };
    // frame
    draw_rectangle(r.x + 4.0 * u, r.y + 6.0 * u, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.4));
    draw_rectangle(r.x, r.y, r.w, r.h, WOOD);
    if selected {
        for k in 1..5 {
            draw_rectangle_lines(r.x - k as f32 * 2.0 * u, r.y - k as f32 * 2.0 * u, r.w + k as f32 * 4.0 * u, r.h + k as f32 * 4.0 * u, 2.0 * u, Color::new(0.95, 0.72, 0.3, 0.16 - k as f32 * 0.03));
        }
    }
    draw_rectangle_lines(r.x, r.y, r.w, r.h, if selected { 3.0 * u } else { 2.0 * u }, if selected { GOLD_TEXT } else if hover { Color::new(0.8, 0.62, 0.32, 1.0) } else { GOLD });
    draw_rectangle_lines(r.x + 4.0 * u, r.y + 4.0 * u, r.w - 8.0 * u, r.h - 8.0 * u, 1.0, Color::new(0.30, 0.22, 0.12, 1.0));
    // portrait stage
    let st_r = Rect::new(r.x + 8.0 * u, r.y + 8.0 * u, r.w - 16.0 * u, r.h - 16.0 * u - 92.0 * u);
    stage_floor(st_r, selected || hover, u);
    let s = (st_r.h * 0.74 / 50.0).max(1.0);
    let feet = vec2(st_r.x + st_r.w / 2.0, st_r.bottom() - st_r.h * 0.10);
    let (heading, walk) = if selected { (FACINGS[((t / 2.2) as usize) % 4].0, t * 1000.0) } else { (3, 0.0) };
    let body = displayable_body(assets, c.body, c.race, c.gender);
    draw_figure(assets, body, c.head, heading, feet, s, walk);
    // text
    let mid = r.x + r.w / 2.0;
    title_fit_centered(&c.name, mid, st_r.bottom() + 28.0 * u, 24.0 * u, r.w - 20.0 * u, if selected { Color::new(1.0, 0.92, 0.7, 1.0) } else { TEXT });
    text_centered(&format!("Nivel {}", c.level), mid, st_r.bottom() + 48.0 * u, 18.0 * u, GOLD_TEXT);
    text_fit(&format!("{} {}", race_name(c.race), class_name(c.class)), r.x + 12.0 * u, st_r.bottom() + 68.0 * u, 17.0 * u, r.w - 24.0 * u, TEXT);
    text_fit(&format!("Mapa {}", c.map), r.x + 12.0 * u, st_r.bottom() + 86.0 * u, 16.0 * u, r.w - 24.0 * u, MUTED);
    (hover, click)
}

/// The body to draw for `body`: some official starting bodies (the orcs' 581/582) have no
/// sprite in this `cuerpos.dat`, so fall back to the race's unarmoured body, then to 1.
fn displayable_body(assets: &Assets, body: u16, race: u8, gender: u8) -> u16 {
    [body, ao_rules::character::naked_body(race, gender), 1].into_iter().find(|b| assets.db.bodies.contains_key(b)).unwrap_or(1)
}

/// A dim stone floor with a soft glow, behind a figure.
fn stage_floor(r: Rect, lit: bool, u: f32) {
    let n = 10;
    for i in 0..n {
        let f = i as f32 / n as f32;
        let g = 0.05 + 0.07 * f;
        draw_rectangle(r.x, r.y + r.h * f, r.w, r.h / n as f32 + 1.0, Color::new(g * 1.1, g * 0.95, g * 0.8, 1.0));
    }
    let glow = if lit { 0.10 } else { 0.05 };
    for k in 0..6 {
        draw_ellipse(r.x + r.w / 2.0, r.bottom() - r.h * 0.12, (r.w * 0.42) * (1.0 - k as f32 * 0.13), r.h * 0.12 * (1.0 - k as f32 * 0.1), 0.0, Color::new(1.0, 0.75, 0.35, glow));
    }
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.30, 0.22, 0.12, 1.0));
    let _ = u;
}

/// An empty character slot; returns true when clicked (only the first free one invites).
fn empty_slot(r: Rect, first: bool, enabled: bool, u: f32, t: f32) -> bool {
    let hover = enabled && r.contains(mouse_position().into());
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.06, 0.045, 0.03, 0.55));
    let c = if hover { Color::new(0.8, 0.62, 0.32, 0.9) } else { Color::new(0.5, 0.38, 0.19, if first { 0.8 } else { 0.4 }) };
    // dashed border
    let dash = 10.0 * u;
    let mut x = r.x;
    while x < r.right() {
        let w = dash.min(r.right() - x);
        draw_line(x, r.y, x + w, r.y, 1.5 * u, c);
        draw_line(x, r.bottom(), x + w, r.bottom(), 1.5 * u, c);
        x += dash * 2.0;
    }
    let mut y = r.y;
    while y < r.bottom() {
        let h = dash.min(r.bottom() - y);
        draw_line(r.x, y, r.x, y + h, 1.5 * u, c);
        draw_line(r.right(), y, r.right(), y + h, 1.5 * u, c);
        y += dash * 2.0;
    }
    let pulse = if first { 0.75 + 0.25 * (t * 2.5).sin() } else { 0.45 };
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h * 0.42);
    draw_line(cx - 16.0 * u, cy, cx + 16.0 * u, cy, 3.0 * u, Color::new(0.88, 0.7, 0.36, pulse));
    draw_line(cx, cy - 16.0 * u, cx, cy + 16.0 * u, 3.0 * u, Color::new(0.88, 0.7, 0.36, pulse));
    text_centered(if first { "Nuevo personaje" } else { "Espacio libre" }, cx, cy + 54.0 * u, 18.0 * u, if first { GOLD_TEXT } else { MUTED });
    hover && is_mouse_button_pressed(MouseButton::Left)
}

/// Delete confirmation: the player must type the character's name. `Some(true)` = delete,
/// `Some(false)` = cancelled, `None` = still open.
fn delete_modal(st: &mut AccountUi, ui: &mut Ui, name: &str, u: f32) -> Option<bool> {
    let (sw, sh) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.7));
    let r = Rect::new(sw / 2.0 - 250.0 * u, sh / 2.0 - 140.0 * u, 500.0 * u, 280.0 * u);
    panel(r, u);
    title_centered("¿Borrar personaje?", sw / 2.0, r.y + 50.0 * u, 32.0 * u, Color::new(0.95, 0.5, 0.42, 1.0));
    rule(sw / 2.0, r.y + 66.0 * u, 380.0 * u, u);
    text_centered(&format!("{name} se perderá para siempre, con todo lo que lleva."), sw / 2.0, r.y + 98.0 * u, 18.0 * u, TEXT);
    text_centered("Esto no se puede deshacer. Escribe su nombre para confirmar:", sw / 2.0, r.y + 124.0 * u, 17.0 * u, MUTED);
    let field = Rect::new(r.x + 60.0 * u, r.y + 140.0 * u, r.w - 120.0 * u, 36.0 * u);
    if ui.focus.is_none() {
        ui.focus = Some("del_confirm".into());
    }
    let enter = text_field(ui, "del_confirm", &mut st.confirm_text, field, 16, false, |c| c.is_ascii_alphanumeric() || c == '_');
    let matches = st.confirm_text.eq_ignore_ascii_case(name);
    let mut result = None;
    if fancy_button(Rect::new(r.x + 40.0 * u, r.bottom() - 62.0 * u, 190.0 * u, 40.0 * u), "Cancelar", Kind::Normal, true, u) || is_key_pressed(KeyCode::Escape) {
        result = Some(false);
    }
    if fancy_button(Rect::new(r.right() - 230.0 * u, r.bottom() - 62.0 * u, 190.0 * u, 40.0 * u), "Borrar para siempre", Kind::Danger, matches, u) || (enter && matches) {
        result = Some(true);
    }
    result
}

// ------------------------------------------------------------------ character creation

/// Character creation (`Screen::Create`).
pub fn create(ui: &mut Ui, game: &mut Game, assets: &mut Assets) {
    with_state(|st| create_screen(st, ui, game, assets));
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

fn create_screen(st: &mut AccountUi, ui: &mut Ui, game: &mut Game, assets: &mut Assets) {
    let u = ui.u;
    let (sw, sh) = (screen_width(), screen_height());
    backdrop(u);
    if !st.begin(game, assets) {
        ui.focus = None;
        return;
    }
    if st.entered {
        ui.focus = None;
    }
    let busy = st.flow.busy();
    let now = game.now;
    let t = get_time() as f32;
    st.balance(game);
    let bal = st.bal.as_ref().and_then(|b| b.as_ref()).cloned();
    st.refresh_heads(assets);

    // Layout in UI units: three columns centred on screen.
    let vw = sw / u;
    let total = 1230.0f32.min(vw - 30.0);
    let x0 = (sw - total * u) / 2.0;
    let (lw, rw, gap) = (296.0f32, 470.0f32.min(total * 0.38), 16.0f32);
    let cw = total - lw - rw - 2.0 * gap;
    let col = |from: f32, w: f32| (x0 + from * u, w * u);
    let (lx, lwp) = col(0.0, lw);
    let (cx0, cwp) = col(lw + gap, cw);
    let (rx, rwp) = col(lw + gap + cw + gap, rw);
    let top = 112.0 * u;

    title_centered("Forja a tu personaje", sw / 2.0, 58.0 * u, 42.0 * u, GOLD_TEXT);
    rule(sw / 2.0, 76.0 * u, 460.0 * u, u);
    let back_label = if game.characters.is_empty() { "Cerrar sesión" } else { "Volver" };
    let mut tip: Vec<String> = Vec::new();
    if fancy_button(Rect::new(x0, 22.0 * u, 150.0 * u, 34.0 * u), back_label, Kind::Normal, !busy, u) || (!busy && is_key_pressed(KeyCode::Escape)) {
        if game.characters.is_empty() {
            game.account_logout();
        } else {
            game.screen = Screen::Characters;
            st.flow.notice = None;
        }
        return;
    }
    if fancy_button(Rect::new(x0 + total * u - 190.0 * u, 22.0 * u, 190.0 * u, 34.0 * u), "Aleatorio", Kind::Normal, !busy, u) {
        let f = &mut st.form;
        f.race = RACES[rand::gen_range(0, RACES.len())].0;
        f.class = CLASSES[rand::gen_range(0, CLASSES.len())].0;
        f.gender = if rand::gen_range(0, 2) == 0 { MALE } else { FEMALE };
        f.head_i = rand::gen_range(0, 1000);
        f.heads_for = (0, 0);
        f.name = random_name();
        st.refresh_heads(assets);
        st.form.head_i %= st.form.heads.len().max(1);
    }

    // ---- left column: race, gender, class
    let mut y = top + 18.0 * u;
    heading("RAZA", lx, y, lwp, u);
    y += 14.0 * u;
    for (id, name) in RACES {
        let r = Rect::new(lx, y, lwp, 34.0 * u);
        let (hover, click) = list_row(r, st.form.race == id, !busy, u);
        let head = valid_heads(id, st.form.gender, |h| assets.db.heads.contains_key(&h)).first().copied().unwrap_or(1);
        let hr = Rect::new(r.x + 8.0 * u, r.y + 2.0 * u, 30.0 * u, 30.0 * u);
        if let Some(g) = assets.db.heads.get(&head).map(|h| h[3]) {
            draw_head_icon(assets, g, hr, u);
        }
        text(name, r.x + 48.0 * u, r.y + 23.0 * u, 20.0 * u, if st.form.race == id { Color::new(1.0, 0.92, 0.7, 1.0) } else { TEXT });
        if hover {
            tip = vec![name.to_string()];
            if let Some(p) = bal.as_ref().and_then(|b| creation_preview(b, id, st.form.class)) {
                let mods: Vec<String> = ATTRIBUTE_NAMES.iter().zip(p.race_mods).map(|(n, v)| format!("{n} {v:+}")).collect();
                tip.push(mods[..3].join("   "));
                tip.push(mods[3..].join("   "));
            }
        }
        if click && st.form.race != id {
            st.form.race = id;
            st.form.head_i = 0;
        }
        y += 38.0 * u;
    }
    y += 22.0 * u;
    heading("GÉNERO", lx, y, lwp, u);
    y += 14.0 * u;
    for (i, g) in [MALE, FEMALE].into_iter().enumerate() {
        let r = Rect::new(lx + i as f32 * (lwp / 2.0 + 3.0 * u), y, lwp / 2.0 - 3.0 * u, 34.0 * u);
        let (_, click) = list_row(r, st.form.gender == g, !busy, u);
        text_centered(gender_name(g), r.x + r.w / 2.0, r.y + 23.0 * u, 20.0 * u, if st.form.gender == g { Color::new(1.0, 0.92, 0.7, 1.0) } else { TEXT });
        if click && st.form.gender != g {
            st.form.gender = g;
            st.form.head_i = 0;
        }
    }
    y += 62.0 * u;
    heading("CLASE", lx, y, lwp, u);
    y += 14.0 * u;
    for (i, (id, name)) in CLASSES.iter().enumerate() {
        let r = Rect::new(lx + (i % 2) as f32 * (lwp / 2.0 + 3.0 * u), y + (i / 2) as f32 * 38.0 * u, lwp / 2.0 - 3.0 * u, 34.0 * u);
        let (hover, click) = list_row(r, st.form.class == *id, !busy, u);
        text_fit(name, r.x + 12.0 * u, r.y + 23.0 * u, 19.0 * u, r.w - 18.0 * u, if st.form.class == *id { Color::new(1.0, 0.92, 0.7, 1.0) } else { TEXT });
        if hover {
            tip = vec![name.to_string(), class_role(*id).to_string()];
        }
        if click {
            st.form.class = *id;
        }
    }

    // ---- center: stage, head selector, name, create
    let stage = Rect::new(cx0, top, cwp, (sh - top - 220.0 * u).max(300.0 * u));
    stage_floor(stage, true, u);
    // Heading cycle
    if st.form.spin && now - st.form.spin_at > 2.4 {
        st.form.facing = (st.form.facing + 1) % 4;
        st.form.spin_at = now;
    }
    let f = &st.form;
    let head = f.heads.get(f.head_i).copied().unwrap_or(1);
    let body = displayable_body(assets, ao_rules::character::starting_body(f.race, f.gender), f.race, f.gender);
    let strip_h = 108.0 * u;
    let main_h = stage.h - strip_h;
    let s = (main_h * 0.80 / 54.0).max(1.0);
    let feet = vec2(stage.x + stage.w / 2.0, stage.y + main_h * 0.88);
    let walk_ms = if f.walk { t * 1000.0 } else { 0.0 };
    draw_figure(assets, body, head, FACINGS[f.facing].0, feet, s, walk_ms);
    draw_rectangle(stage.x, stage.y + main_h, stage.w, strip_h, Color::new(0.0, 0.0, 0.0, 0.35));
    draw_line(stage.x, stage.y + main_h, stage.right(), stage.y + main_h, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.5));
    let cellw = stage.w / 4.0;
    let mut new_facing = None;
    for (i, (hd, label)) in FACINGS.iter().enumerate() {
        let cell = Rect::new(stage.x + i as f32 * cellw, stage.y + main_h, cellw, strip_h);
        let hover = !busy && cell.contains(mouse_position().into());
        if f.facing == i {
            draw_rectangle(cell.x + 2.0, cell.y + 2.0, cell.w - 4.0, cell.h - 4.0, Color::new(0.95, 0.72, 0.3, 0.13));
        } else if hover {
            draw_rectangle(cell.x + 2.0, cell.y + 2.0, cell.w - 4.0, cell.h - 4.0, Color::new(1.0, 1.0, 1.0, 0.05));
        }
        draw_figure(assets, body, head, *hd, vec2(cell.x + cell.w / 2.0, cell.y + cell.h - 24.0 * u), 1.55 * u, if f.walk && f.facing == i { t * 1000.0 } else { 0.0 });
        text_centered(label, cell.x + cell.w / 2.0, cell.bottom() - 6.0 * u, 15.0 * u, if f.facing == i { GOLD_TEXT } else { MUTED });
        if hover && is_mouse_button_pressed(MouseButton::Left) {
            new_facing = Some(i);
        }
    }
    if let Some(i) = new_facing {
        let f = &mut st.form;
        f.facing = i;
        f.spin = false;
    }
    // Stage controls: walking, turning
    let f = &mut st.form;
    let chk = |r: Rect, label: &str, on: bool| -> bool {
        let hover = r.contains(mouse_position().into());
        draw_rectangle(r.x, r.y, 18.0 * u, 18.0 * u, SLOT);
        draw_rectangle_lines(r.x, r.y, 18.0 * u, 18.0 * u, 1.5 * u, if hover { GOLD_TEXT } else { GOLD });
        if on {
            draw_rectangle(r.x + 4.0 * u, r.y + 4.0 * u, 10.0 * u, 10.0 * u, GOLD_TEXT);
        }
        text(label, r.x + 26.0 * u, r.y + 15.0 * u, 17.0 * u, TEXT);
        hover && is_mouse_button_pressed(MouseButton::Left)
    };
    if chk(Rect::new(stage.x + 12.0 * u, stage.y + 12.0 * u, 120.0 * u, 20.0 * u), "Caminar", f.walk) {
        f.walk = !f.walk;
    }
    if chk(Rect::new(stage.x + 12.0 * u, stage.y + 38.0 * u, 120.0 * u, 20.0 * u), "Girar solo", f.spin) {
        f.spin = !f.spin;
        f.spin_at = now;
    }
    let tag = format!("{} {} - {}", race_name(f.race), class_name(f.class), gender_name(f.gender));
    let tw = measure(&tag, 17.0 * u).width;
    text(&tag, stage.right() - tw - 12.0 * u, stage.y + 28.0 * u, 17.0 * u, MUTED);

    // Head selector
    let hy = stage.bottom() + 12.0 * u;
    let hr = Rect::new(cx0, hy, cwp, 38.0 * u);
    let n_heads = st.form.heads.len().max(1);
    let arrow = |r: Rect, label: &str| fancy_button(r, label, Kind::Normal, !busy, u);
    let mut step: i32 = 0;
    if arrow(Rect::new(hr.x, hr.y, 52.0 * u, hr.h), "<") || (!busy && is_key_pressed(KeyCode::Left)) {
        step = -1;
    }
    if arrow(Rect::new(hr.right() - 52.0 * u, hr.y, 52.0 * u, hr.h), ">") || (!busy && is_key_pressed(KeyCode::Right)) {
        step = 1;
    }
    let (_, wheel) = mouse_wheel();
    if !busy && stage.contains(mouse_position().into()) && wheel != 0.0 {
        step = if wheel > 0.0 { -1 } else { 1 };
    }
    if step != 0 {
        let i = (st.form.head_i as i32 + step).rem_euclid(n_heads as i32);
        st.form.head_i = i as usize;
    }
    text_centered(&format!("Rostro {} de {}", st.form.head_i + 1, n_heads), hr.x + hr.w / 2.0, hr.y + 25.0 * u, 20.0 * u, TEXT);

    // Name
    let ny = hr.bottom() + 12.0 * u;
    text("Nombre", cx0, ny + 14.0 * u, 18.0 * u, GOLD_TEXT);
    let check = client_core::p3::account::check_character_name(&st.form.name);
    let (msg, mc) = match (&check, st.form.name.is_empty()) {
        (_, true) => ("3 a 16 letras, números o _".to_string(), MUTED),
        (Ok(()), _) => ("Nombre válido".to_string(), GOOD),
        (Err(e), _) => (e.to_string(), BAD),
    };
    let mw = measure(&msg, 16.0 * u).width;
    text(&msg, cx0 + cwp - mw, ny + 14.0 * u, 16.0 * u, mc);
    let field = Rect::new(cx0, ny + 20.0 * u, cwp, 36.0 * u);
    if ui.focus.is_none() && !busy && st.last_screen == Some(Screen::Create) && st.form.name.is_empty() && !st.auto.active() {
        ui.focus = Some("create_name".into());
    }
    let enter = text_field(ui, "create_name", &mut st.form.name, field, 16, false, |c| c.is_ascii_alphanumeric() || c == '_');
    let name_ok = check.is_ok();
    let by = field.bottom() + 12.0 * u;
    let go = fancy_button(Rect::new(cx0, by, cwp, 46.0 * u), "Crear personaje", Kind::Primary, name_ok && !busy, u) || (enter && name_ok && !busy);
    if let Some((msg, ok)) = st.flow.notice.clone() {
        notice_lines(&msg, ok, cx0, by + 66.0 * u, cwp, 17.0 * u, u);
    }
    if go {
        let f = &st.form;
        let head = f.heads.get(f.head_i).copied().unwrap_or(1);
        let (name, race, class, gender) = (f.name.clone(), f.race, f.class, f.gender);
        st.select_name = Some(name.clone());
        st.flow.create_character(game, &name, race, class, gender, head);
    }

    // ---- right column: description and numbers
    let panel_r = Rect::new(rx, top, rwp, sh - top - 24.0 * u);
    panel(panel_r, u);
    let f = &st.form;
    let preview = bal.as_ref().and_then(|b| creation_preview(b, f.race, f.class));
    let mut y = panel_r.y + 40.0 * u;
    let px = panel_r.x + 20.0 * u;
    let pw = panel_r.w - 40.0 * u;
    title_fit_centered(&format!("{} {}", race_name(f.race), class_name(f.class)), panel_r.x + panel_r.w / 2.0, y, 32.0 * u, pw, Color::new(1.0, 0.92, 0.7, 1.0));
    y += 12.0 * u;
    rule(panel_r.x + panel_r.w / 2.0, y, pw, u);
    y += 26.0 * u;
    for (label, body_text) in [(race_name(f.race), race_text(f.race)), (class_name(f.class), class_text(f.class))] {
        text(label, px, y, 17.0 * u, GOLD_TEXT);
        y += 20.0 * u;
        for l in wrap(body_text, 16.5 * u, pw) {
            text(&l, px, y, 16.5 * u, TEXT);
            y += 19.5 * u;
        }
        y += 8.0 * u;
    }
    y += 6.0 * u;
    heading("ATRIBUTOS INICIALES", px, y, pw, u);
    y += 14.0 * u;
    if let Some(p) = &preview {
        attribute_rows(p, px, &mut y, pw, u);
        y += 4.0 * u;
        let chip_w = (pw - 2.0 * 8.0 * u) / 3.0;
        for (i, (label, value)) in [("Vida", p.hp.to_string()), ("Maná", p.mana.to_string()), ("Hab. libres", START_SKILL_POINTS.to_string())].into_iter().enumerate() {
            let r = Rect::new(px + i as f32 * (chip_w + 8.0 * u), y, chip_w, 46.0 * u);
            draw_rectangle(r.x, r.y, r.w, r.h, SLOT);
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, GOLD);
            text_centered(label, r.x + r.w / 2.0, r.y + 17.0 * u, 15.0 * u, MUTED);
            title_centered(&value, r.x + r.w / 2.0, r.y + 40.0 * u, 24.0 * u, if label == "Maná" && p.mana == 0 { MUTED } else { GOLD_TEXT });
        }
        y += 76.0 * u;
        heading("APTITUDES DE CLASE", px, y, pw, u);
        y += 12.0 * u;
        if let Some(b) = &bal {
            gauge_rows(&p.class, &class_gauge_max(b), px, &mut y, pw, u);
        }
    } else {
        text("No se pudo leer Balance.dat de los recursos.", px, y + 20.0 * u, 17.0 * u, BAD);
    }

    if busy {
        busy_overlay(st.flow.pending.as_ref().map(Pending::label).unwrap_or(""), u);
    } else {
        tooltip(&tip, u);
    }
    st.end(game, "create");
}

/// The head of the official head GRH, scaled into a small square (race list icons).
/// Head sprites are 27x64 with the head somewhere in the middle, so the visible part is
/// measured once per sprite and only that is drawn.
fn draw_head_icon(assets: &mut Assets, grh: u32, r: Rect, _u: f32) {
    thread_local! {
        static BOUNDS: RefCell<std::collections::HashMap<u32, Option<Rect>>> = RefCell::new(std::collections::HashMap::new());
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

fn attribute_rows(p: &Preview, x: f32, y: &mut f32, w: f32, u: f32) {
    for (i, name) in ATTRIBUTE_NAMES.iter().enumerate() {
        let v = p.attributes[i] as f32;
        let m = p.race_mods[i];
        text(name, x, *y + 17.0 * u, 17.0 * u, TEXT);
        let bar_r = Rect::new(x + 124.0 * u, *y + 3.0 * u, w - 124.0 * u - 92.0 * u, 18.0 * u);
        let c = if m > 0 { Color::new(0.45, 0.72, 0.38, 1.0) } else if m < 0 { Color::new(0.78, 0.38, 0.30, 1.0) } else { Color::new(0.72, 0.56, 0.25, 1.0) };
        bar(bar_r, v - 10.0, 14.0, c, "", u);
        title(&format!("{}", p.attributes[i]), bar_r.right() + 12.0 * u, *y + 19.0 * u, 20.0 * u, TEXT);
        let (mt, mc) = match m {
            0 => ("".to_string(), MUTED),
            m if m > 0 => (format!("+{m}"), GOOD),
            m => (format!("{m}"), BAD),
        };
        text(&mt, bar_r.right() + 50.0 * u, *y + 18.0 * u, 17.0 * u, mc);
        *y += 26.0 * u;
    }
}

fn gauge_rows(g: &ClassGauges, max: &ClassGauges, x: f32, y: &mut f32, w: f32, u: f32) {
    let rows = [
        ("Evasión", g.evasion, max.evasion),
        ("Ataque cuerpo a cuerpo", g.melee_attack, max.melee_attack),
        ("Daño con armas", g.melee_damage, max.melee_damage),
        ("Ataque a distancia", g.ranged_attack, max.ranged_attack),
        ("Vida por nivel", g.life_per_level, max.life_per_level),
    ];
    for (label, v, m) in rows {
        text(label, x, *y + 16.0 * u, 16.0 * u, TEXT);
        let r = Rect::new(x + 190.0 * u, *y + 4.0 * u, w - 190.0 * u, 14.0 * u);
        bar(r, v, m.max(0.0001), Color::new(0.55, 0.45, 0.2, 1.0), "", u);
        *y += 23.0 * u;
    }
}

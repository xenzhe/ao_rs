//! In-game HUD, floating over a full-screen world (docs/ESTILO_UI.md, `render::layout`):
//!
//! - **Deck** (bottom): life and mana orbs with animated liquid and a "recent damage" trail,
//!   the action bar (spells 1-9 with icon, cost, radial cooldown, drag to reorder; meditate
//!   and the citizen lock as state buttons), experience above and energy below.
//! - **Portrait** (bottom-left): medallion with the level seal, name, faction, hunger and
//!   thirst as small vessels that empty, gold; altered states as badges above it.
//! - **Window icons** (bottom-right): drawn icons with tooltip and hotkey.
//! - **Minimap** (top-right): round, centred on the player, ornamented ring, map name plaque,
//!   safe/unsafe zone, clock and weather, coordinates and fps.
//! - **Target** (top centre): what is under the mouse (or the last one hovered), and the
//!   armed spell while choosing a target.
//!
//! OWNER: Sonnet 4 (docs/P3_CONTRATO.md). Also holds the small shared state of the owner's
//! windows (textures of minimaps and world maps, spell hotkeys, options) behind plain
//! functions: [`spell_keys`], [`options`], [`bmp_texture`], [`minimap_texture`].
//!
//! Dev: `AO_NOHUD=1` hides the HUD (to measure its cost), `AO_FPSLOG=1` prints the average
//! fps every 2 s, `AO_HUD_DEMO=hp,mana,energy,food,water,exp` (percentages) overrides what
//! the HUD draws (screenshots; the real stats are untouched).
use super::fx_ui::{ease, toast, ToastKind};
use super::theme::{self, FontKind, Surface, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, TEXT, TEXT_DIM, TEXT_FAINT};
use super::widgets::{measure, measure_with, panel, text, text_with, tooltip};
use super::{Ui, WindowId};
use crate::assets::Assets;
use crate::game::Game;
use crate::render::layout::Layout;
use ao_formats::graphics::anchor;
use ao_protocol::{status, EntityKind};
use ao_rules::spells::SpellEffect;
use client_core::p3::hud as logic;
use client_core::p3::hud::{MapInfo, Mood, Options, Rgba, SpellKeys, Trail, WorldData};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::path::{Path, PathBuf};
use std::rc::Rc;

// ---------------------------------------------------------------------------------------
// Shared state of the owner's windows
// ---------------------------------------------------------------------------------------

/// World maps and names, read once from `Recursos/init`.
pub struct WorldInfo {
    pub data: WorldData,
    pub names: HashMap<u16, MapInfo>,
}

#[derive(Default)]
struct Shared {
    minimaps: HashMap<u16, Option<Texture2D>>,
    files: HashMap<PathBuf, Option<Texture2D>>,
    keys: Option<SpellKeys>,
    options: Option<Options>,
    world: Option<Rc<WorldInfo>>,
}

thread_local! {
    static SHARED: RefCell<Shared> = RefCell::new(Shared::default());
}

fn to_texture(img: &Rgba, filter: FilterMode) -> Texture2D {
    let t = Texture2D::from_rgba8(img.w as u16, img.h as u16, &img.data);
    t.set_filter(filter);
    t
}

/// Spell hotkeys (1-9), loaded once from `config/spellkeys.ini`.
pub fn spell_keys() -> SpellKeys {
    SHARED.with(|s| s.borrow_mut().keys.get_or_insert_with(SpellKeys::load).clone())
}

/// Stores and saves the spell hotkeys.
pub fn set_spell_keys(k: SpellKeys) {
    k.save();
    SHARED.with(|s| s.borrow_mut().keys = Some(k));
}

/// The player's options (in memory, loaded once from `config/options.ini`).
pub fn options() -> Options {
    SHARED.with(|s| s.borrow_mut().options.get_or_insert_with(Options::load).clone())
}

/// Stores and saves the options.
pub fn set_options(o: Options) {
    o.save();
    set_options_in_memory(o);
}

/// Stores the options without writing the file (while a slider is being dragged).
pub fn set_options_in_memory(o: Options) {
    SHARED.with(|s| s.borrow_mut().options = Some(o));
}

/// A BMP from disk as a cached texture (None if missing or unreadable).
pub fn bmp_texture(path: &Path, filter: FilterMode) -> Option<Texture2D> {
    if let Some(t) = SHARED.with(|s| s.borrow().files.get(path).cloned()) {
        return t;
    }
    let tex = std::fs::read(path).ok().and_then(|b| logic::decode_bmp(&b)).map(|i| to_texture(&i, filter));
    SHARED.with(|s| s.borrow_mut().files.insert(path.to_path_buf(), tex.clone()));
    tex
}

/// Minimap texture of the current map: the official image, or one generated from the map.
pub fn minimap_texture(game: &Game) -> Option<Texture2D> {
    let map = game.map.as_ref()?;
    if let Some(t) = SHARED.with(|s| s.borrow().minimaps.get(&map.number).cloned()) {
        return t;
    }
    let img = logic::load_minimap(&game.recursos, map.number).unwrap_or_else(|| logic::fallback_minimap(map));
    let tex = Some(to_texture(&img, FilterMode::Linear));
    SHARED.with(|s| s.borrow_mut().minimaps.insert(map.number, tex.clone()));
    tex
}

/// World maps grid and map names.
pub fn world_info(recursos: &Path) -> Rc<WorldInfo> {
    SHARED.with(|s| s.borrow_mut().world.get_or_insert_with(|| Rc::new(WorldInfo { data: WorldData::load(recursos), names: logic::load_map_names(recursos) })).clone())
}

/// Display name of a map: namemapa.dat, else the name inside the map, else its number.
pub fn map_title(info: &WorldInfo, number: u16, own_name: Option<&str>) -> String {
    match info.names.get(&number).map(|m| m.name.trim()).filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => own_name.filter(|n| !n.trim().is_empty()).map(str::to_string).unwrap_or_else(|| format!("Mapa {number}")),
    }
}

// ---------------------------------------------------------------------------------------
// Small drawing helpers (also used by the owner's windows)
// ---------------------------------------------------------------------------------------

pub const MANA_BLUE: Color = Color::new(0.14, 0.34, 0.88, 1.0);
pub const HP_RED: Color = Color::new(0.76, 0.12, 0.10, 1.0);
pub const EXP_GOLD: Color = Color::new(0.78, 0.62, 0.16, 1.0);
const STA_GREEN: Color = Color::new(0.36, 0.66, 0.24, 1.0);

/// 12 345 -> "12.345".
pub fn thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// A meter with the label inside: left text and right value, `h` tall.
pub fn meter(r: Rect, v: f32, max: f32, c: Color, left: &str, right: &str, u: f32) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.72));
    let f = if max > 0.0 { (v / max).clamp(0.0, 1.0) } else { 0.0 };
    draw_rectangle(r.x, r.y, r.w * f, r.h, c);
    draw_rectangle(r.x, r.y, r.w * f, r.h * 0.38, Color::new(1.0, 1.0, 1.0, 0.16));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, GOLD);
    let size = (r.h * 0.78).min(17.0 * u);
    let base = r.y + r.h / 2.0 + size * 0.32;
    text(left, r.x + 6.0 * u, base, size, TEXT);
    let d = measure(right, size);
    text(right, r.right() - d.width - 6.0 * u, base, size, WHITE);
}

/// Width of `s` in the title font (the body-font `measure` is narrower).
pub fn title_width(s: &str, size: f32) -> f32 {
    measure_with(FontKind::Title, s, size).width
}

fn hover(r: Rect) -> bool {
    r.contains(mouse_position().into())
}

fn alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

/// Polygon sides for a circle of radius `r` (smooth at any size, cheap when small).
fn sides(r: f32) -> u8 {
    (r * 0.9).clamp(16.0, 96.0) as u8
}

fn disc(c: Vec2, r: f32, col: Color) {
    draw_poly(c.x, c.y, sides(r), r, 0.0, col);
}

fn ring(c: Vec2, r: f32, w: f32, col: Color) {
    draw_poly_lines(c.x, c.y, sides(r), r, 0.0, w, col);
}

/// Disc with a radial gradient (one mesh).
fn radial(c: Vec2, r: f32, inner: Color, outer: Color) {
    let n = sides(r) as u16;
    let mut v = Vec::with_capacity(n as usize + 2);
    v.push(Vertex::new(c.x, c.y, 0.0, 0.0, 0.0, inner));
    for i in 0..=n {
        let a = i as f32 / n as f32 * TAU;
        v.push(Vertex::new(c.x + a.cos() * r, c.y + a.sin() * r, 0.0, 0.0, 0.0, outer));
    }
    let idx: Vec<u16> = (0..n).flat_map(|i| [0, i + 1, i + 2]).collect();
    draw_mesh(&Mesh { vertices: v, indices: idx, texture: None });
}

/// Ring band from `r0` (colour `c0`) to `r1` (colour `c1`): inner shades and soft rims in
/// one mesh.
fn annulus(c: Vec2, r0: f32, r1: f32, c0: Color, c1: Color) {
    let n = sides(r1) as u16;
    let mut v = Vec::with_capacity(2 * n as usize + 2);
    for i in 0..=n {
        let a = i as f32 / n as f32 * TAU;
        let d = vec2(a.cos(), a.sin());
        v.push(Vertex::new(c.x + d.x * r0, c.y + d.y * r0, 0.0, 0.0, 0.0, c0));
        v.push(Vertex::new(c.x + d.x * r1, c.y + d.y * r1, 0.0, 0.0, 0.0, c1));
    }
    let idx: Vec<u16> = (0..n).flat_map(|i| [2 * i, 2 * i + 1, 2 * i + 3, 2 * i, 2 * i + 3, 2 * i + 2]).collect();
    draw_mesh(&Mesh { vertices: v, indices: idx, texture: None });
}

/// Soft glow around a point (radial gradient to transparent).
fn glow(c: Vec2, r: f32, col: Color) {
    radial(c, r, col, alpha(col, 0.0));
}

/// Pie (clockwise from 12 o'clock) covering `frac` of the disc.
fn pie(c: Vec2, r: f32, frac: f32, col: Color) {
    if frac <= 0.0 {
        return;
    }
    let n = ((sides(r) as f32) * frac).ceil().max(2.0) as u16;
    let mut v = Vec::with_capacity(n as usize + 2);
    v.push(Vertex::new(c.x, c.y, 0.0, 0.0, 0.0, col));
    for i in 0..=n {
        let a = -PI / 2.0 + i as f32 / n as f32 * frac * TAU;
        v.push(Vertex::new(c.x + a.cos() * r, c.y + a.sin() * r, 0.0, 0.0, 0.0, col));
    }
    let idx: Vec<u16> = (0..n).flat_map(|i| [0, i + 1, i + 2]).collect();
    draw_mesh(&Mesh { vertices: v, indices: idx, texture: None });
}

/// Vertical gradient rectangle.
fn vgrad(r: Rect, top: Color, bottom: Color) {
    let v = vec![
        Vertex::new(r.x, r.y, 0.0, 0.0, 0.0, top),
        Vertex::new(r.right(), r.y, 0.0, 0.0, 0.0, top),
        Vertex::new(r.right(), r.bottom(), 0.0, 0.0, 0.0, bottom),
        Vertex::new(r.x, r.bottom(), 0.0, 0.0, 0.0, bottom),
    ];
    draw_mesh(&Mesh { vertices: v, indices: vec![0, 1, 2, 0, 2, 3], texture: None });
}

/// Horizontal gradient rectangle.
fn hgrad(r: Rect, left: Color, right: Color) {
    let v = vec![
        Vertex::new(r.x, r.y, 0.0, 0.0, 0.0, left),
        Vertex::new(r.right(), r.y, 0.0, 0.0, 0.0, right),
        Vertex::new(r.right(), r.bottom(), 0.0, 0.0, 0.0, right),
        Vertex::new(r.x, r.bottom(), 0.0, 0.0, 0.0, left),
    ];
    draw_mesh(&Mesh { vertices: v, indices: vec![0, 1, 2, 0, 2, 3], texture: None });
}

/// Liquid inside a circle up to `level` (0..1), with a moving surface. Columns of one mesh:
/// colour from `top` (at the surface) to `bottom`, and a bright rim on the surface.
#[allow(clippy::too_many_arguments)]
fn liquid(c: Vec2, r: f32, level: f32, top: Color, bottom: Color, t: f32, wave: f32, surface: Color) {
    if level <= 0.0 {
        return;
    }
    let step = (r / 36.0).max(1.0);
    let lvl = c.y + r - 2.0 * r * level.clamp(0.0, 1.0);
    let calm = if level >= 0.995 { 0.0 } else { wave };
    let mut v: Vec<Vertex> = Vec::with_capacity(((2.0 * r / step) as usize + 2) * 8);
    let mut idx: Vec<u16> = Vec::with_capacity(v.capacity() * 2);
    let mut quad = |x0: f32, x1: f32, y0: f32, y1: f32, a: Color, b: Color| {
        let i = v.len() as u16;
        v.extend([Vertex::new(x0, y0, 0.0, 0.0, 0.0, a), Vertex::new(x1, y0, 0.0, 0.0, 0.0, a), Vertex::new(x1, y1, 0.0, 0.0, 0.0, b), Vertex::new(x0, y1, 0.0, 0.0, 0.0, b)]);
        idx.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    };
    let mut x = -r;
    while x < r {
        let x1 = (x + step).min(r);
        let dx = (x + x1) / 2.0;
        let half = (r * r - dx * dx).max(0.0).sqrt();
        let (ytop, ybot) = (c.y - half, c.y + half);
        let k = dx / r;
        let w = (k * 5.0 + t * 2.1).sin() * calm + (k * 11.0 - t * 3.3).sin() * calm * 0.35;
        let y0 = (lvl + w).max(ytop);
        if y0 < ybot {
            let ft = ((y0 - (c.y - r)) / (2.0 * r)).clamp(0.0, 1.0);
            quad(c.x + x, c.x + x1, y0, ybot, mix(top, bottom, ft * 0.6), bottom);
            if y0 > ytop + 1.0 {
                quad(c.x + x, c.x + x1, y0, y0 + (r * 0.035).max(1.2), surface, alpha(surface, 0.0));
            }
        }
        x = x1;
    }
    draw_mesh(&Mesh { vertices: v, indices: idx, texture: None });
}

/// Engraved text in the title font, centred.
fn title_centered(s: &str, cx: f32, y: f32, size: f32, c: Color) {
    text_with(FontKind::Title, s, cx - title_width(s, size) / 2.0, y, size, c);
}

fn body_centered(s: &str, cx: f32, y: f32, size: f32, c: Color) {
    text(s, cx - measure(s, size).width / 2.0, y, size, c);
}

fn hour_label(hour: f32) -> &'static str {
    match hour as i32 {
        5..=7 => "Amanecer",
        8..=11 => "Mañana",
        12..=14 => "Mediodía",
        15..=18 => "Tarde",
        19..=20 => "Atardecer",
        _ => "Noche",
    }
}

fn weather_label(w: (u8, bool), night: bool) -> &'static str {
    match w {
        (_, true) => "Lluvia",
        (f, _) if f > 60 => "Niebla",
        (f, _) if f > 20 => "Nublado",
        _ if night => "Despejado",
        _ => "Soleado",
    }
}

// ---------------------------------------------------------------------------------------
// Icons (vector, drawn with primitives)
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Icon {
    Bag,
    Book,
    Helm,
    Shield,
    Party,
    Scroll,
    Compass,
    Gear,
    Swords,
    Coins,
    Crown,
    Anvil,
    Lotus,
    Lock(bool),
    Fire,
    Bolt,
    Cross,
}

fn line(a: Vec2, b: Vec2, w: f32, c: Color) {
    draw_line(a.x, a.y, b.x, b.y, w, c);
}

/// Draws `icon` centred on `c`, fitting a square of side `s`.
fn icon(kind: Icon, c: Vec2, s: f32, col: Color) {
    let h = s / 2.0;
    let w = (s * 0.075).max(1.3);
    let dark = Color::new(0.0, 0.0, 0.0, 0.55 * col.a);
    let p = |x: f32, y: f32| c + vec2(x * h, y * h);
    match kind {
        Icon::Bag => {
            disc(p(0.0, 0.22), h * 0.62, col);
            draw_triangle(p(-0.42, -0.1), p(0.42, -0.1), p(0.0, 0.4), col);
            draw_rectangle(c.x - h * 0.2, c.y - h * 0.48, h * 0.4, h * 0.3, col);
            line(p(-0.36, -0.2), p(0.36, -0.2), w * 1.3, dark);
            line(p(-0.28, -0.72), p(0.28, -0.72), w * 1.6, col);
            disc(p(0.0, 0.3), h * 0.12, dark);
        }
        Icon::Book => {
            draw_triangle(p(-0.9, -0.55), p(0.0, -0.4), p(0.0, 0.65), col);
            draw_triangle(p(-0.9, -0.55), p(-0.9, 0.5), p(0.0, 0.65), col);
            draw_triangle(p(0.9, -0.55), p(0.0, -0.4), p(0.0, 0.65), alpha(col, 0.8));
            draw_triangle(p(0.9, -0.55), p(0.9, 0.5), p(0.0, 0.65), alpha(col, 0.8));
            line(p(0.0, -0.4), p(0.0, 0.65), w, dark);
            for i in 0..3 {
                let y = -0.25 + i as f32 * 0.25;
                line(p(-0.7, y), p(-0.15, y + 0.08), w * 0.7, dark);
                line(p(0.7, y), p(0.15, y + 0.08), w * 0.7, dark);
            }
        }
        Icon::Helm => {
            disc(p(0.0, -0.05), h * 0.62, col);
            draw_rectangle(c.x - h * 0.62, c.y - h * 0.05, h * 1.24, h * 0.6, col);
            draw_rectangle(c.x - h * 0.42, c.y + h * 0.05, h * 0.84, h * 0.14, dark);
            draw_rectangle(c.x - h * 0.07, c.y + h * 0.05, h * 0.14, h * 0.5, dark);
            draw_triangle(p(-0.15, -0.66), p(0.15, -0.66), p(0.0, -0.95), col);
        }
        Icon::Shield => {
            let a = p(-0.7, -0.75);
            let b = p(0.7, -0.75);
            draw_rectangle(a.x, a.y, b.x - a.x, h * 0.75, col);
            draw_triangle(p(-0.7, -0.01), p(0.7, -0.01), p(0.0, 0.92), col);
            line(p(0.0, -0.6), p(0.0, 0.62), w * 1.4, dark);
            line(p(-0.5, -0.25), p(0.5, -0.25), w * 1.4, dark);
        }
        Icon::Party => {
            for (x, y, k) in [(-0.5, 0.0, 0.8), (0.5, 0.0, 0.8), (0.0, 0.1, 1.0)] {
                disc(p(x, y - 0.42 * k), h * 0.24 * k, col);
                disc(p(x, y + 0.5), h * 0.42 * k, col);
            }
            draw_rectangle(c.x - h, c.y + h * 0.62, s, h * 0.5, Color::new(0.0, 0.0, 0.0, 0.0));
        }
        Icon::Scroll => {
            draw_rectangle(c.x - h * 0.55, c.y - h * 0.6, h * 1.1, h * 1.2, col);
            disc(p(-0.55, -0.6), h * 0.18, col);
            disc(p(0.55, 0.6), h * 0.18, col);
            draw_rectangle(c.x - h * 0.75, c.y - h * 0.78, h * 0.36, h * 0.18, col);
            for i in 0..3 {
                let y = -0.3 + i as f32 * 0.28;
                line(p(-0.35, y), p(0.35, y), w * 0.8, dark);
            }
        }
        Icon::Compass => {
            ring(c, h * 0.8, w * 1.2, col);
            draw_triangle(p(0.0, -0.7), p(0.2, 0.0), p(-0.2, 0.0), col);
            draw_triangle(p(0.0, 0.7), p(0.2, 0.0), p(-0.2, 0.0), alpha(col, 0.45));
            disc(c, h * 0.1, dark);
        }
        Icon::Gear => {
            for i in 0..8 {
                let a = i as f32 / 8.0 * TAU;
                let d = vec2(a.cos(), a.sin());
                line(c + d * h * 0.4, c + d * h * 0.85, h * 0.28, col);
            }
            disc(c, h * 0.6, col);
            disc(c, h * 0.24, dark);
        }
        Icon::Swords => {
            for sx in [-1.0f32, 1.0] {
                line(p(-0.75 * sx, -0.75), p(0.55 * sx, 0.55), w * 1.6, col);
                line(p(0.3 * sx, 0.75), p(0.75 * sx, 0.3), w * 1.6, col);
                disc(p(0.75 * sx, 0.75), h * 0.1, col);
            }
        }
        Icon::Coins => {
            for (i, y) in [0.45f32, 0.15, -0.15].iter().enumerate() {
                let x = if i == 1 { 0.12 } else { -0.06 };
                draw_ellipse(c.x + x * h, c.y + y * h, h * 0.62, h * 0.22, 0.0, col);
                draw_ellipse_lines(c.x + x * h, c.y + y * h, h * 0.62, h * 0.22, 0.0, w * 0.7, dark);
            }
            draw_ellipse(c.x - 0.06 * h, c.y - 0.45 * h, h * 0.62, h * 0.22, 0.0, col);
        }
        Icon::Crown => {
            draw_rectangle(c.x - h * 0.75, c.y + h * 0.15, h * 1.5, h * 0.45, col);
            for x in [-0.75f32, 0.0, 0.75] {
                draw_triangle(p(x - 0.3, 0.2), p(x + 0.3, 0.2), p(x, -0.6), col);
                disc(p(x, -0.66), h * 0.1, col);
            }
        }
        Icon::Anvil => {
            draw_rectangle(c.x - h * 0.6, c.y - h * 0.35, h * 1.3, h * 0.3, col);
            draw_triangle(p(-0.6, -0.35), p(-0.6, -0.05), p(-0.95, -0.35), col);
            draw_rectangle(c.x - h * 0.22, c.y - h * 0.05, h * 0.44, h * 0.4, col);
            draw_rectangle(c.x - h * 0.55, c.y + h * 0.35, h * 1.1, h * 0.25, col);
        }
        Icon::Lotus => {
            for (a, k) in [(-0.9f32, 0.75), (-0.45, 0.9), (0.0, 1.0), (0.45, 0.9), (0.9, 0.75)] {
                let d = vec2(a.sin(), -a.cos());
                let tip = p(0.0, 0.35) + d * h * 0.85 * k;
                let side = vec2(d.y, -d.x) * h * 0.2;
                draw_triangle(p(0.0, 0.35) + side, p(0.0, 0.35) - side, tip, alpha(col, 0.55 + 0.45 * k));
            }
            draw_ellipse(c.x, c.y + h * 0.5, h * 0.7, h * 0.16, 0.0, col);
        }
        Icon::Lock(closed) => {
            let body = Rect::new(c.x - h * 0.55, c.y - h * 0.05, h * 1.1, h * 0.8);
            draw_rectangle(body.x, body.y, body.w, body.h, col);
            let sw = h * 0.36;
            let top = body.y - h * 0.45;
            line(vec2(c.x + sw, body.y), vec2(c.x + sw, top + sw * 0.6), w * 1.6, col);
            if closed {
                line(vec2(c.x - sw, body.y), vec2(c.x - sw, top + sw * 0.6), w * 1.6, col);
                draw_arc(c.x, top + sw * 0.6, 12, sw - w * 0.8, 180.0, w * 1.6, 180.0, col);
            } else {
                draw_arc(c.x + sw * 2.0, top + sw * 0.6, 12, sw - w * 0.8, 180.0, w * 1.6, 180.0, col);
                line(vec2(c.x + sw * 3.0 - w * 0.0, top + sw * 0.6), vec2(c.x + sw * 3.0, top + sw), w * 1.6, col);
            }
            disc(vec2(c.x, body.y + body.h * 0.42), h * 0.12, dark);
        }
        Icon::Fire => {
            draw_triangle(p(-0.55, 0.2), p(0.55, 0.2), p(0.1, -0.95), col);
            draw_triangle(p(-0.55, 0.2), p(0.0, 0.2), p(-0.45, -0.45), col);
            disc(p(0.0, 0.3), h * 0.58, col);
            disc(p(0.0, 0.42), h * 0.3, Color::new(1.0, 0.95, 0.6, col.a));
        }
        Icon::Bolt => {
            let pts = [p(0.25, -0.95), p(-0.45, 0.1), p(-0.02, 0.1), p(-0.25, 0.95), p(0.5, -0.15), p(0.05, -0.15)];
            draw_triangle(pts[0], pts[1], pts[2], col);
            draw_triangle(pts[0], pts[2], pts[5], col);
            draw_triangle(pts[3], pts[4], pts[5], col);
            draw_triangle(pts[3], pts[5], pts[2], col);
        }
        Icon::Cross => {
            let t = h * 0.42;
            draw_rectangle(c.x - t / 2.0, c.y - h * 0.8, t, h * 1.6, col);
            draw_rectangle(c.x - h * 0.8, c.y - t / 2.0, h * 1.6, t, col);
        }
    }
}

// ---------------------------------------------------------------------------------------
// HUD state
// ---------------------------------------------------------------------------------------

#[derive(Default)]
struct HudState {
    hp: Trail,
    mana: Trail,
    level: u32,
    level_at: f64,
    /// Action-bar drag: (key, press position, dragging).
    drag: Option<(usize, Vec2, bool)>,
    /// Last entity hovered in the world, and when.
    target: Option<(u32, f64)>,
    fps_acc: (f64, u32, f64),
}

thread_local! {
    static HUD: RefCell<HudState> = RefCell::new(HudState::default());
}

/// What the HUD draws (the real stats, or the `AO_HUD_DEMO` override for screenshots).
struct Vitals {
    hp: (i32, i32),
    mana: (i32, i32),
    sta: (i32, i32),
    food: i32,
    water: i32,
    exp: (u32, u32),
}

fn vitals(game: &Game) -> Vitals {
    let l = &game.local;
    let mut v = Vitals {
        hp: (game.stats.hp.get(), game.stats.max_hp.get()),
        mana: (game.stats.mana.get(), game.stats.max_mana.get()),
        sta: (l.sta, l.max_sta.max(1)),
        food: l.food,
        water: l.water,
        exp: (game.stats.exp, game.stats.next_exp.max(1)),
    };
    if let Ok(d) = std::env::var("AO_HUD_DEMO") {
        let p: Vec<f32> = d.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        let pct = |i: usize, max: i32| p.get(i).map(|f| (max as f32 * f / 100.0) as i32);
        v.hp.0 = pct(0, v.hp.1).unwrap_or(v.hp.0);
        v.mana.0 = pct(1, v.mana.1).unwrap_or(v.mana.0);
        v.sta.0 = pct(2, v.sta.1).unwrap_or(v.sta.0);
        v.food = pct(3, 100).unwrap_or(v.food);
        v.water = pct(4, 100).unwrap_or(v.water);
        v.exp.0 = pct(5, v.exp.1 as i32).map(|x| x as u32).unwrap_or(v.exp.0);
    }
    v
}

thread_local! {
    /// CPU seconds spent drawing HUD pieces since the last report (`AO_FPSLOG`).
    static COST: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// Dev: `AO_HUDSKIP=orbs,map,...` leaves HUD pieces out (to find what costs).
pub(crate) fn skip(piece: &str) -> bool {
    static SKIP: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SKIP.get_or_init(|| std::env::var("AO_HUDSKIP").unwrap_or_default()).split(',').any(|p| p == piece)
}

/// Adds CPU time spent by a HUD piece (console, name plates) to the `AO_FPSLOG` report.
pub(crate) fn add_cost(secs: f64) {
    COST.set(COST.get() + secs);
}

/// Whether `AO_FPSLOG` is on (then the HUD pieces time themselves).
pub(crate) fn timing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("AO_FPSLOG").is_some())
}

fn fps_log() {
    if !timing() {
        return;
    }
    let now = get_time();
    HUD.with(|h| {
        let mut h = h.borrow_mut();
        let (t0, n, acc) = &mut h.fps_acc;
        if *t0 == 0.0 {
            *t0 = now;
        }
        *n += 1;
        *acc += get_frame_time() as f64;
        if now - *t0 >= 2.0 {
            let cost = COST.get() / *n as f64 * 1000.0;
            eprintln!("HUDFPS avg {:.1} fps ({:.2} ms/frame), HUD cpu {:.3} ms/frame, nohud={}", *n as f64 / *acc, *acc / *n as f64 * 1000.0, cost, std::env::var_os("AO_NOHUD").is_some());
            COST.set(0.0);
            *t0 = now;
            *n = 0;
            *acc = 0.0;
        }
    });
}

// ---------------------------------------------------------------------------------------
// HUD
// ---------------------------------------------------------------------------------------

/// The whole HUD for this frame.
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, lay: &Layout, hour: f32, pixel_perfect: bool) {
    fps_log();
    spell_hotkeys(ui, game);
    if std::env::var_os("AO_NOHUD").is_some() {
        return;
    }
    let t0 = timing().then(get_time);
    let mut tip: Vec<String> = Vec::new();
    let v = vitals(game);
    level_watch(game);
    let deck = deck_geometry(lay);
    let on = |piece: &str| !skip(piece);
    if on("target") {
        target_plaque(game, lay);
    }
    if on("deck") {
        deck_plate(&deck, lay);
        exp_and_energy(&deck, &v, game, lay, &mut tip);
    }
    if on("bar") {
        action_bar(ui, game, &deck, lay, &mut tip);
    }
    if on("orbs") {
        orbs(&deck, &v, game, lay, &mut tip);
    }
    if on("portrait") {
        portrait_cluster(ui, game, assets, &v, lay, &mut tip);
    }
    if on("icons") {
        window_icons(ui, game, &deck, lay, &mut tip);
    }
    if on("map") {
        minimap(ui, game, lay, hour, pixel_perfect, &mut tip);
    }
    tooltip(&tip, lay.u);
    if let Some(t0) = t0 {
        add_cost(get_time() - t0);
    }
}

/// Keys 1-9 arm the spell bound to them. They replace the default of `input.rs` (which
/// picks by book position): while one is pressed the keyboard is reported as captured so the
/// world input leaves it alone.
fn spell_hotkeys(ui: &mut Ui, game: &mut Game) {
    if ui.keyboard_captured {
        return;
    }
    const KEYS: [KeyCode; 9] = [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4, KeyCode::Key5, KeyCode::Key6, KeyCode::Key7, KeyCode::Key8, KeyCode::Key9];
    let Some(k) = KEYS.iter().position(|k| is_key_pressed(*k)) else { return };
    ui.keyboard_captured = true;
    let id = spell_keys().spell_on(k, &game.local.spell_slots);
    if id != 0 {
        game.arm_spell(id);
    }
}

/// Level-up: a toast and a burst on the seal (not on entering the game).
fn level_watch(game: &Game) {
    let lvl = game.stats.level;
    if game.me().is_none() || lvl == 0 {
        return;
    }
    HUD.with(|h| {
        let mut h = h.borrow_mut();
        if h.level != 0 && lvl > h.level {
            toast(format!("¡Has alcanzado el nivel {lvl}!"), ToastKind::Good);
            h.level_at = get_time();
        }
        h.level = lvl;
    });
}

/// Where the deck pieces go.
struct Deck {
    /// Centre x of the deck (the view's centre).
    cx: f32,
    /// Plate between the orbs.
    plate: Rect,
    /// The 9 spell slots, then meditate and the lock.
    slots: [Rect; 11],
    hp_c: Vec2,
    mana_c: Vec2,
    orb_r: f32,
    exp: Rect,
    sta: Rect,
}

const SLOT: f32 = 44.0;
const GAP: f32 = 5.0;

fn deck_geometry(lay: &Layout) -> Deck {
    let u = lay.u;
    let sh = lay.bottom.bottom();
    let cx = lay.view.center().x.round();
    let s = SLOT * u;
    let g = GAP * u;
    let bar_w = 9.0 * s + 8.0 * g + 14.0 * u + 2.0 * s * 0.86 + g;
    let plate = Rect::new((cx - bar_w / 2.0 - 16.0 * u).round(), (sh - 84.0 * u).round(), (bar_w + 32.0 * u).round(), (78.0 * u).round());
    let sy = (plate.y + 19.0 * u).round();
    let mut slots = [Rect::default(); 11];
    let mut x = cx - bar_w / 2.0;
    for r in slots.iter_mut().take(9) {
        *r = Rect::new(x.round(), sy, s.round(), s.round());
        x += s + g;
    }
    x += 14.0 * u - g;
    let ss = (s * 0.86).round();
    slots[9] = Rect::new(x.round(), sy + ((s - ss) / 2.0).round(), ss, ss);
    slots[10] = Rect::new((x + ss + g).round(), sy + ((s - ss) / 2.0).round(), ss, ss);
    let orb_r = (48.0 * u).round();
    let oy = sh - 62.0 * u;
    Deck {
        cx,
        plate,
        slots,
        hp_c: vec2(plate.x - orb_r + 10.0 * u, oy),
        mana_c: vec2(plate.right() + orb_r - 10.0 * u, oy),
        orb_r,
        exp: Rect::new(slots[0].x, plate.y + 8.0 * u, slots[10].right() - slots[0].x, 5.0 * u),
        sta: Rect::new(slots[0].x, sy + s + 7.0 * u, slots[10].right() - slots[0].x, 4.0 * u),
    }
}

/// The leather bridge between the orbs, with a gold rim on top.
fn deck_plate(d: &Deck, lay: &Layout) {
    let u = lay.u;
    let p = d.plate;
    // Shadow and leather, darker towards the bottom.
    draw_rectangle(p.x + 3.0 * u, p.y + 4.0 * u, p.w, p.h, Color::new(0.0, 0.0, 0.0, 0.35));
    theme::surface(p, Surface::Leather, u);
    vgrad(p, Color::new(1.0, 0.85, 0.6, 0.04), Color::new(0.0, 0.0, 0.0, 0.45));
    // Bevelled top rim with a centre stud.
    draw_rectangle(p.x, p.y, p.w, 2.5 * u, GOLD);
    draw_line(p.x, p.y + 0.5, p.right(), p.y + 0.5, 1.0, Color::new(1.0, 0.9, 0.6, 0.7));
    draw_line(p.x, p.y + 3.5 * u, p.right(), p.y + 3.5 * u, 1.0, Color::new(0.0, 0.0, 0.0, 0.6));
    draw_line(p.x + 6.0 * u, p.y + 6.5 * u, p.right() - 6.0 * u, p.y + 6.5 * u, 1.0, alpha(GOLD_DIM, 0.6));
    let c = vec2(d.cx, p.y + 1.0 * u);
    let k = 7.0 * u;
    draw_triangle(c + vec2(0.0, -k), c + vec2(k, 0.0), c + vec2(0.0, k), GOLD);
    draw_triangle(c + vec2(0.0, -k), c + vec2(-k, 0.0), c + vec2(0.0, k), GOLD);
    disc(c, 2.0 * u, GOLD_BRIGHT);
    // Side rims down to the bottom.
    draw_line(p.x + 0.5, p.y, p.x + 0.5, p.bottom(), 1.5 * u, GOLD_DIM);
    draw_line(p.right() - 0.5, p.y, p.right() - 0.5, p.bottom(), 1.5 * u, GOLD_DIM);
}

/// Experience (above the slots, gold) and energy (below, green), thin and quiet.
fn exp_and_energy(d: &Deck, v: &Vitals, game: &Game, lay: &Layout, tip: &mut Vec<String>) {
    let u = lay.u;
    let fe = (v.exp.0 as f32 / v.exp.1 as f32).clamp(0.0, 1.0);
    let fs = (v.sta.0 as f32 / v.sta.1.max(1) as f32).clamp(0.0, 1.0);
    for (r, f, c) in [(d.exp, fe, EXP_GOLD), (d.sta, fs, STA_GREEN)] {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.6));
        if f > 0.0 {
            hgrad(Rect::new(r.x, r.y, r.w * f, r.h), mix(c, BLACK, 0.35), mix(c, WHITE, 0.25));
            draw_rectangle(r.x, r.y, r.w * f, r.h * 0.4, Color::new(1.0, 1.0, 1.0, 0.18));
            glow(vec2(r.x + r.w * f, r.y + r.h / 2.0), r.h * 2.2, alpha(mix(c, WHITE, 0.4), 0.55));
        }
        // Quarter ticks.
        for i in 1..4 {
            let x = r.x + r.w * i as f32 / 4.0;
            draw_line(x, r.y, x, r.bottom(), 1.0, Color::new(0.0, 0.0, 0.0, 0.5));
        }
    }
    let hit = |r: Rect| hover(Rect::new(r.x, r.y - 4.0 * u, r.w, r.h + 8.0 * u));
    if hit(d.exp) {
        *tip = vec![
            "Experiencia".into(),
            format!("{} / {}  ({:.1} %)", thousands(v.exp.0), thousands(v.exp.1), fe * 100.0),
            format!("Faltan {} para el nivel {}", thousands(v.exp.1.saturating_sub(v.exp.0)), game.stats.level + 1),
        ];
        let s = format!("{:.1} %", fe * 100.0);
        body_centered(&s, d.exp.center().x, d.exp.y - 4.0 * u, 14.0 * u, GOLD_TEXT);
    }
    if hit(d.sta) {
        *tip = vec!["Energía".into(), format!("{} / {}", v.sta.0, v.sta.1), "~Se gasta al trabajar y luchar; descansa para recuperarla.".into()];
    }
}

/// One orb: glass, liquid with a trailing "recent damage" level, gloss, metal ring.
#[allow(clippy::too_many_arguments)]
fn orb(c: Vec2, r: f32, tr: &Trail, deep: Color, bright: Color, value: i32, max: i32, label: &str, u: f32) {
    let now = get_time();
    let t = now as f32;
    // Outer shadow and back glass.
    glow(c + vec2(0.0, 4.0 * u), r * 1.35, Color::new(0.0, 0.0, 0.0, 0.65));
    radial(c, r, Color::new(0.06, 0.045, 0.04, 1.0), Color::new(0.015, 0.01, 0.01, 1.0));
    // Trail (pale) under the liquid.
    if tr.trail > tr.value + 0.002 {
        liquid(c, r, tr.trail, Color::new(1.0, 0.92, 0.75, 0.55), Color::new(0.9, 0.75, 0.5, 0.35), t, 0.0, Color::new(1.0, 1.0, 1.0, 0.0));
    }
    let low = tr.value < 0.25;
    let pulse = if low { 0.5 + 0.5 * (t * 6.0).sin() } else { 0.0 };
    let surface = mix(bright, WHITE, 0.55);
    liquid(c, r, tr.value, mix(bright, WHITE, 0.08 * pulse), deep, t, 2.2 * u, alpha(surface, 0.8));
    // Inner glow from the liquid and a darker edge (depth).
    glow(c + vec2(0.0, r * 0.35), r * 0.9, alpha(bright, 0.12 * tr.value.min(1.0)));
    annulus(c, r * 0.8, r, Color::new(0.0, 0.0, 0.0, 0.0), Color::new(0.0, 0.0, 0.0, 0.55));
    // Gloss.
    draw_ellipse(c.x - r * 0.22, c.y - r * 0.5, r * 0.46, r * 0.2, -18.0, Color::new(1.0, 1.0, 1.0, 0.13));
    draw_ellipse(c.x - r * 0.36, c.y - r * 0.56, r * 0.12, r * 0.06, -18.0, Color::new(1.0, 1.0, 1.0, 0.35));
    // Damage flash / heal shimmer.
    let flash = tr.flash(now, 0.35);
    if flash > 0.0 {
        ring(c, r + 6.0 * u, 6.0 * u * flash, Color::new(1.0, 0.25, 0.15, 0.7 * flash));
    }
    let gain = (1.0 - (now - tr.gain) / 0.5).clamp(0.0, 1.0) as f32;
    if gain > 0.0 {
        glow(c, r * 1.25, alpha(mix(bright, WHITE, 0.5), 0.3 * gain));
    }
    if low {
        glow(c, r * 1.4, alpha(bright, 0.18 * pulse));
    }
    // Metal ring with studs.
    ring(c, r + 3.5 * u, 7.0 * u, Color::new(0.0, 0.0, 0.0, 0.8));
    ring(c, r + 3.0 * u, 5.0 * u, GOLD_DIM);
    ring(c, r + 3.0 * u, 2.2 * u, GOLD);
    draw_arc(c.x, c.y, 48, r + 1.8 * u, 200.0, 1.4 * u, 110.0, Color::new(1.0, 0.92, 0.65, 0.6));
    ring(c, r + 0.5, 1.0, Color::new(0.0, 0.0, 0.0, 0.8));
    for i in 0..4 {
        let a = PI / 4.0 + i as f32 * PI / 2.0;
        let q = c + vec2(a.cos(), a.sin()) * (r + 3.0 * u);
        disc(q, 3.4 * u, GOLD);
        disc(q - vec2(0.8, 0.8) * u, 1.3 * u, GOLD_BRIGHT);
    }
    // Value: big number, max small under it.
    let n = value.max(0).to_string();
    title_centered(&n, c.x, c.y + 7.0 * u, 26.0 * u, Color::new(1.0, 0.97, 0.92, 1.0));
    body_centered(&format!("/ {max}"), c.x, c.y + 24.0 * u, 14.0 * u, Color::new(0.95, 0.9, 0.85, 0.75));
    body_centered(label, c.x, c.y - 15.0 * u, 12.0 * u, Color::new(1.0, 0.95, 0.85, 0.6));
}

fn orbs(d: &Deck, v: &Vitals, _game: &Game, lay: &Layout, tip: &mut Vec<String>) {
    let u = lay.u;
    let now = get_time();
    let dt = get_frame_time().min(0.1);
    let (hp, mana) = HUD.with(|h| {
        let mut h = h.borrow_mut();
        h.hp.update(v.hp.0 as f32 / v.hp.1.max(1) as f32, now, dt);
        h.mana.update(v.mana.0 as f32 / v.mana.1.max(1) as f32, now, dt);
        (h.hp, h.mana)
    });
    orb(d.hp_c, d.orb_r, &hp, Color::new(0.22, 0.02, 0.02, 1.0), Color::new(0.86, 0.14, 0.10, 1.0), v.hp.0, v.hp.1, "VIDA", u);
    orb(d.mana_c, d.orb_r, &mana, Color::new(0.02, 0.05, 0.25, 1.0), Color::new(0.20, 0.45, 1.0, 1.0), v.mana.0, v.mana.1, "MANÁ", u);
    if d.hp_c.distance(mouse_position().into()) < d.orb_r {
        *tip = vec!["Vida".into(), format!("{} / {}", v.hp.0, v.hp.1), "~Si llega a cero, caes y vagas como espíritu.".into()];
    }
    if d.mana_c.distance(mouse_position().into()) < d.orb_r {
        *tip = vec!["Maná".into(), format!("{} / {}", v.mana.0, v.mana.1), "~Medita (tecla de meditar o el loto) para recuperarlo.".into()];
    }
}

/// Spells 1-9 (icon, short name, cost, radial cooldown; drag to reorder), meditate and lock.
fn action_bar(ui: &mut Ui, game: &mut Game, d: &Deck, lay: &Layout, tip: &mut Vec<String>) {
    let u = lay.u;
    let now = get_time();
    let keys = spell_keys();
    let book = game.local.spell_slots.clone();
    let armed = game.casting.then(|| game.spells.get(game.selected).map(|s| s.id)).flatten();
    let mana = game.stats.mana.get();
    let mouse: Vec2 = mouse_position().into();
    let drag = HUD.with(|h| h.borrow().drag);
    for (k, &r) in d.slots.iter().enumerate().take(9) {
        let id = keys.spell_on(k, &book);
        let def = game.spells.iter().find(|s| s.id == id).cloned();
        let hov = hover(r);
        let h = ease(super::fx_ui::key_of(r), hov && def.is_some(), 0.08);
        let is_armed = armed == Some(id) && id != 0;
        // Socket.
        // Socket: carved in (shadow from the top), two meshes instead of a line stack.
        vgrad(r, Color::new(0.01, 0.008, 0.006, 0.97), Color::new(0.06, 0.045, 0.03, 0.92));
        vgrad(Rect::new(r.x, r.y, r.w, r.h * 0.25), Color::new(0.0, 0.0, 0.0, 0.35), Color::new(0.0, 0.0, 0.0, 0.0));
        let dragged_out = drag.is_some_and(|(dk, _, on)| on && dk == k);
        if let Some(s) = def.as_ref().filter(|_| !dragged_out) {
            let (col, ic) = spell_look(s.effect, s.target);
            let enough = mana >= s.mana;
            let c = r.center() - vec2(0.0, 4.0 * u);
            glow(c, r.w * 0.55, alpha(col, 0.22 + 0.2 * h));
            icon(ic, c, r.w * 0.5, if enough { mix(col, WHITE, 0.15 + 0.2 * h) } else { mix(col, Color::new(0.3, 0.3, 0.35, 1.0), 0.7) });
            let name = logic::short_spell_name(&s.name, 9);
            body_centered(&name, r.center().x, r.bottom() - 3.5 * u, 12.0 * u, if enough { TEXT } else { TEXT_FAINT });
            let m = s.mana.to_string();
            let mw = measure(&m, 12.0 * u).width;
            text(&m, r.right() - mw - 3.0 * u, r.y + 11.0 * u, 12.0 * u, if enough { Color::new(0.5, 0.7, 1.0, 1.0) } else { BAD });
            // Radial cooldown.
            if let Some(until) = game.cd_until.get(&s.id).copied().filter(|t| *t > game.now) {
                let left = (until - game.now) as f32;
                let f = (left / (s.cooldown_ms.max(1) as f32 / 1000.0)).clamp(0.0, 1.0);
                let cc = r.center();
                pie(cc, r.w * 0.47, f, Color::new(0.0, 0.0, 0.0, 0.62));
                ring(cc, r.w * 0.47, 1.5 * u, alpha(GOLD, 0.4));
                if left >= 0.5 {
                    title_centered(&format!("{:.0}", left.ceil()), cc.x, cc.y + 7.0 * u, 18.0 * u, WHITE);
                }
            }
            if hov && drag.is_none_or(|(_, _, on)| !on) {
                *tip = logic::spell_lines(s, game.stats.level);
                tip.push(format!("Tecla {}  ·  arrastra para cambiar de casilla", k + 1));
            }
        }
        // Frame: gold when armed (pulsing), warm on hover.
        let pulse = 0.6 + 0.4 * (now as f32 * 6.0).sin();
        let border = if is_armed { alpha(GOLD_BRIGHT, pulse) } else { mix(alpha(GOLD_DIM, 0.8), GOLD, h) };
        draw_rectangle_lines(r.x, r.y, r.w, r.h, if is_armed { 2.5 * u } else { 1.2 * u }, border);
        if is_armed {
            draw_rectangle_lines(r.x - 3.0 * u, r.y - 3.0 * u, r.w + 6.0 * u, r.h + 6.0 * u, 1.0, alpha(GOLD_BRIGHT, 0.35 * pulse));
        }
        // Hotkey, engraved in the corner.
        text_with(FontKind::Title, &(k + 1).to_string(), r.x + 3.5 * u, r.y + 13.0 * u, 14.0 * u, if def.is_some() { GOLD_TEXT } else { alpha(GOLD_DIM, 0.8) });
        if hov {
            ui.mouse_captured = true;
            if is_mouse_button_pressed(MouseButton::Left) && def.is_some() {
                HUD.with(|h| h.borrow_mut().drag = Some((k, mouse, false)));
            }
        }
    }
    // Drag to reorder / click to arm.
    if let Some((k, at, on)) = drag {
        let on = on || mouse.distance(at) > 6.0 * u;
        if is_mouse_button_down(MouseButton::Left) {
            HUD.with(|h| h.borrow_mut().drag = Some((k, at, on)));
            if on {
                ui.mouse_captured = true;
                if let Some(s) = game.spells.iter().find(|s| s.id == keys.spell_on(k, &book)) {
                    let (col, ic) = spell_look(s.effect, s.target);
                    glow(mouse, 26.0 * u, alpha(col, 0.35));
                    icon(ic, mouse, 24.0 * u, col);
                }
            }
        } else {
            HUD.with(|h| h.borrow_mut().drag = None);
            if on {
                if let Some(to) = (0..9).find(|j| d.slots[*j].contains(mouse)) {
                    let mut kk = keys.clone();
                    kk.swap(k, to, &book);
                    set_spell_keys(kk);
                }
            } else if d.slots[k].contains(mouse) {
                let id = keys.spell_on(k, &book);
                if id != 0 {
                    game.arm_spell(id);
                }
            }
        }
    }
    // State buttons: meditate, citizen lock.
    let flags = game.me().map(|e| e.status).unwrap_or(0);
    let meditating = flags & status::MEDITATING != 0;
    let safe = game.local.social.faction.safe;
    let states = [
        (d.slots[9], Icon::Lotus, meditating, Color::new(0.55, 0.75, 1.0, 1.0), "Meditar", if meditating { "Meditando: recuperas maná. Clic para parar." } else { "Recupera maná mientras no te mueves." }),
        (
            d.slots[10],
            Icon::Lock(safe),
            safe,
            if safe { GOLD_BRIGHT } else { BAD },
            "Seguro de ciudadano",
            if safe { "Activo: no puedes atacar a ciudadanos ni al ejército." } else { "Desactivado: puedes atacar a ciudadanos (te vuelves criminal)." },
        ),
    ];
    for (i, (r, ic, on, col, title, desc)) in states.into_iter().enumerate() {
        let hov = hover(r);
        let h = ease(super::fx_ui::key_of(r), hov, 0.08);
        let c = r.center();
        disc(c, r.w * 0.5, Color::new(0.03, 0.025, 0.02, 0.95));
        if on {
            let p = 0.75 + 0.25 * (now as f32 * 3.0).sin();
            glow(c, r.w * 0.85, alpha(col, 0.35 * p));
            radial(c, r.w * 0.46, alpha(mix(col, BLACK, 0.55), 1.0), Color::new(0.03, 0.025, 0.02, 1.0));
        }
        icon(ic, c, r.w * 0.52, if on { mix(col, WHITE, 0.2 * h) } else { mix(alpha(col, 0.55), col, h) });
        ring(c, r.w * 0.5, 2.0 * u, if on { col } else { mix(GOLD_DIM, GOLD, h) });
        if hov {
            ui.mouse_captured = true;
            *tip = vec![title.to_string(), desc.to_string(), "Clic para cambiar.".into()];
            if is_mouse_button_pressed(MouseButton::Left) {
                if i == 0 {
                    game.toggle_meditation();
                } else {
                    game.toggle_safe();
                }
            }
        }
    }
}

fn spell_look(effect: SpellEffect, target: ao_rules::spells::SpellTarget) -> (Color, Icon) {
    match effect {
        SpellEffect::Heal if target == ao_rules::spells::SpellTarget::Ally => (Color::new(0.45, 0.95, 0.5, 1.0), Icon::Cross),
        SpellEffect::Heal => (Color::new(0.45, 0.95, 0.5, 1.0), Icon::Cross),
        SpellEffect::Fireball => (Color::new(1.0, 0.55, 0.15, 1.0), Icon::Fire),
        SpellEffect::Lightning => (Color::new(0.55, 0.75, 1.0, 1.0), Icon::Bolt),
    }
}

// ---------------------------------------------------------------------------------------
// Portrait
// ---------------------------------------------------------------------------------------

/// Head and body of the own character, facing south, inside the circle (`c`, `r`).
fn portrait_sprite(assets: &mut Assets, game: &Game, c: Vec2, r: f32, u: f32) {
    let Some(me) = game.me() else { return };
    let Some(body) = assets.db.bodies.get(&me.body).cloned() else { return };
    let heading = 3; // south
    let scale = 1.9 * u;
    // Clip to the inscribed square (crop the source), feet slightly below the centre.
    let clip = Rect::new(c.x - r * 0.72, c.y - r * 0.8, r * 1.44, r * 1.5);
    let origin = vec2(c.x - 16.0 * scale, c.y + r * 0.95 - 33.0 * scale);
    let mut parts: Vec<(u32, f32, f32)> = vec![(body.walk[heading], body.body_offset.0, body.body_offset.1)];
    if let Some(h) = assets.db.heads.get(&me.head).map(|h| h[heading]).filter(|h| *h > 0) {
        parts.push((h, body.body_offset.0 + body.head_offset.0, body.body_offset.1 + body.head_offset.1 - 1.0));
    }
    for (idx, ox, oy) in parts {
        let Some(g) = assets.db.frame(idx, 0.0).cloned() else { continue };
        let (px, py) = anchor(&g, ox, oy, true);
        let Some(tex) = assets.texture(g.file) else { continue };
        let dest = Rect::new(origin.x + px * scale, origin.y + py * scale, g.w * scale, g.h * scale);
        let (cx0, cy0, cx1, cy1) = (dest.x.max(clip.x), dest.y.max(clip.y), dest.right().min(clip.right()), dest.bottom().min(clip.bottom()));
        if cx1 <= cx0 || cy1 <= cy0 {
            continue;
        }
        let src = Rect::new(g.sx + (cx0 - dest.x) / scale, g.sy + (cy0 - dest.y) / scale, (cx1 - cx0) / scale, (cy1 - cy0) / scale);
        draw_texture_ex(&tex, cx0, cy0, WHITE, DrawTextureParams { source: Some(src), dest_size: Some(vec2(cx1 - cx0, cy1 - cy0)), ..Default::default() });
    }
}

/// A small vessel that empties (hunger: a loaf, thirst: a drop).
fn vessel(c: Vec2, r: f32, f: f32, col: Color, drop: bool, u: f32) {
    let t = get_time() as f32;
    let low = f < 0.2;
    if drop {
        draw_triangle(c + vec2(-r * 0.86, -r * 0.45), c + vec2(r * 0.86, -r * 0.45), c + vec2(0.0, -r * 1.75), Color::new(0.04, 0.03, 0.03, 0.95));
    }
    disc(c, r, Color::new(0.04, 0.03, 0.03, 0.95));
    liquid(c, r, f, mix(col, WHITE, 0.15), mix(col, BLACK, 0.45), t, 0.8 * u, alpha(mix(col, WHITE, 0.6), 0.7));
    if drop {
        if f > 0.82 {
            draw_triangle(c + vec2(-r * 0.72, -r * 0.6), c + vec2(r * 0.72, -r * 0.6), c + vec2(0.0, -r * 1.6), mix(col, WHITE, 0.15));
        }
        line(c + vec2(-r * 0.9, -r * 0.45), c + vec2(0.0, -r * 1.8), 1.4 * u, GOLD);
        line(c + vec2(r * 0.9, -r * 0.45), c + vec2(0.0, -r * 1.8), 1.4 * u, GOLD);
        draw_arc(c.x, c.y, 24, r, -25.0, 1.4 * u, 230.0, GOLD);
    } else {
        ring(c, r, 1.4 * u, GOLD);
        // Stem and leaf: an apple.
        line(c + vec2(0.0, -r), c + vec2(r * 0.25, -r * 1.55), 1.6 * u, GOLD);
        draw_ellipse(c.x + r * 0.55, c.y - r * 1.3, r * 0.4, r * 0.18, -25.0, Color::new(0.45, 0.7, 0.3, 1.0));
    }
    draw_ellipse(c.x - r * 0.3, c.y - r * 0.35, r * 0.28, r * 0.14, -30.0, Color::new(1.0, 1.0, 1.0, 0.25));
    if low {
        glow(c, r * 2.2, Color::new(1.0, 0.3, 0.2, 0.25 + 0.2 * (t * 5.0).sin()));
    }
}

fn portrait_cluster(ui: &mut Ui, game: &mut Game, assets: &mut Assets, v: &Vitals, lay: &Layout, tip: &mut Vec<String>) {
    let u = lay.u;
    let sh = screen_height();
    let r = (40.0 * u).round();
    let c = vec2(18.0 * u + r, sh - 14.0 * u - r);
    let now = get_time();
    // Medallion.
    glow(c + vec2(0.0, 3.0 * u), r * 1.4, Color::new(0.0, 0.0, 0.0, 0.6));
    radial(c, r, Color::new(0.22, 0.17, 0.12, 1.0), Color::new(0.05, 0.035, 0.025, 1.0));
    portrait_sprite(assets, game, c, r, u);
    annulus(c, r * 0.82, r, Color::new(0.0, 0.0, 0.0, 0.0), Color::new(0.0, 0.0, 0.0, 0.6));
    ring(c, r + 3.0 * u, 6.0 * u, Color::new(0.0, 0.0, 0.0, 0.8));
    ring(c, r + 2.5 * u, 4.5 * u, GOLD_DIM);
    ring(c, r + 2.5 * u, 2.0 * u, GOLD);
    draw_arc(c.x, c.y, 40, r + 1.5 * u, 200.0, 1.2 * u, 110.0, Color::new(1.0, 0.92, 0.65, 0.6));
    // Faction gem on top.
    let fac = &game.local.social.faction;
    let fcol = crate::render::faction_color(fac.known.then_some(fac.status));
    let gem = c + vec2(0.0, -r - 3.0 * u);
    let g = 6.0 * u;
    draw_triangle(gem + vec2(0.0, -g), gem + vec2(g, 0.0), gem + vec2(0.0, g), GOLD);
    draw_triangle(gem + vec2(0.0, -g), gem + vec2(-g, 0.0), gem + vec2(0.0, g), GOLD);
    disc(gem, g * 0.55, fcol);
    disc(gem - vec2(1.0, 1.0) * u, g * 0.2, alpha(WHITE, 0.7));
    // Level seal (wax with a gold rim), bursting on level-up.
    let lvl_at = HUD.with(|h| h.borrow().level_at);
    let burst = (1.0 - (now - lvl_at) / 2.5).clamp(0.0, 1.0) as f32;
    let sc = c + vec2(r * 0.74, r * 0.74);
    let sr = 15.0 * u;
    if burst > 0.0 {
        for i in 0..3 {
            let k = ((1.0 - burst) * 1.6 + i as f32 * 0.2).fract();
            ring(sc, sr + k * 50.0 * u, 3.0 * u * (1.0 - k), alpha(GOLD_BRIGHT, (1.0 - k) * burst));
        }
        glow(sc, sr * 4.0, alpha(GOLD_BRIGHT, 0.5 * burst));
    }
    disc(sc + vec2(1.0, 2.0) * u, sr, Color::new(0.0, 0.0, 0.0, 0.5));
    radial(sc, sr, Color::new(0.62, 0.12, 0.08, 1.0), Color::new(0.32, 0.04, 0.03, 1.0));
    ring(sc, sr, 2.0 * u, GOLD);
    ring(sc, sr - 3.0 * u, 1.0, alpha(GOLD_DIM, 0.8));
    title_centered(&game.stats.level.to_string(), sc.x, sc.y + 6.0 * u, if game.stats.level >= 10 { 16.0 * u } else { 18.0 * u }, GOLD_BRIGHT);
    let seal_r = Rect::new(sc.x - sr, sc.y - sr, sr * 2.0, sr * 2.0);
    if hover(seal_r) {
        *tip = vec![format!("Nivel {}", game.stats.level), format!("Experiencia {:.1} %", v.exp.0 as f32 / v.exp.1 as f32 * 100.0)];
    }
    // Name, faction, hunger/thirst and gold to the right.
    let x = c.x + r + 16.0 * u;
    let name = game.me().map(|e| e.name.clone()).unwrap_or_default();
    let top = c.y - r + 4.0 * u;
    text_with(FontKind::Title, &name, x, top + 14.0 * u, 20.0 * u, Color::new(0.97, 0.93, 0.85, 1.0));
    if fac.known {
        let same = fac.title.trim().eq_ignore_ascii_case(logic::faction_name(fac.status));
        let label = if fac.title.trim().is_empty() || same { logic::faction_name(fac.status).to_string() } else { format!("{} · {}", logic::faction_name(fac.status), fac.title.trim()) };
        text(&label, x, top + 33.0 * u, 14.0 * u, alpha(fcol, 0.9));
    }
    let vy = c.y + r - 16.0 * u;
    let vr = 9.0 * u;
    let food_c = vec2(x + vr, vy);
    let water_c = vec2(x + vr * 4.2, vy);
    vessel(food_c, vr, v.food as f32 / 100.0, Color::new(0.9, 0.45, 0.12, 1.0), false, u);
    vessel(water_c, vr, v.water as f32 / 100.0, Color::new(0.25, 0.65, 0.95, 1.0), true, u);
    for (cc, label, val) in [(food_c, "Hambre", v.food), (water_c, "Sed", v.water)] {
        if cc.distance(mouse_position().into()) < vr * 1.6 {
            let mut t = vec![label.to_string(), format!("{val} / 100")];
            if val < 20 {
                t.push(format!("!{} — come o bebe algo pronto.", if label == "Sed" { "Tienes mucha sed" } else { "Tienes mucha hambre" }));
            }
            *tip = t;
        }
    }
    // Gold: coin and amount.
    let gx = water_c.x + vr * 2.6;
    let coin = vec2(gx + 7.0 * u, vy);
    disc(coin, 7.0 * u, Color::new(0.95, 0.76, 0.22, 1.0));
    ring(coin, 7.0 * u, 1.3 * u, Color::new(0.55, 0.36, 0.05, 1.0));
    ring(coin, 4.0 * u, 1.0, Color::new(0.75, 0.52, 0.1, 1.0));
    disc(coin - vec2(2.0, 2.0) * u, 1.6 * u, alpha(WHITE, 0.7));
    let gs = thousands(game.local.gold);
    text(&gs, coin.x + 11.0 * u, vy + 5.5 * u, 17.0 * u, Color::new(1.0, 0.86, 0.45, 1.0));
    let gold_r = Rect::new(gx, vy - 9.0 * u, 20.0 * u + measure(&gs, 17.0 * u).width, 18.0 * u);
    if hover(gold_r) {
        *tip = vec!["Oro".into(), format!("{} monedas en la bolsa", thousands(game.local.gold))];
    }
    let cluster = Rect::new(c.x - r - 6.0 * u, c.y - r - 10.0 * u, (gold_r.right() - c.x + r + 12.0 * u).max(200.0 * u), r * 2.0 + 16.0 * u);
    ui.capture_mouse_if_over(Rect::new(c.x - r, c.y - r, r * 2.0, r * 2.0));
    // Altered states as badges above the medallion (`AO_STATUS` adds bits for screenshots,
    // like the character renderer does).
    let dev_bits = std::env::var("AO_STATUS").ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let flags = game.me().map(|e| e.status).unwrap_or(0) | dev_bits;
    let mut bx = c.x - r;
    let by = cluster.y - 26.0 * u;
    for (label, mood) in logic::status_chips(flags) {
        let col = match mood {
            Mood::Good => Color::new(0.45, 0.9, 0.55, 1.0),
            Mood::Bad => Color::new(1.0, 0.4, 0.32, 1.0),
            Mood::Neutral => Color::new(0.65, 0.7, 1.0, 1.0),
        };
        let size = 14.0 * u;
        let w = measure(label, size).width + 26.0 * u;
        let b = Rect::new(bx, by, w, 21.0 * u);
        let pulse = if mood == Mood::Bad { 0.75 + 0.25 * (now as f32 * 4.0).sin() } else { 1.0 };
        hgrad(b, Color::new(0.03, 0.02, 0.02, 0.9), Color::new(0.03, 0.02, 0.02, 0.55));
        draw_rectangle(b.x, b.y, 3.0 * u, b.h, alpha(col, pulse));
        disc(vec2(b.x + 12.0 * u, b.center().y), 4.0 * u, alpha(col, pulse));
        glow(vec2(b.x + 12.0 * u, b.center().y), 9.0 * u, alpha(col, 0.35 * pulse));
        text(label, b.x + 20.0 * u, b.y + 15.0 * u, size, mix(col, WHITE, 0.45));
        bx += w + 5.0 * u;
    }
}

// ---------------------------------------------------------------------------------------
// Window icons
// ---------------------------------------------------------------------------------------

fn key_name(k: KeyCode) -> String {
    match k {
        KeyCode::F10 => "F10".into(),
        other => format!("{other:?}"),
    }
}

fn window_icon(w: WindowId) -> Icon {
    match w {
        WindowId::Inventory => Icon::Bag,
        WindowId::Spells => Icon::Book,
        WindowId::Character => Icon::Helm,
        WindowId::Guild => Icon::Shield,
        WindowId::Party => Icon::Party,
        WindowId::Quests => Icon::Scroll,
        WindowId::WorldMap => Icon::Compass,
        WindowId::Options => Icon::Gear,
        WindowId::Events => Icon::Swords,
        WindowId::Auction => Icon::Coins,
        WindowId::Gm => Icon::Crown,
        WindowId::Crafting => Icon::Anvil,
        _ => Icon::Scroll,
    }
}

fn window_icons(ui: &mut Ui, game: &Game, d: &Deck, lay: &Layout, tip: &mut Vec<String>) {
    let u = lay.u;
    let sw = screen_width();
    let sh = screen_height();
    let list: Vec<WindowId> = WindowId::ALL.into_iter().filter(|w| w.hotkey().is_some() && (*w != WindowId::Gm || game.local.p4.gm.is_gm())).collect();
    let s = (34.0 * u).round();
    let g = 5.0 * u;
    let left = d.mana_c.x + d.orb_r + 18.0 * u;
    let room = (sw - 16.0 * u - left).max(s * 3.0);
    let per_row = ((room + g) / (s + g)).floor().clamp(3.0, list.len() as f32) as usize;
    let rows = list.len().div_ceil(per_row);
    for (i, w) in list.iter().enumerate() {
        let (row, col) = (i / per_row, i % per_row);
        // Rows fill from the bottom, right-aligned.
        let n_in_row = if row == rows - 1 { list.len() - row * per_row } else { per_row };
        let x = sw - 16.0 * u - (n_in_row - col) as f32 * (s + g) + g;
        let y = sh - 16.0 * u - (rows - row) as f32 * (s + g) + g;
        let r = Rect::new(x.round(), y.round(), s, s);
        let open = ui.is_open(*w);
        let hov = hover(r);
        let h = ease(super::fx_ui::key_of(r), hov || open, 0.08);
        draw_rectangle(r.x + 2.0 * u, r.y + 3.0 * u, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.4));
        theme::surface(r, Surface::Raised, u);
        vgrad(r, Color::new(1.0, 0.85, 0.6, 0.06 * (1.0 + h)), Color::new(0.0, 0.0, 0.0, 0.35));
        if open {
            glow(r.center(), r.w * 0.7, alpha(GOLD_BRIGHT, 0.25));
        }
        icon(window_icon(*w), r.center() - vec2(0.0, 1.0 * u), r.w * 0.56, mix(alpha(GOLD, 0.85), GOLD_BRIGHT, h));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, if open { 1.8 * u } else { 1.0 }, mix(alpha(GOLD_DIM, 0.9), GOLD, h));
        let key = key_name(w.hotkey().unwrap_or(KeyCode::Unknown));
        let ks = 11.0 * u;
        let kw = measure(&key, ks).width;
        draw_rectangle(r.right() - kw - 5.0 * u, r.bottom() - 12.0 * u, kw + 4.0 * u, 11.0 * u, Color::new(0.0, 0.0, 0.0, 0.55));
        text(&key, r.right() - kw - 3.0 * u, r.bottom() - 3.0 * u, ks, alpha(TEXT, 0.85));
        if hov {
            ui.mouse_captured = true;
            *tip = vec![w.title().to_string(), format!("Tecla {key}"), if open { "Clic para cerrar." } else { "Clic para abrir." }.into()];
            if is_mouse_button_pressed(MouseButton::Left) {
                ui.toggle(*w);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Minimap
// ---------------------------------------------------------------------------------------

/// Tiles of the map shown across the minimap's diameter.
const MINIMAP_SPAN: f32 = 60.0;

fn minimap(ui: &mut Ui, game: &mut Game, lay: &Layout, hour: f32, pixel_perfect: bool, tip: &mut Vec<String>) {
    let u = lay.u;
    let p = lay.panel;
    let r = (p.w / 2.0 - 10.0 * u).round();
    let c = vec2(p.center().x.round(), (p.y + 8.0 * u + r).round());
    let n = ao_formats::csm::MAP_SIZE as f32;
    let me = game.me().map(|e| (e.x, e.y));
    // Map region centred on the player, kept inside the map so nothing smears.
    let half = MINIMAP_SPAN / 2.0;
    let (px, py) = me.map(|(x, y)| (x as f32 - 0.5, y as f32 - 0.5)).unwrap_or((n / 2.0, n / 2.0));
    let center = vec2(px.clamp(half, n - half), py.clamp(half, n - half));
    let to = |x: f32, y: f32| c + (vec2(x, y) - center) / MINIMAP_SPAN * 2.0 * r;
    glow(c + vec2(0.0, 4.0 * u), r * 1.3, Color::new(0.0, 0.0, 0.0, 0.6));
    disc(c, r, Color::new(0.02, 0.02, 0.02, 1.0));
    if let Some(t) = minimap_texture(game) {
        // Textured disc: UVs of the map region.
        let k = sides(r) as u16;
        let uv = |q: Vec2| ((q - c) / (2.0 * r) * MINIMAP_SPAN + center) / n;
        let mut v = Vec::with_capacity(k as usize + 2);
        let cu = uv(c);
        v.push(Vertex::new(c.x, c.y, 0.0, cu.x, cu.y, WHITE));
        for i in 0..=k {
            let a = i as f32 / k as f32 * TAU;
            let q = c + vec2(a.cos(), a.sin()) * r;
            let w = uv(q);
            v.push(Vertex::new(q.x, q.y, 0.0, w.x, w.y, WHITE));
        }
        let idx: Vec<u16> = (0..k).flat_map(|i| [0, i + 1, i + 2]).collect();
        draw_mesh(&Mesh { vertices: v, indices: idx, texture: Some(t) });
    }
    // Night darkens it a little.
    let night = !(6.0..20.0).contains(&hour);
    if night {
        disc(c, r, Color::new(0.02, 0.03, 0.1, 0.3));
    }
    // Everyone we see.
    let faction = &game.local.social.faction;
    for (id, e) in &game.ents {
        if *id == game.my_id || e.status & status::DEAD != 0 && e.kind == EntityKind::Creature {
            continue;
        }
        let q = to(e.x as f32 - 0.5, e.y as f32 - 0.5);
        if q.distance(c) > r - 3.0 * u {
            continue;
        }
        let col = match e.kind {
            EntityKind::Creature => Color::new(0.98, 0.6, 0.2, 1.0),
            EntityKind::Player => crate::render::faction_color(faction.others.get(id).copied()),
        };
        disc(q, 2.6 * u, Color::new(0.0, 0.0, 0.0, 0.7));
        disc(q, 1.8 * u, col);
    }
    if let Some((x, y)) = me {
        let q = to(x as f32 - 0.5, y as f32 - 0.5);
        // Official view (23 x 18 tiles) and the pulsing marker.
        let (vw, vh) = (ao_rules::VIEW_TILES_W as f32 / MINIMAP_SPAN * 2.0 * r, ao_rules::VIEW_TILES_H as f32 / MINIMAP_SPAN * 2.0 * r);
        draw_rectangle_lines(q.x - vw / 2.0, q.y - vh / 2.0, vw, vh, 1.0, Color::new(1.0, 1.0, 1.0, 0.25));
        let pulse = (get_time() * 1.2).fract() as f32;
        ring(q, 3.0 * u + pulse * 9.0 * u, 1.5 * u, Color::new(1.0, 0.92, 0.5, 0.7 * (1.0 - pulse)));
        disc(q, 3.6 * u, Color::new(0.0, 0.0, 0.0, 0.8));
        disc(q, 2.6 * u, Color::new(1.0, 0.95, 0.5, 1.0));
    }
    // Inner shade and the ornamented ring with compass ticks.
    annulus(c, r * 0.85, r, Color::new(0.0, 0.0, 0.0, 0.0), Color::new(0.0, 0.0, 0.0, 0.6));
    ring(c, r + 4.0 * u, 9.0 * u, Color::new(0.0, 0.0, 0.0, 0.8));
    ring(c, r + 3.5 * u, 7.0 * u, Color::new(0.16, 0.11, 0.07, 1.0));
    ring(c, r + 0.8 * u, 1.6 * u, GOLD);
    ring(c, r + 6.5 * u, 1.6 * u, GOLD_DIM);
    draw_arc(c.x, c.y, 64, r + 6.0 * u, 200.0, 1.2 * u, 110.0, Color::new(1.0, 0.92, 0.65, 0.5));
    for i in 0..24 {
        let a = i as f32 / 24.0 * TAU;
        let d = vec2(a.cos(), a.sin());
        let len = if i % 6 == 0 { 4.0 } else { 2.0 } * u;
        line(c + d * (r + 3.5 * u - len / 2.0), c + d * (r + 3.5 * u + len / 2.0), 1.0, alpha(GOLD, 0.8));
    }
    // North mark.
    let nt = c + vec2(0.0, -r - 3.5 * u);
    let k = 7.0 * u;
    draw_triangle(nt + vec2(-k * 0.7, k * 0.2), nt + vec2(k * 0.7, k * 0.2), nt + vec2(0.0, -k * 1.1), GOLD);
    disc(nt, 2.0 * u, GOLD_BRIGHT);
    // Clock and weather dial on the ring (upper-left).
    let dc = c + vec2(-0.72, -0.72) * (r + 4.0 * u);
    let dr = 15.0 * u;
    let sky = if night { Color::new(0.05, 0.07, 0.2, 1.0) } else { Color::new(0.25, 0.45, 0.75, 1.0) };
    disc(dc + vec2(1.0, 2.0) * u, dr, Color::new(0.0, 0.0, 0.0, 0.5));
    radial(dc, dr, mix(sky, WHITE, 0.15), mix(sky, BLACK, 0.4));
    let weather = game.local.weather;
    if night {
        disc(dc + vec2(-2.0, -2.0) * u, dr * 0.45, Color::new(0.95, 0.93, 0.8, 1.0));
        disc(dc + vec2(1.5, -4.0) * u, dr * 0.42, mix(sky, WHITE, 0.1));
    } else {
        glow(dc + vec2(-2.0, -2.0) * u, dr * 0.9, Color::new(1.0, 0.9, 0.4, 0.6));
        disc(dc + vec2(-2.0, -2.0) * u, dr * 0.38, Color::new(1.0, 0.88, 0.4, 1.0));
    }
    if weather.0 > 20 || weather.1 {
        let cl = Color::new(0.85, 0.87, 0.92, 0.95);
        disc(dc + vec2(2.0, 3.0) * u, dr * 0.32, cl);
        disc(dc + vec2(-3.0, 4.0) * u, dr * 0.26, cl);
        disc(dc + vec2(6.0, 4.5) * u, dr * 0.22, cl);
        if weather.1 {
            for i in 0..3 {
                let x = dc.x - 4.0 * u + i as f32 * 4.0 * u;
                line(vec2(x, dc.y + 8.0 * u), vec2(x - 1.5 * u, dc.y + 12.0 * u), 1.2 * u, Color::new(0.6, 0.8, 1.0, 1.0));
            }
        }
    }
    ring(dc, dr, 2.0 * u, GOLD);
    let dial = Rect::new(dc.x - dr, dc.y - dr, dr * 2.0, dr * 2.0);
    let clock = format!("{:02}:{:02}", hour as i32, (hour.fract() * 60.0) as i32);
    if hover(dial) {
        *tip = vec![format!("{clock} · {}", hour_label(hour)), weather_label(weather, night).to_string()];
    }
    // Plaque: map name, zone, clock/coords.
    let info = world_info(&game.recursos);
    let (title, safe, zone) = game.map.as_ref().map(|m| (map_title(&info, m.number, Some(&m.name)), m.safe, m.zone.trim().to_string())).unwrap_or_default();
    let ty = c.y + r + 12.0 * u;
    // The plaque must fit between the view's edge zone and the screen edge.
    let room = ((screen_width() - c.x) * 2.0 - 40.0 * u).min(p.w + 60.0 * u);
    let ts = if title_width(&title, 18.0 * u) <= room { 18.0 * u } else if title_width(&title, 16.0 * u) <= room { 16.0 * u } else { 14.0 * u };
    let tw = title_width(&title, ts);
    let plaque = Rect::new(c.x - tw / 2.0 - 16.0 * u, ty, tw + 32.0 * u, 26.0 * u);
    hgrad(Rect::new(plaque.x, plaque.y, plaque.w / 2.0, plaque.h), Color::new(0.03, 0.02, 0.015, 0.0), Color::new(0.03, 0.02, 0.015, 0.85));
    hgrad(Rect::new(plaque.center().x, plaque.y, plaque.w / 2.0, plaque.h), Color::new(0.03, 0.02, 0.015, 0.85), Color::new(0.03, 0.02, 0.015, 0.0));
    title_centered(&title, c.x, ty + 19.0 * u, ts, GOLD_BRIGHT);
    let (zl, zc) = if safe { ("Zona segura", GOOD) } else { ("Zona insegura", BAD) };
    let zone_line = if zone.is_empty() || zone.eq_ignore_ascii_case("CIUDAD") && safe { zl.to_string() } else { format!("{zl} · {}", capitalize(&zone)) };
    let zy = ty + 42.0 * u;
    let zw = measure(&zone_line, 14.0 * u).width;
    let zx = c.x - (zw + 14.0 * u) / 2.0;
    icon(Icon::Shield, vec2(zx + 5.0 * u, zy - 5.0 * u), 11.0 * u, zc);
    text(&zone_line, zx + 14.0 * u, zy, 14.0 * u, alpha(zc, 0.95));
    let mut info_line = format!("{clock}  ·  {}", weather_label(weather, night));
    if let Some((x, y)) = me {
        info_line.push_str(&format!("  ·  {x}, {y}"));
    }
    body_centered(&info_line, c.x, zy + 17.0 * u, 13.0 * u, TEXT_DIM);
    if options().show_fps {
        let fps = format!("{} fps  x{:.2}{}", get_fps(), lay.scale, if pixel_perfect { " PP" } else { "" });
        body_centered(&fps, c.x, zy + 32.0 * u, 12.0 * u, TEXT_FAINT);
    }
    // Hover and click.
    let m: Vec2 = mouse_position().into();
    if m.distance(c) < r {
        ui.mouse_captured = true;
        let q = (m - c) / (2.0 * r) * MINIMAP_SPAN + center;
        *tip = vec![title.clone(), format!("Casilla [{}, {}]", q.x as i32 + 1, q.y as i32 + 1), "Clic: mapa del mundo".into()];
        if is_mouse_button_pressed(MouseButton::Left) {
            ui.toggle(WindowId::WorldMap);
        }
    }
}

fn capitalize(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut c = lower.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------
// Target
// ---------------------------------------------------------------------------------------

/// The entity under the mouse (or the last one hovered, for 4 s) at the top centre, and the
/// armed spell while choosing a target.
fn target_plaque(game: &Game, lay: &Layout) {
    let u = lay.u;
    let now = get_time();
    let hovered = crate::input::tile_under_mouse(game, lay).and_then(|(tx, ty)| game.ents.iter().find(|(id, e)| e.x == tx && e.y == ty && **id != game.my_id).map(|(id, _)| *id));
    let target = HUD.with(|h| {
        let mut h = h.borrow_mut();
        if let Some(id) = hovered {
            h.target = Some((id, now));
        }
        h.target.filter(|(id, at)| now - at < 4.0 && game.ents.contains_key(id))
    });
    let cx = lay.view.center().x;
    let y = lay.view.y + 10.0 * u;
    if game.casting {
        let name = game.spells.get(game.selected).map(|s| s.name.clone()).unwrap_or_default();
        let s = format!("{name}: elige el objetivo  ·  clic derecho cancela");
        let w = measure(&s, 16.0 * u).width + 40.0 * u;
        let r = Rect::new(cx - w / 2.0, lay.view.bottom() - 40.0 * u, w, 28.0 * u);
        let p = 0.7 + 0.3 * (now as f32 * 5.0).sin();
        hgrad(Rect::new(r.x, r.y, r.w / 2.0, r.h), Color::new(0.05, 0.03, 0.01, 0.0), Color::new(0.05, 0.03, 0.01, 0.85));
        hgrad(Rect::new(r.center().x, r.y, r.w / 2.0, r.h), Color::new(0.05, 0.03, 0.01, 0.85), Color::new(0.05, 0.03, 0.01, 0.0));
        draw_line(r.x + 20.0 * u, r.y, r.right() - 20.0 * u, r.y, 1.0, alpha(GOLD, p));
        body_centered(&s, cx, r.y + 19.0 * u, 16.0 * u, alpha(GOLD_BRIGHT, 0.8 + 0.2 * p));
    }
    let Some((id, at)) = target else { return };
    let Some(e) = game.ents.get(&id) else { return };
    let fade = (1.0 - ((now - at) as f32 - 3.0).max(0.0)).clamp(0.0, 1.0);
    let w = 250.0 * u;
    let r = Rect::new((cx - w / 2.0).round(), y.round(), w, 44.0 * u);
    let col = match e.kind {
        EntityKind::Creature => Color::new(1.0, 0.75, 0.45, 1.0),
        EntityKind::Player => crate::render::faction_color(game.local.social.faction.others.get(&id).copied()),
    };
    hgrad(Rect::new(r.x, r.y, r.w / 2.0, r.h), Color::new(0.03, 0.02, 0.015, 0.0), Color::new(0.03, 0.02, 0.015, 0.8 * fade));
    hgrad(Rect::new(r.center().x, r.y, r.w / 2.0, r.h), Color::new(0.03, 0.02, 0.015, 0.8 * fade), Color::new(0.03, 0.02, 0.015, 0.0));
    title_centered(&e.name, cx, r.y + 18.0 * u, 18.0 * u, alpha(col, fade));
    let f = if e.max_hp > 0 { (e.hp.max(0) as f32 / e.max_hp as f32).clamp(0.0, 1.0) } else { 1.0 };
    let bar = Rect::new(cx - 90.0 * u, r.y + 26.0 * u, 180.0 * u, 7.0 * u);
    draw_rectangle(bar.x - 1.0, bar.y - 1.0, bar.w + 2.0, bar.h + 2.0, Color::new(0.0, 0.0, 0.0, 0.8 * fade));
    hgrad(Rect::new(bar.x, bar.y, bar.w * f, bar.h), Color::new(0.45, 0.05, 0.04, fade), Color::new(0.9, 0.2, 0.12, fade));
    draw_rectangle(bar.x, bar.y, bar.w * f, bar.h * 0.4, Color::new(1.0, 1.0, 1.0, 0.2 * fade));
    draw_rectangle_lines(bar.x - 1.0, bar.y - 1.0, bar.w + 2.0, bar.h + 2.0, 1.0, alpha(GOLD_DIM, fade));
    if e.max_hp > 0 {
        let s = format!("{} / {}", e.hp.max(0), e.max_hp);
        body_centered(&s, cx, bar.bottom() + 13.0 * u, 12.0 * u, alpha(TEXT_DIM, fade));
    }
}

/// Screen shown after a disconnection or kick.
pub fn kicked(reason: &str, u: f32) {
    let r = Rect::new(screen_width() / 2.0 - 240.0 * u, screen_height() / 2.0 - 70.0 * u, 480.0 * u, 140.0 * u);
    panel(r, u);
    title_centered("Desconectado", r.center().x, r.y + 48.0 * u, 28.0 * u, GOLD_BRIGHT);
    theme::divider(r.center().x, r.y + 64.0 * u, r.w * 0.6, u);
    body_centered(reason, r.center().x, r.y + 100.0 * u, 18.0 * u, TEXT);
}

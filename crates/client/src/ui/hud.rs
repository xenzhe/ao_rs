//! In-game HUD: side panel (portrait, vitals, quick buttons, minimap, quick spells) and the
//! bottom bar. OWNER: Sonnet 4 (docs/P3_CONTRATO.md). Also holds the small shared state of
//! the owner's windows (textures of minimaps and world maps, spell hotkeys, options) behind
//! plain functions: [`spell_keys`], [`options`], [`bmp_texture`], [`minimap_texture`].
use super::theme::{FontKind, BAD, GOLD, GOLD_TEXT, GOOD, MUTED, TEXT, WOOD_LIGHT};
use super::widgets::{measure, panel, text, text_centered, text_fit, text_with, tooltip};
use super::{Ui, WindowId};
use crate::assets::Assets;
use crate::game::Game;
use crate::render::layout::Layout;
use ao_formats::graphics::anchor;
use ao_protocol::{status, EntityKind};
use client_core::p3::hud as logic;
use client_core::p3::hud::{MapInfo, Mood, Options, Rgba, SpellKeys, WorldData};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
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
    let tex = Some(to_texture(&img, FilterMode::Nearest));
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
const STA_GREEN: Color = Color::new(0.20, 0.62, 0.22, 1.0);
const FOOD_ORANGE: Color = Color::new(0.80, 0.45, 0.14, 1.0);
const WATER_CYAN: Color = Color::new(0.16, 0.55, 0.78, 1.0);

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
    super::widgets::measure_with(FontKind::Title, s, size).width
}

fn hover(r: Rect) -> bool {
    r.contains(mouse_position().into())
}

fn clicked(r: Rect) -> bool {
    hover(r) && is_mouse_button_pressed(MouseButton::Left)
}

/// Small square button with a short label and an optional hint in the corner. Returns
/// whether it was clicked.
fn tile_button(r: Rect, label: &str, hint: &str, active: bool, u: f32) -> bool {
    let h = hover(r);
    let bg = if active {
        Color::new(0.42, 0.28, 0.09, 1.0)
    } else if h {
        Color::new(0.26, 0.19, 0.10, 1.0)
    } else {
        Color::new(0.15, 0.11, 0.07, 1.0)
    };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    draw_rectangle(r.x, r.y, r.w, r.h * 0.4, Color::new(1.0, 0.9, 0.6, 0.05));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, if active { GOLD_TEXT } else { GOLD });
    let size = 14.0 * u;
    let d = measure(label, size);
    text(label, r.x + (r.w - d.width) / 2.0, r.y + r.h * 0.5 + 4.0 * u, size, if active { Color::new(1.0, 0.92, 0.55, 1.0) } else { TEXT });
    if !hint.is_empty() {
        text(hint, r.x + 3.0 * u, r.y + 9.5 * u, 10.5 * u, GOLD_TEXT);
    }
    clicked(r)
}

fn draw_padlock(r: Rect, closed: bool, c: Color, u: f32) {
    let (cx, w) = (r.x + r.w / 2.0, r.w * 0.5);
    let body = Rect::new(cx - w / 2.0, r.y + r.h * 0.46, w, r.h * 0.36);
    draw_rectangle(body.x, body.y, body.w, body.h, c);
    draw_rectangle_lines(body.x, body.y, body.w, body.h, 1.0, Color::new(0.0, 0.0, 0.0, 0.6));
    let top = body.y;
    let sw = w * 0.62;
    let t = 2.2 * u;
    if closed {
        draw_line(cx - sw / 2.0, top, cx - sw / 2.0, top - r.h * 0.22, t, c);
        draw_line(cx + sw / 2.0, top, cx + sw / 2.0, top - r.h * 0.22, t, c);
        draw_line(cx - sw / 2.0, top - r.h * 0.22, cx + sw / 2.0, top - r.h * 0.22, t, c);
    } else {
        draw_line(cx - sw / 2.0, top, cx - sw / 2.0, top - r.h * 0.22, t, c);
        draw_line(cx - sw / 2.0, top - r.h * 0.22, cx + sw / 2.0, top - r.h * 0.22, t, c);
        draw_line(cx + sw / 2.0, top - r.h * 0.22, cx + sw / 2.0, top - r.h * 0.34, t, c);
    }
    draw_circle(cx, body.y + body.h * 0.45, 1.6 * u, Color::new(0.0, 0.0, 0.0, 0.8));
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

// ---------------------------------------------------------------------------------------
// HUD
// ---------------------------------------------------------------------------------------

/// HUD: side panel (character, bars, quick spells, minimap) and the bottom bar.
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, lay: &Layout, hour: f32, pixel_perfect: bool) {
    ui.capture_mouse_if_over(lay.panel);
    ui.capture_mouse_if_over(lay.bottom);
    spell_hotkeys(ui, game);
    let mut tip: Vec<String> = Vec::new();
    bottom_bar(game, lay, hour, pixel_perfect);
    side_panel(ui, game, assets, lay, &mut tip);
    tooltip(&tip, lay.u);
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

fn bottom_bar(game: &Game, lay: &Layout, hour: f32, pixel_perfect: bool) {
    let (u, b) = (lay.u, lay.bottom);
    panel(b, u);
    let (x, right) = (b.x + 12.0 * u, b.right() - 12.0 * u);
    let l1 = b.y + 22.0 * u;
    let l2 = b.y + 45.0 * u;
    match &game.chat {
        Some(t) => {
            draw_rectangle(b.x + 8.0 * u, b.y + 7.0 * u, b.w - 16.0 * u, 22.0 * u, Color::new(0.0, 0.0, 0.0, 0.6));
            text(&format!("> {t}_"), b.x + 12.0 * u, l1 + 1.0 * u, 18.0 * u, WHITE);
        }
        None => {
            // Key hints: gold key, muted action.
            let mut cx = x;
            for (k, a) in [("Enter", "hablar"), ("Flechas", "caminar"), ("1-9", "hechizos"), ("Clic", "lanzar"), ("I H C G P Q M O", "ventanas"), ("F2", "pixel-perfect")] {
                if cx + measure(k, 15.0 * u).width + measure(a, 15.0 * u).width + 24.0 * u > right {
                    break;
                }
                text(k, cx, l1, 15.0 * u, GOLD_TEXT);
                cx += measure(k, 15.0 * u).width + 5.0 * u;
                text(a, cx, l1, 15.0 * u, MUTED);
                cx += measure(a, 15.0 * u).width + 16.0 * u;
            }
        }
    }
    draw_line(b.x + 8.0 * u, b.y + 30.0 * u, b.right() - 8.0 * u, b.y + 30.0 * u, 1.0, Color::new(0.30, 0.22, 0.12, 1.0));
    let mut cx = x;
    if let (Some(me), Some(map)) = (game.me(), game.map.as_ref()) {
        let s = format!("{}  (Mapa {})  [{}, {}]", map.name, map.number, me.x, me.y);
        text(&s, cx, l2, 16.0 * u, Color::new(0.88, 0.74, 0.46, 1.0));
        cx += measure(&s, 16.0 * u).width + 14.0 * u;
        let (label, c) = if map.safe { ("Zona segura", GOOD) } else { ("Zona insegura", BAD) };
        text(label, cx, l2, 16.0 * u, c);
        cx += measure(label, 16.0 * u).width + 14.0 * u;
        if !map.zone.trim().is_empty() {
            text_fit(&map.zone, cx, l2, 16.0 * u, (right - cx - 330.0 * u).max(40.0 * u), MUTED);
        }
    }
    let clock = format!("{:02}:{:02} hs  {}", hour as i32, (hour.fract() * 60.0) as i32, hour_label(hour));
    let mut rx = right;
    if options().show_fps {
        let fps = format!("{} fps  x{:.2}{}", get_fps(), lay.scale, if pixel_perfect { " PP" } else { "" });
        rx -= measure(&fps, 16.0 * u).width;
        text(&fps, rx, l2, 16.0 * u, MUTED);
        rx -= 18.0 * u;
    }
    rx -= measure(&clock, 16.0 * u).width;
    text(&clock, rx, l2, 16.0 * u, Color::new(0.75, 0.82, 0.95, 1.0));
}

/// Head and body of the own character, facing south, scaled into `r`.
fn portrait(assets: &mut Assets, game: &Game, r: Rect, u: f32) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.04, 0.035, 0.03, 0.95));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, GOLD);
    let Some(me) = game.me() else { return };
    let Some(body) = assets.db.bodies.get(&me.body).cloned() else { return };
    let heading = 3; // south
    let scale = 1.5 * u;
    // Tile whose top-left is (0, 0); the picture is placed so the feet rest near the bottom.
    let origin = vec2(r.x + r.w / 2.0 - 16.0 * scale, r.y + r.h - 33.0 * scale);
    let mut parts: Vec<(u32, f32, f32)> = vec![(body.walk[heading], body.body_offset.0, body.body_offset.1)];
    if let Some(h) = assets.db.heads.get(&me.head).map(|h| h[heading]).filter(|h| *h > 0) {
        parts.push((h, body.body_offset.0 + body.head_offset.0, body.body_offset.1 + body.head_offset.1 - 1.0));
    }
    for (idx, ox, oy) in parts {
        let Some(g) = assets.db.frame(idx, 0.0).cloned() else { continue };
        let (px, py) = anchor(&g, ox, oy, true);
        let Some(tex) = assets.texture(g.file) else { continue };
        let dest = Rect::new(origin.x + px * scale, origin.y + py * scale, g.w * scale, g.h * scale);
        // Clip to the frame by cropping the source instead of using scissor.
        let (cx0, cy0, cx1, cy1) = (dest.x.max(r.x + 1.0), dest.y.max(r.y + 1.0), dest.right().min(r.right() - 1.0), dest.bottom().min(r.bottom() - 1.0));
        if cx1 <= cx0 || cy1 <= cy0 {
            continue;
        }
        let src = Rect::new(g.sx + (cx0 - dest.x) / scale, g.sy + (cy0 - dest.y) / scale, (cx1 - cx0) / scale, (cy1 - cy0) / scale);
        draw_texture_ex(&tex, cx0, cy0, WHITE, DrawTextureParams { source: Some(src), dest_size: Some(vec2(cx1 - cx0, cy1 - cy0)), ..Default::default() });
    }
}

fn side_panel(ui: &mut Ui, game: &mut Game, assets: &mut Assets, lay: &Layout, tip: &mut Vec<String>) {
    let (u, p) = (lay.u, lay.panel);
    panel(p, u);
    let (x, w) = (p.x + 12.0 * u, p.w - 24.0 * u);
    let mut y = p.y + 12.0 * u;

    // Header: portrait, name, level, faction.
    let name = game.me().map(|e| e.name.clone()).unwrap_or_default();
    let flags = game.me().map(|e| e.status).unwrap_or(0);
    let pr = Rect::new(x, y, 54.0 * u, 62.0 * u);
    portrait(assets, game, pr, u);
    let tx = pr.right() + 10.0 * u;
    text_fit(&name, tx, y + 20.0 * u, 22.0 * u, p.right() - tx - 12.0 * u, Color::new(0.80, 0.90, 1.0, 1.0));
    text_with(FontKind::Title, &format!("Nivel {}", game.stats.level), tx, y + 41.0 * u, 18.0 * u, GOLD_TEXT);
    let fac = &game.local.social.faction;
    if fac.known {
        let c = if logic::is_criminal(fac.status) { BAD } else if fac.status == 1 { GOOD } else { GOLD_TEXT };
        text_fit(logic::faction_name(fac.status), tx, y + 59.0 * u, 15.0 * u, p.right() - tx - 12.0 * u, c);
    }
    y += 68.0 * u;

    let (exp, next) = (game.stats.exp, game.stats.next_exp.max(1));
    let r = Rect::new(x, y, w, 17.0 * u);
    meter(r, exp as f32, next as f32, EXP_GOLD, "Experiencia", &format!("{}%", (exp as f32 / next as f32 * 100.0).min(100.0) as u32), u);
    if hover(r) {
        *tip = vec!["Experiencia".into(), format!("{} / {}", thousands(exp), thousands(next)), format!("Faltan {} para el nivel {}", thousands(next.saturating_sub(exp)), game.stats.level + 1)];
    }
    y += 22.0 * u;

    // Vitals.
    let (hp, mhp, mp, mmp) = (game.stats.hp.get(), game.stats.max_hp.get(), game.stats.mana.get(), game.stats.max_mana.get());
    meter(Rect::new(x, y, w, 20.0 * u), hp as f32, mhp as f32, HP_RED, "Vida", &format!("{hp}/{mhp}"), u);
    y += 24.0 * u;
    meter(Rect::new(x, y, w, 20.0 * u), mp as f32, mmp as f32, MANA_BLUE, "Maná", &format!("{mp}/{mmp}"), u);
    y += 25.0 * u;
    let l = &game.local;
    let small = [("Energía", l.sta, l.max_sta.max(1), STA_GREEN), ("Hambre", l.food, 100, FOOD_ORANGE), ("Sed", l.water, 100, WATER_CYAN)];
    for (label, v, max, c) in small {
        let r = Rect::new(x, y, w, 17.0 * u);
        meter(r, v as f32, max as f32, c, label, &format!("{v}/{max}"), u);
        if hover(r) && label != "Energía" && v < 20 {
            *tip = vec![label.into(), "Estás por debajo del 20 %: come o bebe.".into()];
        }
        y += 20.0 * u;
    }
    y += 3.0 * u;

    // Gold, meditation and the citizen lock.
    let gold_r = Rect::new(x, y, w - 56.0 * u, 24.0 * u);
    draw_rectangle(gold_r.x, gold_r.y, gold_r.w, gold_r.h, Color::new(0.0, 0.0, 0.0, 0.55));
    draw_rectangle_lines(gold_r.x, gold_r.y, gold_r.w, gold_r.h, 1.0, GOLD);
    draw_circle(gold_r.x + 13.0 * u, gold_r.y + 12.0 * u, 7.0 * u, Color::new(0.95, 0.78, 0.22, 1.0));
    draw_circle_lines(gold_r.x + 13.0 * u, gold_r.y + 12.0 * u, 7.0 * u, 1.5, Color::new(0.55, 0.38, 0.05, 1.0));
    let g = thousands(game.local.gold);
    let gs = 17.0 * u;
    text(&g, gold_r.right() - measure(&g, gs).width - 8.0 * u, gold_r.y + 17.5 * u, gs, Color::new(1.0, 0.9, 0.5, 1.0));
    if hover(gold_r) {
        *tip = vec!["Oro".into(), format!("{} monedas en la bolsa", thousands(game.local.gold))];
    }
    let med_r = Rect::new(gold_r.right() + 4.0 * u, y, 24.0 * u, 24.0 * u);
    let meditating = flags & status::MEDITATING != 0;
    if tile_button(med_r, "Me", "", meditating, u) {
        game.toggle_meditation();
    }
    if hover(med_r) {
        *tip = vec!["Meditar".into(), if meditating { "Activo: clic para dejar de meditar." } else { "Recupera maná mientras no te mueves." }.into()];
    }
    let lock_r = Rect::new(med_r.right() + 4.0 * u, y, 24.0 * u, 24.0 * u);
    let safe = game.local.social.faction.safe;
    draw_rectangle(lock_r.x, lock_r.y, lock_r.w, lock_r.h, if hover(lock_r) { Color::new(0.26, 0.19, 0.10, 1.0) } else { Color::new(0.15, 0.11, 0.07, 1.0) });
    draw_rectangle_lines(lock_r.x, lock_r.y, lock_r.w, lock_r.h, 1.0, if safe { GOLD_TEXT } else { BAD });
    draw_padlock(lock_r, safe, if safe { GOLD_TEXT } else { BAD }, u);
    if clicked(lock_r) {
        game.toggle_safe();
    }
    if hover(lock_r) {
        *tip = vec![
            "Seguro de ciudadano".into(),
            if safe { "Activo: no puedes atacar a ciudadanos ni al ejército." } else { "Desactivado: puedes atacar a ciudadanos (te vuelves criminal)." }.into(),
            "Clic para cambiar.".into(),
        ];
    }
    y += 29.0 * u;

    // Status chips (always one line, so the panel does not jump).
    let chips = logic::status_chips(flags);
    if chips.is_empty() {
        text("Sin estados alterados", x, y + 13.0 * u, 14.0 * u, MUTED);
    } else {
        let mut cx = x;
        for (label, mood) in chips {
            let d = measure(label, 13.0 * u);
            if cx + d.width + 12.0 * u > x + w {
                break;
            }
            let c = match mood {
                Mood::Good => Color::new(0.14, 0.40, 0.18, 1.0),
                Mood::Bad => Color::new(0.50, 0.12, 0.10, 1.0),
                Mood::Neutral => Color::new(0.22, 0.24, 0.40, 1.0),
            };
            draw_rectangle(cx, y, d.width + 10.0 * u, 18.0 * u, c);
            draw_rectangle_lines(cx, y, d.width + 10.0 * u, 18.0 * u, 1.0, GOLD);
            text(label, cx + 5.0 * u, y + 13.5 * u, 13.0 * u, WHITE);
            cx += d.width + 15.0 * u;
        }
    }
    y += 24.0 * u;

    // Quick buttons to the windows.
    const BUTTONS: [(WindowId, &str, &str); 8] = [
        (WindowId::Inventory, "Inv.", "I"),
        (WindowId::Spells, "Magia", "H"),
        (WindowId::Character, "Pers.", "C"),
        (WindowId::Guild, "Clan", "G"),
        (WindowId::Party, "Grupo", "P"),
        (WindowId::Quests, "Misión", "Q"),
        (WindowId::WorldMap, "Mapa", "M"),
        (WindowId::Options, "Opc.", "O"),
    ];
    let bw = (w - 3.0 * 4.0 * u) / 4.0;
    for (i, (win, label, key)) in BUTTONS.iter().enumerate() {
        let r = Rect::new(x + (i % 4) as f32 * (bw + 4.0 * u), y + (i / 4) as f32 * 29.0 * u, bw, 25.0 * u);
        if tile_button(r, label, key, ui.is_open(*win), u) {
            ui.toggle(*win);
        }
        if hover(r) {
            *tip = vec![win.title().to_string(), format!("Tecla {key}")];
        }
    }
    y += 62.0 * u;

    // The room left is shared: minimap on top, quick spells below.
    let spells_h = 20.0 * u + 9.0 * 18.5 * u;
    let room = (p.bottom() - 10.0 * u - y - spells_h - 8.0 * u).max(60.0 * u);
    let side = room.min(w);
    minimap(ui, game, Rect::new(x + (w - side) / 2.0, y, side, side), u, tip);
    y += side + 8.0 * u;
    quick_spells(game, x, y, w, u, tip);
}

fn minimap(ui: &mut Ui, game: &mut Game, r: Rect, u: f32, tip: &mut Vec<String>) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.02, 0.02, 0.02, 1.0));
    if let Some(t) = minimap_texture(game) {
        draw_texture_ex(&t, r.x, r.y, WHITE, DrawTextureParams { dest_size: Some(vec2(r.w, r.h)), ..Default::default() });
    }
    let n = ao_formats::csm::MAP_SIZE as f32;
    let to = |x: i32, y: i32| vec2(r.x + (x as f32 - 0.5) / n * r.w, r.y + (y as f32 - 0.5) / n * r.h);
    // Everyone we see.
    let faction = &game.local.social.faction;
    for (id, e) in &game.ents {
        if *id == game.my_id || e.status & status::DEAD != 0 && e.kind == EntityKind::Creature {
            continue;
        }
        let c = match e.kind {
            EntityKind::Creature => Color::new(0.95, 0.55, 0.15, 1.0),
            EntityKind::Player if faction.others.get(id).is_some_and(|s| logic::is_criminal(*s)) => Color::new(1.0, 0.25, 0.2, 1.0),
            EntityKind::Player => Color::new(0.35, 0.8, 1.0, 1.0),
        };
        let q = to(e.x, e.y);
        draw_circle(q.x, q.y, (1.6 * u).max(1.5), c);
    }
    if let Some(me) = game.me() {
        let q = to(me.x, me.y);
        // View window (23 x 18 tiles) and the blinking marker.
        let (vw, vh) = (ao_rules::VIEW_TILES_W as f32 / n * r.w, ao_rules::VIEW_TILES_H as f32 / n * r.h);
        draw_rectangle_lines((q.x - vw / 2.0).clamp(r.x, r.right() - vw), (q.y - vh / 2.0).clamp(r.y, r.bottom() - vh), vw, vh, 1.0, Color::new(1.0, 1.0, 1.0, 0.35));
        let pulse = 0.5 + 0.5 * (get_time() * 4.0).sin() as f32;
        draw_circle(q.x, q.y, 4.5 * u, Color::new(1.0, 0.9, 0.3, 0.25 + 0.3 * pulse));
        draw_circle(q.x, q.y, 2.6 * u, Color::new(1.0, 0.95, 0.4, 1.0));
        draw_circle_lines(q.x, q.y, 2.6 * u, 1.0, BLACK);
    }
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0 * u, GOLD);
    if hover(r) {
        let m: Vec2 = mouse_position().into();
        let (tx, ty) = (((m.x - r.x) / r.w * n) as i32 + 1, ((m.y - r.y) / r.h * n) as i32 + 1);
        let info = world_info(&game.recursos);
        let title = game.map.as_ref().map(|m| map_title(&info, m.number, Some(&m.name))).unwrap_or_default();
        *tip = vec![title, format!("Casilla [{tx}, {ty}]"), "Clic: mapa del mundo".into()];
        if is_mouse_button_pressed(MouseButton::Left) {
            ui.toggle(WindowId::WorldMap);
        }
    }
}

fn quick_spells(game: &mut Game, x: f32, mut y: f32, w: f32, u: f32, tip: &mut Vec<String>) {
    text_with(FontKind::Title, "Hechizos rápidos", x, y + 13.0 * u, 15.0 * u, GOLD_TEXT);
    text("teclas 1-9", x + w - measure("teclas 1-9", 13.0 * u).width, y + 13.0 * u, 13.0 * u, MUTED);
    y += 20.0 * u;
    let keys = spell_keys();
    let armed = game.casting.then(|| game.spells.get(game.selected).map(|s| s.id)).flatten();
    let mana = game.stats.mana.get();
    for k in 0..9 {
        let id = keys.spell_on(k, &game.local.spell_slots);
        let r = Rect::new(x, y, w, 17.5 * u);
        let def = game.spells.iter().find(|s| s.id == id).cloned();
        let is_armed = armed == Some(id) && id != 0;
        draw_rectangle(r.x, r.y, r.w, r.h, if is_armed { Color::new(0.40, 0.28, 0.08, 0.95) } else if hover(r) && def.is_some() { Color::new(0.22, 0.16, 0.09, 1.0) } else { Color::new(0.06, 0.05, 0.04, 0.8) });
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, if is_armed { GOLD_TEXT } else { Color::new(0.30, 0.22, 0.12, 1.0) });
        text(&(k + 1).to_string(), r.x + 5.0 * u, r.y + 13.0 * u, 14.0 * u, GOLD_TEXT);
        match def {
            Some(s) => {
                if let Some(until) = game.cd_until.get(&s.id).copied().filter(|t| *t > game.now) {
                    let f = ((until - game.now) as f32 / (s.cooldown_ms.max(1) as f32 / 1000.0)).clamp(0.0, 1.0);
                    draw_rectangle(r.x + 1.0, r.y + 1.0, (r.w - 2.0) * f, r.h - 2.0, Color::new(0.0, 0.0, 0.0, 0.5));
                }
                let m = format!("{}", s.mana);
                let md = measure(&m, 13.0 * u);
                text(&m, r.right() - md.width - 5.0 * u, r.y + 13.0 * u, 13.0 * u, if mana >= s.mana { Color::new(0.45, 0.65, 1.0, 1.0) } else { BAD });
                text_fit(&s.name, r.x + 20.0 * u, r.y + 13.0 * u, 14.5 * u, r.w - 32.0 * u - md.width, TEXT);
                if hover(r) {
                    *tip = logic::spell_lines(&s, game.stats.level);
                    if clicked(r) {
                        game.arm_spell(s.id);
                    }
                }
            }
            None => text("-", r.x + 20.0 * u, r.y + 13.0 * u, 14.0 * u, WOOD_LIGHT),
        }
        y += 18.5 * u;
    }
    if game.casting {
        text_centered("Elige el objetivo (clic derecho cancela)", x + w / 2.0, y + 11.0 * u, 12.5 * u, Color::new(1.0, 0.9, 0.4, 1.0));
    }
}

/// Screen shown after a disconnection or kick.
pub fn kicked(reason: &str, u: f32) {
    let r = Rect::new(screen_width() / 2.0 - 220.0 * u, screen_height() / 2.0 - 60.0 * u, 440.0 * u, 120.0 * u);
    panel(r, u);
    text("Desconectado", r.x + 24.0 * u, r.y + 44.0 * u, 30.0 * u, YELLOW);
    text(reason, r.x + 24.0 * u, r.y + 80.0 * u, 18.0 * u, WHITE);
}

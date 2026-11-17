//! Window: map of the world. OWNER: Sonnet 4 (docs/P3_CONTRATO.md).
//!
//! Pictures: `Recursos/interface/es_mapa<N>.bmp` (513x594, one per world); the grid that says
//! which map is each 27x27 px cell is `init/mapsworlddata.dat`, the names `init/namemapa.dat`
//! (see `client_core::p3::hud`). The player is placed inside the cell of the current map
//! like the official minimap does (`ModMinimap.bas`).
use super::hud::{bmp_texture, map_title, world_info};
use super::theme::{FontKind, BAD, GOLD, GOLD_TEXT, GOOD, MUTED, TEXT};
use super::widgets::{button, text, text_fit, text_with};
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::Ui;
use client_core::p3::hud as logic;
use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    /// World tab chosen by the player (None: follow the current map).
    static TAB: Cell<Option<usize>> = const { Cell::new(None) };
}

const TAB_NAMES: [&str; 4] = ["Superficie", "Mazmorras", "Mundo 3", "Mundo 4"];

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let _ = assets;
    let u = ui.u;
    let info = world_info(&game.recursos);
    let here = game.map.as_ref().and_then(|m| info.data.locate(m.number));
    // Worlds that have a picture.
    let pics: Vec<usize> = (0..info.data.worlds.len()).filter(|i| game.recursos.join("interface").join(format!("es_mapa{}.bmp", i + 1)).exists()).collect();
    if pics.is_empty() {
        text("No se encontraron las imágenes del mundo (Recursos/interface/es_mapa*.bmp).", r.x, r.y + 20.0 * u, 16.0 * u, BAD);
        return;
    }
    let world = TAB.with(Cell::get).filter(|w| pics.contains(w)).or(here.map(|h| h.0).filter(|w| pics.contains(w))).unwrap_or(pics[0]);

    let side_w = 262.0 * u;
    let left = Rect::new(r.x, r.y, r.w - side_w - 10.0 * u, r.h);
    let right = Rect::new(left.right() + 10.0 * u, r.y, side_w, r.h);

    // Tabs.
    let tw = (left.w - 4.0 * u * (pics.len() as f32 - 1.0)) / pics.len() as f32;
    for (i, w) in pics.iter().enumerate() {
        let tr = Rect::new(left.x + i as f32 * (tw + 4.0 * u), left.y, tw, 24.0 * u);
        if button(tr, TAB_NAMES.get(*w).copied().unwrap_or("Mundo"), *w == world, u) {
            TAB.with(|t| t.set(Some(*w)));
        }
    }

    // Picture, fitted keeping its aspect.
    let area = Rect::new(left.x, left.y + 30.0 * u, left.w, left.h - 30.0 * u);
    let tex = bmp_texture(&game.recursos.join("interface").join(format!("es_mapa{}.bmp", world + 1)), FilterMode::Linear);
    let (pw, ph) = tex.as_ref().map(|t| (t.width(), t.height())).unwrap_or((513.0, 594.0));
    let s = (area.w / pw).min(area.h / ph);
    let img = Rect::new((area.x + (area.w - pw * s) / 2.0).round(), area.y, (pw * s).round(), (ph * s).round());
    draw_rectangle(img.x, img.y, img.w, img.h, Color::new(0.01, 0.01, 0.02, 1.0));
    if let Some(t) = &tex {
        draw_texture_ex(t, img.x, img.y, WHITE, DrawTextureParams { dest_size: Some(vec2(img.w, img.h)), ..Default::default() });
    }
    draw_rectangle_lines(img.x, img.y, img.w, img.h, 2.0 * u, GOLD);

    let w = &info.data.worlds[world];
    let (cw, ch) = (img.w / w.cols.max(1) as f32, img.h / w.rows.max(1) as f32);
    let cell_rect = |cx: u32, cy: u32| Rect::new(img.x + cx as f32 * cw, img.y + cy as f32 * ch, cw, ch);

    // Cell under the mouse.
    let m: Vec2 = mouse_position().into();
    let mut hovered: Option<(u16, Rect)> = None;
    if img.contains(m) {
        let (cx, cy) = (((m.x - img.x) / cw) as u32, ((m.y - img.y) / ch) as u32);
        let n = w.map_at(cx, cy);
        if n != 0 {
            hovered = Some((n, cell_rect(cx, cy)));
        }
    }
    if let Some((_, cr)) = hovered {
        draw_rectangle(cr.x, cr.y, cr.w, cr.h, Color::new(1.0, 1.0, 1.0, 0.18));
        draw_rectangle_lines(cr.x, cr.y, cr.w, cr.h, 1.5, Color::new(1.0, 1.0, 1.0, 0.8));
    }

    // You are here.
    if let (Some((hw, cx, cy)), Some(me)) = (here, game.me()) {
        if hw == world {
            let cr = cell_rect(cx, cy);
            let (fx, fy) = logic::tile_in_cell(me.x, me.y);
            let p = vec2(cr.x + fx * cr.w, cr.y + fy * cr.h);
            let pulse = 0.5 + 0.5 * (get_time() * 3.5).sin() as f32;
            draw_rectangle_lines(cr.x, cr.y, cr.w, cr.h, 2.0, Color::new(1.0, 0.9, 0.3, 0.6 + 0.4 * pulse));
            draw_circle(p.x, p.y, (7.0 + 3.0 * pulse) * u, Color::new(1.0, 0.85, 0.2, 0.28));
            draw_circle(p.x, p.y, 3.4 * u, Color::new(1.0, 0.95, 0.35, 1.0));
            draw_circle_lines(p.x, p.y, 3.4 * u, 1.2, BLACK);
            let label = "Tú";
            let d = super::widgets::measure(label, 15.0 * u);
            let lx = (p.x - d.width / 2.0).clamp(img.x + 2.0, img.right() - d.width - 2.0);
            let ly = if p.y - 14.0 * u > img.y + 14.0 * u { p.y - 12.0 * u } else { p.y + 22.0 * u };
            draw_rectangle(lx - 3.0 * u, ly - 13.0 * u, d.width + 6.0 * u, 17.0 * u, Color::new(0.0, 0.0, 0.0, 0.65));
            text(label, lx, ly, 15.0 * u, Color::new(1.0, 0.95, 0.4, 1.0));
        }
    }

    // Side card: hovered map, else the current one.
    let current = game.map.as_ref().map(|m| m.number);
    let shown = hovered.map(|h| h.0).or(current);
    draw_rectangle(right.x, right.y, right.w, right.h, Color::new(0.04, 0.03, 0.02, 0.9));
    draw_rectangle_lines(right.x, right.y, right.w, right.h, 1.0, GOLD);
    let (x, mut y) = (right.x + 12.0 * u, right.y + 12.0 * u);
    let inner = right.w - 24.0 * u;
    text(if hovered.is_some() { "Bajo el cursor" } else { "Estás en" }, x, y + 12.0 * u, 14.0 * u, MUTED);
    y += 18.0 * u;
    match shown {
        Some(n) => {
            let own = (current == Some(n)).then(|| game.map.as_ref().map(|m| m.name.clone())).flatten();
            let title = map_title(&info, n, own.as_deref());
            wrapped(&title, x, &mut y, inner, 22.0 * u, GOLD_TEXT, true);
            text(&format!("Mapa {n}"), x, y + 13.0 * u, 15.0 * u, MUTED);
            y += 24.0 * u;
            if let Some(d) = info.names.get(&n).map(|m| m.desc.trim()).filter(|d| !d.is_empty()) {
                wrapped(d, x, &mut y, inner, 16.0 * u, TEXT, false);
                y += 6.0 * u;
            }
            if current == Some(n) {
                if let (Some(map), Some(me)) = (game.map.as_ref(), game.me()) {
                    text(&format!("Casilla [{}, {}]", me.x, me.y), x, y + 14.0 * u, 16.0 * u, TEXT);
                    y += 22.0 * u;
                    let (label, c) = if map.safe { ("Zona segura", GOOD) } else { ("Zona insegura", BAD) };
                    text(label, x, y + 14.0 * u, 16.0 * u, c);
                    y += 22.0 * u;
                    if !map.zone.trim().is_empty() {
                        text_fit(&format!("Zona: {}", map.zone), x, y + 14.0 * u, 16.0 * u, inner, MUTED);
                        y += 22.0 * u;
                    }
                }
            }
        }
        None => text("Sin mapa", x, y + 16.0 * u, 18.0 * u, MUTED),
    }
    if here.is_none() && current.is_some() {
        wrapped("Este mapa no aparece en ningún mundo dibujado.", x, &mut y, inner, 14.0 * u, MUTED, false);
    }
    // Legend, at the bottom of the card.
    let ly = right.bottom() - 70.0 * u;
    draw_line(x, ly - 6.0 * u, x + inner, ly - 6.0 * u, 1.0, Color::new(0.30, 0.22, 0.12, 1.0));
    text_with(FontKind::Title, "Leyenda", x, ly + 12.0 * u, 15.0 * u, GOLD_TEXT);
    draw_circle(x + 6.0 * u, ly + 28.0 * u, 3.4 * u, Color::new(1.0, 0.95, 0.35, 1.0));
    text("Tu posición", x + 18.0 * u, ly + 33.0 * u, 14.0 * u, TEXT);
    draw_rectangle(x + 2.0 * u, ly + 41.0 * u, 8.0 * u, 8.0 * u, Color::new(1.0, 1.0, 1.0, 0.3));
    text("Pasa el cursor para ver el nombre", x + 18.0 * u, ly + 50.0 * u, 14.0 * u, MUTED);
}

/// Draws `s` word-wrapped in `width`, advancing `y` by the lines used.
fn wrapped(s: &str, x: f32, y: &mut f32, width: f32, size: f32, c: Color, title: bool) {
    let mut line = String::new();
    let flush = |line: &mut String, y: &mut f32| {
        if line.is_empty() {
            return;
        }
        *y += size * 0.95;
        if title {
            text_with(FontKind::Title, line, x, *y, size, c);
        } else {
            text(line, x, *y, size, c);
        }
        *y += size * 0.35;
        line.clear();
    };
    for word in s.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if super::widgets::measure(&candidate, size).width > width && !line.is_empty() {
            flush(&mut line, y);
            line = word.to_string();
        } else {
            line = candidate;
        }
    }
    flush(&mut line, y);
}

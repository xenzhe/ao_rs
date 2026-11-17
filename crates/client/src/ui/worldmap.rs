//! Window: map of the world. The picture is the protagonist: framed like a cartographer's
//! plate, with your position pulsing on it, and a side column that tells where you are (or
//! what is under the cursor).
//!
//! Pictures: `Recursos/interface/es_mapa<N>.bmp` (513x594, one per world); the grid that says
//! which map is each 27x27 px cell is `init/mapsworlddata.dat`, the names `init/namemapa.dat`
//! (see `client_core::p3::hud`). The player is placed inside the cell of the current map
//! like the official minimap does (`ModMinimap.bas`).
use super::character::kit::{self, Icon};
use super::hud::{bmp_texture, map_title, world_info};
use super::theme::{self, FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, TEXT, TEXT_DIM, TEXT_FAINT};
use super::widgets::{measure_with, text_with};
use super::WindowId;
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
    kit::begin(ui, WindowId::WorldMap);
    let info = world_info(&game.recursos);
    let here = game.map.as_ref().and_then(|m| info.data.locate(m.number));
    // Worlds that have a picture.
    let pics: Vec<usize> = (0..info.data.worlds.len()).filter(|i| game.recursos.join("interface").join(format!("es_mapa{}.bmp", i + 1)).exists()).collect();
    if pics.is_empty() {
        kit::empty(r, "No se encontraron las imágenes del mundo.", "(Recursos/interface/es_mapa*.bmp)", u);
        return;
    }
    let world = TAB.with(Cell::get).filter(|w| pics.contains(w)).or(here.map(|h| h.0).filter(|w| pics.contains(w))).unwrap_or(pics[0]);

    let side_w = 252.0 * u;
    let left = Rect::new(r.x + 4.0 * u, r.y, r.w - side_w - 26.0 * u, r.h);
    let right = Rect::new(left.right() + 22.0 * u, r.y + 2.0 * u, side_w, r.h - 4.0 * u);

    // World tabs, only when there is more than one picture.
    let mut top = left.y;
    if pics.len() > 1 {
        let labels: Vec<String> = pics.iter().map(|w| TAB_NAMES.get(*w).copied().unwrap_or("Mundo").to_string()).collect();
        let active = pics.iter().position(|w| *w == world).unwrap_or(0);
        if let Some(i) = kit::tabs(left.x + 6.0 * u, left.y, &labels, active, u) {
            TAB.with(|t| t.set(Some(pics[i])));
        }
        top += 34.0 * u;
    }

    // Picture, fitted keeping its aspect, inside a plate with a frame.
    let area = Rect::new(left.x, top, left.w, left.bottom() - top);
    let plate_pad = 12.0 * u;
    let tex = bmp_texture(&game.recursos.join("interface").join(format!("es_mapa{}.bmp", world + 1)), FilterMode::Linear);
    let (pw, ph) = tex.as_ref().map(|t| (t.width(), t.height())).unwrap_or((513.0, 594.0));
    let s = ((area.w - 2.0 * plate_pad) / pw).min((area.h - 2.0 * plate_pad) / ph);
    let img = Rect::new((area.x + (area.w - pw * s) / 2.0).round(), (area.y + (area.h - ph * s) / 2.0).round(), (pw * s).round(), (ph * s).round());
    let plate = Rect::new(img.x - plate_pad, img.y - plate_pad, img.w + 2.0 * plate_pad, img.h + 2.0 * plate_pad);
    // Plate: shadow, dark wood, the picture, a warm wash and edge darkening.
    for i in 1..=5 {
        draw_rectangle(plate.x + i as f32 * 1.6 * u, plate.y + i as f32 * 2.2 * u, plate.w, plate.h, Color::new(0.0, 0.0, 0.0, 0.10));
    }
    draw_rectangle(plate.x, plate.y, plate.w, plate.h, Color::new(0.06, 0.045, 0.03, 1.0));
    draw_rectangle(img.x, img.y, img.w, img.h, Color::new(0.01, 0.01, 0.02, 1.0));
    if let Some(t) = &tex {
        draw_texture_ex(t, img.x, img.y, Color::new(1.0, 0.97, 0.90, 1.0), DrawTextureParams { dest_size: Some(vec2(img.w, img.h)), ..Default::default() });
    }
    theme::inner_vignette(img, 34.0 * u, 0.5);
    draw_rectangle(img.x, img.y, img.w, img.h, Color::new(0.45, 0.30, 0.1, 0.07));
    draw_rectangle_lines(img.x - 1.0, img.y - 1.0, img.w + 2.0, img.h + 2.0, 2.0, Color::new(0.0, 0.0, 0.0, 0.8));
    theme::metal_border(plate, 2.5 * u);
    for (px, py, sx, sy) in [(plate.x, plate.y, 1.0, 1.0), (plate.right(), plate.y, -1.0, 1.0), (plate.x, plate.bottom(), 1.0, -1.0), (plate.right(), plate.bottom(), -1.0, -1.0)] {
        theme::corner(vec2(px + sx * 2.0 * u, py + sy * 2.0 * u), sx, sy, 20.0 * u);
    }

    let w = &info.data.worlds[world];
    let (cw, ch) = (img.w / w.cols.max(1) as f32, img.h / w.rows.max(1) as f32);
    let cell_rect = |cx: u32, cy: u32| Rect::new(img.x + cx as f32 * cw, img.y + cy as f32 * ch, cw, ch);

    // Cell under the mouse.
    let m: Vec2 = mouse_position().into();
    let mut hovered: Option<(u16, Rect)> = None;
    if kit::active() && img.contains(m) {
        let (cx, cy) = (((m.x - img.x) / cw) as u32, ((m.y - img.y) / ch) as u32);
        let n = w.map_at(cx, cy);
        if n != 0 {
            hovered = Some((n, cell_rect(cx, cy)));
        }
    }
    if let Some((n, cr)) = hovered {
        draw_rectangle(cr.x, cr.y, cr.w, cr.h, Color::new(1.0, 0.92, 0.7, 0.16));
        draw_rectangle_lines(cr.x, cr.y, cr.w, cr.h, 1.5, Color::new(1.0, 0.92, 0.7, 0.9));
        // A small plaque with its name beside the cursor.
        let own = game.map.as_ref().filter(|mm| mm.number == n).map(|mm| mm.name.clone());
        let title = map_title(&info, n, own.as_deref());
        let tw = measure_with(FontKind::Title, &title, 15.0 * u).width + 18.0 * u;
        let px = (m.x + 14.0 * u).min(img.right() - tw - 4.0 * u);
        let py = (m.y - 30.0 * u).max(img.y + 4.0 * u);
        draw_rectangle(px, py, tw, 24.0 * u, Color::new(0.05, 0.04, 0.03, 0.92));
        draw_rectangle_lines(px, py, tw, 24.0 * u, 1.0, GOLD);
        text_with(FontKind::Title, &title, px + 9.0 * u, py + 17.0 * u, 15.0 * u, GOLD_BRIGHT);
    }

    // You are here: a breathing ring, a glow and a lit dot.
    if let (Some((hw, cx, cy)), Some(me)) = (here, game.me()) {
        if hw == world {
            let cr = cell_rect(cx, cy);
            let (fx, fy) = logic::tile_in_cell(me.x, me.y);
            let p = vec2(cr.x + fx * cr.w, cr.y + fy * cr.h);
            let t = get_time() as f32;
            let pulse = 0.5 + 0.5 * (t * 3.0).sin();
            let ring = ((t * 0.9).fract()).powf(0.7);
            draw_rectangle_lines(cr.x, cr.y, cr.w, cr.h, 2.0, Color::new(1.0, 0.9, 0.3, 0.45 + 0.4 * pulse));
            draw_circle(p.x, p.y, (13.0 + 5.0 * pulse) * u, Color::new(1.0, 0.85, 0.2, 0.18));
            draw_circle_lines(p.x, p.y, (5.0 + 22.0 * ring) * u, 1.5 * u, Color::new(1.0, 0.9, 0.35, 0.8 * (1.0 - ring)));
            draw_circle(p.x, p.y, 5.0 * u, Color::new(1.0, 0.95, 0.35, 1.0));
            draw_circle_lines(p.x, p.y, 5.0 * u, 1.5, BLACK);
            let label = "Tú";
            let d = measure_with(FontKind::Title, label, 14.0 * u);
            let lx = (p.x - d.width / 2.0).clamp(img.x + 6.0 * u, img.right() - d.width - 6.0 * u);
            let ly = if p.y - 26.0 * u > img.y + 20.0 * u { p.y - 20.0 * u } else { p.y + 32.0 * u };
            draw_rectangle(lx - 7.0 * u, ly - 14.0 * u, d.width + 14.0 * u, 20.0 * u, Color::new(0.0, 0.0, 0.0, 0.72));
            draw_rectangle_lines(lx - 7.0 * u, ly - 14.0 * u, d.width + 14.0 * u, 20.0 * u, 1.0, GOLD_BRIGHT);
            text_with(FontKind::Title, label, lx, ly, 14.0 * u, Color::new(1.0, 0.95, 0.4, 1.0));
        }
    }

    // ---- side column: where you are
    let current = game.map.as_ref().map(|m| m.number);
    let shown = hovered.map(|h| h.0).or(current);
    let x = right.x;
    let inner = right.w;
    let mut y = right.y;
    let kicker = if hovered.is_some() && hovered.map(|h| h.0) != current { "BAJO EL CURSOR" } else { "ESTÁS EN" };
    text_with(FontKind::Title, kicker, x, y + 14.0 * u, 13.0 * u, GOLD_DIM);
    y += 28.0 * u;
    match shown {
        Some(n) => {
            let own = (current == Some(n)).then(|| game.map.as_ref().map(|m| m.name.clone())).flatten();
            let title = map_title(&info, n, own.as_deref());
            let rows = super::widgets::wrap_lines(FontKind::Title, &title, 26.0 * u, inner);
            for row in rows.iter().take(3) {
                text_with(FontKind::Title, row, x, y + 24.0 * u, 26.0 * u, GOLD_BRIGHT);
                y += 30.0 * u;
            }
            kit::rule(x, y + 6.0 * u, inner, GOLD);
            y += 18.0 * u;
            text_with(FontKind::Body, &format!("Mapa {n}"), x, y + 12.0 * u, 14.0 * u, TEXT_FAINT);
            y += 26.0 * u;
            if let Some(d) = info.names.get(&n).map(|m| m.desc.trim()).filter(|d| !d.is_empty()) {
                y = kit::paragraph(FontKind::NarrativeItalic, d, x, y, inner, 17.0 * u, TEXT_DIM, 6) + 8.0 * u;
            }
            if current == Some(n) {
                if let (Some(map), Some(me)) = (game.map.as_ref(), game.me()) {
                    y += 4.0 * u;
                    kit::icon(Icon::Target, x + 9.0 * u, y + 9.0 * u, 15.0 * u, GOLD);
                    text_with(FontKind::Body, &format!("Casilla {}, {}", me.x, me.y), x + 26.0 * u, y + 14.0 * u, 16.0 * u, TEXT);
                    y += 28.0 * u;
                    let (label, c, ic) = if map.safe { ("Zona segura", GOOD, Icon::Shield) } else { ("Zona insegura", BAD, Icon::Skull) };
                    kit::icon(ic, x + 9.0 * u, y + 9.0 * u, 15.0 * u, c);
                    text_with(FontKind::Body, label, x + 26.0 * u, y + 14.0 * u, 16.0 * u, c);
                    y += 28.0 * u;
                    if !map.zone.trim().is_empty() {
                        kit::icon(Icon::Banner, x + 9.0 * u, y + 9.0 * u, 15.0 * u, GOLD_TEXT);
                        kit::fit(FontKind::Body, &map.zone, x + 26.0 * u, y + 14.0 * u, 16.0 * u, inner - 26.0 * u, GOLD_TEXT);
                    }
                }
            }
        }
        None => text_with(FontKind::NarrativeItalic, "Sin mapa", x, y + 18.0 * u, 18.0 * u, TEXT_DIM),
    }
    if here.is_none() && current.is_some() {
        kit::paragraph(FontKind::NarrativeItalic, "Este lugar no aparece en ningún mundo dibujado.", x, y + 8.0 * u, inner, 15.0 * u, TEXT_FAINT, 3);
    }

    // Legend and compass at the bottom of the column.
    let ly = right.bottom() - 92.0 * u;
    kit::rule(x, ly - 8.0 * u, inner, GOLD);
    text_with(FontKind::Title, "LEYENDA", x, ly + 12.0 * u, 13.0 * u, GOLD_TEXT);
    let pulse = 0.5 + 0.5 * (get_time() * 3.0).sin() as f32;
    draw_circle(x + 8.0 * u, ly + 30.0 * u, (7.0 + 2.0 * pulse) * u, Color::new(1.0, 0.85, 0.2, 0.2));
    draw_circle(x + 8.0 * u, ly + 30.0 * u, 4.0 * u, Color::new(1.0, 0.95, 0.35, 1.0));
    text_with(FontKind::Body, "Tu posición", x + 24.0 * u, ly + 35.0 * u, 14.0 * u, TEXT);
    draw_rectangle(x + 3.0 * u, ly + 46.0 * u, 10.0 * u, 10.0 * u, Color::new(1.0, 0.92, 0.7, 0.3));
    draw_rectangle_lines(x + 3.0 * u, ly + 46.0 * u, 10.0 * u, 10.0 * u, 1.0, Color::new(1.0, 0.92, 0.7, 0.9));
    text_with(FontKind::Body, "Pasa el cursor: su nombre", x + 24.0 * u, ly + 56.0 * u, 14.0 * u, TEXT_DIM);
    compass(vec2(right.right() - 30.0 * u, ly + 44.0 * u), 24.0 * u);
}

/// A small compass rose.
fn compass(c: Vec2, r: f32) {
    let gold = Color::new(GOLD.r, GOLD.g, GOLD.b, 0.9);
    draw_circle_lines(c.x, c.y, r * 0.78, 1.0, Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.8));
    draw_triangle(vec2(c.x, c.y - r), vec2(c.x - r * 0.2, c.y), vec2(c.x + r * 0.2, c.y), gold);
    draw_triangle(vec2(c.x, c.y + r), vec2(c.x - r * 0.2, c.y), vec2(c.x + r * 0.2, c.y), Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.9));
    draw_triangle(vec2(c.x - r, c.y), vec2(c.x, c.y - r * 0.2), vec2(c.x, c.y + r * 0.2), Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.9));
    draw_triangle(vec2(c.x + r, c.y), vec2(c.x, c.y - r * 0.2), vec2(c.x, c.y + r * 0.2), Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.9));
    text_with(FontKind::Title, "N", c.x - 4.5, c.y - r - 3.0, 13.0, GOLD_BRIGHT);
    let _ = (TEXT, TEXT_FAINT);
}

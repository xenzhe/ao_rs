//! In-game interface: console on top, chat/status bar at the bottom, character and spell
//! panel on the right.
use super::widgets::{bar, button, panel, text, GOLD_TEXT};
use crate::game::{tone_color, Game};
use crate::render::layout::Layout;
use macroquad::prelude::*;

pub fn draw(game: &mut Game, lay: &Layout, hour: f32, pixel_perfect: bool) {
    console(game, lay);
    bottom_bar(game, lay, hour, pixel_perfect);
    side_panel(game, lay);
}

fn console(game: &Game, lay: &Layout) {
    let (u, r) = (lay.u, lay.console);
    panel(r, u);
    let lines: Vec<_> = game.console.iter().rev().take(5).collect();
    for (i, (t, c)) in lines.iter().rev().enumerate() {
        text(t, r.x + 12.0 * u, r.y + 22.0 * u + i as f32 * 17.0 * u, 17.0 * u, tone_color(*c));
    }
}

fn bottom_bar(game: &Game, lay: &Layout, hour: f32, pixel_perfect: bool) {
    let (u, b) = (lay.u, lay.bottom);
    panel(b, u);
    match &game.chat {
        Some(t) => {
            draw_rectangle(b.x + 8.0 * u, b.y + 8.0 * u, b.w - 16.0 * u, 22.0 * u, Color::new(0.0, 0.0, 0.0, 0.6));
            text(&format!("> {t}_"), b.x + 12.0 * u, b.y + 25.0 * u, 18.0 * u, WHITE);
        }
        None => text("Enter: hablar   Flechas: caminar   1-9: hechizo   Clic: lanzar   F2: pixel-perfect", b.x + 12.0 * u, b.y + 24.0 * u, 16.0 * u, GRAY),
    }
    if let (Some(me), Some(map)) = (game.me(), game.map.as_ref()) {
        let s = format!("{}  (Mapa {})  [{}, {}]   {:02}:{:02} hs", map.name, map.number, me.x, me.y, hour as i32, (hour.fract() * 60.0) as i32);
        text(&s, b.x + 12.0 * u, b.y + 48.0 * u, 16.0 * u, Color::new(0.85, 0.7, 0.4, 1.0));
    }
    let fps = format!("{} fps  x{:.2}{}", get_fps(), lay.scale, if pixel_perfect { " PP" } else { "" });
    let d = measure_text(&fps, None, (16.0 * u) as u16, 1.0);
    text(&fps, b.right() - d.width - 12.0 * u, b.y + 48.0 * u, 16.0 * u, GRAY);
}

fn side_panel(game: &mut Game, lay: &Layout) {
    let (u, p) = (lay.u, lay.panel);
    panel(p, u);
    let (x, w) = (p.x + 14.0 * u, p.w - 28.0 * u);
    let mut y = p.y + 34.0 * u;
    let name = game.me().map(|e| e.name.clone()).unwrap_or_default();
    text(&name, x, y, 24.0 * u, Color::new(0.55, 0.8, 1.0, 1.0));
    y += 22.0 * u;
    text(&format!("Nivel {}", game.stats.level), x, y, 18.0 * u, Color::new(0.85, 0.7, 0.4, 1.0));
    y += 10.0 * u;
    bar(Rect::new(x, y, w, 12.0 * u), game.stats.exp as f32, game.stats.next_exp as f32, Color::new(0.75, 0.6, 0.15, 1.0), "", u);
    y += 30.0 * u;
    text(&format!("Exp {}/{}", game.stats.exp, game.stats.next_exp), x, y, 15.0 * u, GRAY);
    y += 14.0 * u;
    let (hp, mhp, mp, mmp) = (game.stats.hp.get(), game.stats.max_hp.get(), game.stats.mana.get(), game.stats.max_mana.get());
    bar(Rect::new(x, y, w, 20.0 * u), hp as f32, mhp as f32, Color::new(0.75, 0.1, 0.08, 1.0), &format!("{hp}/{mhp}"), u);
    y += 26.0 * u;
    bar(Rect::new(x, y, w, 20.0 * u), mp as f32, mmp as f32, Color::new(0.12, 0.3, 0.85, 1.0), &format!("{mp}/{mmp}"), u);
    y += 50.0 * u;

    text("HECHIZOS", x, y, 20.0 * u, GOLD_TEXT);
    y += 10.0 * u;
    let spells: Vec<(String, i32)> = game.spells.iter().map(|s| (s.name.clone(), s.mana)).collect();
    for (i, (name, mana)) in spells.iter().enumerate() {
        if button(Rect::new(x, y, w, 28.0 * u), &format!("{}. {} ({})", i + 1, name, mana), game.selected == i, u) {
            game.selected = i;
        }
        y += 34.0 * u;
    }
    if button(Rect::new(x, y, w, 34.0 * u), if game.casting { "Elige objetivo..." } else { "LANZAR" }, game.casting, u) {
        game.casting = true;
    }
    y += 56.0 * u;
    if let Some(s) = game.spells.get(game.selected) {
        text(&format!("Alcance {} - Enfr. {:.1}s", s.range, s.cooldown_ms as f32 / 1000.0), x, y, 15.0 * u, GRAY);
    }
    y += 40.0 * u;
    text("SEGURIDAD", x, y, 20.0 * u, GOLD_TEXT);
    for l in ["Servidor autoritativo:", " - paso y bloqueos de AO20", " - secuencia anti-replay", " - mana, alcance y vision", " - limite de paquetes", "Stats ofuscados en RAM"] {
        y += 18.0 * u;
        text(l, x, y, 15.0 * u, Color::new(0.6, 0.75, 0.6, 1.0));
    }
}

/// Screen shown after a disconnection or kick.
pub fn kicked(reason: &str, u: f32) {
    let r = Rect::new(screen_width() / 2.0 - 220.0 * u, screen_height() / 2.0 - 60.0 * u, 440.0 * u, 120.0 * u);
    panel(r, u);
    text("Desconectado", r.x + 24.0 * u, r.y + 44.0 * u, 30.0 * u, YELLOW);
    text(reason, r.x + 24.0 * u, r.y + 80.0 * u, 18.0 * u, WHITE);
}

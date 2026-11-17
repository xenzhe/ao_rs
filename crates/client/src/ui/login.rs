//! Login screen. OWNER: Sonnet 1 (docs/P3_CONTRATO.md): account login and new account;
//! the development login (name only) stays available when the server allows it.
use super::widgets::{button, panel, text, text_centered};
use super::Ui;
use crate::assets::Assets;
use crate::game::{tone_color, Game};
use ao_protocol::Tone;
use ao_rules::valid_name;
use macroquad::prelude::*;

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets) {
    let _ = assets;
    let u = ui.u;
    let t = get_time() as f32;
    let (sw, sh) = (screen_width(), screen_height());
    clear_background(Color::new(0.04, 0.03, 0.02, 1.0));
    for i in 0..60 {
        let x = (i as f32 * 97.0 + t * 12.0 * (1.0 + (i % 3) as f32)) % sw;
        let y = sh - (i as f32 * 53.0 + t * 30.0) % sh;
        draw_circle(x, y, 1.5 * u, Color::new(1.0, 0.6, 0.2, 0.5));
    }
    let r = Rect::new(sw / 2.0 - 190.0 * u, sh / 2.0 - 140.0 * u, 380.0 * u, 270.0 * u);
    panel(r, u);
    text_centered("ARGENTUM", sw / 2.0, r.y + 66.0 * u, 50.0 * u, Color::new(0.85, 0.65, 0.3, 1.0));
    text_centered("prototipo en Rust - mapas de AO20", sw / 2.0, r.y + 92.0 * u, 17.0 * u, GRAY);
    text("Nombre del personaje:", r.x + 30.0 * u, r.y + 135.0 * u, 20.0 * u, WHITE);
    let field = Rect::new(r.x + 30.0 * u, r.y + 145.0 * u, r.w - 60.0 * u, 32.0 * u);
    draw_rectangle(field.x, field.y, field.w, field.h, Color::new(0.0, 0.0, 0.0, 0.6));
    draw_rectangle_lines(field.x, field.y, field.w, field.h, 1.0, Color::new(0.62, 0.48, 0.24, 1.0));
    let cursor = if (t * 2.0) as i32 % 2 == 0 { "_" } else { "" };
    text(&format!("{}{}", game.name_input, cursor), field.x + 8.0 * u, field.y + 23.0 * u, 22.0 * u, WHITE);

    while let Some(ch) = get_char_pressed() {
        if (ch.is_ascii_alphanumeric() || ch == '_') && game.name_input.len() < 16 {
            game.name_input.push(ch);
        }
    }
    if is_key_pressed(KeyCode::Backspace) {
        game.name_input.pop();
    }
    let go = button(Rect::new(sw / 2.0 - 70.0 * u, r.y + 195.0 * u, 140.0 * u, 36.0 * u), "Conectar", false, u) || is_key_pressed(KeyCode::Enter);
    if go {
        if valid_name(&game.name_input) {
            game.connect();
        } else {
            game.say("Nombre invalido: 3 a 16 letras o numeros.", Tone::Warn);
        }
    }
    if let Some((msg, c)) = game.console.back() {
        text(msg, r.x + 20.0 * u, r.bottom() - 14.0 * u, 16.0 * u, tone_color(*c));
    }
}

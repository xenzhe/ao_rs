//! Console (messages on top) and chat typing. OWNER: Sonnet 3 (docs/P3_CONTRATO.md):
//! channel tabs (normal, whisper, party, guild, global), `/` commands to the channels...
use crate::game::{tone_color, Game};
use crate::render::layout::Layout;
use crate::ui::widgets::{panel, text};
use crate::ui::Ui;
use ao_rules::MAX_CHAT;
use macroquad::prelude::*;

pub fn draw(ui: &mut Ui, game: &mut Game, lay: &Layout) {
    let (u, r) = (lay.u, lay.console);
    panel(r, u);
    let lines: Vec<_> = game.console.iter().rev().take(5).collect();
    for (i, (t, c)) in lines.iter().rev().enumerate() {
        text(t, r.x + 12.0 * u, r.y + 22.0 * u + i as f32 * 17.0 * u, 17.0 * u, tone_color(*c));
    }
    chat_input(ui, game);
}

/// Enter opens the chat line, Enter sends, Escape cancels. While typing the keyboard is
/// the UI's.
fn chat_input(ui: &mut Ui, game: &mut Game) {
    if game.chat.is_none() {
        if !ui.keyboard_captured && is_key_pressed(KeyCode::Enter) {
            game.chat = Some(String::new());
            while get_char_pressed().is_some() {}
        }
        return;
    }
    ui.keyboard_captured = true;
    let Some(t) = &mut game.chat else { return };
    while let Some(ch) = get_char_pressed() {
        if !ch.is_control() && t.chars().count() < MAX_CHAT {
            t.push(ch);
        }
    }
    if is_key_pressed(KeyCode::Backspace) {
        t.pop();
    }
    if is_key_pressed(KeyCode::Enter) {
        let text = game.chat.take().unwrap_or_default();
        game.send_chat(text);
    } else if is_key_pressed(KeyCode::Escape) {
        game.chat = None;
    }
}

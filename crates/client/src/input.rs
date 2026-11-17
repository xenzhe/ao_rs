//! Keyboard and mouse during the game, turned into game actions.
use crate::game::Game;
use crate::render::layout::{Layout, TILE};
use crate::render::Renderer;
use ao_protocol::Tone;
use ao_rules::{Dir, MAX_CHAT};
use macroquad::prelude::*;

const SPELL_KEYS: [KeyCode; 9] = [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4, KeyCode::Key5, KeyCode::Key6, KeyCode::Key7, KeyCode::Key8, KeyCode::Key9];

/// Returns the direction held this frame (for movement), if any.
pub fn update(game: &mut Game, renderer: &mut Renderer, lay: &Layout, now: f64) -> Option<Dir> {
    if is_key_pressed(KeyCode::F2) {
        renderer.pixel_perfect = !renderer.pixel_perfect;
    }
    if game.chat.is_some() {
        chat_input(game);
        return None;
    }
    while get_char_pressed().is_some() {}
    if is_key_pressed(KeyCode::Enter) {
        game.chat = Some(String::new());
    }
    for (i, k) in SPELL_KEYS.iter().enumerate() {
        if is_key_pressed(*k) && i < game.spells.len() {
            game.selected = i;
            game.casting = true;
        }
    }
    if is_key_pressed(KeyCode::Escape) || is_mouse_button_pressed(MouseButton::Right) {
        game.casting = false;
    }
    if is_mouse_button_pressed(MouseButton::Left) {
        world_click(game, lay);
    }
    let dir = [(KeyCode::Up, Dir::Up), (KeyCode::Down, Dir::Down), (KeyCode::Left, Dir::Left), (KeyCode::Right, Dir::Right)]
        .into_iter()
        .find(|(k, _)| is_key_down(*k))
        .map(|(_, d)| d);
    if let Some(d) = dir {
        game.try_step(d, now);
    }
    dir
}

fn chat_input(game: &mut Game) {
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

/// Click on the world: cast the armed spell, or look at whoever is there.
fn world_click(game: &mut Game, lay: &Layout) {
    let origin = Renderer::origin(game);
    let Some(wp) = lay.screen_to_world(mouse_position().into(), origin) else { return };
    let (tx, ty) = ((wp.x / TILE).floor() as i32 + 1, (wp.y / TILE).floor() as i32 + 1);
    if game.casting {
        game.cast_at(tx, ty);
        game.casting = false;
    } else if let Some(name) = game.ents.values().find(|e| e.x == tx && e.y == ty).map(|e| e.name.clone()) {
        game.say(format!("Ves a {name}."), Tone::Info);
    }
}

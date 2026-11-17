//! Keyboard and mouse on the world, after the UI had its turn (`ui.mouse_captured` /
//! `ui.keyboard_captured` say what it used this frame). Chat typing is the console's.
use crate::game::Game;
use crate::render::layout::{Layout, TILE};
use crate::render::Renderer;
use crate::ui::Ui;
use ao_protocol::Tone;
use ao_rules::Dir;
use macroquad::prelude::*;

const SPELL_KEYS: [KeyCode; 9] = [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4, KeyCode::Key5, KeyCode::Key6, KeyCode::Key7, KeyCode::Key8, KeyCode::Key9];

/// Returns the direction held this frame (for movement), if any.
pub fn update(game: &mut Game, renderer: &mut Renderer, lay: &Layout, now: f64, ui: &Ui) -> Option<Dir> {
    if !ui.keyboard_captured {
        if is_key_pressed(KeyCode::F2) {
            renderer.pixel_perfect = !renderer.pixel_perfect;
        }
        for (i, k) in SPELL_KEYS.iter().enumerate() {
            if is_key_pressed(*k) && i < game.spells.len() {
                game.selected = i;
                game.casting = true;
            }
        }
        if is_key_pressed(KeyCode::Escape) {
            game.casting = false;
        }
    }
    if !ui.mouse_captured {
        if is_mouse_button_pressed(MouseButton::Right) {
            game.casting = false;
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            world_click(game, lay);
        }
    }
    if ui.keyboard_captured {
        return None;
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

/// Tile under the mouse (1-based map coordinates), if over the world view.
pub fn tile_under_mouse(game: &Game, lay: &Layout) -> Option<(i32, i32)> {
    let origin = Renderer::origin(game);
    let wp = lay.screen_to_world(mouse_position().into(), origin)?;
    Some(((wp.x / TILE).floor() as i32 + 1, (wp.y / TILE).floor() as i32 + 1))
}

/// Click on the world: cast the armed spell, or interact with whatever is there (NPCs:
/// banker, merchant, priest, quests...; players: look).
fn world_click(game: &mut Game, lay: &Layout) {
    let Some((tx, ty)) = tile_under_mouse(game, lay) else { return };
    if game.casting {
        game.cast_at(tx, ty);
        game.casting = false;
        return;
    }
    let who = game.ents.values().find(|e| e.x == tx && e.y == ty).map(|e| (e.name.clone(), e.kind));
    match who {
        Some((_, ao_protocol::EntityKind::Creature)) => game.send(&ao_protocol::C2S::Interact { tx, ty }),
        Some((name, _)) => game.say(format!("Ves a {name}."), Tone::Info),
        None => {}
    }
}

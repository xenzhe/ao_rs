//! Keyboard and mouse on the world, after the UI had its turn (`ui.mouse_captured` /
//! `ui.keyboard_captured` say what it used this frame). Chat typing is the console's.
use crate::game::Game;
use crate::render::layout::{Layout, TILE};
use crate::render::Renderer;
use crate::ui::Ui;
use ao_protocol::Tone;
use ao_rules::Dir;
use macroquad::prelude::*;

thread_local! {
    /// The steal key was pressed: the next left click picks the victim.
    static STEALING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TAMING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Returns the direction held this frame (for movement), if any.
pub fn update(game: &mut Game, renderer: &mut Renderer, lay: &Layout, now: f64, ui: &Ui, assets: &crate::assets::Assets) -> Option<Dir> {
    if !ui.keyboard_captured {
        if is_key_pressed(KeyCode::F2) {
            renderer.pixel_perfect = !renderer.pixel_perfect;
        }
        // Keys 1-9 arm spells in the HUD (player-assigned, config/spellkeys.ini).
        // Action keys (config/keys.ini, Options window): the item ones act on the slot
        // selected in the inventory.
        use client_core::p3::keys::Action;
        let sel = crate::ui::inventory::selected_inv();
        let item = (sel > 0).then(|| game.local.inventory.get(sel as usize - 1).copied().flatten()).flatten();
        if crate::keys::pressed(Action::Meditate) {
            game.toggle_meditation();
        }
        if crate::keys::pressed(Action::PickUp) {
            game.send(&ao_protocol::C2S::PickUp);
        }
        if crate::keys::pressed(Action::Safe) {
            game.toggle_safe();
        }
        if crate::keys::pressed(Action::Hide) {
            game.send(&ao_protocol::C2S::Skill(ao_protocol::p4::skill::SkillC2S::Hide));
        }
        if crate::keys::pressed(Action::Steal) {
            STEALING.with(|s| s.set(true));
            TAMING.with(|s| s.set(false));
            game.say("Haz clic sobre el jugador al que quieres robar.", Tone::Info);
        }
        if crate::keys::pressed(Action::Tame) {
            TAMING.with(|s| s.set(true));
            STEALING.with(|s| s.set(false));
            game.say("Haz clic sobre la criatura que quieres domar.", Tone::Info);
        }
        if let Some(it) = item {
            if crate::keys::pressed(Action::Use) {
                game.send(&ao_protocol::C2S::UseItem { slot: sel });
            }
            if crate::keys::pressed(Action::Equip) {
                game.send(&ao_protocol::C2S::EquipItem { slot: sel });
            }
            if crate::keys::pressed(Action::Drop) && !it.equipped {
                game.send(&ao_protocol::C2S::DropItem { slot: sel, amount: 1 });
            }
        }
        // Attack (Ctrl by default): melee on the tile in front, at the server's pace while held.
        if crate::keys::down(Action::Attack) {
            thread_local! {
                static LAST: std::cell::Cell<f64> = const { std::cell::Cell::new(-10.0) };
            }
            let interval = ao_rules::combat::ATTACK_INTERVAL_MS as f64 / 1000.0;
            if now - LAST.get() >= interval && game.me().is_some() {
                LAST.set(now);
                let seq = game.next_seq();
                game.send(&ao_protocol::C2S::Melee { seq });
            }
        }
        if is_key_pressed(KeyCode::Escape) {
            game.casting = false;
            // Esc cancels a pending work target, or stops the running work.
            if game.local.p4.work.pending.take().is_none() {
                if let Ok(m) = game.plan_stop_work() {
                    game.send(&m);
                }
            }
        }
    }
    if !ui.mouse_captured {
        if is_mouse_button_pressed(MouseButton::Right) {
            if game.casting {
                game.casting = false;
            } else if let Some((tx, ty)) = tile_under_mouse(game, lay) {
                // Right click on a nearby player: propose a trade.
                game.trade_with_tile(tx, ty);
            }
        }
        if is_mouse_button_pressed(MouseButton::Left) && STEALING.with(|s| s.replace(false)) {
            if let Some((x, y)) = tile_under_mouse(game, lay) {
                game.send(&ao_protocol::C2S::Skill(ao_protocol::p4::skill::SkillC2S::Steal { x, y }));
            }
        } else if is_mouse_button_pressed(MouseButton::Left) && TAMING.with(|s| s.replace(false)) {
            if let Some((x, y)) = tile_under_mouse(game, lay) {
                game.send(&ao_protocol::C2S::Skill(ao_protocol::p4::skill::SkillC2S::Tame { x, y }));
            }
        } else if is_mouse_button_pressed(MouseButton::Left) {
            if game.local.p4.work.pending.is_some() {
                work_click(game, lay, assets);
            } else {
                world_click(game, lay);
            }
        }
    }
    if ui.keyboard_captured {
        return None;
    }
    use client_core::p3::keys::Action;
    let dir = [(Action::MoveUp, Dir::Up), (Action::MoveDown, Dir::Down), (Action::MoveLeft, Dir::Left), (Action::MoveRight, Dir::Right)]
        .into_iter()
        .find(|(a, _)| crate::keys::down(*a))
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

/// Click on the world after "use" on a tool or ore (P4 work): start working on that tile.
fn work_click(game: &mut Game, lay: &Layout, assets: &crate::assets::Assets) {
    use client_core::p4::work::Targeting;
    let Some(target) = game.local.p4.work.pending.take() else { return };
    let Some((tx, ty)) = tile_under_mouse(game, lay) else { return };
    let plan = match target {
        Targeting::Tool(_) => {
            let tool = game.local.inventory.iter().flatten().find(|s| s.equipped && assets.objs.get(s.obj as u32).is_some_and(|o| o.obj_type == ao_formats::dat::obj::ObjType::WorkingTools)).and_then(|s| assets.objs.get(s.obj as u32));
            game.plan_work(tool, tx, ty)
        }
        Targeting::Smelt { slot } => game.plan_smelt(slot, tx, ty),
    };
    match plan {
        Ok(m) => game.send(&m),
        Err(why) => game.say(why, Tone::Warn),
    }
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
        // Map objects with an action (anvil, forge...) are the server's to answer.
        None => game.send(&ao_protocol::C2S::Interact { tx, ty }),
    }
}

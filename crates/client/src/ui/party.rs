//! Window: party (members, invite answer, leave/kick). OWNER: Sonnet 3 (docs/P3_CONTRATO.md).
//!
//! Besides the window (key P) this file draws the floating invitation notice (visible with
//! the window closed) and the "invite a player" mode: the button arms it and the next click
//! on a player of the map sends the invitation. `invite_player_under_mouse` does the same
//! for any other trigger (e.g. a shift-click handler in `input.rs`).
use crate::assets::Assets;
use crate::game::Game;
use crate::render::layout::Layout;
use crate::ui::theme::{BAD, GOLD_TEXT, GOOD, MUTED, TEXT};
use crate::ui::widgets::{button, measure, text, text_fit, tooltip};
use crate::ui::Ui;
use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    /// "Click a player to invite" is on.
    static INVITING: Cell<bool> = const { Cell::new(false) };
}

/// Sends an invitation to the player under the mouse (the tile the mouse is on, in the
/// world view). Returns whether an invitation was sent; says why in the console if not.
///
/// To hook it in the world click (`input.rs::world_click`, not ours), call it where a
/// player is clicked, e.g. `if is_key_down(KeyCode::LeftShift) { party::invite_player_under_mouse(game, lay); return; }`.
pub fn invite_player_under_mouse(game: &mut Game, lay: &Layout) -> bool {
    let Some((tx, ty)) = crate::input::tile_under_mouse(game, lay) else { return false };
    match game.player_at(tx, ty) {
        Some(id) => game.party_invite(id),
        None => {
            game.say("No hay ningún jugador ahí.", ao_protocol::Tone::Warn);
            false
        }
    }
}

/// Per-frame pieces outside the window (called by `console::draw`).
pub fn frame_hooks(ui: &mut Ui, game: &mut Game, lay: &Layout) {
    invite_mode(ui, game, lay);
    invitation_notice(ui, game, lay);
}

fn invite_mode(ui: &mut Ui, game: &mut Game, lay: &Layout) {
    if !INVITING.get() {
        return;
    }
    let u = lay.u;
    if is_key_pressed(KeyCode::Escape) || is_mouse_button_pressed(MouseButton::Right) {
        INVITING.set(false);
        return;
    }
    let msg = "Haz clic en el jugador que quieres invitar al grupo  (Esc o clic derecho: cancelar)";
    let d = measure(msg, 17.0 * u);
    let b = Rect::new(lay.world.center().x - d.width / 2.0 - 12.0 * u, lay.world.y + 8.0 * u, d.width + 24.0 * u, 28.0 * u);
    draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.04, 0.03, 0.02, 0.92));
    draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.5 * u, GOLD_TEXT);
    text(msg, b.x + 12.0 * u, b.y + 19.0 * u, 17.0 * u, TEXT);
    if is_mouse_button_pressed(MouseButton::Left) && !ui.mouse_captured && lay.world.contains(mouse_position().into()) {
        invite_player_under_mouse(game, lay);
        INVITING.set(false);
        ui.mouse_captured = true;
    }
}

/// The pending invitation, floating over the bottom of the world with its buttons, so it
/// is seen (and answered) with the window closed.
fn invitation_notice(ui: &mut Ui, game: &mut Game, lay: &Layout) {
    let Some((_, who)) = game.local.social.party.invitation.clone() else { return };
    let u = lay.u;
    let (w, h) = (380.0 * u, 64.0 * u);
    let r = Rect::new((lay.world.center().x - w / 2.0).round(), (lay.world.bottom() - h - 10.0 * u).round(), w, h);
    let pulse = 0.55 + 0.45 * ((get_time() * 4.0).sin() as f32 * 0.5 + 0.5);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.07, 0.05, 0.03, 0.96));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0 * u, Color::new(GOLD_TEXT.r, GOLD_TEXT.g, GOLD_TEXT.b, pulse));
    text_fit(&format!("{who} te invita a su grupo"), r.x + 12.0 * u, r.y + 23.0 * u, 20.0 * u, r.w - 24.0 * u, GOLD_TEXT);
    let by = r.y + 32.0 * u;
    if button(Rect::new(r.x + 12.0 * u, by, 110.0 * u, 24.0 * u), "Aceptar", false, u) {
        game.party_respond(true);
    }
    if button(Rect::new(r.x + 130.0 * u, by, 110.0 * u, 24.0 * u), "Rechazar", false, u) {
        game.party_respond(false);
    }
    text("P: ver el grupo", r.x + 252.0 * u, by + 17.0 * u, 15.0 * u, MUTED);
    ui.capture_mouse_if_over(r);
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let _ = assets;
    let u = ui.u;
    let p = &game.local.social.party;
    let (members, leader, safe, invitation) = (p.members.clone(), p.leader, p.safe, p.invitation.clone());
    let i_lead = game.is_party_leader();
    let mut y = r.y + 16.0 * u;

    if members.is_empty() {
        text("No estás en ningún grupo.", r.x, y, 19.0 * u, TEXT);
        y += 22.0 * u;
        text_fit("Invita a un jugador y serás el líder del grupo.", r.x, y, 16.0 * u, r.w, MUTED);
    } else {
        let lead_name = members.iter().find(|m| m.id == leader).map(|m| m.name.as_str()).unwrap_or("?");
        text_fit(&format!("Líder: {lead_name}"), r.x, y, 19.0 * u, r.w, GOLD_TEXT);
        text(&format!("{} miembros", members.len()), r.x, y + 20.0 * u, 15.0 * u, MUTED);
        y += 30.0 * u;
        for m in &members {
            let row = Rect::new(r.x, y, r.w, 26.0 * u);
            draw_rectangle(row.x, row.y, row.w, row.h, Color::new(0.0, 0.0, 0.0, if row.contains(mouse_position().into()) { 0.4 } else { 0.25 }));
            let is_leader = m.id == leader;
            if is_leader {
                // A small gold crown-like diamond for the leader.
                let c = vec2(row.x + 12.0 * u, row.center().y);
                draw_poly(c.x, c.y, 4, 6.0 * u, 0.0, GOLD_TEXT);
            }
            let me = m.id == game.my_id;
            let label = if me { format!("{} (tú)", m.name) } else { m.name.clone() };
            text_fit(&label, row.x + 26.0 * u, row.y + 18.0 * u, 18.0 * u, row.w - 120.0 * u, if is_leader { GOLD_TEXT } else { TEXT });
            if i_lead && !me && button(Rect::new(row.right() - 86.0 * u, row.y + 2.0 * u, 82.0 * u, 22.0 * u), "Expulsar", false, u) {
                game.party_kick(m.id);
            }
            y += 28.0 * u;
        }
    }

    if let Some((_, who)) = invitation {
        let b = Rect::new(r.x, y + 6.0 * u, r.w, 58.0 * u);
        draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.2, 0.14, 0.05, 0.8));
        draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, GOLD_TEXT);
        text_fit(&format!("{who} te invita a su grupo"), b.x + 8.0 * u, b.y + 20.0 * u, 17.0 * u, b.w - 16.0 * u, GOLD_TEXT);
        if button(Rect::new(b.x + 8.0 * u, b.y + 28.0 * u, 100.0 * u, 24.0 * u), "Aceptar", false, u) {
            game.party_respond(true);
        }
        if button(Rect::new(b.x + 116.0 * u, b.y + 28.0 * u, 100.0 * u, 24.0 * u), "Rechazar", false, u) {
            game.party_respond(false);
        }
    }

    // Bottom: actions.
    let by = r.bottom() - 30.0 * u;
    let w = (r.w - 2.0 * 6.0 * u) / 3.0;
    let inviting = INVITING.get();
    let invite = Rect::new(r.x, by, w, 28.0 * u);
    if button(invite, if inviting { "Haz clic..." } else { "Invitar" }, inviting, u) {
        INVITING.set(!inviting);
    }
    let leave = Rect::new(r.x + w + 6.0 * u, by, w, 28.0 * u);
    let in_party = !members.is_empty();
    if button(leave, if i_lead { "Disolver" } else { "Salir" }, false, u) && in_party {
        game.party_leave();
    }
    let seguro = Rect::new(r.x + 2.0 * (w + 6.0 * u), by, w, 28.0 * u);
    if button(seguro, if safe { "Seguro: sí" } else { "Seguro: no" }, safe, u) {
        game.party_toggle_safe();
    }
    let status_y = by - 8.0 * u;
    let (msg, c) = if safe { ("Seguro puesto: nadie puede invitarte.", GOOD) } else { ("Seguro quitado: pueden invitarte.", BAD) };
    text_fit(msg, r.x, status_y, 15.0 * u, r.w, c);
    let m: Vec2 = mouse_position().into();
    if invite.contains(m) {
        tooltip(&["Invitar a un jugador".into(), "Pulsa y haz clic sobre él en el mapa.".into(), "El seguro del otro debe estar quitado.".into()], u);
    } else if seguro.contains(m) {
        tooltip(&["Seguro de grupo".into(), "Puesto: nadie puede invitarte a un grupo.".into(), "Quítalo para recibir invitaciones.".into()], u);
    } else if leave.contains(m) && in_party {
        tooltip(&[if i_lead { "Disolver el grupo".into() } else { "Salir del grupo".into() }], u);
    }
}

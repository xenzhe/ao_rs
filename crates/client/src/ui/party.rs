//! Window: the company (party). Each member is a medallion with their face, name and life
//! when they are in sight; the leader wears a crown. Besides the window (key P) this file
//! draws the floating invitation notice (visible with the window closed) and the "invite a
//! player" mode: the button arms it and the next click on a player of the map sends the
//! invitation. `invite_player_under_mouse` does the same for any other trigger (e.g. a
//! shift-click handler in `input.rs`).
use crate::assets::Assets;
use crate::game::Game;
use crate::render::layout::Layout;
use crate::ui::character::kit::{self, Icon};
use crate::ui::theme::{self, FontKind, Surface, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, HP, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::ui::widgets::{button_kind, measure_with, text_with, tooltip, Kind};
use crate::ui::{Ui, WindowId};
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

/// A banner over the world: engraved leather strip with a gold hairline.
fn strip(r: Rect, u: f32, accent: Color) {
    draw_rectangle(r.x + 3.0 * u, r.y + 5.0 * u, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.3));
    theme::surface(r, Surface::Leather, u);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5 * u, accent);
    draw_rectangle_lines(r.x + 4.0 * u, r.y + 4.0 * u, r.w - 8.0 * u, r.h - 8.0 * u, 1.0, Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.6));
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
    let msg = "Haz clic en el jugador que quieres invitar";
    let hint = "Esc o clic derecho: cancelar";
    let w = measure_with(FontKind::Title, msg, 19.0 * u).width + 70.0 * u;
    let b = Rect::new((lay.world.center().x - w / 2.0).round(), lay.world.y + 10.0 * u, w, 50.0 * u);
    strip(b, u, GOLD);
    kit::icon(Icon::Banner, b.x + 24.0 * u, b.center().y - 3.0 * u, 20.0 * u, GOLD_BRIGHT);
    text_with(FontKind::Title, msg, b.x + 44.0 * u, b.y + 24.0 * u, 19.0 * u, GOLD_BRIGHT);
    text_with(FontKind::NarrativeItalic, hint, b.x + 44.0 * u, b.y + 42.0 * u, 14.0 * u, TEXT_DIM);
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
    let (w, h) = (540.0 * u, 86.0 * u);
    let r = Rect::new((lay.world.center().x - w / 2.0).round(), (lay.world.bottom() - h - 12.0 * u).round(), w, h);
    let pulse = 0.5 + 0.5 * (get_time() * 3.0).sin() as f32;
    strip(r, u, kit::mix(GOLD_DIM, GOLD_BRIGHT, pulse));
    kit::icon(Icon::Banner, r.x + 32.0 * u, r.y + 34.0 * u, 28.0 * u, GOLD_BRIGHT);
    kit::fit(FontKind::Title, &format!("{who} te invita"), r.x + 60.0 * u, r.y + 34.0 * u, 22.0 * u, r.w - 60.0 * u - 170.0 * u, GOLD_BRIGHT);
    text_with(FontKind::NarrativeItalic, "A su compañía: viajaréis juntos.", r.x + 60.0 * u, r.y + 54.0 * u, 16.0 * u, TEXT_DIM);
    text_with(FontKind::Body, "P: ver el grupo", r.x + 60.0 * u, r.y + 72.0 * u, 13.0 * u, TEXT_FAINT);
    if button_kind(Rect::new(r.right() - 150.0 * u, r.y + 14.0 * u, 132.0 * u, 28.0 * u), "Aceptar", Kind::Primary, true, u) {
        game.party_respond(true);
    }
    if button_kind(Rect::new(r.right() - 150.0 * u, r.y + 48.0 * u, 132.0 * u, 26.0 * u), "Rechazar", Kind::Secondary, true, u) {
        game.party_respond(false);
    }
    ui.capture_mouse_if_over(r);
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Party);
    let p = &game.local.social.party;
    let (members, leader, safe, invitation) = (p.members.clone(), p.leader, p.safe, p.invitation.clone());
    let i_lead = game.is_party_leader();
    let in_party = !members.is_empty();
    let mut tip: Vec<String> = Vec::new();
    let r = Rect::new(r.x + 8.0 * u, r.y + 2.0 * u, r.w - 16.0 * u, r.h - 4.0 * u);
    let foot_h = 120.0 * u;
    let body = Rect::new(r.x, r.y, r.w, r.h - foot_h);

    if !in_party {
        kit::empty(body, "No viajas en compañía.", "Invita a alguien y serás su capitán.", u);
    } else {
        let lead_name = members.iter().find(|m| m.id == leader).map(|m| m.name.as_str()).unwrap_or("?");
        let mut y = kit::section("La compañía", r.x, r.y, r.w, u);
        text_with(FontKind::NarrativeItalic, &format!("Capitaneada por {lead_name}"), r.x, y + 12.0 * u, 16.0 * u, TEXT_DIM);
        let cnt = format!("{} miembros", members.len());
        kit::text_right(FontKind::Body, &cnt, r.right(), y + 12.0 * u, 14.0 * u, TEXT_FAINT);
        y += 26.0 * u;
        let row_h = 54.0 * u;
        for m in &members {
            let row = Rect::new(r.x, y, r.w, row_h - 4.0 * u);
            if y + row_h > body.bottom() {
                break;
            }
            let (hov, _) = kit::row(row, false, u);
            let is_leader = m.id == leader;
            let me = m.id == game.my_id;
            let ent = game.ents.get(&m.id);
            let head = ent.map(|e| e.head);
            let ring = if is_leader { GOLD_BRIGHT } else { GOLD_DIM };
            kit::medallion(assets, head, &m.name, row.x + 28.0 * u, row.center().y, 21.0 * u, ring, u);
            if is_leader {
                kit::icon(Icon::Crown, row.x + 28.0 * u, row.y + 1.0 * u, 15.0 * u, GOLD_BRIGHT);
            }
            let label = if me { format!("{} (tú)", m.name) } else { m.name.clone() };
            kit::fit(FontKind::Title, &label, row.x + 62.0 * u, row.y + 22.0 * u, 18.0 * u, row.w - 62.0 * u - 100.0 * u, if is_leader { GOLD_BRIGHT } else { TEXT });
            // Life, when the member is in sight.
            let bar = Rect::new(row.x + 62.0 * u, row.y + 30.0 * u, row.w - 62.0 * u - 110.0 * u, 7.0 * u);
            match ent {
                Some(e) if e.max_hp > 0 => {
                    kit::thin_bar(bar, e.hp as f32, e.max_hp as f32, HP);
                    draw_rectangle_lines(bar.x, bar.y, bar.w, bar.h, 1.0, Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.6));
                    text_with(FontKind::Body, &format!("{}/{}", e.hp, e.max_hp), bar.right() + 8.0 * u, bar.y + 8.0 * u, 13.0 * u, TEXT_DIM);
                }
                _ => text_with(FontKind::NarrativeItalic, "fuera de tu vista", bar.x, bar.y + 10.0 * u, 14.0 * u, TEXT_FAINT),
            }
            if i_lead && !me {
                let kb = Rect::new(row.right() - 86.0 * u, row.center().y - 12.0 * u, 80.0 * u, 24.0 * u);
                if button_kind(kb, "Expulsar", Kind::Ghost, true, u) {
                    game.party_kick(m.id);
                }
                if kit::hover(kb) {
                    tip = vec![format!("Expulsar a {}", m.name), "~Dejará de compartir la experiencia contigo.".into()];
                }
            }
            let _ = hov;
            y += row_h;
        }
    }

    // A pending invitation is answered on the floating notice (visible with this window
    // closed too); here only a hint.
    if let Some((_, who)) = invitation {
        text_with(FontKind::NarrativeItalic, &format!("{who} te ha invitado: responde en el aviso de abajo."), r.x, body.bottom() - 6.0 * u, 15.0 * u, GOLD_TEXT);
    }

    // ---- footer: the seguro switch and the actions
    let fy = body.bottom() + 10.0 * u;
    kit::rule(r.x, fy, r.w, GOLD);
    let sw = Rect::new(r.right() - 48.0 * u, fy + 14.0 * u, 48.0 * u, 24.0 * u);
    let nsafe = kit::toggle(sw, safe, u);
    if nsafe != safe {
        game.party_toggle_safe();
    }
    text_with(FontKind::Title, "SEGURO DE GRUPO", r.x, fy + 28.0 * u, 15.0 * u, GOLD_TEXT);
    let (msg, c) = if safe { ("Nadie puede invitarte a un grupo.", GOOD) } else { ("Pueden invitarte a su compañía.", BAD) };
    text_with(FontKind::NarrativeItalic, msg, r.x, fy + 48.0 * u, 15.0 * u, c);
    if kit::hover(Rect::new(r.x, fy + 8.0 * u, r.w, 48.0 * u)) {
        tip = vec!["Seguro de grupo".into(), "~Puesto: nadie puede invitarte.".into(), "~Quítalo para recibir invitaciones.".into()];
    }
    let by = fy + 64.0 * u;
    let inviting = INVITING.get();
    let half = (r.w - 10.0 * u) / 2.0;
    let invite = Rect::new(r.x, by, half, 36.0 * u);
    if button_kind(invite, if inviting { "Haz clic en él..." } else { "Invitar" }, Kind::Primary, true, u) {
        INVITING.set(!inviting);
    }
    let leave = Rect::new(r.x + half + 10.0 * u, by, half, 36.0 * u);
    if button_kind(leave, if i_lead { "Disolver" } else { "Salir" }, Kind::Danger, in_party, u) && in_party {
        game.party_leave();
    }
    if kit::hover(invite) {
        tip = vec!["Invitar a un jugador".into(), "~Pulsa y haz clic sobre él en el mapa.".into(), "El seguro del otro debe estar quitado.".into()];
    } else if kit::hover(leave) && in_party {
        tip = vec![if i_lead { "Disolver el grupo".into() } else { "Salir del grupo".into() }];
    }
    let _ = (GOLD, TEXT_DIM);
    tooltip(&tip, u);
}

//! Login screen. OWNER: Sonnet 1 (docs/P3_CONTRATO.md): account login and new account;
//! the development login (name only) stays available when the server allows it.
//! State and shared look live in `account.rs`.
use super::account::{backdrop, fancy_button, notice_lines, rule, title_centered, with_state, AccountUi, Kind, Mode};
use super::theme::{BAD, GOLD_TEXT, GOOD, MUTED, TEXT};
use super::widgets::{panel, text, text_centered, text_field};
use super::Ui;
use crate::assets::Assets;
use crate::game::Game;
use ao_rules::valid_name;
use macroquad::prelude::*;

/// Where the remembered user name is kept (only the name, never the password).
const REMEMBER_FILE: &str = "config/last_account.txt";

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets) {
    with_state(|st| screen(st, ui, game, assets));
}

fn screen(st: &mut AccountUi, ui: &mut Ui, game: &mut Game, assets: &mut Assets) {
    let u = ui.u;
    let (sw, sh) = (screen_width(), screen_height());
    backdrop(u);
    if !st.begin(game, assets) {
        ui.focus = None;
        return;
    }
    if !st.login_init {
        st.login_init = true;
        if let Ok(user) = std::fs::read_to_string(REMEMBER_FILE) {
            st.user = user.trim().to_string();
            st.remember = !st.user.is_empty();
        }
    }
    if st.entered {
        ui.focus = Some(if st.user.is_empty() { "login_user" } else { "login_pass" }.into());
    }
    let busy = st.flow.busy();
    let mode = st.mode;

    // Panel: height depends on the mode.
    let pw = 440.0 * u;
    let ph = match mode {
        Mode::Login => 468.0,
        Mode::Register => 540.0,
        Mode::Dev => 400.0,
    } * u;
    let r = Rect::new(sw / 2.0 - pw / 2.0, ((sh - ph) / 2.0).max(10.0 * u), pw, ph);
    panel(r, u);
    let cx = r.x + r.w / 2.0;
    let px = r.x + 36.0 * u;
    let fw = r.w - 72.0 * u;

    title_centered("ARGENTUM", cx, r.y + 70.0 * u, 62.0 * u, Color::new(0.9, 0.7, 0.34, 1.0));
    text_centered("Argentum Online remasterizado", cx, r.y + 96.0 * u, 18.0 * u, MUTED);
    rule(cx, r.y + 112.0 * u, fw, u);

    // Tabs
    let mut y = r.y + 128.0 * u;
    if mode != Mode::Dev {
        let tw = fw / 2.0 - 3.0 * u;
        for (i, (m, label)) in [(Mode::Login, "Entrar"), (Mode::Register, "Crear cuenta")].into_iter().enumerate() {
            let tr = Rect::new(px + i as f32 * (tw + 6.0 * u), y, tw, 34.0 * u);
            if fancy_button(tr, label, if mode == m { Kind::Primary } else { Kind::Normal }, !busy, u) && mode != m {
                st.mode = m;
                st.flow.notice = None;
                ui.focus = Some(if m == Mode::Register && !st.user.is_empty() { "login_pass" } else { "login_user" }.into());
            }
        }
        y += 52.0 * u;
    } else {
        text_centered("Acceso de desarrollo", cx, y + 22.0 * u, 20.0 * u, GOLD_TEXT);
        y += 40.0 * u;
    }

    let mut submit = false;
    let mut ids: Vec<&str> = Vec::new();
    if mode == Mode::Dev {
        ids.push("login_dev");
        text("Nombre del personaje", px, y + 14.0 * u, 18.0 * u, TEXT);
        let f = Rect::new(px, y + 22.0 * u, fw, 36.0 * u);
        submit |= text_field(ui, "login_dev", &mut game.name_input, f, 16, false, |c| c.is_ascii_alphanumeric() || c == '_');
        y += 66.0 * u;
        text("Entra sin cuenta con un personaje de prueba.", px, y + 12.0 * u, 16.0 * u, MUTED);
        text("Solo funciona si el servidor lo permite.", px, y + 32.0 * u, 16.0 * u, MUTED);
        y += 56.0 * u;
    } else {
        ids.extend(["login_user", "login_pass"]);
        text("Usuario", px, y + 14.0 * u, 18.0 * u, TEXT);
        let f = Rect::new(px, y + 22.0 * u, fw, 36.0 * u);
        submit |= text_field(ui, "login_user", &mut st.user, f, 20, false, |c| c.is_ascii_alphanumeric() || c == '_');
        y += 68.0 * u;
        text("Contraseña", px, y + 14.0 * u, 18.0 * u, TEXT);
        let f = Rect::new(px, y + 22.0 * u, fw, 36.0 * u);
        submit |= text_field(ui, "login_pass", &mut st.pass, f, 64, true, |c| !c.is_control());
        y += 68.0 * u;
        if mode == Mode::Register {
            ids.push("login_pass2");
            text("Repite la contraseña", px, y + 14.0 * u, 18.0 * u, TEXT);
            let f = Rect::new(px, y + 22.0 * u, fw, 36.0 * u);
            submit |= text_field(ui, "login_pass2", &mut st.pass2, f, 64, true, |c| !c.is_control());
            let (hint, hc) = hints(st);
            text(&hint, px, y + 78.0 * u, 15.0 * u, hc);
            y += 90.0 * u;
        } else {
            // Remember checkbox
            let cr = Rect::new(px, y + 2.0 * u, 18.0 * u, 18.0 * u);
            let hover = Rect::new(px, y, fw, 24.0 * u).contains(mouse_position().into());
            draw_rectangle(cr.x, cr.y, cr.w, cr.h, Color::new(0.0, 0.0, 0.0, 0.6));
            draw_rectangle_lines(cr.x, cr.y, cr.w, cr.h, 1.5 * u, if hover { GOLD_TEXT } else { super::theme::GOLD });
            if st.remember {
                draw_rectangle(cr.x + 4.0 * u, cr.y + 4.0 * u, 10.0 * u, 10.0 * u, GOLD_TEXT);
            }
            text("Recordar mi usuario", px + 28.0 * u, y + 17.0 * u, 17.0 * u, TEXT);
            if hover && !busy && is_mouse_button_pressed(MouseButton::Left) {
                st.remember = !st.remember;
            }
            y += 34.0 * u;
        }
    }

    if is_key_pressed(KeyCode::Tab) && !busy {
        let i = ids.iter().position(|id| ui.focus.as_deref() == Some(*id));
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let next = match (i, shift) {
            (None, _) => 0,
            (Some(i), false) => (i + 1) % ids.len(),
            (Some(i), true) => (i + ids.len() - 1) % ids.len(),
        };
        ui.focus = Some(ids[next].to_string());
    }

    // Primary action
    let label = match mode {
        Mode::Login => "Entrar al mundo",
        Mode::Register => "Crear mi cuenta",
        Mode::Dev => "Entrar como visitante",
    };
    if fancy_button(Rect::new(px, y, fw, 46.0 * u), label, Kind::Primary, !busy, u) {
        submit = true;
    }
    y += 62.0 * u;
    if submit && !busy {
        act(st, ui, game);
    }

    // Message
    if busy {
        text_centered(st.flow.pending.as_ref().map(|p| p.label()).unwrap_or(""), cx, y + 8.0 * u, 18.0 * u, GOLD_TEXT);
    } else if let Some((msg, ok)) = st.flow.notice.clone() {
        notice_lines(&msg, ok, px, y + 8.0 * u, fw, 17.0 * u, u);
    }

    // Dev toggle
    let dev_label = if mode == Mode::Dev { "Volver al acceso con cuenta" } else { "Acceso de desarrollo (solo nombre)" };
    let lr = Rect::new(cx - 160.0 * u, r.bottom() - 34.0 * u, 320.0 * u, 24.0 * u);
    let hover = lr.contains(mouse_position().into());
    text_centered(dev_label, cx, lr.y + 17.0 * u, 16.0 * u, if hover { GOLD_TEXT } else { MUTED });
    if hover && !busy && is_mouse_button_pressed(MouseButton::Left) {
        st.mode = if mode == Mode::Dev { Mode::Login } else { Mode::Dev };
        st.flow.notice = None;
    }

    text_centered(&format!("Servidor: {}", game.server_addr), sw / 2.0, sh - 34.0 * u, 16.0 * u, MUTED);
    text_centered("Mapas y gráficos de AO20 - proyecto personal sin fines comerciales", sw / 2.0, sh - 14.0 * u, 14.0 * u, Color::new(0.5, 0.46, 0.4, 1.0));
    st.end(game, "login");
}

/// Live hints while registering.
fn hints(st: &AccountUi) -> (String, Color) {
    use client_core::p3::account::{check_account_name, check_password};
    if st.user.is_empty() && st.pass.is_empty() {
        return ("Usuario de 3 a 20 letras, números o _. Contraseña de 6 a 64.".into(), MUTED);
    }
    if let Err(e) = check_account_name(&st.user) {
        return (e.to_string(), BAD);
    }
    if let Err(e) = check_password(&st.pass) {
        return (e.to_string(), BAD);
    }
    if !st.pass2.is_empty() && st.pass != st.pass2 {
        return ("Las contraseñas no coinciden.".into(), BAD);
    }
    if st.pass2.is_empty() {
        ("Repite la contraseña para continuar.".into(), MUTED)
    } else {
        ("Todo listo.".into(), GOOD)
    }
}

fn act(st: &mut AccountUi, ui: &mut Ui, game: &mut Game) {
    match st.mode {
        Mode::Login => {
            remember(st);
            let (u, p) = (st.user.clone(), st.pass.clone());
            st.flow.login(game, &u, &p);
        }
        Mode::Register => {
            if st.pass != st.pass2 {
                st.flow.notice = Some(("Las contraseñas no coinciden.".into(), false));
                return;
            }
            remember(st);
            let (u, p) = (st.user.clone(), st.pass.clone());
            st.flow.register(game, &u, &p);
        }
        Mode::Dev => {
            if !valid_name(&game.name_input) {
                st.flow.notice = Some(("Nombre inválido: 3 a 16 letras, números o _.".into(), false));
                return;
            }
            game.connect();
            if game.screen != crate::game::Screen::Login {
                ui.focus = None;
            } else {
                let msg = game.console.back().map(|(m, _)| m.clone()).unwrap_or_else(|| "No se pudo conectar.".into());
                st.flow.notice = Some((msg, false));
            }
        }
    }
}

/// Saves (or forgets) the user name next to the other local config.
fn remember(st: &AccountUi) {
    if st.remember {
        let _ = std::fs::write(REMEMBER_FILE, &st.user);
    } else {
        let _ = std::fs::remove_file(REMEMBER_FILE);
    }
}

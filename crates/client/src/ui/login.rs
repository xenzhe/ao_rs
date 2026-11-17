//! Login screen over the town at dusk (docs/ESTILO_UI.md §1.8): the Numic AO logo with its
//! halo on one side and, on the other, a parchment scroll where you write your name. The
//! scroll unrolls to switch between entering, creating an account and the development login
//! (name only, when the server allows it). State and shared look live in `account.rs`.
use super::account::{band, glow_centered, humane, prepare, with_state, AccountUi, Mode};
use super::fx_ui::{ease, key_of};
use super::theme::{self, FontKind, Surface, GOLD, GOLD_BRIGHT, INK, TEXT_DIM, TEXT_FAINT};
use super::widgets::{button_kind, measure_with, text_centered_with, text_with, Kind};
use super::Ui;
use crate::assets::Assets;
use crate::game::Game;
use crate::render::Renderer;
use ao_rules::valid_name;
use macroquad::prelude::*;

/// Where the remembered user name is kept (only the name, never the password).
const REMEMBER_FILE: &str = "config/last_account.txt";

/// Ink colours on the parchment.
const INK_SOFT: Color = Color::new(0.30, 0.20, 0.11, 0.85);
const INK_FAINT: Color = Color::new(0.30, 0.20, 0.11, 0.45);
const INK_RED: Color = Color::new(0.58, 0.10, 0.06, 1.0);
const INK_GREEN: Color = Color::new(0.16, 0.38, 0.12, 1.0);

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, renderer: &mut Renderer, dt: f32) {
    with_state(|st| {
        let ok = st.begin(game, assets);
        prepare(st, assets);
        if !st.scene.draw(renderer, assets, dt) {
            super::account::backdrop(ui.u);
        }
        if ok {
            screen(st, ui, game);
        } else {
            ui.focus = None;
        }
        st.scene.draw_black();
        st.end(game, "login");
    });
}

/// Which field an error message talks about (to show it under that field).
#[derive(PartialEq, Clone, Copy)]
enum Field {
    User,
    Pass,
    Pass2,
    Other,
}

fn field_of(msg: &str) -> Field {
    let m = msg.to_lowercase();
    if m.contains("no coinciden") {
        Field::Pass2
    } else if m.contains("contraseña") || m.contains("contrasena") || m.contains("incorrect") {
        Field::Pass
    } else if m.contains("usuario") || m.contains("cuenta") || m.contains("nombre") {
        Field::User
    } else {
        Field::Other
    }
}

fn screen(st: &mut AccountUi, ui: &mut Ui, game: &mut Game) {
    let u = ui.u;
    let (sw, sh) = (screen_width(), screen_height());
    let now = get_time();
    let t = now as f32;
    if !st.login_init {
        st.login_init = true;
        if let Ok(user) = std::fs::read_to_string(REMEMBER_FILE) {
            st.user = user.trim().to_string();
            st.remember = !st.user.is_empty();
        }
    }
    if st.entered {
        ui.focus = Some(if st.user.is_empty() || st.mode == Mode::Dev { first_field(st.mode) } else { "login_pass" }.into());
    }
    let busy = st.flow.busy();
    let mode = st.mode;

    // ---- framing: vignette and a darker floor for the footer
    theme::inner_vignette(Rect::new(0.0, 0.0, sw, sh), 200.0 * u, 0.9);
    band(sh - 90.0 * u, 90.0 * u, 0.6, false);

    // ---- the logo (N1), left
    let wide = sw / u > 1100.0;
    let (lx, ly) = if wide { (sw * 0.33, sh * 0.42) } else { (sw * 0.5, sh * 0.16) };
    logo(lx, ly, u, t, wide);

    // ---- the scroll, right
    let target_h = match mode {
        Mode::Login => 345.0,
        Mode::Register => 420.0,
        Mode::Dev => 285.0,
    } * u;
    if st.scroll_h <= 0.0 {
        st.scroll_h = target_h;
    }
    st.scroll_h += (target_h - st.scroll_h) * (1.0 - (-get_frame_time() * 9.0).exp());
    let w = 384.0 * u;
    let cx = if wide { sw * 0.74 } else { sw * 0.5 };
    let total = st.scroll_h + 44.0 * u;
    let top = if wide { ((sh - total) / 2.0 - 10.0 * u).max(16.0 * u) } else { (sh * 0.28).max(ly + 60.0 * u) };
    let paper = Rect::new(cx - w / 2.0, top + 16.0 * u, w, st.scroll_h);
    // Shadow, paper, burnt edges.
    for i in 1..=5 {
        let o = i as f32 * 3.0 * u;
        draw_rectangle(paper.x + o * 0.4, paper.y + o, paper.w, paper.h, Color::new(0.0, 0.0, 0.0, 0.07));
    }
    theme::surface(paper, Surface::Parchment, u);
    for k in 0..6 {
        let a = 0.10 - k as f32 * 0.015;
        let d = k as f32 * 1.5 * u;
        draw_line(paper.x + d, paper.y, paper.x + d, paper.bottom(), 1.5 * u, Color::new(0.35, 0.2, 0.08, a));
        draw_line(paper.right() - d, paper.y, paper.right() - d, paper.bottom(), 1.5 * u, Color::new(0.35, 0.2, 0.08, a));
    }
    let visible = |y: f32| y < paper.bottom() - 6.0 * u;

    // Heading
    let pcx = paper.x + paper.w / 2.0;
    let mut y = paper.y + 44.0 * u;
    let (head, sub) = match mode {
        Mode::Login => ("Bienvenido, viajero", "Escribe tu nombre en el registro de la ciudad"),
        Mode::Register => ("Un nuevo nombre", "Firma el registro y elige una contraseña"),
        Mode::Dev => ("Acceso de desarrollo", "Un personaje de prueba, sin cuenta"),
    };
    text_centered_with(FontKind::Title, head, pcx, y, 24.0 * u, INK);
    y += 22.0 * u;
    text_centered_with(FontKind::NarrativeItalic, sub, pcx, y, 15.0 * u, INK_SOFT);
    y += 14.0 * u;
    ink_divider(pcx, y, 200.0 * u, u);
    y += 26.0 * u;

    // Fields
    let fx = paper.x + 34.0 * u;
    let fw = paper.w - 68.0 * u;
    let shake_k = ((now - st.notice_at) as f32 / 0.4).min(1.0);
    let shake = (t * 70.0).sin() * 5.0 * u * (1.0 - shake_k) * (1.0 - shake_k);
    let notice = st.flow.notice.clone().filter(|_| !busy).map(|(m, ok)| (humane(&m), ok));
    let err_field = notice.as_ref().filter(|n| !n.1).map(|n| field_of(&n.0));
    let mut submit = false;
    let mut ids: Vec<&'static str> = Vec::new();
    let hints = if mode == Mode::Register { register_hints(st) } else { [None, None, None] };
    let mut field = |ui: &mut Ui, st: &mut AccountUi, id: &'static str, label: &str, which: Field, y: &mut f32| -> bool {
        ids.push(id);
        if !visible(*y + 40.0 * u) {
            *y += 70.0 * u;
            return false;
        }
        let dx = if err_field == Some(which) { shake } else { 0.0 };
        let r = Rect::new(fx + dx, *y, fw, 36.0 * u);
        let (value, max, mask): (&mut String, usize, bool) = match id {
            "login_user" => (&mut st.user, 20, false),
            "login_pass" => (&mut st.pass, 64, true),
            "login_pass2" => (&mut st.pass2, 64, true),
            _ => (&mut game.name_input, 16, false),
        };
        let accept: fn(char) -> bool = if mask { |c| !c.is_control() } else { |c| c.is_ascii_alphanumeric() || c == '_' };
        let enter = ink_field(ui, id, label, value, r, max, mask, accept, !busy);
        // The message for this field: the server's error, else the live hint.
        let hint = match which {
            Field::User => hints[0].clone(),
            Field::Pass => hints[1].clone(),
            Field::Pass2 => hints[2].clone(),
            Field::Other => None,
        };
        let msg = match (&notice, err_field == Some(which)) {
            (Some((m, _)), true) => Some((m.clone(), false)),
            _ => hint,
        };
        if let Some((m, ok)) = msg {
            text_with(FontKind::Body, &m, fx + dx, *y + 54.0 * u, 14.0 * u, if ok { INK_GREEN } else { INK_RED });
        }
        *y += 70.0 * u;
        enter
    };
    match mode {
        Mode::Dev => {
            submit |= field(ui, st, "login_dev", "Nombre del personaje", Field::User, &mut y);
        }
        _ => {
            submit |= field(ui, st, "login_user", "Usuario", Field::User, &mut y);
            submit |= field(ui, st, "login_pass", "Contraseña", Field::Pass, &mut y);
            if mode == Mode::Register {
                submit |= field(ui, st, "login_pass2", "Repite la contraseña", Field::Pass2, &mut y);
            }
        }
    }
    // Remember me: a wax seal.
    if mode == Mode::Login && visible(y + 20.0 * u) {
        let r = Rect::new(fx, y - 6.0 * u, 220.0 * u, 26.0 * u);
        let hover = !busy && r.contains(mouse_position().into());
        let h = ease(key_of(r), hover, 0.08);
        let c = vec2(fx + 10.0 * u, y + 6.0 * u);
        if st.remember {
            draw_circle(c.x, c.y + 1.0 * u, 10.0 * u, Color::new(0.25, 0.03, 0.02, 0.5));
            draw_circle(c.x, c.y, 10.0 * u, Color::new(0.55, 0.07, 0.05, 1.0));
            draw_circle(c.x - 2.5 * u, c.y - 2.5 * u, 4.0 * u, Color::new(0.85, 0.25, 0.18, 0.5));
            draw_poly(c.x, c.y, 4, 3.5 * u, 45.0, GOLD_BRIGHT);
        } else {
            draw_circle_lines(c.x, c.y, 9.0 * u, 1.5 * u, with_a(INK_SOFT, 0.6 + 0.4 * h));
        }
        text_with(FontKind::Body, "Recordar mi usuario", fx + 30.0 * u, y + 12.0 * u, 16.0 * u, with_a(INK, 0.8 + 0.2 * h));
        if hover && is_mouse_button_pressed(MouseButton::Left) {
            st.remember = !st.remember;
        }
        y += 34.0 * u;
    }

    // Keyboard focus: Tab / Shift+Tab between the fields.
    if is_key_pressed(KeyCode::Tab) && !busy && !ids.is_empty() {
        let i = ids.iter().position(|id| ui.focus.as_deref() == Some(*id));
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let next = match (i, shift) {
            (None, _) => 0,
            (Some(i), false) => (i + 1) % ids.len(),
            (Some(i), true) => (i + ids.len() - 1) % ids.len(),
        };
        ui.focus = Some(ids[next].to_string());
    }

    // The one primary action.
    let label = match (busy, mode) {
        (true, _) => st.flow.pending.as_ref().map(|p| p.label()).unwrap_or("..."),
        (_, Mode::Login) => "Entrar al mundo",
        (_, Mode::Register) => "Firmar y entrar",
        (_, Mode::Dev) => "Entrar como visitante",
    };
    if visible(y + 30.0 * u) {
        let br = Rect::new(fx, y, fw, 44.0 * u);
        if button_kind(br, label, Kind::Primary, !busy, u) {
            submit = true;
        }
        if busy {
            // A small spinning quill-light while waiting.
            let a = t * 5.0;
            draw_circle(br.right() - 22.0 * u + a.cos() * 7.0 * u, br.y + br.h / 2.0 + a.sin() * 7.0 * u, 2.5 * u, GOLD_BRIGHT);
        }
        // Errors that are not about one field go under the button.
        if let Some((m, ok)) = &notice {
            if err_field == Some(Field::Other) || *ok {
                text_centered_with(FontKind::Body, m, pcx + shake, br.bottom() + 20.0 * u, 14.0 * u, if *ok { INK_GREEN } else { INK_RED });
            }
        }
    }
    if submit && !busy {
        act(st, ui, game);
    }

    // Switch between entering and creating an account (an ink link under the scroll).
    let link_y = paper.bottom() + 52.0 * u;
    let (q, l) = match mode {
        Mode::Login => ("¿Primera vez en Argentum?", "Crea tu cuenta"),
        Mode::Register => ("¿Ya tienes una cuenta?", "Entra con ella"),
        Mode::Dev => ("", "Volver al acceso con cuenta"),
    };
    let qw = if q.is_empty() { 0.0 } else { measure_with(FontKind::NarrativeItalic, q, 16.0 * u).width + 8.0 * u };
    let lw = measure_with(FontKind::Title, l, 16.0 * u).width;
    let x0 = cx - (qw + lw) / 2.0;
    text_with(FontKind::NarrativeItalic, q, x0, link_y, 16.0 * u, TEXT_DIM);
    let lr = Rect::new(x0 + qw - 4.0 * u, link_y - 18.0 * u, lw + 8.0 * u, 26.0 * u);
    let hover = !busy && lr.contains(mouse_position().into());
    let h = ease(key_of(lr), hover, 0.08);
    text_with(FontKind::Title, l, x0 + qw, link_y, 16.0 * u, Color::new(0.92 + 0.06 * h, 0.76 + 0.06 * h, 0.44 + 0.02 * h, 1.0));
    draw_line(x0 + qw, link_y + 4.0 * u, x0 + qw + lw * (0.35 + 0.65 * h), link_y + 4.0 * u, 1.0, GOLD);
    if hover && is_mouse_button_pressed(MouseButton::Left) {
        let m = if mode == Mode::Login { Mode::Register } else { Mode::Login };
        switch_mode(st, ui, m);
    }

    // Footer: server, development access, credits.
    text_with(FontKind::Body, &format!("Servidor  {}", game.server_addr), 24.0 * u, sh - 18.0 * u, 14.0 * u, TEXT_FAINT);
    let credit = "Mapas y gráficos de AO20  ·  proyecto sin fines comerciales";
    let cw = measure_with(FontKind::Body, credit, 14.0 * u).width;
    text_with(FontKind::Body, credit, sw - cw - 24.0 * u, sh - 18.0 * u, 14.0 * u, TEXT_FAINT);
    if mode != Mode::Dev && button_kind(Rect::new(sw / 2.0 - 110.0 * u, sh - 36.0 * u, 220.0 * u, 26.0 * u), "Acceso de desarrollo", Kind::Ghost, !busy, u) {
        switch_mode(st, ui, Mode::Dev);
    }

    // Wooden rods over the paper ends (drawn last: the paper unrolls from under them).
    rod(paper.x - 14.0 * u, top, paper.w + 28.0 * u, u);
    rod(paper.x - 14.0 * u, paper.bottom() - 8.0 * u, paper.w + 28.0 * u, u);
}

fn first_field(mode: Mode) -> &'static str {
    if mode == Mode::Dev {
        "login_dev"
    } else {
        "login_user"
    }
}

fn switch_mode(st: &mut AccountUi, ui: &mut Ui, m: Mode) {
    st.mode = m;
    st.mode_at = get_time();
    st.flow.notice = None;
    ui.focus = Some(if m == Mode::Register && !st.user.is_empty() { "login_pass" } else { first_field(m) }.into());
}

fn with_a(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0.0, 1.0))
}

/// The game's name with its halo: a dark soft pool behind (legible over any scene), a warm
/// glow that breathes, the name in Cinzel Decorative and a tagline.
fn logo(cx: f32, cy: f32, u: f32, t: f32, wide: bool) {
    let size = if wide { 72.0 } else { 54.0 } * u;
    for k in 0..14 {
        let f = 1.0 - k as f32 / 14.0;
        draw_ellipse(cx, cy - size * 0.3, (330.0 * f + 40.0) * u, (110.0 * f + 20.0) * u, 0.0, Color::new(0.0, 0.0, 0.0, 0.045));
    }
    let breathe = 0.85 + 0.15 * (t * 0.9).sin();
    for k in 0..12 {
        let f = 1.0 - k as f32 / 12.0;
        draw_ellipse(cx, cy - size * 0.32, (250.0 * f + 30.0) * u, (60.0 * f + 10.0) * u, 0.0, Color::new(1.0, 0.62, 0.25, 0.018 * breathe));
    }
    glow_centered(FontKind::Display, "Numic AO", cx, cy, size, GOLD_BRIGHT, Color::new(1.0, 0.66, 0.28, 0.07 * breathe), 4.0 * u);
    // A thin light that runs across the name now and then.
    let w = measure_with(FontKind::Display, "Numic AO", size).width;
    let phase = (t * 0.16).fract();
    if phase < 0.25 {
        let x = cx - w / 2.0 + w * (phase / 0.25);
        for k in 0..8 {
            let a = 0.05 * (1.0 - k as f32 / 8.0);
            draw_rectangle(x - k as f32 * 3.0 * u, cy - size * 0.85, 3.0 * u, size * 0.9, Color::new(1.0, 0.95, 0.8, a));
        }
    }
    let ty = cy + 34.0 * u;
    text_centered_with(FontKind::NarrativeItalic, "Las tierras de Argentum te esperan", cx, ty, 22.0 * u, Color::new(0.93, 0.88, 0.78, 0.92));
    theme::divider(cx, ty + 20.0 * u, 300.0 * u, u);
}

/// A wooden rod with gold caps (the ends of the scroll).
fn rod(x: f32, y: f32, w: f32, u: f32) {
    let h = 16.0 * u;
    let n = 6;
    for i in 0..n {
        let f = i as f32 / (n - 1) as f32;
        let k = 1.0 - (f - 0.35).abs() * 1.4;
        draw_rectangle(x, y + h * i as f32 / n as f32, w, h / n as f32 + 1.0, Color::new(0.20 + 0.18 * k, 0.12 + 0.10 * k, 0.06 + 0.04 * k, 1.0));
    }
    draw_rectangle(x, y + h, w, 3.0 * u, Color::new(0.0, 0.0, 0.0, 0.25));
    for cx in [x, x + w] {
        draw_circle(cx, y + h / 2.0, h * 0.62, Color::new(0.30, 0.20, 0.08, 1.0));
        draw_circle(cx, y + h / 2.0, h * 0.5, GOLD);
        draw_circle(cx - h * 0.15, y + h * 0.35, h * 0.18, Color::new(1.0, 0.9, 0.6, 0.7));
    }
}

/// `—◆—` in ink.
fn ink_divider(cx: f32, y: f32, w: f32, u: f32) {
    let d = 4.0 * u;
    for side in [-1.0f32, 1.0] {
        let n = 10;
        for i in 0..n {
            let f0 = i as f32 / n as f32;
            let f1 = (i + 1) as f32 / n as f32;
            let a = 0.6 * (1.0 - f0);
            draw_line(cx + side * (2.0 * d + (w / 2.0 - 2.0 * d) * f0), y, cx + side * (2.0 * d + (w / 2.0 - 2.0 * d) * f1), y, 1.2 * u, with_a(INK, a));
        }
    }
    let c = vec2(cx, y);
    draw_triangle(c + vec2(0.0, -d), c + vec2(d, 0.0), c + vec2(0.0, d), INK_SOFT);
    draw_triangle(c + vec2(0.0, -d), c + vec2(-d, 0.0), c + vec2(0.0, d), INK_SOFT);
}

/// A field written on the parchment: label in small caps, the text in ink over a line that
/// lights up with the focus. Takes the keyboard focus on click; Enter returns true.
#[allow(clippy::too_many_arguments)]
fn ink_field(ui: &mut Ui, id: &str, label: &str, value: &mut String, r: Rect, max: usize, mask: bool, accept: fn(char) -> bool, enabled: bool) -> bool {
    let u = ui.u;
    if enabled && r.contains(mouse_position().into()) && is_mouse_button_pressed(MouseButton::Left) {
        ui.focus = Some(id.to_string());
        ui.mouse_captured = true;
    }
    let focused = enabled && ui.focus.as_deref() == Some(id);
    let h = ease(key_of(r), focused, 0.1);
    text_with(FontKind::Title, &label.to_uppercase(), r.x, r.y + 2.0 * u, 13.0 * u, with_a(INK_SOFT, 0.75 + 0.25 * h));
    // A faint wash behind the line when focused, and the line itself.
    if h > 0.0 {
        draw_rectangle(r.x - 6.0 * u, r.y + 8.0 * u, r.w + 12.0 * u, r.h - 4.0 * u, Color::new(1.0, 0.95, 0.8, 0.22 * h));
    }
    let ly = r.bottom() + 2.0 * u;
    draw_line(r.x, ly, r.right(), ly, 1.0 + 1.2 * h, Color::new(0.30 + 0.30 * h, 0.20 + 0.16 * h, 0.10, 0.55 + 0.45 * h));
    let mut enter = false;
    if focused {
        ui.keyboard_captured = true;
        while let Some(ch) = get_char_pressed() {
            if !ch.is_control() && accept(ch) && value.chars().count() < max {
                value.push(ch);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            value.pop();
        }
        enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
    }
    let shown = if mask { "•".repeat(value.chars().count()) } else { value.clone() };
    let size = 20.0 * u;
    let base = r.bottom() - 7.0 * u;
    let tw = measure_with(FontKind::Body, &shown, size).width;
    text_with(FontKind::Body, &shown, r.x + 2.0 * u, base, size, INK);
    if focused && (get_time() * 1.6) as i32 % 2 == 0 {
        let x = r.x + 4.0 * u + tw.min(r.w - 8.0 * u);
        draw_line(x, base - size * 0.75, x, base + 3.0 * u, 1.5 * u, INK);
    }
    if value.is_empty() && !focused {
        let hint = if mask { "········" } else { "..." };
        text_with(FontKind::NarrativeItalic, hint, r.x + 2.0 * u, base, 16.0 * u, INK_FAINT);
    }
    enter
}

/// Live hints while registering, one per field (only once something was written).
fn register_hints(st: &AccountUi) -> [Option<(String, bool)>; 3] {
    use client_core::p3::account::{check_account_name, check_password};
    let user = (!st.user.is_empty()).then(|| match check_account_name(&st.user) {
        Ok(()) => ("Libre para firmar.".to_string(), true),
        Err(e) => (e.to_string(), false),
    });
    let pass = (!st.pass.is_empty()).then(|| match check_password(&st.pass) {
        Ok(()) => ("Contraseña válida.".to_string(), true),
        Err(e) => (e.to_string(), false),
    });
    let pass2 = (!st.pass2.is_empty()).then(|| if st.pass == st.pass2 { ("Coinciden.".to_string(), true) } else { ("Las contraseñas no coinciden.".to_string(), false) });
    [user.filter(|u| !u.1).or(None), pass, pass2]
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
                st.flow.notice = Some(("Nombre inválido: de 3 a 16 letras, números o _.".into(), false));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_go_under_their_field() {
        assert!(field_of("Contraseña incorrecta.") == Field::Pass);
        assert!(field_of("Usuario o contrasena incorrectos.") == Field::Pass, "a wrong password is the usual case");
        assert!(field_of(&super::super::account::humane("Ese nombre de cuenta ya existe.")) == Field::User);
        assert!(field_of("Las contraseñas no coinciden.") == Field::Pass2);
        assert!(field_of("Esa cuenta no existe.") == Field::User);
        assert!(field_of("El servidor no responde.") == Field::Other);
    }
}

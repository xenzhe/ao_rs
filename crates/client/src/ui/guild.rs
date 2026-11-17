//! Window: guild. A banner for the clan (emblem, name, alignment, level) with the rest as
//! pages beside it: news on parchment, members with their state, applicants as letters,
//! relations. Without a guild: the list of clans, the application letter and the founding
//! deed with the requirements ticked against the character.
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::character::kit::{self, Icon, INK_DIM, WAX};
use crate::ui::theme::{FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, INK, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::ui::widgets::{button_kind, measure_with, text_with, tooltip, Kind};
use crate::ui::{Ui, WindowId};
use ao_protocol::social::guild::{GuildInfo, GuildRelation};
use ao_rules::guild::{self, Alignment};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Info,
    Members,
    Applicants,
    Relations,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Edit {
    News,
    Description,
}

struct State {
    tab: Tab,
    creating: bool,
    found_name: String,
    found_desc: String,
    found_align: u8,
    selected: Option<String>,
    application: String,
    edit: Option<(Edit, String)>,
    reject: Option<String>,
    reject_reason: String,
    rel_target: Option<String>,
    rel_text: String,
    confirm_leave: bool,
    scroll: HashMap<&'static str, usize>,
    last_draw: f64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        tab: match std::env::var("AO_GUILD_TAB").as_deref() {
            Ok("members") => Tab::Members,
            Ok("applicants") => Tab::Applicants,
            Ok("relations") => Tab::Relations,
            _ => Tab::Info,
        },
        creating: std::env::var_os("AO_GUILD_CREATE").is_some(),
        found_name: String::new(),
        found_desc: String::new(),
        found_align: 0,
        selected: None,
        application: String::new(),
        edit: None,
        reject: None,
        reject_reason: String::new(),
        rel_target: None,
        rel_text: String::new(),
        confirm_leave: false,
        scroll: HashMap::new(),
        last_draw: -10.0,
    });
}

/// Called every frame by the console: a text field of a closed window must not keep the
/// keyboard.
pub fn frame_hooks(ui: &mut Ui) {
    if !ui.is_open(WindowId::Guild) && ui.focus.as_deref().is_some_and(|f| f.starts_with("guild.")) {
        ui.focus = None;
    }
}

fn align_name(a: u8) -> &'static str {
    Alignment::from_u8(a).map(Alignment::name).unwrap_or("?")
}

/// Cloth colour and heraldic symbol of an alignment.
fn align_look(a: u8) -> (Color, Icon) {
    match a {
        1 => (Color::new(0.22, 0.38, 0.78, 1.0), Icon::Crown),
        2 => (Color::new(0.50, 0.10, 0.12, 1.0), Icon::Skull),
        3 => (Color::new(0.20, 0.52, 0.28, 1.0), Icon::Shield),
        4 => (Color::new(0.62, 0.34, 0.12, 1.0), Icon::Sword),
        _ => (Color::new(0.38, 0.36, 0.34, 1.0), Icon::Banner),
    }
}

fn relation_look(r: GuildRelation) -> (&'static str, Color, Icon) {
    match r {
        GuildRelation::War => ("Guerra", BAD, Icon::Sword),
        GuildRelation::Peace => ("Paz", GOOD, Icon::Leaf),
        GuildRelation::Alliance => ("Alianza", Color::new(0.55, 0.75, 1.0, 1.0), Icon::Banner),
    }
}

/// Small underlined link in ink (actions on parchment).
fn ink_link(r: Rect, label: &str, u: f32) -> bool {
    let hv = kit::hover(r);
    let c = if hv { WAX } else { Color::new(0.45, 0.30, 0.14, 1.0) };
    let w = measure_with(FontKind::Body, label, 14.0 * u).width;
    kit::ink(FontKind::Body, label, r.x, r.y + 14.0 * u, 14.0 * u, c);
    draw_line(r.x, r.y + 18.0 * u, r.x + w * if hv { 1.0 } else { 0.0 }, r.y + 18.0 * u, 1.0, c);
    kit::clicked(r)
}

/// A text field written on parchment: ruled lines, ink text, wraps over several rows.
/// Enter returns true (single-line fields); Esc drops the focus.
#[allow(clippy::too_many_arguments)]
fn letter_field(ui: &mut Ui, id: &str, value: &mut String, r: Rect, max: usize, hint: &str, single: bool, accept: impl Fn(char) -> bool) -> bool {
    let u = ui.u;
    let focused = ui.focus.as_deref() == Some(id);
    if kit::hover(r) && is_mouse_button_pressed(MouseButton::Left) {
        ui.focus = Some(id.to_string());
        ui.mouse_captured = true;
    }
    let size = 18.0 * u;
    let lh = 26.0 * u;
    let rows_fit = ((r.h / lh).floor() as usize).max(1);
    for i in 0..rows_fit {
        let y = r.y + (i as f32 + 1.0) * lh - 5.0 * u;
        draw_line(r.x, y, r.right(), y, 1.0, Color::new(0.35, 0.22, 0.10, if focused { 0.55 } else { 0.30 }));
    }
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
        crate::ui::widgets::paste_into(value, max, &accept);
        enter = is_key_pressed(KeyCode::Enter);
        if is_key_pressed(KeyCode::Escape) {
            ui.focus = None;
        }
    }
    if value.is_empty() && !focused {
        kit::ink(FontKind::NarrativeItalic, hint, r.x + 4.0 * u, r.y + lh - 9.0 * u, size, Color::new(0.45, 0.34, 0.2, 0.8));
        return false;
    }
    let cursor = focused && (get_time() * 2.0) as i32 % 2 == 0;
    let shown = if cursor { format!("{value}|") } else { value.clone() };
    let rows: Vec<String> = if single { vec![shown] } else { kit::wrap(FontKind::Narrative, &shown, size, r.w - 8.0 * u) };
    let skip = rows.len().saturating_sub(rows_fit);
    for (i, row) in rows.iter().skip(skip).take(rows_fit).enumerate() {
        kit::ink_fit(FontKind::Narrative, row, r.x + 4.0 * u, r.y + (i as f32 + 1.0) * lh - 9.0 * u, size, r.w - 8.0 * u, INK);
    }
    if focused {
        kit::ink(FontKind::Body, &format!("{}/{max}", value.chars().count()), r.right() - 56.0 * u, r.bottom() + 12.0 * u, 12.0 * u, INK_DIM);
    }
    enter
}

/// A hanging banner: gold rod, cloth in `c` with folds and fringe, and a swallow-tail cut.
fn banner(r: Rect, c: Color, u: f32) -> Rect {
    let rod_y = r.y + 6.0 * u;
    let cloth = Rect::new(r.x + 10.0 * u, rod_y + 6.0 * u, r.w - 20.0 * u, r.h - 20.0 * u);
    let tail = 34.0 * u;
    let body_h = cloth.h - tail;
    // Shadow.
    draw_rectangle(cloth.x + 4.0 * u, cloth.y + 6.0 * u, cloth.w, body_h, Color::new(0.0, 0.0, 0.0, 0.28));
    let dark = kit::mix(c, BLACK, 0.45);
    draw_rectangle(cloth.x, cloth.y, cloth.w, body_h, dark);
    // Vertical gradient and soft vertical folds.
    let n = 14;
    for i in 0..n {
        let t = i as f32 / n as f32;
        draw_rectangle(cloth.x, cloth.y + body_h * t, cloth.w, body_h / n as f32 + 1.0, kit::alpha(kit::mix(c, BLACK, 0.1 + 0.3 * t), 1.0));
    }
    for i in 0..5 {
        let fx = cloth.x + cloth.w * (i as f32 + 0.5) / 5.0;
        draw_rectangle(fx - 6.0 * u, cloth.y, 12.0 * u, body_h, Color::new(1.0, 1.0, 1.0, if i % 2 == 0 { 0.035 } else { 0.0 }));
        draw_rectangle(fx + 6.0 * u, cloth.y, 4.0 * u, body_h, Color::new(0.0, 0.0, 0.0, 0.10));
    }
    // Swallow tail.
    let base = cloth.y + body_h;
    let tail_c = kit::mix(c, BLACK, 0.45);
    draw_triangle(vec2(cloth.x, base), vec2(cloth.x + cloth.w / 2.0, base), vec2(cloth.x, base + tail), tail_c);
    draw_triangle(vec2(cloth.x + cloth.w / 2.0, base), vec2(cloth.right(), base), vec2(cloth.right(), base + tail), tail_c);
    draw_triangle(vec2(cloth.x, base), vec2(cloth.x + cloth.w / 2.0, base), vec2(cloth.x + cloth.w / 4.0, base + tail * 0.75), kit::mix(c, BLACK, 0.5));
    // Gold fringe trim around the cloth.
    let g = kit::alpha(GOLD, 0.9);
    draw_line(cloth.x, cloth.y, cloth.x, base + tail, 2.0 * u, g);
    draw_line(cloth.right(), cloth.y, cloth.right(), base + tail, 2.0 * u, g);
    draw_line(cloth.x + 8.0 * u, cloth.y + 8.0 * u, cloth.right() - 8.0 * u, cloth.y + 8.0 * u, 1.0, kit::alpha(GOLD_BRIGHT, 0.5));
    draw_line(cloth.x, base + tail, cloth.x + cloth.w / 4.0, base + tail * 0.1, 1.5 * u, g);
    // Rod with finials.
    draw_rectangle(r.x, rod_y - 3.0 * u, r.w, 7.0 * u, GOLD_DIM);
    draw_rectangle(r.x, rod_y - 3.0 * u, r.w, 2.5 * u, GOLD_BRIGHT);
    draw_circle(r.x, rod_y, 6.0 * u, GOLD);
    draw_circle(r.right(), rod_y, 6.0 * u, GOLD);
    Rect::new(cloth.x, cloth.y, cloth.w, body_h)
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let _ = assets;
    kit::begin(ui, WindowId::Guild);
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        let st = &mut *st;
        if game.now - st.last_draw > 0.6 {
            // Opened: refresh both the guild and the list of guilds.
            if !crate::ui::console::demo_on() {
                game.guild_info();
                game.guild_list();
            }
            st.confirm_leave = false;
            st.edit = None;
        }
        st.last_draw = game.now;
        let r = Rect::new(r.x + 6.0 * ui.u, r.y + 2.0 * ui.u, r.w - 12.0 * ui.u, r.h - 4.0 * ui.u);
        match game.local.social.guild.info.clone() {
            None => no_guild(ui, game, st, r),
            Some(info) => with_guild(ui, game, st, &info, r),
        }
    });
}

/// Rows scrolled by wheel in a list identified by `key`; returns (first, rows visible).
fn scroll_rows(st: &mut State, key: &'static str, area: Rect, total: usize, row_h: f32, u: f32) -> (usize, usize) {
    let vis = ((area.h / row_h).floor() as usize).max(1);
    let cur = *st.scroll.get(key).unwrap_or(&0);
    let off = kit::wheel(area, cur, total, vis, 1);
    st.scroll.insert(key, off);
    kit::scrollbar(area, total, vis, off, u);
    (off, vis)
}

// ------------------------------------------------------------------ no guild

fn no_guild(ui: &mut Ui, game: &mut Game, st: &mut State, r: Rect) {
    let u = ui.u;
    let labels = ["Clanes del reino".to_string(), "Fundar un clan".to_string()];
    if let Some(i) = kit::tabs(r.x + 8.0 * u, r.y, &labels, if st.creating { 1 } else { 0 }, u) {
        st.creating = i == 1;
    }
    if !st.creating && button_kind(Rect::new(r.right() - 100.0 * u, r.y + 2.0 * u, 100.0 * u, 26.0 * u), "Actualizar", Kind::Ghost, true, u) {
        game.guild_list();
    }
    kit::rule(r.x, r.y + 30.0 * u, r.w, GOLD);
    let body = Rect::new(r.x, r.y + 40.0 * u, r.w, r.h - 40.0 * u);
    if st.creating {
        found_form(ui, game, st, body);
    } else {
        guild_list(ui, game, st, body);
    }
}

fn guild_list(ui: &mut Ui, game: &mut Game, st: &mut State, r: Rect) {
    let u = ui.u;
    let list = game.local.social.guild.list.clone();
    let left = Rect::new(r.x, r.y, 252.0 * u, r.h);
    if st.selected.as_ref().is_none_or(|s| !list.iter().any(|(n, _)| n == s)) {
        st.selected = list.first().map(|(n, _)| n.clone());
    }
    text_with(FontKind::NarrativeItalic, "No perteneces a ningún clan.", left.x + 4.0 * u, left.y + 14.0 * u, 16.0 * u, TEXT_DIM);
    let area = Rect::new(left.x, left.y + 26.0 * u, left.w, left.h - 26.0 * u);
    if list.is_empty() {
        kit::empty(area, "Aún no hay clanes.", "Funda el primero.", u);
    }
    let rh = 46.0 * u;
    let (off, vis) = scroll_rows(st, "guilds", area, list.len(), rh, u);
    for (i, (name, al)) in list.iter().enumerate().skip(off).take(vis) {
        let row = Rect::new(area.x, area.y + (i - off) as f32 * rh, area.w - 8.0 * u, rh - 2.0 * u);
        let (_, click) = kit::row(row, st.selected.as_deref() == Some(name), u);
        if click {
            st.selected = Some(name.clone());
        }
        let (col, sym) = align_look(*al);
        kit::emblem(row.x + 22.0 * u, row.center().y, 24.0 * u, col, sym, u * 0.6);
        kit::fit(FontKind::Title, name, row.x + 44.0 * u, row.y + 21.0 * u, 16.0 * u, row.w - 50.0 * u, TEXT);
        text_with(FontKind::Body, align_name(*al), row.x + 44.0 * u, row.y + 38.0 * u, 13.0 * u, kit::mix(col, WHITE, 0.45));
    }
    let right = Rect::new(left.right() + 22.0 * u, r.y, r.w - left.w - 22.0 * u, r.h);
    draw_line(left.right() + 11.0 * u, r.y + 4.0 * u, left.right() + 11.0 * u, r.bottom() - 4.0 * u, 1.0, kit::alpha(GOLD, 0.35));
    let Some(sel) = st.selected.clone().filter(|s| list.iter().any(|(n, _)| n == s)) else {
        kit::empty(right, "Elige un clan de la lista.", "Podrás enviarle tu solicitud.", u);
        return;
    };
    let al = list.iter().find(|(n, _)| *n == sel).map(|(_, a)| *a).unwrap_or(0);
    let (col, sym) = align_look(al);
    kit::emblem(right.x + 34.0 * u, right.y + 44.0 * u, 52.0 * u, col, sym, u);
    kit::fit(FontKind::Title, &sel, right.x + 82.0 * u, right.y + 40.0 * u, 26.0 * u, right.w - 82.0 * u, GOLD_BRIGHT);
    text_with(FontKind::NarrativeItalic, align_name(al), right.x + 82.0 * u, right.y + 64.0 * u, 17.0 * u, kit::mix(col, WHITE, 0.55));
    // The application letter.
    let page = Rect::new(right.x, right.y + 96.0 * u, right.w, 188.0 * u);
    kit::page(page, u);
    kit::ink(FontKind::Title, "CARTA AL LÍDER", page.x + 22.0 * u, page.y + 32.0 * u, 16.0 * u, WAX);
    kit::rule(page.x + 20.0 * u, page.y + 42.0 * u, page.w - 40.0 * u, Color::new(0.35, 0.2, 0.08, 0.9));
    let field = Rect::new(page.x + 20.0 * u, page.y + 52.0 * u, page.w - 40.0 * u, 120.0 * u);
    let enter = letter_field(ui, "guild.application", &mut st.application, field, guild::MAX_APPLICATION_LEN, "Cuéntales quién eres y qué sabes hacer...", false, |_| true);
    let send = Rect::new(right.x, page.bottom() + 14.0 * u, 200.0 * u, 36.0 * u);
    if (button_kind(send, "Solicitar ingreso", Kind::Primary, true, u) || enter) && game.guild_request_join(&sel, &st.application) {
        st.application.clear();
        ui.focus = None;
    }
    let tip_y = send.bottom() + 12.0 * u;
    let t = "El líder recibirá tu carta y podrá aceptarla o rechazarla. Los personajes de nivel 12 o menos no pueden entrar en un clan.";
    for (i, row) in kit::wrap(FontKind::NarrativeItalic, t, 15.0 * u, right.w).iter().enumerate() {
        text_with(FontKind::NarrativeItalic, row, right.x, tip_y + 14.0 * u + i as f32 * 20.0 * u, 15.0 * u, TEXT_DIM);
    }
}

fn found_form(ui: &mut Ui, game: &mut Game, st: &mut State, r: Rect) {
    let u = ui.u;
    let deed = Rect::new(r.x, r.y, r.w * 0.58, r.h);
    kit::page(deed, u);
    let px = deed.x + 22.0 * u;
    let pw = deed.w - 44.0 * u;
    kit::ink_mid(FontKind::Title, "ACTA DE FUNDACIÓN", deed.center().x, deed.y + 34.0 * u, 20.0 * u, pw, WAX);
    kit::rule(px, deed.y + 46.0 * u, pw, Color::new(0.35, 0.2, 0.08, 0.9));
    kit::ink(FontKind::Body, "Nombre del clan", px, deed.y + 72.0 * u, 14.0 * u, INK_DIM);
    let name_r = Rect::new(px, deed.y + 76.0 * u, pw, 30.0 * u);
    let _ = letter_field(ui, "guild.found_name", &mut st.found_name, name_r, guild::MAX_NAME_LEN, "El nombre que dará fama a tu estandarte", true, |c| c.is_ascii_alphabetic() || c == ' ' || c == 'ÿ');
    kit::ink(FontKind::Body, "Alineación", px, deed.y + 134.0 * u, 14.0 * u, INK_DIM);
    let aligns = [0u8, 3, 4, 1, 2];
    let cw = pw / 5.0;
    for (i, a) in aligns.iter().enumerate() {
        let cell = Rect::new(px + cw * i as f32, deed.y + 142.0 * u, cw, 86.0 * u);
        let sel = st.found_align == *a;
        let (col, sym) = align_look(*a);
        let (hv, click) = kit::row_ink(cell, sel, u);
        kit::emblem(cell.center().x, cell.y + 34.0 * u, 40.0 * u, col, sym, u);
        let nm = align_name(*a);
        for (k, word) in nm.split(' ').enumerate() {
            kit::ink_mid(FontKind::Body, word, cell.center().x, cell.y + 70.0 * u + k as f32 * 13.0 * u, 13.0 * u, cell.w, if sel { WAX } else { INK_DIM });
        }
        if click {
            st.found_align = *a;
        }
        let _ = hv;
    }
    kit::ink(FontKind::Body, "Descripción", px, deed.y + 252.0 * u, 14.0 * u, INK_DIM);
    let desc_r = Rect::new(px, deed.y + 258.0 * u, pw, 104.0 * u);
    letter_field(ui, "guild.found_desc", &mut st.found_desc, desc_r, guild::MAX_DESC_LEN, "Qué clase de clan es, qué buscáis...", false, |_| true);

    // Requirements, ticked against the character.
    let rx = deed.right() + 26.0 * u;
    let rw = r.right() - rx;
    let mut y = kit::section("Requisitos", rx, r.y, rw, u);
    for (label, ok) in game.found_requirements() {
        let c = if ok { GOOD } else { BAD };
        draw_circle(rx + 10.0 * u, y + 11.0 * u, 10.0 * u, kit::alpha(c, 0.18));
        draw_circle_lines(rx + 10.0 * u, y + 11.0 * u, 10.0 * u, 1.3, c);
        kit::icon(if ok { Icon::Check } else { Icon::Cross }, rx + 10.0 * u, y + 11.0 * u, 12.0 * u, c);
        let rows = kit::wrap(FontKind::Body, &label, 15.0 * u, rw - 30.0 * u);
        for (i, row) in rows.iter().enumerate() {
            text_with(FontKind::Body, row, rx + 28.0 * u, y + 16.0 * u + i as f32 * 17.0 * u, 15.0 * u, if ok { TEXT } else { Color::new(1.0, 0.72, 0.66, 1.0) });
        }
        y += 14.0 * u + rows.len() as f32 * 17.0 * u;
    }
    let refusal = game.found_refusal(st.found_align);
    y += 8.0 * u;
    if let Some(why) = refusal {
        for (i, row) in kit::wrap(FontKind::NarrativeItalic, why, 15.0 * u, rw).iter().enumerate().take(5) {
            text_with(FontKind::NarrativeItalic, row, rx, y + 14.0 * u + i as f32 * 18.0 * u, 15.0 * u, BAD);
        }
    } else {
        text_with(FontKind::NarrativeItalic, "Cumples todos los requisitos.", rx, y + 14.0 * u, 16.0 * u, GOOD);
    }
    let found = Rect::new(rx, r.bottom() - 40.0 * u, rw, 38.0 * u);
    if button_kind(found, "Fundar clan", Kind::Primary, refusal.is_none(), u) {
        let (n, d, a) = (st.found_name.clone(), st.found_desc.clone(), st.found_align);
        if game.guild_found(&n, &d, a) {
            ui.focus = None;
        }
    }
    if kit::hover(found) {
        tooltip(&["Fundar el clan".into(), "~Se consumen las cuatro gemas de fundación.".into()], u);
    }
}

// ------------------------------------------------------------------ with a guild

fn with_guild(ui: &mut Ui, game: &mut Game, st: &mut State, info: &GuildInfo, r: Rect) {
    let u = ui.u;
    let leader = game.is_guild_leader();
    let left = Rect::new(r.x, r.y, 200.0 * u, r.h);
    let right = Rect::new(left.right() + 22.0 * u, r.y, r.w - left.w - 22.0 * u, r.h);

    // ---- the banner column
    let (col, sym) = align_look(info.alignment);
    let bn = Rect::new(left.x, left.y, left.w, 296.0 * u);
    let cloth = banner(bn, col, u);
    kit::emblem(cloth.center().x, cloth.y + 58.0 * u, 70.0 * u, col, sym, u);
    let nrows = kit::wrap(FontKind::Title, &info.name.to_uppercase(), 18.0 * u, cloth.w - 18.0 * u);
    for (i, row) in nrows.iter().take(2).enumerate() {
        kit::text_mid(FontKind::Title, row, cloth.center().x, cloth.y + 126.0 * u + i as f32 * 22.0 * u, 18.0 * u, GOLD_BRIGHT);
    }
    let ay = cloth.y + 126.0 * u + nrows.len().min(2) as f32 * 22.0 * u + 8.0 * u;
    kit::text_mid(FontKind::NarrativeItalic, align_name(info.alignment), cloth.center().x, ay, 16.0 * u, kit::mix(col, WHITE, 0.75));
    kit::seal(cloth.center().x, cloth.bottom() + 2.0 * u, 23.0 * u, &info.level.to_string(), "Nivel", u);

    // Under the banner: experience, members, leader, founder.
    let mut y = bn.bottom() + 6.0 * u;
    let exp_next = if info.exp_next > 0 { info.exp_next as f32 } else { info.exp.max(1) as f32 };
    let bar = Rect::new(left.x + 4.0 * u, y + 6.0 * u, left.w - 8.0 * u, 7.0 * u);
    kit::thin_bar(bar, info.exp as f32, exp_next, Color::new(0.82, 0.64, 0.18, 1.0));
    draw_rectangle_lines(bar.x, bar.y, bar.w, bar.h, 1.0, kit::alpha(GOLD_DIM, 0.7));
    let exp_txt = if info.exp_next > 0 { format!("{} / {} de experiencia", kit::thousands(info.exp as u64), kit::thousands(info.exp_next as u64)) } else { format!("{} exp · nivel máximo", kit::thousands(info.exp as u64)) };
    text_with(FontKind::Body, &exp_txt, left.x + 4.0 * u, y + 28.0 * u, 12.0 * u, TEXT_DIM);
    y += 40.0 * u;
    let facts = [(Icon::Banner, format!("{} de {} miembros", info.members.len(), info.max_members), TEXT), (Icon::Crown, info.leader.clone(), GOLD_BRIGHT), (Icon::Star, format!("Fundó {}", info.founder), TEXT_DIM)];
    for (ic, txt, c) in facts {
        kit::icon(ic, left.x + 12.0 * u, y + 9.0 * u, 14.0 * u, kit::alpha(GOLD, 0.9));
        kit::fit(FontKind::Body, &txt, left.x + 28.0 * u, y + 14.0 * u, 15.0 * u, left.w - 32.0 * u, c);
        y += 24.0 * u;
    }
    // Leave.
    let b = Rect::new(left.x + 4.0 * u, left.bottom() - 30.0 * u, left.w - 8.0 * u, 28.0 * u);
    let label = if st.confirm_leave { "¿Seguro? Pulsa otra vez" } else if leader { "Disolver el clan" } else { "Abandonar el clan" };
    if button_kind(b, label, if st.confirm_leave { Kind::Danger } else { Kind::Ghost }, true, u) {
        if st.confirm_leave {
            game.guild_leave();
            st.confirm_leave = false;
        } else {
            st.confirm_leave = true;
        }
    }
    if kit::hover(b) {
        tooltip(&["Abandonar el clan".into(), if leader { "~Eres el líder: el clan se queda sin él.".into() } else { "~Dejarás de ser miembro.".into() }], u);
    }

    // ---- the pages
    let mut tabs = vec![(Tab::Info, "Noticias".to_string()), (Tab::Members, format!("Miembros ({})", info.members.len()))];
    if leader {
        tabs.push((Tab::Applicants, format!("Aspirantes ({})", info.applicants.len())));
    }
    tabs.push((Tab::Relations, "Relaciones".to_string()));
    if st.tab == Tab::Applicants && !leader {
        st.tab = Tab::Info;
    }
    let labels: Vec<String> = tabs.iter().map(|t| t.1.clone()).collect();
    let active = tabs.iter().position(|t| t.0 == st.tab).unwrap_or(0);
    if let Some(i) = kit::tabs(right.x + 6.0 * u, right.y, &labels, active, u) {
        st.tab = tabs[i].0;
        st.edit = None;
    }
    kit::rule(right.x, right.y + 30.0 * u, right.w, GOLD);
    let body = Rect::new(right.x, right.y + 42.0 * u, right.w, right.h - 42.0 * u);
    match st.tab {
        Tab::Info => info_tab(ui, game, st, info, body, leader),
        Tab::Members => members_tab(game, st, info, body, leader, u),
        Tab::Applicants => applicants_tab(ui, game, st, info, body),
        Tab::Relations => relations_tab(ui, game, st, info, body, leader),
    }
}

fn info_tab(ui: &mut Ui, game: &mut Game, st: &mut State, info: &GuildInfo, r: Rect, leader: bool) {
    let u = ui.u;
    // News on parchment.
    let news_h = r.h * 0.58;
    let page = Rect::new(r.x, r.y, r.w, news_h);
    kit::page(page, u);
    kit::ink(FontKind::Title, "TABLÓN DE NOTICIAS", page.x + 22.0 * u, page.y + 32.0 * u, 16.0 * u, WAX);
    kit::icon(Icon::Scroll, page.right() - 26.0 * u, page.y + 26.0 * u, 18.0 * u, INK_DIM);
    kit::rule(page.x + 20.0 * u, page.y + 42.0 * u, page.w - 40.0 * u, Color::new(0.35, 0.2, 0.08, 0.9));
    let editing_news = st.edit.as_ref().is_some_and(|(e, _)| *e == Edit::News);
    if leader && !editing_news && ink_link(Rect::new(page.right() - 100.0 * u, page.y + 8.0 * u, 60.0 * u, 20.0 * u), "Editar", u) {
        st.edit = Some((Edit::News, info.news.clone()));
        ui.focus = Some("guild.edit".into());
    }
    let text_r = Rect::new(page.x + 22.0 * u, page.y + 52.0 * u, page.w - 44.0 * u, page.h - 62.0 * u);
    if editing_news {
        edit_box(ui, game, st, Edit::News, text_r, guild::MAX_NEWS_LEN, page.bottom() + 8.0 * u);
    } else if info.news.trim().is_empty() {
        kit::ink(FontKind::NarrativeItalic, "El tablón está vacío.", text_r.x, text_r.y + 22.0 * u, 18.0 * u, INK_DIM);
    } else {
        let rows = (text_r.h / (18.0 * u * 1.32)).floor() as usize;
        kit::paragraph(FontKind::Narrative, &info.news, text_r.x, text_r.y, text_r.w, 18.0 * u, INK, rows.max(1));
    }

    // Description on the dark side.
    let dy = page.bottom() + 18.0 * u;
    let mut y = kit::section("Sobre el clan", r.x, dy, r.w, u);
    let editing_desc = st.edit.as_ref().is_some_and(|(e, _)| *e == Edit::Description);
    if leader && !editing_desc && button_kind(Rect::new(r.right() - 70.0 * u, dy, 70.0 * u, 22.0 * u), "Editar", Kind::Ghost, true, u) {
        st.edit = Some((Edit::Description, info.description.clone()));
        ui.focus = Some("guild.edit".into());
    }
    if editing_desc {
        let area = Rect::new(r.x, y, r.w, 70.0 * u);
        kit::page(area, u);
        let field = Rect::new(area.x + 14.0 * u, area.y + 10.0 * u, area.w - 28.0 * u, area.h - 14.0 * u);
        edit_box(ui, game, st, Edit::Description, field, guild::MAX_DESC_LEN, area.bottom() + 8.0 * u);
    } else if info.description.trim().is_empty() {
        text_with(FontKind::NarrativeItalic, "(sin descripción)", r.x, y + 16.0 * u, 16.0 * u, TEXT_FAINT);
    } else {
        let rows = ((r.bottom() - y) / (16.0 * u * 1.32)).floor() as usize;
        y = kit::paragraph(FontKind::NarrativeItalic, &info.description, r.x, y, r.w, 16.0 * u, TEXT_DIM, rows.max(1));
        let _ = y;
    }
}

/// The edit field of news/description with Save and Cancel beside it.
fn edit_box(ui: &mut Ui, game: &mut Game, st: &mut State, which: Edit, field: Rect, max: usize, buttons_y: f32) {
    let u = ui.u;
    let mut buf = st.edit.as_ref().map(|(_, b)| b.clone()).unwrap_or_default();
    let enter = false;
    letter_field(ui, "guild.edit", &mut buf, field, max, "", false, |_| true);
    st.edit = Some((which, buf.clone()));
    let ok = Rect::new(field.right() - 200.0 * u, buttons_y - 4.0 * u, 96.0 * u, 26.0 * u);
    let cancel = Rect::new(ok.right() + 8.0 * u, ok.y, 96.0 * u, 26.0 * u);
    if button_kind(ok, "Guardar", Kind::Primary, true, u) || enter {
        if which == Edit::News {
            game.guild_update_news(&buf);
            st.edit = None;
        } else if game.guild_update_description(&buf) {
            st.edit = None;
        }
        ui.focus = None;
    }
    if button_kind(cancel, "Cancelar", Kind::Secondary, true, u) {
        st.edit = None;
        ui.focus = None;
    }
}

fn members_tab(game: &mut Game, st: &mut State, info: &GuildInfo, r: Rect, leader: bool, u: f32) {
    let me = game.me().map(|e| e.name.clone()).unwrap_or_default();
    let mut members = info.members.clone();
    members.sort_by(|a, b| b.online.cmp(&a.online).then(b.leader.cmp(&a.leader)).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    let online = members.iter().filter(|m| m.online).count();
    text_with(FontKind::NarrativeItalic, &format!("{online} despiertos de {}", members.len()), r.x + 4.0 * u, r.y + 12.0 * u, 16.0 * u, TEXT_DIM);
    let area = Rect::new(r.x, r.y + 24.0 * u, r.w, r.h - 24.0 * u);
    let rh = 38.0 * u;
    let (off, vis) = scroll_rows(st, "members", area, members.len(), rh, u);
    for (i, m) in members.iter().enumerate().skip(off).take(vis) {
        let row = Rect::new(area.x, area.y + (i - off) as f32 * rh, area.w - 10.0 * u, rh - 2.0 * u);
        let (hov, _) = kit::row(row, false, u);
        let dot = vec2(row.x + 16.0 * u, row.center().y);
        if m.online {
            draw_circle(dot.x, dot.y, 8.0 * u, Color::new(0.4, 0.9, 0.4, 0.15));
            draw_circle(dot.x, dot.y, 5.0 * u, GOOD);
        } else {
            draw_circle_lines(dot.x, dot.y, 4.5 * u, 1.3, TEXT_FAINT);
        }
        let label = if m.name == me { format!("{} (tú)", m.name) } else { m.name.clone() };
        kit::fit(FontKind::Title, &label, row.x + 34.0 * u, row.center().y + 6.0 * u, 18.0 * u, 200.0 * u, if m.online { Color::new(1.0, 0.97, 0.85, 1.0) } else { TEXT_DIM });
        let mut tx = row.x + 250.0 * u;
        if m.leader {
            kit::icon(Icon::Crown, tx + 7.0 * u, row.center().y, 15.0 * u, GOLD_BRIGHT);
            text_with(FontKind::Body, "Líder", tx + 20.0 * u, row.center().y + 5.0 * u, 14.0 * u, GOLD_TEXT);
            tx += 68.0 * u;
        }
        if m.founder {
            kit::icon(Icon::Star, tx + 7.0 * u, row.center().y, 14.0 * u, GOLD);
            text_with(FontKind::Body, "Fundador", tx + 20.0 * u, row.center().y + 5.0 * u, 14.0 * u, TEXT_DIM);
        }
        if leader && !m.leader && m.name != me && hov {
            let kb = Rect::new(row.right() - 88.0 * u, row.center().y - 12.0 * u, 82.0 * u, 24.0 * u);
            if button_kind(kb, "Expulsar", Kind::Danger, true, u) {
                game.guild_kick(&m.name);
            }
        } else if !m.online {
            kit::text_right(FontKind::NarrativeItalic, "lejos", row.right() - 10.0 * u, row.center().y + 5.0 * u, 14.0 * u, TEXT_FAINT);
        }
    }
}

fn applicants_tab(ui: &mut Ui, game: &mut Game, st: &mut State, info: &GuildInfo, r: Rect) {
    let u = ui.u;
    if info.applicants.is_empty() {
        kit::empty(r, "Nadie ha llamado a la puerta.", "Las solicitudes llegarán como cartas.", u);
        return;
    }
    let reject_h = if st.reject.is_some() { 128.0 * u } else { 0.0 };
    if st.reject.is_none() {
        text_with(FontKind::NarrativeItalic, "Aceptar a un aspirante cuesta oro del líder.", r.x + 4.0 * u, r.bottom() - 4.0 * u, 14.0 * u, TEXT_FAINT);
    }
    let area = Rect::new(r.x, r.y, r.w, r.h - reject_h - if st.reject.is_none() { 22.0 * u } else { 0.0 });
    let row_h = 100.0 * u;
    let (off, vis) = scroll_rows(st, "applicants", area, info.applicants.len(), row_h, u);
    for (i, a) in info.applicants.iter().enumerate().skip(off).take(vis) {
        let card = Rect::new(area.x, area.y + (i - off) as f32 * row_h, area.w - 12.0 * u, row_h - 10.0 * u);
        let letter = Rect::new(card.x, card.y, card.w - 128.0 * u, card.h);
        kit::page(letter, u);
        kit::ink(FontKind::Title, &a.name, letter.x + 18.0 * u, letter.y + 28.0 * u, 18.0 * u, WAX);
        kit::paragraph(FontKind::Narrative, &a.application, letter.x + 18.0 * u, letter.y + 34.0 * u, letter.w - 36.0 * u, 16.0 * u, INK, 3);
        let bx = card.right() - 118.0 * u;
        let accept = Rect::new(bx, card.y + 6.0 * u, 118.0 * u, 32.0 * u);
        if button_kind(accept, "Aceptar", Kind::Primary, true, u) {
            game.guild_accept(&a.name);
        }
        if kit::hover(accept) {
            tooltip(&["Aceptar al aspirante".into(), "~Cuesta oro del líder.".into()], u);
        }
        let rejecting = st.reject.as_deref() == Some(&a.name);
        if button_kind(Rect::new(bx, card.y + 46.0 * u, 118.0 * u, 30.0 * u), "Rechazar", if rejecting { Kind::Danger } else { Kind::Secondary }, true, u) {
            st.reject = Some(a.name.clone());
            st.reject_reason.clear();
            ui.focus = Some("guild.reject".into());
        }
    }
    if let Some(name) = st.reject.clone() {
        if !info.applicants.iter().any(|a| a.name == name) {
            st.reject = None;
            return;
        }
        let b = Rect::new(r.x, r.bottom() - reject_h + 4.0 * u, r.w, reject_h - 4.0 * u);
        kit::page(b, u);
        kit::ink(FontKind::Title, &format!("MOTIVO DEL RECHAZO A {}", name.to_uppercase()), b.x + 18.0 * u, b.y + 28.0 * u, 14.0 * u, WAX);
        let field = Rect::new(b.x + 18.0 * u, b.y + 38.0 * u, b.w - 190.0 * u, 52.0 * u);
        let enter = letter_field(ui, "guild.reject", &mut st.reject_reason, field, guild::MAX_APPLICATION_LEN, "Por qué no entra...", false, |_| true);
        if (button_kind(Rect::new(field.right() + 14.0 * u, b.y + 40.0 * u, 128.0 * u, 30.0 * u), "Rechazar", Kind::Danger, true, u) || enter) && game.guild_reject(&name, &st.reject_reason) {
            st.reject = None;
            st.reject_reason.clear();
            ui.focus = None;
        }
        if button_kind(Rect::new(field.right() + 14.0 * u, b.y + 76.0 * u, 128.0 * u, 26.0 * u), "Cancelar", Kind::Ghost, true, u) {
            st.reject = None;
            ui.focus = None;
        }
    }
}

fn relations_tab(ui: &mut Ui, game: &mut Game, st: &mut State, info: &GuildInfo, r: Rect, leader: bool) {
    let u = ui.u;
    let top_h = if leader { r.h - 214.0 * u } else { r.h };
    let top = Rect::new(r.x, r.y, r.w, top_h);
    // One list: relations first, then the proposals received.
    enum Item<'a> {
        Rel(&'a str, GuildRelation),
        Prop(&'a str, GuildRelation, &'a str),
    }
    let mut items: Vec<Item> = info.relations.iter().map(|(n, rel)| Item::Rel(n, *rel)).collect();
    items.extend(info.proposals.iter().map(|(n, rel, m)| Item::Prop(n, *rel, m)));
    if items.is_empty() {
        kit::empty(top, "En paz con todos los clanes.", "Nadie os ha declarado nada.", u);
    }
    let rh = 46.0 * u;
    let (off, vis) = scroll_rows(st, "relations", top, items.len(), rh, u);
    for (i, it) in items.iter().enumerate().skip(off).take(vis) {
        let row = Rect::new(top.x, top.y + (i - off) as f32 * rh, top.w - 10.0 * u, rh - 2.0 * u);
        kit::row(row, false, u);
        match it {
            Item::Rel(name, rel) => {
                let (label, c, ic) = relation_look(*rel);
                kit::badge_icon(ic, row.x + 22.0 * u, row.center().y, 14.0 * u, c);
                kit::fit(FontKind::Title, name, row.x + 46.0 * u, row.center().y + 6.0 * u, 17.0 * u, row.w - 240.0 * u, TEXT);
                text_with(FontKind::Body, label, row.x + row.w - 190.0 * u, row.center().y + 5.0 * u, 15.0 * u, c);
                if leader && *rel == GuildRelation::War && button_kind(Rect::new(row.right() - 90.0 * u, row.center().y - 13.0 * u, 84.0 * u, 26.0 * u), "Pedir paz", Kind::Secondary, true, u) {
                    game.guild_propose(name, GuildRelation::Peace, &st.rel_text.clone());
                }
            }
            Item::Prop(name, rel, msg) => {
                let (label, c, ic) = relation_look(*rel);
                kit::badge_icon(ic, row.x + 22.0 * u, row.center().y, 14.0 * u, WARN_GOLD);
                kit::fit(FontKind::Title, &format!("{name} propone {}", label.to_lowercase()), row.x + 46.0 * u, row.y + 20.0 * u, 16.0 * u, row.w - 200.0 * u, c);
                kit::fit(FontKind::NarrativeItalic, msg, row.x + 46.0 * u, row.y + 38.0 * u, 14.0 * u, row.w - 200.0 * u, TEXT_DIM);
                if leader {
                    if button_kind(Rect::new(row.right() - 146.0 * u, row.center().y - 13.0 * u, 68.0 * u, 26.0 * u), "Sí", Kind::Primary, true, u) {
                        game.guild_answer_proposal(name, true);
                    }
                    if button_kind(Rect::new(row.right() - 72.0 * u, row.center().y - 13.0 * u, 66.0 * u, 26.0 * u), "No", Kind::Secondary, true, u) {
                        game.guild_answer_proposal(name, false);
                    }
                }
            }
        }
    }
    if !leader {
        text_with(FontKind::NarrativeItalic, "Solo el líder trata con otros clanes.", r.x, r.bottom() - 8.0 * u, 15.0 * u, TEXT_FAINT);
        return;
    }
    // New proposal.
    let ny = top.bottom() + 8.0 * u;
    let lower = Rect::new(r.x, ny, r.w, r.bottom() - ny);
    let hy = kit::section("Mensaje a otro clan", lower.x, lower.y, lower.w, u);
    let others: Vec<String> = game.local.social.guild.list.iter().map(|(n, _)| n.clone()).filter(|n| *n != info.name).collect();
    let area = Rect::new(lower.x, hy, 170.0 * u, 3.0 * 30.0 * u);
    if others.is_empty() {
        text_with(FontKind::NarrativeItalic, "No hay otros clanes.", area.x, area.y + 18.0 * u, 15.0 * u, TEXT_FAINT);
    }
    let (off, vis) = scroll_rows(st, "relguilds", area, others.len(), 30.0 * u, u);
    for (i, name) in others.iter().enumerate().skip(off).take(vis) {
        let row = Rect::new(area.x, area.y + (i - off) as f32 * 30.0 * u, area.w - 8.0 * u, 28.0 * u);
        let (_, click) = kit::row(row, st.rel_target.as_deref() == Some(name), u);
        if click {
            st.rel_target = Some(name.clone());
        }
        kit::fit(FontKind::Body, name, row.x + 10.0 * u, row.y + 19.0 * u, 16.0 * u, row.w - 14.0 * u, TEXT);
    }
    let fx = area.right() + 16.0 * u;
    let fw = lower.right() - fx;
    let page = Rect::new(fx, hy - 2.0 * u, fw, 62.0 * u);
    kit::page(page, u);
    let msg_field = Rect::new(page.x + 14.0 * u, page.y + 8.0 * u, page.w - 28.0 * u, 50.0 * u);
    letter_field(ui, "guild.rel", &mut st.rel_text, msg_field, 200, "Mensaje (opcional)", false, |_| true);
    let target = st.rel_target.clone().filter(|t| others.contains(t));
    let by = page.bottom() + 12.0 * u;
    let bw = (fw - 16.0 * u) / 3.0;
    let msg = st.rel_text.clone();
    for (i, (label, rel, kind)) in [("Paz", GuildRelation::Peace, Kind::Secondary), ("Alianza", GuildRelation::Alliance, Kind::Secondary)].iter().enumerate() {
        if button_kind(Rect::new(fx + i as f32 * (bw + 8.0 * u), by, bw, 30.0 * u), label, *kind, target.is_some(), u) {
            if let Some(t) = &target {
                game.guild_propose(t, *rel, &msg);
            }
        }
    }
    if button_kind(Rect::new(fx + 2.0 * (bw + 8.0 * u), by, bw, 30.0 * u), "Guerra", Kind::Danger, target.is_some(), u) {
        if let Some(t) = &target {
            game.guild_declare_war(t);
        }
    }
    let hint = target.as_deref().map(|t| format!("Dirigido a {t}")).unwrap_or_else(|| "Elige un clan de la lista.".into());
    text_with(FontKind::NarrativeItalic, &hint, fx, by + 50.0 * u, 15.0 * u, TEXT_DIM);
}

const WARN_GOLD: Color = Color::new(1.0, 0.78, 0.30, 1.0);

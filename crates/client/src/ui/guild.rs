//! Window: guild (info, members, applicants, news, relations). OWNER: Sonnet 3 (docs/P3_CONTRATO.md).
//!
//! Without a guild: the list of guilds (apply with a text) and the founding form with the
//! requirements checked against the character. With one: information, members (online
//! ones highlighted), applicants (leader), relations (leader proposes / answers).
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::console::wrap;
use crate::ui::theme::{FontKind, BAD, GOLD, GOLD_TEXT, GOOD, MUTED, TEXT};
use crate::ui::widgets::{bar, button, text, text_field, text_fit, text_with, tooltip};
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

fn align_color(a: u8) -> Color {
    match a {
        1 => Color::new(0.55, 0.75, 1.0, 1.0),
        2 => Color::new(1.0, 0.45, 0.4, 1.0),
        3 => Color::new(0.6, 0.95, 0.55, 1.0),
        4 => Color::new(0.85, 0.5, 0.3, 1.0),
        _ => MUTED,
    }
}

fn relation_label(r: GuildRelation) -> (&'static str, Color) {
    match r {
        GuildRelation::War => ("Guerra", BAD),
        GuildRelation::Peace => ("Paz", GOOD),
        GuildRelation::Alliance => ("Alianza", Color::new(0.55, 0.75, 1.0, 1.0)),
    }
}

/// A text field that wraps over several rows (single logical line). Enter returns true.
fn wrapped_field(ui: &mut Ui, id: &str, value: &mut String, r: Rect, max: usize, hint: &str) -> bool {
    let u = ui.u;
    let focused = ui.focus.as_deref() == Some(id);
    if r.contains(mouse_position().into()) && is_mouse_button_pressed(MouseButton::Left) {
        ui.focus = Some(id.to_string());
        ui.mouse_captured = true;
    }
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.6));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, if focused { 2.0 } else { 1.0 }, if focused { GOLD_TEXT } else { GOLD });
    let mut enter = false;
    if focused {
        ui.keyboard_captured = true;
        while let Some(ch) = get_char_pressed() {
            if !ch.is_control() && value.chars().count() < max {
                value.push(ch);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            value.pop();
        }
        enter = is_key_pressed(KeyCode::Enter);
        if is_key_pressed(KeyCode::Escape) {
            ui.focus = None;
        }
    }
    let size = 16.0 * u;
    if value.is_empty() && !focused {
        text(hint, r.x + 8.0 * u, r.y + 18.0 * u, size, MUTED);
        return false;
    }
    let cursor = if focused && (get_time() * 2.0) as i32 % 2 == 0 { "|" } else { "" };
    let rows = wrap(&format!("{value}{cursor}"), size, r.w - 16.0 * u);
    let fit = (((r.h - 6.0 * u) / (17.0 * u)).floor() as usize).max(1);
    for (i, row) in rows.iter().skip(rows.len().saturating_sub(fit)).enumerate() {
        text(row, r.x + 8.0 * u, r.y + 18.0 * u + i as f32 * 17.0 * u, size, TEXT);
    }
    if focused {
        text(&format!("{}/{max}", value.chars().count()), r.right() - 70.0 * u, r.bottom() - 4.0 * u, 13.0 * u, MUTED);
    }
    enter
}

/// Wheel scrolling of a list; returns (first row, rows that fit) and draws the scroll bar.
fn scroll_rows(st: &mut State, key: &'static str, area: Rect, total: usize, row_h: f32, u: f32) -> (usize, usize) {
    let vis = ((area.h / row_h).floor() as usize).max(1);
    let off = st.scroll.entry(key).or_insert(0);
    let wheel = mouse_wheel().1;
    if area.contains(mouse_position().into()) {
        if wheel > 0.0 {
            *off = off.saturating_sub(1);
        } else if wheel < 0.0 {
            *off += 1;
        }
    }
    *off = (*off).min(total.saturating_sub(vis));
    if total > vis {
        let tr = Rect::new(area.right() - 5.0 * u, area.y, 5.0 * u, area.h);
        draw_rectangle(tr.x, tr.y, tr.w, tr.h, Color::new(0.0, 0.0, 0.0, 0.4));
        let h = (tr.h * vis as f32 / total as f32).max(14.0 * u);
        draw_rectangle(tr.x, tr.y + (tr.h - h) * *off as f32 / (total - vis) as f32, tr.w, h, GOLD);
    }
    (*off, vis)
}

fn row_bg(r: Rect, selected: bool) -> bool {
    let hover = r.contains(mouse_position().into());
    draw_rectangle(r.x, r.y, r.w, r.h, if selected { Color::new(0.32, 0.22, 0.08, 0.95) } else if hover { Color::new(0.2, 0.14, 0.07, 0.9) } else { Color::new(0.0, 0.0, 0.0, 0.3) });
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, if selected { GOLD_TEXT } else { Color::new(0.3, 0.22, 0.12, 1.0) });
    hover && is_mouse_button_pressed(MouseButton::Left)
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let _ = assets;
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
        match game.local.social.guild.info.clone() {
            None => no_guild(ui, game, st, r),
            Some(info) => with_guild(ui, game, st, &info, r),
        }
    });
}

// ------------------------------------------------------------------ no guild

fn no_guild(ui: &mut Ui, game: &mut Game, st: &mut State, r: Rect) {
    let u = ui.u;
    if button(Rect::new(r.x, r.y, 150.0 * u, 26.0 * u), "Clanes", !st.creating, u) {
        st.creating = false;
    }
    if button(Rect::new(r.x + 156.0 * u, r.y, 150.0 * u, 26.0 * u), "Fundar un clan", st.creating, u) {
        st.creating = true;
    }
    if !st.creating && button(Rect::new(r.right() - 110.0 * u, r.y, 110.0 * u, 26.0 * u), "Actualizar", false, u) {
        game.guild_list();
    }
    let body = Rect::new(r.x, r.y + 38.0 * u, r.w, r.h - 38.0 * u);
    if st.creating {
        found_form(ui, game, st, body);
    } else {
        guild_list(ui, game, st, body);
    }
}

fn guild_list(ui: &mut Ui, game: &mut Game, st: &mut State, r: Rect) {
    let u = ui.u;
    let list = game.local.social.guild.list.clone();
    let left = Rect::new(r.x, r.y, 240.0 * u, r.h);
    text("No perteneces a ningún clan.", left.x, left.y + 14.0 * u, 17.0 * u, TEXT);
    let area = Rect::new(left.x, left.y + 26.0 * u, left.w, left.h - 26.0 * u);
    if list.is_empty() {
        text("Todavía no hay clanes fundados.", area.x, area.y + 18.0 * u, 16.0 * u, MUTED);
    }
    let (off, vis) = scroll_rows(st, "guilds", area, list.len(), 30.0 * u, u);
    for (i, (name, al)) in list.iter().enumerate().skip(off).take(vis) {
        let row = Rect::new(area.x, area.y + (i - off) as f32 * 30.0 * u, area.w - 8.0 * u, 28.0 * u);
        if row_bg(row, st.selected.as_deref() == Some(name)) {
            st.selected = Some(name.clone());
        }
        text_fit(name, row.x + 8.0 * u, row.y + 19.0 * u, 17.0 * u, row.w - 110.0 * u, TEXT);
        text(align_name(*al), row.right() - 98.0 * u, row.y + 19.0 * u, 14.0 * u, align_color(*al));
    }
    let right = Rect::new(left.right() + 14.0 * u, r.y, r.w - left.w - 14.0 * u, r.h);
    draw_line(left.right() + 7.0 * u, r.y, left.right() + 7.0 * u, r.bottom(), 1.0, GOLD);
    let Some(sel) = st.selected.clone().filter(|s| list.iter().any(|(n, _)| n == s)) else {
        text("Elige un clan de la lista", right.x, right.y + 16.0 * u, 16.0 * u, MUTED);
        text("para pedir el ingreso.", right.x, right.y + 34.0 * u, 16.0 * u, MUTED);
        return;
    };
    let al = list.iter().find(|(n, _)| *n == sel).map(|(_, a)| *a).unwrap_or(0);
    text_with(FontKind::Title, &sel, right.x, right.y + 20.0 * u, 22.0 * u, GOLD_TEXT);
    text(align_name(al), right.x, right.y + 40.0 * u, 16.0 * u, align_color(al));
    text("Presentación para el líder:", right.x, right.y + 72.0 * u, 16.0 * u, TEXT);
    let field = Rect::new(right.x, right.y + 80.0 * u, right.w, 100.0 * u);
    let enter = wrapped_field(ui, "guild.application", &mut st.application, field, guild::MAX_APPLICATION_LEN, "Cuéntales quién eres y qué sabes hacer...");
    let send = Rect::new(right.x, field.bottom() + 8.0 * u, 170.0 * u, 28.0 * u);
    if (button(send, "Solicitar ingreso", true, u) || enter) && game.guild_request_join(&sel, &st.application) {
        st.application.clear();
        ui.focus = None;
    }
    let tip_y = send.bottom() + 16.0 * u;
    for (i, row) in wrap("El líder recibirá tu solicitud y podrá aceptarla o rechazarla. Los personajes de nivel 12 o menos no pueden entrar en un clan.", 15.0 * u, right.w).iter().enumerate() {
        text(row, right.x, tip_y + i as f32 * 16.0 * u, 15.0 * u, MUTED);
    }
}

fn found_form(ui: &mut Ui, game: &mut Game, st: &mut State, r: Rect) {
    let u = ui.u;
    let left_w = 300.0 * u;
    text("Nombre del clan", r.x, r.y + 14.0 * u, 16.0 * u, TEXT);
    let name_r = Rect::new(r.x, r.y + 20.0 * u, left_w, 28.0 * u);
    let _ = text_field(ui, "guild.found_name", &mut st.found_name, name_r, guild::MAX_NAME_LEN, false, |c| c.is_ascii_alphabetic() || c == ' ' || c == 'ÿ');
    text("Alineación", r.x, r.y + 70.0 * u, 16.0 * u, TEXT);
    let aligns = [0u8, 3, 4, 1, 2];
    let bw = (left_w - 2.0 * 4.0 * u) / 3.0;
    for (i, a) in aligns.iter().enumerate() {
        let b = Rect::new(r.x + (i % 3) as f32 * (bw + 4.0 * u), r.y + 78.0 * u + (i / 3) as f32 * 30.0 * u, bw, 26.0 * u);
        if button(b, align_name(*a), st.found_align == *a, u) {
            st.found_align = *a;
        }
    }
    text("Descripción", r.x, r.y + 152.0 * u, 16.0 * u, TEXT);
    let desc_r = Rect::new(r.x, r.y + 158.0 * u, left_w, 96.0 * u);
    wrapped_field(ui, "guild.found_desc", &mut st.found_desc, desc_r, guild::MAX_DESC_LEN, "Qué clase de clan es, qué buscáis...");

    // Requirements, ticked against the character.
    let rx = r.x + left_w + 18.0 * u;
    let rw = r.w - left_w - 18.0 * u;
    draw_line(rx - 9.0 * u, r.y, rx - 9.0 * u, r.bottom(), 1.0, GOLD);
    text_with(FontKind::Title, "Requisitos", rx, r.y + 18.0 * u, 19.0 * u, GOLD_TEXT);
    let mut y = r.y + 30.0 * u;
    for (label, ok) in game.found_requirements() {
        let b = Rect::new(rx, y + 3.0 * u, 14.0 * u, 14.0 * u);
        draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.0, 0.0, 0.0, 0.6));
        draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, if ok { GOOD } else { BAD });
        if ok {
            draw_line(b.x + 3.0 * u, b.y + 7.0 * u, b.x + 6.0 * u, b.y + 11.0 * u, 2.0, GOOD);
            draw_line(b.x + 6.0 * u, b.y + 11.0 * u, b.x + 12.0 * u, b.y + 3.0 * u, 2.0, GOOD);
        } else {
            draw_line(b.x + 3.0 * u, b.y + 3.0 * u, b.x + 11.0 * u, b.y + 11.0 * u, 2.0, BAD);
            draw_line(b.x + 11.0 * u, b.y + 3.0 * u, b.x + 3.0 * u, b.y + 11.0 * u, 2.0, BAD);
        }
        let rows = wrap(&label, 15.0 * u, rw - 24.0 * u);
        for (i, row) in rows.iter().enumerate() {
            text(row, rx + 22.0 * u, y + 15.0 * u + i as f32 * 15.0 * u, 15.0 * u, if ok { TEXT } else { Color::new(1.0, 0.7, 0.65, 1.0) });
        }
        y += 8.0 * u + rows.len() as f32 * 15.0 * u;
    }
    let refusal = game.found_refusal(st.found_align);
    y += 6.0 * u;
    if let Some(why) = refusal {
        for (i, row) in wrap(why, 15.0 * u, rw).iter().enumerate().take(5) {
            text(row, rx, y + 14.0 * u + i as f32 * 15.0 * u, 15.0 * u, BAD);
        }
    } else {
        text("Cumples todos los requisitos.", rx, y + 14.0 * u, 16.0 * u, GOOD);
    }
    let found = Rect::new(r.x, r.bottom() - 30.0 * u, left_w, 30.0 * u);
    if button(found, "Fundar clan", refusal.is_none(), u) {
        let (n, d, a) = (st.found_name.clone(), st.found_desc.clone(), st.found_align);
        if game.guild_found(&n, &d, a) {
            ui.focus = None;
        }
    }
    if found.contains(mouse_position().into()) {
        tooltip(&["Fundar el clan".into(), "Se consumen las cuatro gemas de fundación.".into()], u);
    }
}

// ------------------------------------------------------------------ with a guild

fn with_guild(ui: &mut Ui, game: &mut Game, st: &mut State, info: &GuildInfo, r: Rect) {
    let u = ui.u;
    let leader = game.is_guild_leader();
    // Header.
    text_with(FontKind::Title, &info.name, r.x, r.y + 20.0 * u, 23.0 * u, GOLD_TEXT);
    let sub = format!("{}   -   Nivel {}", align_name(info.alignment), info.level);
    text(&sub, r.x, r.y + 40.0 * u, 16.0 * u, align_color(info.alignment));
    let tag = if leader { "Eres el líder" } else { "Miembro" };
    text(tag, r.right() - 110.0 * u, r.y + 18.0 * u, 16.0 * u, if leader { GOLD_TEXT } else { MUTED });

    // Tabs.
    let mut tabs = vec![(Tab::Info, "Información".to_string()), (Tab::Members, format!("Miembros ({})", info.members.len()))];
    if leader {
        tabs.push((Tab::Applicants, format!("Aspirantes ({})", info.applicants.len())));
    }
    tabs.push((Tab::Relations, "Relaciones".to_string()));
    if st.tab == Tab::Applicants && !leader {
        st.tab = Tab::Info;
    }
    let ty = r.y + 50.0 * u;
    let tw = 128.0 * u;
    for (i, (t, label)) in tabs.iter().enumerate() {
        if button(Rect::new(r.x + i as f32 * (tw + 4.0 * u), ty, tw, 26.0 * u), label, st.tab == *t, u) {
            st.tab = *t;
            st.edit = None;
        }
    }
    let body = Rect::new(r.x, ty + 36.0 * u, r.w, r.bottom() - ty - 36.0 * u - 34.0 * u);
    match st.tab {
        Tab::Info => info_tab(ui, game, st, info, body, leader),
        Tab::Members => members_tab(game, st, info, body, leader, u),
        Tab::Applicants => applicants_tab(ui, game, st, info, body),
        Tab::Relations => relations_tab(ui, game, st, info, body, leader),
    }

    // Footer: leave.
    let b = Rect::new(r.x, r.bottom() - 28.0 * u, 190.0 * u, 28.0 * u);
    let label = if st.confirm_leave { "¿Seguro? Pulsa otra vez" } else if leader { "Abandonar (disolver)" } else { "Abandonar el clan" };
    if button(b, label, st.confirm_leave, u) {
        if st.confirm_leave {
            game.guild_leave();
            st.confirm_leave = false;
        } else {
            st.confirm_leave = true;
        }
    }
    if b.contains(mouse_position().into()) {
        tooltip(&["Abandonar el clan".into(), if leader { "Eres el líder: el clan se queda sin él.".into() } else { "Dejarás de ser miembro.".into() }], u);
    }
    text("Aceptar un aspirante cuesta oro del líder.", r.right() - 262.0 * u, r.bottom() - 8.0 * u, 14.0 * u, MUTED);
}

fn label_value(x: f32, y: f32, u: f32, label: &str, value: &str) {
    text(label, x, y, 16.0 * u, MUTED);
    text(value, x + 78.0 * u, y, 16.0 * u, TEXT);
}

fn info_tab(ui: &mut Ui, game: &mut Game, st: &mut State, info: &GuildInfo, r: Rect, leader: bool) {
    let u = ui.u;
    let exp_label = if info.exp_next > 0 { format!("{} / {} exp", info.exp, info.exp_next) } else { format!("{} exp (nivel máximo)", info.exp) };
    bar(Rect::new(r.x, r.y + 2.0 * u, 250.0 * u, 18.0 * u), info.exp as f32, if info.exp_next > 0 { info.exp_next as f32 } else { info.exp.max(1) as f32 }, Color::new(0.75, 0.6, 0.15, 1.0), &exp_label, u);
    label_value(r.x + 270.0 * u, r.y + 16.0 * u, u, "Miembros", &format!("{}/{}", info.members.len(), info.max_members));
    label_value(r.x, r.y + 42.0 * u, u, "Líder", &info.leader);
    label_value(r.x + 270.0 * u, r.y + 42.0 * u, u, "Fundador", &info.founder);

    let mut y = r.y + 62.0 * u;
    let block_h = (r.bottom() - y - 6.0 * u) / 2.0;
    for (which, title, body) in [(Edit::News, "Noticias", &info.news), (Edit::Description, "Descripción", &info.description)] {
        text_with(FontKind::Title, title, r.x, y + 14.0 * u, 17.0 * u, GOLD_TEXT);
        let editing = st.edit.as_ref().is_some_and(|(e, _)| *e == which);
        let box_r = Rect::new(r.x, y + 22.0 * u, r.w, block_h - 30.0 * u);
        if leader && !editing && button(Rect::new(r.right() - 90.0 * u, y, 90.0 * u, 22.0 * u), "Editar", false, u) {
            st.edit = Some((which, body.clone()));
            ui.focus = Some("guild.edit".into());
        }
        if editing {
            let max = if which == Edit::News { guild::MAX_NEWS_LEN } else { guild::MAX_DESC_LEN };
            let mut buf = st.edit.as_ref().map(|(_, b)| b.clone()).unwrap_or_default();
            let field = Rect::new(box_r.x, box_r.y, box_r.w - 190.0 * u, box_r.h);
            let enter = wrapped_field(ui, "guild.edit", &mut buf, field, max, "");
            st.edit = Some((which, buf.clone()));
            let ok = Rect::new(field.right() + 8.0 * u, box_r.y, 86.0 * u, 26.0 * u);
            let cancel = Rect::new(ok.right() + 6.0 * u, box_r.y, 86.0 * u, 26.0 * u);
            if button(ok, "Guardar", true, u) || enter {
                if which == Edit::News {
                    game.guild_update_news(&buf);
                    st.edit = None;
                } else if game.guild_update_description(&buf) {
                    st.edit = None;
                }
                ui.focus = None;
            }
            if button(cancel, "Cancelar", false, u) {
                st.edit = None;
                ui.focus = None;
            }
        } else {
            draw_rectangle(box_r.x, box_r.y, box_r.w, box_r.h, Color::new(0.0, 0.0, 0.0, 0.3));
            let max_rows = ((box_r.h - 8.0 * u) / (16.5 * u)).floor() as usize;
            let rows = wrap(if body.is_empty() { "(sin texto)" } else { body }, 16.0 * u, box_r.w - 16.0 * u);
            for (i, row) in rows.iter().take(max_rows.max(1)).enumerate() {
                let shown = if i + 1 == max_rows.max(1) && rows.len() > max_rows { format!("{row}...") } else { row.clone() };
                text(&shown, box_r.x + 8.0 * u, box_r.y + 18.0 * u + i as f32 * 16.5 * u, 16.0 * u, if body.is_empty() { MUTED } else { TEXT });
            }
        }
        y += block_h;
    }
}

fn members_tab(game: &mut Game, st: &mut State, info: &GuildInfo, r: Rect, leader: bool, u: f32) {
    let me = game.me().map(|e| e.name.clone()).unwrap_or_default();
    let mut members = info.members.clone();
    members.sort_by(|a, b| b.online.cmp(&a.online).then(b.leader.cmp(&a.leader)).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    let online = members.iter().filter(|m| m.online).count();
    text(&format!("{online} conectados de {}", members.len()), r.x, r.y + 12.0 * u, 15.0 * u, MUTED);
    let area = Rect::new(r.x, r.y + 20.0 * u, r.w, r.h - 20.0 * u);
    let (off, vis) = scroll_rows(st, "members", area, members.len(), 30.0 * u, u);
    for (i, m) in members.iter().enumerate().skip(off).take(vis) {
        let row = Rect::new(area.x, area.y + (i - off) as f32 * 30.0 * u, area.w - 10.0 * u, 28.0 * u);
        row_bg(row, false);
        draw_circle(row.x + 14.0 * u, row.center().y, 5.0 * u, if m.online { GOOD } else { Color::new(0.3, 0.3, 0.3, 1.0) });
        let label = if m.name == me { format!("{} (tú)", m.name) } else { m.name.clone() };
        text_fit(&label, row.x + 28.0 * u, row.y + 19.0 * u, 18.0 * u, 230.0 * u, if m.online { Color::new(1.0, 0.97, 0.85, 1.0) } else { MUTED });
        let mut tag_x = row.x + 270.0 * u;
        for (flag, name) in [(m.leader, "Líder"), (m.founder, "Fundador")] {
            if flag {
                text(name, tag_x, row.y + 19.0 * u, 15.0 * u, GOLD_TEXT);
                tag_x += 66.0 * u;
            }
        }
        if !m.online {
            text("desconectado", row.right() - 190.0 * u, row.y + 19.0 * u, 13.0 * u, MUTED);
        }
        if leader && !m.leader && m.name != me && button(Rect::new(row.right() - 86.0 * u, row.y + 3.0 * u, 82.0 * u, 22.0 * u), "Expulsar", false, u) {
            game.guild_kick(&m.name);
        }
    }
}

fn applicants_tab(ui: &mut Ui, game: &mut Game, st: &mut State, info: &GuildInfo, r: Rect) {
    let u = ui.u;
    if info.applicants.is_empty() {
        text("Nadie ha pedido entrar en el clan.", r.x, r.y + 16.0 * u, 17.0 * u, MUTED);
        return;
    }
    let reject_h = if st.reject.is_some() { 74.0 * u } else { 0.0 };
    let area = Rect::new(r.x, r.y, r.w, r.h - reject_h);
    let row_h = 58.0 * u;
    let (off, vis) = scroll_rows(st, "applicants", area, info.applicants.len(), row_h, u);
    for (i, a) in info.applicants.iter().enumerate().skip(off).take(vis) {
        let row = Rect::new(area.x, area.y + (i - off) as f32 * row_h, area.w - 10.0 * u, row_h - 4.0 * u);
        row_bg(row, st.reject.as_deref() == Some(&a.name));
        text(&a.name, row.x + 8.0 * u, row.y + 18.0 * u, 18.0 * u, GOLD_TEXT);
        for (k, line) in wrap(&a.application, 14.0 * u, row.w - 200.0 * u).iter().take(2).enumerate() {
            text(line, row.x + 8.0 * u, row.y + 34.0 * u + k as f32 * 14.0 * u, 14.0 * u, TEXT);
        }
        if button(Rect::new(row.right() - 180.0 * u, row.y + 14.0 * u, 84.0 * u, 26.0 * u), "Aceptar", false, u) {
            game.guild_accept(&a.name);
        }
        if button(Rect::new(row.right() - 92.0 * u, row.y + 14.0 * u, 84.0 * u, 26.0 * u), "Rechazar", st.reject.as_deref() == Some(&a.name), u) {
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
        let b = Rect::new(r.x, r.bottom() - reject_h + 6.0 * u, r.w, reject_h - 6.0 * u);
        text(&format!("Motivo del rechazo a {name}:"), b.x, b.y + 12.0 * u, 15.0 * u, TEXT);
        let field = Rect::new(b.x, b.y + 18.0 * u, b.w - 190.0 * u, 44.0 * u);
        let enter = wrapped_field(ui, "guild.reject", &mut st.reject_reason, field, guild::MAX_APPLICATION_LEN, "Por qué no entra...");
        if (button(Rect::new(field.right() + 8.0 * u, b.y + 18.0 * u, 90.0 * u, 26.0 * u), "Rechazar", true, u) || enter) && game.guild_reject(&name, &st.reject_reason) {
            st.reject = None;
            st.reject_reason.clear();
            ui.focus = None;
        }
        if button(Rect::new(field.right() + 104.0 * u, b.y + 18.0 * u, 78.0 * u, 26.0 * u), "Cancelar", false, u) {
            st.reject = None;
            ui.focus = None;
        }
    }
}

fn relations_tab(ui: &mut Ui, game: &mut Game, st: &mut State, info: &GuildInfo, r: Rect, leader: bool) {
    let u = ui.u;
    let left = Rect::new(r.x, r.y, 266.0 * u, r.h);
    text_with(FontKind::Title, "Relaciones", left.x, left.y + 14.0 * u, 17.0 * u, GOLD_TEXT);
    let mut y = left.y + 22.0 * u;
    if info.relations.is_empty() {
        text("En paz con todos los clanes.", left.x, y + 16.0 * u, 15.0 * u, MUTED);
        y += 22.0 * u;
    }
    for (name, rel) in info.relations.iter().take(5) {
        let (label, c) = relation_label(*rel);
        let row = Rect::new(left.x, y, left.w, 24.0 * u);
        row_bg(row, false);
        text_fit(name, row.x + 8.0 * u, row.y + 17.0 * u, 16.0 * u, row.w - 150.0 * u, TEXT);
        text(label, row.x + row.w - 76.0 * u, row.y + 17.0 * u, 15.0 * u, c);
        if leader && *rel == GuildRelation::War && button(Rect::new(row.right() - 148.0 * u, row.y + 1.0 * u, 64.0 * u, 22.0 * u), "Paz", false, u) {
            game.guild_propose(name, GuildRelation::Peace, &st.rel_text.clone());
        }
        y += 27.0 * u;
    }
    y += 8.0 * u;
    text_with(FontKind::Title, "Propuestas recibidas", left.x, y + 14.0 * u, 17.0 * u, GOLD_TEXT);
    y += 22.0 * u;
    if info.proposals.is_empty() {
        text("Ninguna.", left.x, y + 16.0 * u, 15.0 * u, MUTED);
    }
    for (name, rel, msg) in info.proposals.iter().take(3) {
        let (label, c) = relation_label(*rel);
        let row = Rect::new(left.x, y, left.w, 46.0 * u);
        row_bg(row, false);
        text_fit(&format!("{name}: {label}"), row.x + 8.0 * u, row.y + 16.0 * u, 16.0 * u, row.w - 150.0 * u, c);
        text_fit(msg, row.x + 8.0 * u, row.y + 34.0 * u, 14.0 * u, row.w - 150.0 * u, MUTED);
        if leader {
            if button(Rect::new(row.right() - 140.0 * u, row.y + 10.0 * u, 66.0 * u, 26.0 * u), "Sí", false, u) {
                game.guild_answer_proposal(name, true);
            }
            if button(Rect::new(row.right() - 70.0 * u, row.y + 10.0 * u, 66.0 * u, 26.0 * u), "No", false, u) {
                game.guild_answer_proposal(name, false);
            }
        }
        y += 50.0 * u;
    }

    // Right: new proposal (leader).
    let right = Rect::new(left.right() + 16.0 * u, r.y, r.w - left.w - 16.0 * u, r.h);
    draw_line(left.right() + 8.0 * u, r.y, left.right() + 8.0 * u, r.bottom(), 1.0, GOLD);
    text_with(FontKind::Title, "Nueva propuesta", right.x, right.y + 14.0 * u, 17.0 * u, GOLD_TEXT);
    if !leader {
        text("Solo el líder trata con otros clanes.", right.x, right.y + 38.0 * u, 15.0 * u, MUTED);
        return;
    }
    let others: Vec<String> = game.local.social.guild.list.iter().map(|(n, _)| n.clone()).filter(|n| *n != info.name).collect();
    let area = Rect::new(right.x, right.y + 22.0 * u, right.w, 4.0 * 26.0 * u);
    if others.is_empty() {
        text("No hay otros clanes.", area.x, area.y + 16.0 * u, 15.0 * u, MUTED);
    }
    let (off, vis) = scroll_rows(st, "relguilds", area, others.len(), 26.0 * u, u);
    for (i, name) in others.iter().enumerate().skip(off).take(vis) {
        let row = Rect::new(area.x, area.y + (i - off) as f32 * 26.0 * u, area.w - 8.0 * u, 24.0 * u);
        if row_bg(row, st.rel_target.as_deref() == Some(name)) {
            st.rel_target = Some(name.clone());
        }
        text_fit(name, row.x + 8.0 * u, row.y + 17.0 * u, 16.0 * u, row.w - 12.0 * u, TEXT);
    }
    let fy = area.bottom() + 8.0 * u;
    let field = Rect::new(right.x, fy, right.w, 56.0 * u);
    wrapped_field(ui, "guild.rel", &mut st.rel_text, field, 200, "Mensaje para el otro clan (opcional)");
    let target = st.rel_target.clone().filter(|t| others.contains(t));
    let bw = (right.w - 8.0 * u) / 3.0;
    let by = field.bottom() + 8.0 * u;
    let msg = st.rel_text.clone();
    for (i, (label, rel)) in [("Paz", GuildRelation::Peace), ("Alianza", GuildRelation::Alliance)].iter().enumerate() {
        if button(Rect::new(right.x + i as f32 * (bw + 4.0 * u), by, bw, 26.0 * u), label, target.is_some(), u) {
            if let Some(t) = &target {
                game.guild_propose(t, *rel, &msg);
            } else {
                game.say("Elige un clan de la lista.", ao_protocol::Tone::Warn);
            }
        }
    }
    if button(Rect::new(right.x + 2.0 * (bw + 4.0 * u), by, bw, 26.0 * u), "Guerra", false, u) {
        match &target {
            Some(t) => game.guild_declare_war(t),
            None => game.say("Elige un clan de la lista.", ao_protocol::Tone::Warn),
        }
    }
    text(target.as_deref().map(|t| format!("Elegido: {t}")).as_deref().unwrap_or("Elige un clan."), right.x, by + 44.0 * u, 15.0 * u, MUTED);
}

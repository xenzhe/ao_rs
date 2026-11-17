//! Window: game master panel. Sober and quick: who is online with their state, the actions
//! for the chosen player, a free command line and the recent GM log.
//!
//! It only draws for GMs: for anyone else it closes itself (the server answers nothing to
//! them anyway). Every button just sends the typed command, so the server checks the rank
//! exactly as for the chat.
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::character::kit::{self, Icon};
use crate::ui::theme::{FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::ui::widgets::{button_kind, text_field, text_with, tooltip, Kind};
use crate::ui::{Ui, WindowId};
use client_core::p4::gm::action_line;
use macroquad::prelude::*;
use std::cell::RefCell;

/// Seconds between automatic refreshes while the panel is open.
const REFRESH_SECS: f64 = 2.0;

#[derive(Default)]
struct PanelState {
    search: String,
    selected: Option<String>,
    minutes: String,
    reason: String,
    line: String,
    last_refresh: f64,
    scroll: usize,
}

thread_local! {
    // `AO_GM_SEARCH` / `AO_GM_SELECT` pre-fill the search and the selection (screenshots).
    static STATE: RefCell<PanelState> = RefCell::new(PanelState {
        minutes: "10".into(),
        last_refresh: -100.0,
        search: std::env::var("AO_GM_SEARCH").unwrap_or_default(),
        selected: std::env::var("AO_GM_SELECT").ok(),
        ..Default::default()
    });
}

fn hhmm(unix: i64) -> String {
    let s = unix.rem_euclid(86_400);
    format!("{:02}:{:02}", s / 3600, (s / 60) % 60)
}

fn mins(secs: u32) -> u32 {
    secs.div_ceil(60)
}

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let _ = assets;
    if !game.local.p4.gm.is_gm() {
        ui.close(WindowId::Gm);
        return;
    }
    kit::begin(ui, WindowId::Gm);
    STATE.with_borrow_mut(|st| panel(ui, game, r, st));
}

fn panel(ui: &mut Ui, game: &mut Game, r: Rect, st: &mut PanelState) {
    let u = ui.u;
    let now = get_time();
    if now - st.last_refresh > REFRESH_SECS {
        st.last_refresh = now;
        game.gm_refresh();
    }
    let gm = &game.local.p4.gm;
    let rank = gm.rank.label();
    let mut tips: Vec<String> = Vec::new();
    let r = Rect::new(r.x + 6.0 * u, r.y + 2.0 * u, r.w - 12.0 * u, r.h - 4.0 * u);

    // ---- header: rank, online count, search
    let y0 = r.y;
    kit::icon(Icon::Crown, r.x + 12.0 * u, y0 + 16.0 * u, 18.0 * u, GOLD_BRIGHT);
    text_with(FontKind::Title, &rank.to_uppercase(), r.x + 30.0 * u, y0 + 22.0 * u, 18.0 * u, GOLD_BRIGHT);
    text_with(FontKind::Body, &format!("{} conectados", gm.online.len()), r.x + 150.0 * u, y0 + 22.0 * u, 16.0 * u, TEXT_DIM);
    let search = Rect::new(r.right() - 230.0 * u, y0 + 2.0 * u, 230.0 * u, 28.0 * u);
    text_field(ui, "gm_search", &mut st.search, search, 16, false, |c| c.is_alphanumeric());
    if st.search.is_empty() && ui.focus.as_deref() != Some("gm_search") {
        super::widgets::placeholder(search, "Buscar jugador", u);
    }
    kit::rule(r.x, y0 + 40.0 * u, r.w, GOLD);

    // ---- player list (left)
    let list_w = (r.w * 0.55).floor();
    let list = Rect::new(r.x, y0 + 52.0 * u, list_w, 262.0 * u);
    let head_y = list.y;
    text_with(FontKind::Title, "JUGADOR", list.x + 10.0 * u, head_y + 12.0 * u, 12.0 * u, GOLD_DIM);
    text_with(FontKind::Title, "NV", list.x + list.w * 0.43, head_y + 12.0 * u, 12.0 * u, GOLD_DIM);
    text_with(FontKind::Title, "UBICACIÓN", list.x + list.w * 0.55, head_y + 12.0 * u, 12.0 * u, GOLD_DIM);
    let body = Rect::new(list.x, list.y + 20.0 * u, list.w, list.h - 20.0 * u);
    kit::well(body, u);
    let row_h = 28.0 * u;
    let rows: Vec<_> = game.local.p4.gm.filtered(&st.search).into_iter().cloned().collect();
    let fits = ((body.h - 4.0 * u) / row_h).floor().max(1.0) as usize;
    st.scroll = kit::wheel(body, st.scroll, rows.len(), fits, 1);
    if rows.is_empty() {
        text_with(FontKind::NarrativeItalic, "Nadie coincide con la búsqueda.", body.x + 12.0 * u, body.y + 26.0 * u, 16.0 * u, TEXT_FAINT);
    }
    for (i, p) in rows.iter().skip(st.scroll).take(fits).enumerate() {
        let row = Rect::new(body.x + 2.0 * u, body.y + 2.0 * u + i as f32 * row_h, body.w - 8.0 * u, row_h - 1.0 * u);
        let sel = st.selected.as_deref() == Some(p.name.as_str());
        let (_, click) = kit::row(row, sel, u);
        if click {
            st.selected = Some(p.name.clone());
        }
        let color = if p.rank.is_gm() { GOLD_TEXT } else if p.dead { TEXT_FAINT } else { TEXT };
        kit::fit(FontKind::Body, &p.name, row.x + 10.0 * u, row.y + 19.0 * u, 16.0 * u, row.w * 0.38, color);
        text_with(FontKind::Body, &p.level.to_string(), row.x + list.w * 0.43 - 2.0 * u, row.y + 19.0 * u, 15.0 * u, TEXT_DIM);
        kit::fit(FontKind::Body, &format!("{} · {},{}", p.map, p.x, p.y), row.x + list.w * 0.55 - 2.0 * u, row.y + 19.0 * u, 14.0 * u, row.w * 0.2, TEXT_DIM);
        // Badges: jail, silence, dead, rank.
        let mut bx = row.right() - 6.0 * u;
        let mut badge = |label: &str, c: Color, tip: String| {
            let w = 20.0 * u;
            bx -= w + 3.0 * u;
            let b = Rect::new(bx, row.y + 4.0 * u, w, row.h - 8.0 * u);
            draw_rectangle(b.x, b.y, b.w, b.h, Color::new(c.r, c.g, c.b, 0.22));
            draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, c);
            kit::text_mid(FontKind::Body, label, b.center().x, b.bottom() - 4.0 * u, 13.0 * u, c);
            if kit::hover(b) {
                tips = vec![tip];
            }
        };
        if p.jail_secs > 0 {
            badge("C", BAD, format!("En la cárcel: {} min", mins(p.jail_secs)));
        }
        if p.silence_secs > 0 {
            badge("S", BAD, format!("Silenciado: {} min", mins(p.silence_secs)));
        }
        if p.dead {
            badge("M", TEXT_DIM, "Muerto".into());
        }
        if p.rank.is_gm() {
            badge("G", GOLD_TEXT, p.rank.label().into());
        }
    }
    kit::scrollbar(body, rows.len(), fits, st.scroll, u);

    // ---- actions (right)
    let ax = list.right() + 20.0 * u;
    let aw = r.right() - ax;
    let mut y = list.y;
    let selected = st.selected.clone().filter(|n| rows.iter().any(|p| &p.name == n));
    match &selected {
        Some(n) => {
            text_with(FontKind::Title, "JUGADOR", ax, y + 12.0 * u, 12.0 * u, GOLD_DIM);
            kit::fit(FontKind::Title, n, ax, y + 40.0 * u, 24.0 * u, aw, GOLD_BRIGHT);
            if let Some(p) = rows.iter().find(|p| &p.name == n) {
                text_with(FontKind::Body, &format!("Nivel {} · mapa {} ({}, {})", p.level, p.map, p.x, p.y), ax, y + 60.0 * u, 14.0 * u, TEXT_DIM);
            }
        }
        None => {
            text_with(FontKind::Title, "JUGADOR", ax, y + 12.0 * u, 12.0 * u, GOLD_DIM);
            text_with(FontKind::NarrativeItalic, "Elige un jugador de la lista", ax, y + 40.0 * u, 17.0 * u, TEXT_FAINT);
        }
    }
    y += 70.0 * u;
    let bw = (aw - 16.0 * u) / 3.0;
    let bh = 28.0 * u;
    let mut send: Option<&str> = None;
    let grid = [("Ir", "ir", "Ir hasta él (/IRA)"), ("Traer", "traer", "Traerlo a tu lado (/SUM)"), ("Info", "info", "Datos del personaje (/INFO)"), ("Penas", "penas", "Historial de penas (/PENAS)"), ("Revivir", "revivir", "Resucitar o curar (/REVIVIR)"), ("Echar", "echar", "Desconectarlo (/ECHAR)")];
    for (i, (label, action, tip)) in grid.iter().enumerate() {
        let b = Rect::new(ax + (i % 3) as f32 * (bw + 8.0 * u), y + (i / 3) as f32 * (bh + 6.0 * u), bw, bh);
        if button_kind(b, label, Kind::Secondary, selected.is_some(), u) && selected.is_some() {
            send = Some(action);
        }
        if kit::hover(b) {
            tips = vec![tip.to_string()];
        }
    }
    y += 2.0 * (bh + 6.0 * u) + 2.0 * u;
    let mut sy = kit::section("Sanción", ax, y, aw, u);
    let min_r = Rect::new(ax, sy, 64.0 * u, 26.0 * u);
    text_field(ui, "gm_minutes", &mut st.minutes, min_r, 6, false, |c| c.is_ascii_digit());
    if st.minutes.is_empty() && ui.focus.as_deref() != Some("gm_minutes") {
        super::widgets::placeholder(min_r, "min", u);
    }
    let why_r = Rect::new(min_r.right() + 8.0 * u, sy, aw - 72.0 * u, 26.0 * u);
    text_field(ui, "gm_reason", &mut st.reason, why_r, 60, false, |c| c != '@');
    if st.reason.is_empty() && ui.focus.as_deref() != Some("gm_reason") {
        super::widgets::placeholder(why_r, "Motivo (opcional)", u);
    }
    sy += 34.0 * u;
    let hw = (aw - 8.0 * u) / 2.0;
    let b1 = Rect::new(ax, sy, hw, bh);
    let b2 = Rect::new(ax + hw + 8.0 * u, sy, hw, bh);
    if button_kind(b1, "Silenciar", Kind::Secondary, selected.is_some(), u) && selected.is_some() {
        send = Some("silenciar");
    }
    if button_kind(b2, "Encarcelar", Kind::Danger, selected.is_some(), u) && selected.is_some() {
        send = Some("carcel");
    }
    if kit::hover(b1) {
        tips = vec!["Silenciar los minutos indicados (0 = quitar)".into()];
    }
    if kit::hover(b2) {
        tips = vec!["Encarcelar los minutos indicados (0 = liberar)".into()];
    }
    if let (Some(action), Some(name)) = (send, &selected) {
        let minutes = st.minutes.parse().unwrap_or(0);
        game.gm_command(action_line(action, name, minutes, &st.reason));
        st.last_refresh = -100.0;
    }

    // ---- command line
    let cy = list.bottom() + 12.0 * u;
    text_with(FontKind::Title, "COMANDO", r.x, cy + 19.0 * u, 13.0 * u, GOLD_TEXT);
    let field = Rect::new(r.x + 84.0 * u, cy, r.w - 84.0 * u - 100.0 * u, 28.0 * u);
    let enter = text_field(ui, "gm_line", &mut st.line, field, 200, false, |_| true);
    if st.line.is_empty() && ui.focus.as_deref() != Some("gm_line") {
        super::widgets::placeholder(field, "/comando argumentos", u);
    }
    if (button_kind(Rect::new(field.right() + 10.0 * u, cy, 90.0 * u, 28.0 * u), "Enviar", Kind::Primary, true, u) || enter) && !st.line.trim().is_empty() {
        let line = if st.line.starts_with('/') { st.line.clone() } else { format!("/{}", st.line) };
        game.gm_command(line);
        st.line.clear();
        st.last_refresh = -100.0;
    }

    // ---- recent log
    let ly = cy + 40.0 * u;
    let hy = kit::section("Registro reciente", r.x, ly, r.w, u);
    let log_r = Rect::new(r.x, hy - 4.0 * u, r.w, r.bottom() - hy + 4.0 * u);
    kit::well(log_r, u);
    let lines = ((log_r.h - 8.0 * u) / (20.0 * u)).floor().max(0.0) as usize;
    let log = &game.local.p4.gm.log;
    if log.is_empty() {
        text_with(FontKind::NarrativeItalic, "Sin acciones registradas.", log_r.x + 12.0 * u, log_r.y + 24.0 * u, 15.0 * u, TEXT_FAINT);
    }
    for (i, e) in log.iter().take(lines).enumerate() {
        let yy = log_r.y + 22.0 * u + i as f32 * 20.0 * u;
        text_with(FontKind::Body, &hhmm(e.at), log_r.x + 12.0 * u, yy, 14.0 * u, TEXT_FAINT);
        kit::fit(FontKind::Body, &e.gm, log_r.x + 62.0 * u, yy, 14.0 * u, 90.0 * u, GOLD_TEXT);
        kit::fit(FontKind::Body, &e.action, log_r.x + 160.0 * u, yy, 14.0 * u, 90.0 * u, GOOD);
        kit::fit(FontKind::Body, &format!("{} {}", e.target, e.detail), log_r.x + 256.0 * u, yy, 14.0 * u, log_r.w - 270.0 * u, TEXT);
    }
    if !tips.is_empty() {
        tooltip(&tips, u);
    }
}

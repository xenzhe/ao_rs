//! Window: game master panel. OWNER: Opus C (docs/P4_CONTRATO.md).
//!
//! Online players (search, select) with the usual actions, a free command line and the
//! recent GM log. It only draws for GMs: for anyone else it closes itself (the server
//! answers nothing to them anyway). Every button just sends the typed command, so the
//! server checks the rank exactly as for the chat.
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::theme::{BAD, GOLD, GOLD_TEXT, GOOD, MUTED, TEXT};
use crate::ui::widgets::{button, text, text_field, text_fit, tooltip};
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

    // ---- header: rank and search
    let y0 = r.y + 6.0 * u;
    text(&format!("Rango: {rank}"), r.x, y0 + 17.0 * u, 18.0 * u, GOLD_TEXT);
    text(&format!("{} conectados", gm.online.len()), r.x + 150.0 * u, y0 + 17.0 * u, 16.0 * u, MUTED);
    let search = Rect::new(r.right() - 220.0 * u, y0, 220.0 * u, 26.0 * u);
    text("Buscar", search.x - 54.0 * u, y0 + 18.0 * u, 16.0 * u, MUTED);
    text_field(ui, "gm_search", &mut st.search, search, 16, false, |c| c.is_alphanumeric());

    // ---- player list (left)
    let list_w = r.w * 0.52;
    let list = Rect::new(r.x, y0 + 34.0 * u, list_w, r.h * 0.56);
    draw_rectangle(list.x, list.y, list.w, list.h, Color::new(0.0, 0.0, 0.0, 0.3));
    draw_rectangle_lines(list.x, list.y, list.w, list.h, 1.0, GOLD);
    let row_h = 26.0 * u;
    let rows: Vec<_> = game.local.p4.gm.filtered(&st.search).into_iter().cloned().collect();
    let fits = ((list.h - 4.0 * u) / row_h).floor().max(1.0) as usize;
    if list.contains(mouse_position().into()) {
        let wheel = mouse_wheel().1;
        if wheel < 0.0 {
            st.scroll += 1;
        } else if wheel > 0.0 {
            st.scroll = st.scroll.saturating_sub(1);
        }
    }
    st.scroll = st.scroll.min(rows.len().saturating_sub(fits));
    if rows.is_empty() {
        text("Nadie coincide con la búsqueda.", list.x + 10.0 * u, list.y + 22.0 * u, 16.0 * u, MUTED);
    }
    for (i, p) in rows.iter().skip(st.scroll).take(fits).enumerate() {
        let row = Rect::new(list.x + 2.0 * u, list.y + 2.0 * u + i as f32 * row_h, list.w - 4.0 * u, row_h - 2.0 * u);
        let sel = st.selected.as_deref() == Some(p.name.as_str());
        let hover = row.contains(mouse_position().into());
        draw_rectangle(row.x, row.y, row.w, row.h, if sel { Color::new(0.45, 0.30, 0.10, 0.55) } else if hover { Color::new(1.0, 1.0, 1.0, 0.06) } else { Color::new(0.0, 0.0, 0.0, 0.0) });
        if hover && is_mouse_button_pressed(MouseButton::Left) {
            st.selected = Some(p.name.clone());
        }
        let color = if p.rank.is_gm() { GOLD_TEXT } else if p.dead { MUTED } else { TEXT };
        text_fit(&p.name, row.x + 8.0 * u, row.y + 18.0 * u, 17.0 * u, row.w * 0.42, color);
        text(&format!("Nv {}", p.level), row.x + row.w * 0.45, row.y + 18.0 * u, 15.0 * u, MUTED);
        text(&format!("{} · {},{}", p.map, p.x, p.y), row.x + row.w * 0.6, row.y + 18.0 * u, 15.0 * u, MUTED);
        // Badges: jail, silence, dead, rank.
        let mut bx = row.right() - 8.0 * u;
        let mut badge = |label: &str, c: Color, tip: String| {
            let w = 20.0 * u;
            bx -= w + 3.0 * u;
            let b = Rect::new(bx, row.y + 4.0 * u, w, row.h - 8.0 * u);
            draw_rectangle(b.x, b.y, b.w, b.h, Color::new(c.r, c.g, c.b, 0.25));
            draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, c);
            text(label, b.x + 5.0 * u, b.y + b.h - 4.0 * u, 14.0 * u, c);
            if b.contains(mouse_position().into()) {
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
            badge("M", MUTED, "Muerto".into());
        }
        if p.rank.is_gm() {
            badge("G", GOLD_TEXT, p.rank.label().into());
        }
    }

    // ---- actions (right)
    let ax = list.right() + 12.0 * u;
    let aw = r.right() - ax;
    let mut y = list.y;
    let selected = st.selected.clone().filter(|n| rows.iter().any(|p| &p.name == n));
    match &selected {
        Some(n) => text_fit(n, ax, y + 18.0 * u, 20.0 * u, aw, GOLD_TEXT),
        None => text_fit("Elige un jugador de la lista", ax, y + 18.0 * u, 16.0 * u, aw, MUTED),
    }
    y += 28.0 * u;
    let bw = (aw - 8.0 * u) / 2.0;
    let bh = 26.0 * u;
    let mut send: Option<&str> = None;
    let grid = [("Ir", "ir", "Ir hasta él (/IRA)"), ("Traer", "traer", "Traerlo a tu lado (/SUM)"), ("Info", "info", "Datos del personaje (/INFO)"), ("Penas", "penas", "Historial de penas (/PENAS)"), ("Revivir", "revivir", "Resucitar o curar (/REVIVIR)"), ("Echar", "echar", "Desconectarlo (/ECHAR)")];
    for (i, (label, action, tip)) in grid.iter().enumerate() {
        let b = Rect::new(ax + (i % 2) as f32 * (bw + 8.0 * u), y + (i / 2) as f32 * (bh + 6.0 * u), bw, bh);
        if button(b, label, false, u) && selected.is_some() {
            send = Some(action);
        }
        if b.contains(mouse_position().into()) {
            tips = vec![tip.to_string()];
        }
    }
    y += 3.0 * (bh + 6.0 * u) + 6.0 * u;
    text("Minutos", ax, y + 17.0 * u, 16.0 * u, MUTED);
    text_field(ui, "gm_minutes", &mut st.minutes, Rect::new(ax + 70.0 * u, y, aw - 70.0 * u, 26.0 * u), 6, false, |c| c.is_ascii_digit());
    y += 32.0 * u;
    text("Motivo", ax, y + 17.0 * u, 16.0 * u, MUTED);
    text_field(ui, "gm_reason", &mut st.reason, Rect::new(ax + 70.0 * u, y, aw - 70.0 * u, 26.0 * u), 60, false, |c| c != '@');
    y += 34.0 * u;
    let b1 = Rect::new(ax, y, bw, bh);
    let b2 = Rect::new(ax + bw + 8.0 * u, y, bw, bh);
    if button(b1, "Silenciar", false, u) && selected.is_some() {
        send = Some("silenciar");
    }
    if button(b2, "Encarcelar", false, u) && selected.is_some() {
        send = Some("carcel");
    }
    if b1.contains(mouse_position().into()) {
        tips = vec!["Silenciar los minutos indicados (0 = quitar)".into()];
    }
    if b2.contains(mouse_position().into()) {
        tips = vec!["Encarcelar los minutos indicados (0 = liberar)".into()];
    }
    if let (Some(action), Some(name)) = (send, &selected) {
        let minutes = st.minutes.parse().unwrap_or(0);
        game.gm_command(action_line(action, name, minutes, &st.reason));
        st.last_refresh = -100.0;
    }

    // ---- command line
    let cy = list.bottom() + 10.0 * u;
    text("Comando", r.x, cy + 18.0 * u, 16.0 * u, MUTED);
    let field = Rect::new(r.x + 72.0 * u, cy, r.w - 72.0 * u - 96.0 * u, 26.0 * u);
    let enter = text_field(ui, "gm_line", &mut st.line, field, 200, false, |_| true);
    if (button(Rect::new(field.right() + 8.0 * u, cy, 88.0 * u, 26.0 * u), "Enviar", false, u) || enter) && !st.line.trim().is_empty() {
        let line = if st.line.starts_with('/') { st.line.clone() } else { format!("/{}", st.line) };
        game.gm_command(line);
        st.line.clear();
        st.last_refresh = -100.0;
    }

    // ---- recent log
    let ly = cy + 34.0 * u;
    text("Registro reciente", r.x, ly + 16.0 * u, 17.0 * u, GOLD_TEXT);
    let log_r = Rect::new(r.x, ly + 22.0 * u, r.w, r.bottom() - ly - 22.0 * u);
    draw_rectangle(log_r.x, log_r.y, log_r.w, log_r.h, Color::new(0.0, 0.0, 0.0, 0.3));
    let lines = ((log_r.h - 6.0 * u) / (19.0 * u)).floor().max(0.0) as usize;
    let log = &game.local.p4.gm.log;
    if log.is_empty() {
        text("Sin acciones registradas.", log_r.x + 8.0 * u, log_r.y + 19.0 * u, 15.0 * u, MUTED);
    }
    for (i, e) in log.iter().take(lines).enumerate() {
        let yy = log_r.y + 18.0 * u + i as f32 * 19.0 * u;
        text(&hhmm(e.at), log_r.x + 8.0 * u, yy, 15.0 * u, MUTED);
        text_fit(&e.gm, log_r.x + 56.0 * u, yy, 15.0 * u, 90.0 * u, GOLD_TEXT);
        text_fit(&e.action, log_r.x + 150.0 * u, yy, 15.0 * u, 90.0 * u, GOOD);
        text_fit(&format!("{} {}", e.target, e.detail), log_r.x + 244.0 * u, yy, 15.0 * u, log_r.w - 252.0 * u, TEXT);
    }
    if !tips.is_empty() {
        tooltip(&tips, u);
    }
}

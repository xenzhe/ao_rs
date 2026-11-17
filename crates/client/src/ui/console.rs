//! Console (messages on top) and chat typing. OWNER: Sonnet 3 (docs/P3_CONTRATO.md):
//! channel tabs (all, map, system, whispers, party, guild, global), `/` commands to the
//! channels, wheel scrolling, history with the arrow keys.
//!
//! It is also the per-frame hook of the social screens (it runs every frame, before the
//! windows): the floating party invitation, "click a player to invite", and opening the
//! quest window when a quest NPC answers. See `party::frame_hooks` and `quests::auto_open`.
use crate::game::{tone_color, Game};
use crate::render::layout::Layout;
use crate::ui::fx_ui::ease;
use crate::ui::theme::{FontKind, GOLD, GOLD_DIM, GOLD_BRIGHT, GOLD_TEXT, MUTED, TEXT, TEXT_DIM};
use crate::ui::widgets::{measure, measure_with, text, text_with};
use crate::ui::{party, quests, Ui};
use ao_rules::MAX_CHAT;
use client_core::p3::social::{parse_chat, ConsoleTab, Feed, FeedLine, InputHistory, LineKind};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;

struct State {
    feed: Feed,
    tab: ConsoleTab,
    /// Rows scrolled up from the newest (0 = following the newest).
    scroll: usize,
    /// Lines seen per tab when it was last looked at (for the unread counters).
    seen: [u64; 7],
    history: InputHistory,
    expanded: bool,
    wrap_cache: HashMap<(String, i32, i32), Vec<String>>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        feed: Feed::default(),
        tab: std::env::var("AO_CONSOLE_TAB").ok().and_then(|n| ConsoleTab::ALL.into_iter().find(|t| t.label().to_lowercase() == n.to_lowercase())).unwrap_or(ConsoleTab::All),
        scroll: 0,
        seen: [0; 7],
        history: InputHistory::default(),
        expanded: false,
        wrap_cache: HashMap::new(),
    });
}

/// Colour of a console line by where it came from.
fn line_color(l: &FeedLine) -> Color {
    match l.kind {
        LineKind::Map | LineKind::System => tone_color(l.tone),
        LineKind::Whisper => Color::new(0.88, 0.62, 1.0, 1.0),
        LineKind::Yell => Color::new(1.0, 0.42, 0.36, 1.0),
        LineKind::Party => Color::new(0.45, 0.88, 1.0, 1.0),
        LineKind::Guild => Color::new(0.6, 0.95, 0.5, 1.0),
        LineKind::Global => Color::new(1.0, 0.76, 0.32, 1.0),
    }
}

fn tab_color(t: ConsoleTab) -> Color {
    match t {
        ConsoleTab::All | ConsoleTab::Map => TEXT,
        ConsoleTab::System => tone_color(ao_protocol::Tone::Warn),
        ConsoleTab::Whisper => line_color(&FeedLine { kind: LineKind::Whisper, text: String::new(), tone: ao_protocol::Tone::Chat }),
        ConsoleTab::Party => line_color(&FeedLine { kind: LineKind::Party, text: String::new(), tone: ao_protocol::Tone::Chat }),
        ConsoleTab::Guild => line_color(&FeedLine { kind: LineKind::Guild, text: String::new(), tone: ao_protocol::Tone::Chat }),
        ConsoleTab::Global => line_color(&FeedLine { kind: LineKind::Global, text: String::new(), tone: ao_protocol::Tone::Chat }),
    }
}

/// Cuts `s` into rows that fit `max_w` at `size` (breaking at spaces, or inside a word
/// longer than a row).
pub(crate) fn wrap(s: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut rows = Vec::new();
    let mut cur = String::new();
    for word in s.split(' ') {
        let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if measure(&candidate, size).width <= max_w {
            cur = candidate;
            continue;
        }
        if !cur.is_empty() {
            rows.push(std::mem::take(&mut cur));
        }
        cur = word.to_string();
        while measure(&cur, size).width > max_w && cur.chars().count() > 1 {
            let mut cut = cur.chars().count() - 1;
            while cut > 1 && measure(&cur.chars().take(cut).collect::<String>(), size).width > max_w {
                cut -= 1;
            }
            rows.push(cur.chars().take(cut).collect());
            cur = cur.chars().skip(cut).collect();
        }
    }
    rows.push(cur);
    rows
}

pub fn draw(ui: &mut Ui, game: &mut Game, lay: &Layout) {
    let u = lay.u;
    party::frame_hooks(ui, game, lay);
    quests::auto_open(ui, game);
    crate::ui::guild::frame_hooks(ui);
    demo(game);

    STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.feed.update(game);
        // `AO_NOHUD` (measuring the HUD's cost): no console either.
        if std::env::var_os("AO_NOHUD").is_none() && !crate::ui::hud::skip("console") {
            let t0 = crate::ui::hud::timing().then(get_time);
            draw_console(ui, game, lay, &mut st, u);
            if let Some(t0) = t0 {
                crate::ui::hud::add_cost(get_time() - t0);
            }
        }
    });
}

/// Demo mode (`AO_SOCIAL_DEMO` set): the windows do not ask the server for their data, so
/// what the demo put in the state stays.
pub fn demo_on() -> bool {
    std::env::var_os("AO_SOCIAL_DEMO").is_some()
}

/// Screenshots: `AO_SOCIAL_DEMO=console,guild,...` feeds the game the server messages of
/// `client_core::p3::social::demo_script` once, after entering the map.
fn demo(game: &mut Game) {
    thread_local! {
        static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static SINCE: std::cell::Cell<f64> = const { std::cell::Cell::new(-1.0) };
    }
    let Some(what) = std::env::var("AO_SOCIAL_DEMO").ok() else { return };
    if DONE.get() || game.me().is_none() {
        return;
    }
    // A moment after entering, so the server's own first messages (no guild...) are in.
    if SINCE.get() < 0.0 {
        SINCE.set(game.now);
    }
    if game.now - SINCE.get() < 1.0 {
        return;
    }
    DONE.set(true);
    let me = game.me().map(|e| e.name.clone()).unwrap_or_default();
    for m in client_core::p3::social::demo_script(&what, &me, game.my_id) {
        game.handle(m);
    }
}

/// Vertical gradient rectangle (one mesh).
fn vgrad(r: Rect, top: Color, bottom: Color) {
    let v = vec![
        Vertex::new(r.x, r.y, 0.0, 0.0, 0.0, top),
        Vertex::new(r.right(), r.y, 0.0, 0.0, 0.0, top),
        Vertex::new(r.right(), r.bottom(), 0.0, 0.0, 0.0, bottom),
        Vertex::new(r.x, r.bottom(), 0.0, 0.0, 0.0, bottom),
    ];
    draw_mesh(&Mesh { vertices: v, indices: vec![0, 1, 2, 0, 2, 3], texture: None });
}

/// Horizontal gradient rectangle (one mesh).
fn hgrad(r: Rect, left: Color, right: Color) {
    quad4(r, left, right, right, left);
}

/// Rectangle with a colour per corner (top-left, top-right, bottom-right, bottom-left).
fn quad4(r: Rect, tl: Color, tr: Color, br: Color, bl: Color) {
    let v = vec![
        Vertex::new(r.x, r.y, 0.0, 0.0, 0.0, tl),
        Vertex::new(r.right(), r.y, 0.0, 0.0, 0.0, tr),
        Vertex::new(r.right(), r.bottom(), 0.0, 0.0, 0.0, br),
        Vertex::new(r.x, r.bottom(), 0.0, 0.0, 0.0, bl),
    ];
    draw_mesh(&Mesh { vertices: v, indices: vec![0, 1, 2, 0, 2, 3], texture: None });
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

/// Splits "[Canal] Nombre: texto" / "Nombre: texto" into (speaker part, message), when the
/// speaker part is short enough to be a name.
fn speaker(row: &str) -> Option<(&str, &str)> {
    let i = row.find(": ")?;
    (i <= 40 && !row[..i].contains("  ")).then(|| row.split_at(i + 2))
}

fn draw_console(ui: &mut Ui, game: &mut Game, lay: &Layout, st: &mut State, u: f32) {
    let small = lay.console;
    let typing = game.chat.is_some();
    let mouse: Vec2 = mouse_position().into();
    let tall = Rect::new(small.x, small.y, small.w, (300.0 * u).min(lay.bottom.y - small.y - 120.0 * u).max(small.h));
    let tab_h = 22.0 * u;
    let strip = Rect::new(small.x, small.y, small.w, tab_h + 4.0 * u);
    // `mouse_captured` here still only says "over a window or dragging" (set by the frame).
    // The console lies over the world: unused, it only takes the mouse on its tab strip, so
    // the tiles under its lines can still be clicked.
    let free = !ui.mouse_captured;
    st.expanded = typing || std::env::var_os("AO_CONSOLE_OPEN").is_some() || (free && (strip.contains(mouse) || (st.expanded && tall.contains(mouse))));
    let r = if st.expanded { tall } else { small };
    let open = ease((-7, -7), st.expanded, 0.12);
    // Backdrop: a soft shade for legibility that fades to the right and below (no hard
    // edge on the world); expanded, a dark glass pane with gold hairlines.
    let shade = |a: f32| Color::new(0.02, 0.015, 0.01, a);
    let k = 1.0 - open;
    quad4(Rect::new(r.x, r.y, r.w, r.h), shade(0.5 + 0.35 * open), shade(0.1 * k + 0.85 * open), shade(0.85 * open), shade(0.35 * k + 0.85 * open));
    if k > 0.01 {
        let fade = Rect::new(r.x, r.bottom(), r.w, 18.0 * u);
        quad4(fade, shade(0.35 * k), shade(0.0), shade(0.0), shade(0.0));
    }
    if open > 0.01 {
        vgrad(Rect::new(r.x, r.y, r.w, r.h * 0.4), Color::new(1.0, 0.85, 0.6, 0.035 * open), Color::new(1.0, 0.85, 0.6, 0.0));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, with_alpha(GOLD_DIM, 0.9 * open));
    }
    hgrad(Rect::new(r.x, r.y + tab_h + 2.0 * u, r.w * 0.6, 1.0), with_alpha(GOLD, 0.65), with_alpha(GOLD, 0.0));
    if st.expanded && r.contains(mouse) || strip.contains(mouse) {
        ui.mouse_captured = true;
    }

    // ---- tabs (text with a channel-coloured underline; unread counts as a small pill)
    let mut x = r.x + 10.0 * u;
    let ty = r.y + 2.0 * u;
    let tsize = 15.0 * u;
    for t in ConsoleTab::ALL {
        let unread = if t == st.tab || t == ConsoleTab::All { 0 } else { st.feed.total[t.index()].saturating_sub(st.seen[t.index()]) };
        let label = t.label();
        let lw = measure_with(FontKind::Title, label, tsize).width;
        let pill = if unread > 0 { Some(unread.min(99).to_string()) } else { None };
        let pw = pill.as_ref().map(|p| measure(p, 12.0 * u).width + 9.0 * u).unwrap_or(0.0);
        let br = Rect::new(x - 5.0 * u, ty, lw + pw + 10.0 * u + if pill.is_some() { 4.0 * u } else { 0.0 }, tab_h);
        let active = t == st.tab;
        let hov = br.contains(mouse);
        let h = ease(crate::ui::fx_ui::key_of(br), hov || active, 0.08);
        let base = if active { GOLD_BRIGHT } else { with_alpha(TEXT_DIM, 0.75 + 0.25 * open) };
        let c = Color::new(base.r + (GOLD_TEXT.r - base.r) * h * 0.4, base.g + (GOLD_TEXT.g - base.g) * h * 0.4, base.b + (GOLD_TEXT.b - base.b) * h * 0.4, base.a);
        text_with(FontKind::Title, label, x, ty + 16.0 * u, tsize, c);
        let ch = tab_color(t);
        let uw = if active { lw } else { lw * h };
        if uw > 0.5 {
            draw_rectangle(x + (lw - uw) / 2.0, br.bottom() - 2.0 * u, uw, 2.0 * u, ch);
        }
        if let Some(p) = pill {
            let pr = Rect::new(x + lw + 4.0 * u, ty + 5.0 * u, pw, 13.0 * u);
            draw_rectangle(pr.x, pr.y, pr.w, pr.h, with_alpha(ch, 0.85));
            text(&p, pr.x + 4.5 * u, pr.bottom() - 2.5 * u, 12.0 * u, Color::new(0.05, 0.04, 0.03, 1.0));
        }
        if hov && is_mouse_button_pressed(MouseButton::Left) {
            st.tab = t;
            st.scroll = 0;
        }
        x += br.w + 8.0 * u;
    }
    st.seen[st.tab.index()] = st.feed.total[st.tab.index()];
    if st.tab == ConsoleTab::All {
        st.seen = st.feed.total;
    }

    // ---- lines
    let size = 15.0 * u;
    let row_h = 17.0 * u;
    let input_h = if typing { 28.0 * u } else { 0.0 };
    let foot = if st.expanded && !typing { 16.0 * u } else { 0.0 };
    let area = Rect::new(r.x + 10.0 * u, ty + tab_h + 6.0 * u, r.w - 20.0 * u - 8.0 * u, r.bottom() - (ty + tab_h + 6.0 * u) - 5.0 * u - input_h - foot);
    let rows = ((area.h / row_h).floor() as usize).max(1);
    if r.contains(mouse) {
        let wheel = mouse_wheel().1;
        if wheel > 0.0 {
            st.scroll += 3;
        } else if wheel < 0.0 {
            st.scroll = st.scroll.saturating_sub(3);
        }
        if is_key_pressed(KeyCode::PageUp) {
            st.scroll += rows;
        }
        if is_key_pressed(KeyCode::PageDown) {
            st.scroll = st.scroll.saturating_sub(rows);
        }
    }
    // Rows from the newest backwards: enough to fill the area at the current scroll.
    let need = rows + st.scroll;
    // (index into the tab, text, first row of its line), newest first.
    let mut visual: Vec<(usize, String, bool)> = Vec::new();
    let tab = st.tab;
    let lines: Vec<&FeedLine> = st.feed.tab(tab).collect();
    let total_lines = lines.len();
    let width_key = area.w.round() as i32;
    for (back, l) in lines.iter().rev().enumerate() {
        if visual.len() >= need {
            break;
        }
        let key = (l.text.clone(), width_key, size as i32);
        let wrapped = st.wrap_cache.entry(key).or_insert_with(|| wrap(&l.text, size, area.w)).clone();
        for (i, row) in wrapped.into_iter().enumerate().rev() {
            visual.push((back, row, i == 0));
        }
    }
    if st.wrap_cache.len() > 800 {
        st.wrap_cache.clear();
    }
    let max_scroll = visual.len().saturating_sub(rows);
    if visual.len() < need {
        st.scroll = st.scroll.min(max_scroll);
    }
    let shown: Vec<_> = visual.iter().skip(st.scroll).take(rows).collect();
    let n = shown.len();
    for (i, (back, row, first)) in shown.iter().rev().enumerate() {
        let y = area.y + area.h - (n - 1 - i) as f32 * row_h - 3.0 * u;
        let l = lines[total_lines - 1 - back];
        // Collapsed, older rows fade so the newest reads first.
        let age = (n - 1 - i) as f32 / rows.max(1) as f32;
        let a = 1.0 - (1.0 - open) * age * 0.55;
        let c = line_color(l);
        match (l.kind, *first, speaker(row)) {
            // System notices: quieter than what people say.
            (LineKind::System, _, _) => {
                let dim = Color::new(c.r * 0.82 + 0.1, c.g * 0.82 + 0.1, c.b * 0.82 + 0.1, 0.88 * a);
                text(row, area.x, y, size, dim);
            }
            (_, true, Some((who, msg))) => {
                let name_c = if l.kind == LineKind::Map { Color::new(0.62, 0.80, 1.0, a) } else { with_alpha(c, a) };
                text(who, area.x, y, size, name_c);
                let w = measure(who, size).width;
                let msg_c = if l.kind == LineKind::Map { with_alpha(TEXT, a) } else { Color::new((c.r + TEXT.r) / 2.0, (c.g + TEXT.g) / 2.0, (c.b + TEXT.b) / 2.0, a) };
                text(msg, area.x + w, y, size, msg_c);
            }
            _ => {
                let msg_c = if l.kind == LineKind::Map { with_alpha(TEXT, a) } else { Color::new((c.r + TEXT.r) / 2.0, (c.g + TEXT.g) / 2.0, (c.b + TEXT.b) / 2.0, a) };
                text(row, area.x, y, size, msg_c);
            }
        }
    }
    if lines.is_empty() {
        text_with(FontKind::NarrativeItalic, "Sin mensajes en este canal.", area.x, area.y + 16.0 * u, size, MUTED);
    }
    // Scroll bar and the "back to the newest" hint.
    if st.scroll > 0 {
        let tr = Rect::new(r.right() - 10.0 * u, area.y, 3.0 * u, area.h);
        draw_rectangle(tr.x, tr.y, tr.w, tr.h, Color::new(0.0, 0.0, 0.0, 0.5));
        let f = (st.scroll as f32 / (st.scroll + rows) as f32).clamp(0.0, 1.0);
        draw_rectangle(tr.x, tr.y + (tr.h - 24.0 * u) * (1.0 - f), tr.w, 24.0 * u, GOLD);
        let hint = "Rueda abajo: lo más reciente";
        let d = measure(hint, 13.0 * u);
        text(hint, r.right() - d.width - 18.0 * u, r.y + tab_h + 18.0 * u, 13.0 * u, MUTED);
    }

    if typing {
        let ir = Rect::new(r.x + 8.0 * u, r.bottom() - input_h - 6.0 * u, r.w - 16.0 * u, input_h);
        chat_input(ui, game, st, ir, u);
    } else {
        chat_start(ui, game);
        if st.expanded {
            let hint = "Enter escribir   /s susurro   /g grupo   /c clan   /gl global   /y gritar";
            text(hint, r.x + 10.0 * u, r.bottom() - 7.0 * u, 13.0 * u, with_alpha(MUTED, open));
        }
    }
}

/// Enter opens the chat line.
fn chat_start(ui: &mut Ui, game: &mut Game) {
    if !ui.keyboard_captured && is_key_pressed(KeyCode::Enter) {
        game.chat = Some(String::new());
        while get_char_pressed().is_some() {}
    }
}

/// Typing: Enter sends, Escape cancels, arrows up/down recall what was typed. While typing
/// the keyboard is the UI's.
fn chat_input(ui: &mut Ui, game: &mut Game, st: &mut State, ir: Rect, u: f32) {
    ui.keyboard_captured = true;
    let Some(t) = &mut game.chat else { return };
    while let Some(ch) = get_char_pressed() {
        if !ch.is_control() && t.chars().count() < MAX_CHAT {
            t.push(ch);
        }
    }
    if is_key_pressed(KeyCode::Backspace) {
        t.pop();
    }
    if is_key_pressed(KeyCode::Up) {
        if let Some(prev) = st.history.up(t) {
            *t = prev;
        }
    }
    if is_key_pressed(KeyCode::Down) {
        if let Some(next) = st.history.down() {
            *t = next;
        }
    }
    // Where the line will go.
    let reply = game.local.social.channel.last_whisper_from.clone();
    let preview = parse_chat(game.chat.as_deref().unwrap_or(""), reply.as_deref());
    let dest = preview.as_ref().map(|c| c.label()).unwrap_or_else(|| "Mapa".into());
    let line = game.chat.clone().unwrap_or_default();
    // Recessed field with a channel badge on the left (in the channel's colour).
    crate::ui::theme::surface(ir, crate::ui::theme::Surface::Recess, u);
    draw_rectangle_lines(ir.x, ir.y, ir.w, ir.h, 1.0, GOLD_TEXT);
    let badge_c = preview.as_ref().map(|c| {
        let kind = match c.label().as_str() {
            l if l.starts_with("Susurro") => LineKind::Whisper,
            "Grupo" => LineKind::Party,
            "Clan" => LineKind::Guild,
            "Global" => LineKind::Global,
            "Grito" => LineKind::Yell,
            _ => LineKind::Map,
        };
        line_color(&FeedLine { kind, text: String::new(), tone: ao_protocol::Tone::Chat })
    });
    let badge_c = badge_c.filter(|c| *c != WHITE).unwrap_or(GOLD_TEXT);
    let lw = measure_with(FontKind::Title, &dest, 14.0 * u).width + 16.0 * u;
    let badge = Rect::new(ir.x + 3.0 * u, ir.y + 3.0 * u, lw, ir.h - 6.0 * u);
    draw_rectangle(badge.x, badge.y, badge.w, badge.h, with_alpha(badge_c, 0.22));
    draw_rectangle(badge.x, badge.y, 2.0 * u, badge.h, badge_c);
    text_with(FontKind::Title, &dest, badge.x + 9.0 * u, ir.y + ir.h / 2.0 + 5.0 * u, 14.0 * u, badge_c);
    let cursor = if (get_time() * 2.0) as i32 % 2 == 0 { "|" } else { "" };
    let mut shown = format!("{line}{cursor}");
    // Show the end of a long line.
    while measure(&shown, 17.0 * u).width > ir.w - lw - 28.0 * u && shown.chars().count() > 1 {
        shown.remove(0);
    }
    text(&shown, badge.right() + 10.0 * u, ir.y + ir.h / 2.0 + 6.0 * u, 17.0 * u, WHITE);
    let chars = line.chars().count();
    if chars > MAX_CHAT * 3 / 4 {
        let s = format!("{chars}/{MAX_CHAT}");
        let w = measure(&s, 12.0 * u).width;
        text(&s, ir.right() - w - 6.0 * u, ir.y - 4.0 * u, 12.0 * u, if chars >= MAX_CHAT { crate::ui::theme::BAD } else { MUTED });
    }

    if is_key_pressed(KeyCode::Enter) {
        let text = game.chat.take().unwrap_or_default();
        st.history.push(&text);
        game.submit_chat(&text);
        st.scroll = 0;
    } else if is_key_pressed(KeyCode::Escape) {
        game.chat = None;
        st.history.reset();
    }
}

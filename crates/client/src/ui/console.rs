//! Console (messages on top) and chat typing. OWNER: Sonnet 3 (docs/P3_CONTRATO.md):
//! channel tabs (all, map, system, whispers, party, guild, global), `/` commands to the
//! channels, wheel scrolling, history with the arrow keys.
//!
//! It is also the per-frame hook of the social screens (it runs every frame, before the
//! windows): the floating party invitation, "click a player to invite", and opening the
//! quest window when a quest NPC answers. See `party::frame_hooks` and `quests::auto_open`.
use crate::game::{tone_color, Game};
use crate::render::layout::Layout;
use crate::ui::theme::{GOLD, GOLD_TEXT, MUTED, TEXT};
use crate::ui::widgets::{button, measure, panel, text};
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
        draw_console(ui, game, lay, &mut st, u);
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

fn draw_console(ui: &mut Ui, game: &mut Game, lay: &Layout, st: &mut State, u: f32) {
    let small = lay.console;
    let typing = game.chat.is_some();
    let mouse: Vec2 = mouse_position().into();
    let tall = Rect::new(small.x, small.y, small.w, 292.0 * u);
    // `mouse_captured` here still only says "over a window or dragging" (set by the frame).
    let free = !ui.mouse_captured;
    st.expanded = typing || std::env::var_os("AO_CONSOLE_OPEN").is_some() || (free && (small.contains(mouse) || (st.expanded && tall.contains(mouse))));
    let r = if st.expanded { tall } else { small };
    panel(r, u);
    if r.contains(mouse) {
        ui.mouse_captured = true;
    }

    // ---- tabs
    let tab_h = 19.0 * u;
    let mut x = r.x + 8.0 * u;
    let ty = r.y + 5.0 * u;
    for t in ConsoleTab::ALL {
        let unread = if t == st.tab || t == ConsoleTab::All { 0 } else { st.feed.total[t.index()].saturating_sub(st.seen[t.index()]) };
        let label = if unread > 0 { format!("{} ({})", t.label(), unread.min(99)) } else { t.label().to_string() };
        let w = measure(&label, 17.0 * u).width + 18.0 * u;
        let br = Rect::new(x, ty, w, tab_h);
        let active = t == st.tab;
        if button(br, &label, active, u) {
            st.tab = t;
            st.scroll = 0;
        }
        // A colour stripe under the tab says which channel it is.
        draw_rectangle(br.x + 3.0 * u, br.bottom() - 3.0 * u, br.w - 6.0 * u, 2.0 * u, tab_color(t));
        x += w + 4.0 * u;
    }
    st.seen[st.tab.index()] = st.feed.total[st.tab.index()];
    if st.tab == ConsoleTab::All {
        st.seen = st.feed.total;
    }

    // ---- lines
    let size = 15.0 * u;
    let row_h = 16.0 * u;
    let input_h = if typing { 26.0 * u } else { 0.0 };
    let foot = if st.expanded && !typing { 16.0 * u } else { 0.0 };
    let area = Rect::new(r.x + 10.0 * u, ty + tab_h + 4.0 * u, r.w - 20.0 * u - 14.0 * u, r.bottom() - (ty + tab_h + 4.0 * u) - 3.0 * u - input_h - foot);
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
    let mut visual: Vec<(usize, String)> = Vec::new(); // (index into the tab, text), newest first
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
        for row in wrapped.into_iter().rev() {
            visual.push((back, row));
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
    for (i, (back, row)) in shown.iter().rev().enumerate() {
        let y = area.y + area.h - (shown.len() - 1 - i) as f32 * row_h - 3.0 * u;
        let l = lines[total_lines - 1 - back];
        text(row, area.x, y, size, line_color(l));
    }
    if lines.is_empty() {
        text("Sin mensajes en este canal.", area.x, area.y + 14.0 * u, size, MUTED);
    }
    // Scroll bar and the "back to the newest" hint.
    if st.scroll > 0 {
        let tr = Rect::new(r.right() - 14.0 * u, area.y, 6.0 * u, area.h);
        draw_rectangle(tr.x, tr.y, tr.w, tr.h, Color::new(0.0, 0.0, 0.0, 0.4));
        let f = (st.scroll as f32 / (st.scroll + rows) as f32).clamp(0.0, 1.0);
        draw_rectangle(tr.x, tr.y + (tr.h - 20.0 * u) * (1.0 - f), tr.w, 20.0 * u, GOLD);
        let hint = "Rueda: más reciente";
        let d = measure(hint, 14.0 * u);
        text(hint, r.right() - d.width - 24.0 * u, r.y + 20.0 * u, 14.0 * u, MUTED);
    }

    if typing {
        let ir = Rect::new(r.x + 8.0 * u, r.bottom() - input_h - 4.0 * u, r.w - 16.0 * u, input_h);
        chat_input(ui, game, st, ir, u);
    } else {
        chat_start(ui, game);
        if st.expanded {
            let hint = "Enter: escribir   /s nombre texto   /g grupo   /c clan   /gl global   /y gritar";
            text(hint, r.x + 10.0 * u, r.bottom() - 7.0 * u, 14.0 * u, MUTED);
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
    draw_rectangle(ir.x, ir.y, ir.w, ir.h, Color::new(0.0, 0.0, 0.0, 0.65));
    draw_rectangle_lines(ir.x, ir.y, ir.w, ir.h, 1.0, GOLD_TEXT);
    let label = format!("[{dest}]");
    let lw = measure(&label, 15.0 * u).width;
    text(&label, ir.x + 8.0 * u, ir.y + ir.h / 2.0 + 5.0 * u, 15.0 * u, GOLD_TEXT);
    let cursor = if (get_time() * 2.0) as i32 % 2 == 0 { "|" } else { "" };
    let mut shown = format!("{line}{cursor}");
    // Show the end of a long line.
    while measure(&shown, 17.0 * u).width > ir.w - lw - 28.0 * u && shown.chars().count() > 1 {
        shown.remove(0);
    }
    text(&shown, ir.x + lw + 18.0 * u, ir.y + ir.h / 2.0 + 6.0 * u, 17.0 * u, WHITE);

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

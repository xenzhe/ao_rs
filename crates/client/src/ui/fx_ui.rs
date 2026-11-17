//! UI motion (docs/ESTILO_UI.md §1.7): eased hover/press values per element, and toasts
//! (short notices that slide in at the top and leave by themselves). State is kept per
//! element key and swept every frame, so nothing allocates in the hot path once warm.
#![allow(dead_code)] // toolkit
use super::theme::{self, FontKind};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;

/// A stable key for an element: its rounded position (widgets do not carry ids).
pub fn key_of(r: Rect) -> (i32, i32) {
    (r.x.round() as i32, r.y.round() as i32)
}

#[derive(Clone, Copy)]
struct Anim {
    value: f32,
    seen: u64,
}

/// How a toast looks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Info,
    Good,
    Warn,
    Bad,
}

struct Toast {
    text: String,
    kind: ToastKind,
    born: f64,
}

#[derive(Default)]
struct State {
    anims: HashMap<(i32, i32), Anim>,
    frame: u64,
    toasts: Vec<Toast>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Called once per frame (by `ui::frame` and the screens): ages the animations and drops
/// the ones not drawn for a while.
pub fn begin_frame() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.frame += 1;
        let f = s.frame;
        if f % 120 == 0 {
            s.anims.retain(|_, a| f - a.seen < 120);
        }
    });
}

/// Eased value 0..1 that moves towards `on` (1) or off (0) in about `secs`.
pub fn ease(key: (i32, i32), on: bool, secs: f32) -> f32 {
    let dt = get_frame_time().min(0.05);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let frame = s.frame;
        let a = s.anims.entry(key).or_insert(Anim { value: 0.0, seen: frame });
        let target = if on { 1.0 } else { 0.0 };
        let k = (dt / secs.max(0.001)).min(1.0);
        a.value += (target - a.value) * k * 2.2;
        a.value = a.value.clamp(0.0, 1.0);
        a.seen = frame;
        a.value
    })
}

/// Shows a toast at the top of the screen for ~3.5 s (newest on top, at most 4).
pub fn toast(text: impl Into<String>, kind: ToastKind) {
    let text = text.into();
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.toasts.iter().any(|t| t.text == text) {
            return;
        }
        s.toasts.insert(0, Toast { text, kind, born: get_time() });
        s.toasts.truncate(4);
    });
}

const TOAST_SECS: f64 = 3.5;

/// Draws the toasts (call last, over everything).
pub fn draw_toasts(u: f32) {
    let now = get_time();
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.toasts.retain(|t| now - t.born < TOAST_SECS);
        let cx = screen_width() / 2.0;
        let mut y = 120.0 * u;
        for t in &s.toasts {
            let age = (now - t.born) as f32;
            let left = TOAST_SECS as f32 - age;
            let a = (age / 0.18).min(1.0).min(left / 0.4).clamp(0.0, 1.0);
            let slide = (1.0 - (age / 0.18).min(1.0)) * -12.0 * u;
            let size = 22.0 * u;
            let w = super::widgets::measure_with(FontKind::Title, &t.text, size).width + 70.0 * u;
            let r = Rect::new(cx - w / 2.0, y + slide, w, 40.0 * u);
            let accent = match t.kind {
                ToastKind::Info => theme::GOLD,
                ToastKind::Good => theme::GOOD,
                ToastKind::Warn => theme::WARN,
                ToastKind::Bad => theme::BAD,
            };
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.04, 0.03, 0.02, 0.88 * a));
            draw_line(r.x, r.y, r.right(), r.y, 1.5 * u, Color::new(accent.r, accent.g, accent.b, 0.8 * a));
            draw_line(r.x, r.bottom(), r.right(), r.bottom(), 1.5 * u, Color::new(accent.r, accent.g, accent.b, 0.8 * a));
            for (side, x) in [(-1.0f32, r.x), (1.0, r.right())] {
                let c = vec2(x, r.y + r.h / 2.0);
                let d = 6.0 * u;
                draw_triangle(c + vec2(0.0, -d), c + vec2(side * d, 0.0), c + vec2(0.0, d), Color::new(accent.r, accent.g, accent.b, a));
            }
            let tw = super::widgets::measure_with(FontKind::Title, &t.text, size).width;
            super::widgets::text_with(FontKind::Title, &t.text, cx - tw / 2.0, r.y + r.h / 2.0 + size * 0.35, size, Color::new(theme::TEXT.r, theme::TEXT.g, theme::TEXT.b, a));
            y += 48.0 * u;
        }
    });
}

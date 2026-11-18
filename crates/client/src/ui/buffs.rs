//! Buff bar of the HUD (`S2C::Buff`, the VB `BuffList` / `DeBuffList` / `CDList` drawn by
//! `ModCooldown.bas`): a row of round icons above the portrait's state badges. Each one is
//! the official `init/Effects.ini` GRH in a dark recess with a ring: gold for a buff, red
//! for a debuff, dim for a cooldown; a dark sweep shows the time used, the seconds left sit
//! under it and drunk-like stacks in the corner. Tooltip with what it is and how long.
use super::theme::{BAD, GOLD, GOLD_DIM, TEXT};
use super::widgets::{measure, text};
use crate::assets::Assets;
use crate::game::Game;
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::time::Instant;

thread_local! {
    /// `LoadBuffResources`: icon (ClientEffectTypeId) -> GRH.
    static ICONS: RefCell<Option<HashMap<u16, u32>>> = const { RefCell::new(None) };
}

fn icon_grh(assets: &Assets, icon: u16) -> u32 {
    ICONS.with(|c| {
        let mut c = c.borrow_mut();
        let map = c.get_or_insert_with(|| {
            let bytes = std::fs::read(assets.root.join("init").join("Effects.ini")).unwrap_or_default();
            parse_effects_ini(&String::from_utf8_lossy(&bytes))
        });
        map.get(&icon).copied().unwrap_or(0)
    })
}

/// `[EffectN] GRH=g` sections.
pub(crate) fn parse_effects_ini(s: &str) -> HashMap<u16, u32> {
    let mut out = HashMap::new();
    let mut cur: Option<u16> = None;
    for line in s.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('[') {
            let name = rest.split(']').next().unwrap_or("");
            cur = name.strip_prefix("Effect").and_then(|n| n.parse().ok());
        } else if let (Some(id), Some((k, v))) = (cur, line.split_once('=')) {
            if k.trim().eq_ignore_ascii_case("GRH") {
                if let Ok(g) = v.trim().split(|c: char| !c.is_ascii_digit()).next().unwrap_or("").parse() {
                    out.insert(id, g);
                }
            }
        }
    }
    out
}

fn label(kind: u8) -> &'static str {
    match kind {
        ao_protocol::buff_kind::DEBUFF | ao_protocol::buff_kind::INFO_DEBUFF => "Perjuicio",
        ao_protocol::buff_kind::COOLDOWN => "Enfriamiento",
        _ => "Beneficio",
    }
}

fn sweep(c: Vec2, r: f32, frac: f32, col: Color) {
    if frac <= 0.0 {
        return;
    }
    let n = (32.0 * frac).ceil().max(2.0) as u16;
    let mut v = Vec::with_capacity(n as usize + 2);
    v.push(Vertex::new(c.x, c.y, 0.0, 0.0, 0.0, col));
    for i in 0..=n {
        let a = -PI / 2.0 + i as f32 / n as f32 * frac * TAU;
        v.push(Vertex::new(c.x + a.cos() * r, c.y + a.sin() * r, 0.0, 0.0, 0.0, col));
    }
    let idx: Vec<u16> = (0..n).flat_map(|i| [0, i + 1, i + 2]).collect();
    draw_mesh(&Mesh { vertices: v, indices: idx, texture: None });
}

fn secs_label(ms: u64) -> String {
    let s = ms.div_ceil(1000);
    if s >= 60 {
        format!("{}m", s.div_ceil(60))
    } else {
        format!("{s}")
    }
}

/// Draws the bar with its bottom-left corner at (`x`, `bottom`).
pub(crate) fn draw(game: &mut Game, assets: &mut Assets, x: f32, bottom: f32, u: f32, tip: &mut Vec<String>) {
    let now = Instant::now();
    game.local.buffs.expire(now);
    if game.local.buffs.list.is_empty() {
        return;
    }
    // Buffs, then debuffs, then cooldowns; within each, what ends first goes first.
    let mut list = game.local.buffs.list.clone();
    list.sort_by_key(|b| (if b.is_cooldown() { 2 } else if b.is_debuff() { 1 } else { 0 }, b.remaining_ms(now)));
    let r = 15.0 * u;
    let gap = 7.0 * u;
    let cy = bottom - r - 14.0 * u;
    let mut cx = x + r;
    let mouse: Vec2 = mouse_position().into();
    for b in &list {
        let c = vec2(cx, cy);
        let ring_col = if b.is_cooldown() {
            GOLD_DIM
        } else if b.is_debuff() {
            BAD
        } else {
            GOLD
        };
        draw_circle(c.x, c.y + 2.0 * u, r + 1.0, Color::new(0.0, 0.0, 0.0, 0.55));
        draw_circle(c.x, c.y, r, Color::new(0.04, 0.03, 0.022, 0.92));
        let grh = icon_grh(assets, b.icon);
        if let Some(g) = assets.db.frame(grh, 0.0).cloned() {
            if let Some(tex) = assets.texture(g.file) {
                let side = r * 1.5;
                let s = (side / g.w).min(side / g.h);
                let (w, h) = (g.w * s, g.h * s);
                let tint = if b.is_cooldown() { Color::new(0.7, 0.7, 0.7, 1.0) } else { WHITE };
                draw_texture_ex(&tex, (c.x - w / 2.0).round(), (c.y - h / 2.0).round(), tint, DrawTextureParams { source: Some(Rect::new(g.sx, g.sy, g.w, g.h)), dest_size: Some(vec2(w, h)), ..Default::default() });
            }
        }
        // Time used, as a dark sweep over the icon.
        sweep(c, r, 1.0 - b.left_fraction(now), Color::new(0.0, 0.0, 0.0, 0.5));
        draw_circle_lines(c.x, c.y, r, 2.0 * u, ring_col);
        let left = b.remaining_ms(now);
        let finite = b.total_ms > 0;
        if finite {
            let s = secs_label(left);
            let size = 12.0 * u;
            let w = measure(&s, size).width;
            // A blink in the last 3 seconds (it is about to end).
            let a = if left < 3_000 { 0.55 + 0.45 * ((get_time() * 6.0).sin() as f32).abs() } else { 1.0 };
            text(&s, c.x - w / 2.0, c.y + r + 12.0 * u, size, Color::new(TEXT.r, TEXT.g, TEXT.b, a));
        }
        if b.stacks > 1 {
            let s = b.stacks.to_string();
            draw_circle(c.x + r * 0.72, c.y - r * 0.72, 7.0 * u, Color::new(0.32, 0.04, 0.03, 1.0));
            draw_circle_lines(c.x + r * 0.72, c.y - r * 0.72, 7.0 * u, 1.2 * u, GOLD);
            let w = measure(&s, 11.0 * u).width;
            text(&s, c.x + r * 0.72 - w / 2.0, c.y - r * 0.72 + 4.0 * u, 11.0 * u, TEXT);
        }
        if mouse.distance(c) < r {
            let mut t = vec![label(b.kind).to_string()];
            if finite {
                t.push(format!("Quedan {} s", left.div_ceil(1000)));
            } else {
                t.push("Mientras dure".to_string());
            }
            if b.stacks > 1 {
                t.push(format!("Acumulado x{}", b.stacks));
            }
            *tip = t;
        }
        cx += r * 2.0 + gap;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn effects_ini_grh_per_row() {
        let m = super::parse_effects_ini("[INIT]\nEffectCount=2\n[Effect4] 'taunt\nGRH=74708\n[Effect6]'alcohol\nGRH=28875\nNameKey=X\n");
        assert_eq!((m[&4], m[&6]), (74708, 28875));
        assert!(!m.contains_key(&1));
    }
}

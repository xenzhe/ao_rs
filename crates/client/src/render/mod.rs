//! Rendering. The world is drawn into an off-screen target that covers the whole world
//! window (the official 23x18 view plus, on wide screens, fogged surroundings), lit by a
//! light map, then scaled to the screen; names and bars are drawn afterwards in screen
//! space so they stay crisp at any resolution.
//! - [`layout`]: where everything goes on screen
//! - [`world`]: map layers, neighbour maps and characters
//! - [`lighting`]: day/night and light map
//! - [`fx`]: particles, projectiles, lightning, floating text
//!
//! Target coordinates: (0, 0) is always the top-left of the official view (the camera
//! `origin`), so everything that draws `world - origin` keeps working; the wider window just
//! extends to negative and beyond `RT_W`/`RT_H` (see `Layout::rt_offset`).
pub mod characters;
pub mod fx;
pub mod layout;
pub mod lighting;
pub mod weather;
pub mod world;

use crate::assets::Assets;
use crate::game::{v, Game};
use ao_protocol::EntityKind;
use client_core::p3::world::ContinuousWorld;
use fx::Fx;
use layout::{Layout, RT_H, RT_W, TILE};
use macroquad::prelude::*;
use world::WorldFades;

/// Player tile's top-left inside the official view (centre column, middle row).
const PLAYER_ANCHOR: Vec2 = Vec2::new((ao_rules::VIEW_TILES_W / 2) as f32 * TILE, (ao_rules::VIEW_TILES_H / 2) as f32 * TILE - 16.0);
/// Seconds for the camera to settle after a map crossing that was mid-step.
const CROSS_SETTLE_SECS: f32 = 0.12;

pub struct Renderer {
    world_rt: RenderTarget,
    light_rt: RenderTarget,
    rt_size: Vec2,
    pub fx: Fx,
    pub fades: WorldFades,
    pub pixel_perfect: bool,
    pub hour_offset: f32,
    map_shown: Option<u16>,
    /// Neighbour maps for the continuous world (created with the first map).
    pub continuous: Option<ContinuousWorld>,
    /// Last frame's map and origin, to keep the camera still across a crossing.
    last_view: Option<(u16, Vec2)>,
    /// Camera offset that decays to zero after a crossing.
    cam_offset: Vec2,
}

fn new_targets(size: Vec2) -> (RenderTarget, RenderTarget) {
    let world_rt = render_target(size.x as u32, size.y as u32);
    world_rt.texture.set_filter(FilterMode::Nearest);
    let light_rt = render_target(size.x as u32, size.y as u32);
    light_rt.texture.set_filter(FilterMode::Linear);
    (world_rt, light_rt)
}

/// Camera for a target of `size` whose top-left is `off` (target pixels, origin-relative).
fn rt_camera(rt: &RenderTarget, size: Vec2, off: Vec2) -> Camera2D {
    Camera2D { zoom: vec2(2.0 / size.x, 2.0 / size.y), target: off + size / 2.0, render_target: Some(rt.clone()), ..Default::default() }
}

impl Renderer {
    pub fn new(pixel_perfect: bool, hour_offset: f32) -> Renderer {
        let rt_size = vec2(RT_W, RT_H);
        let (world_rt, light_rt) = new_targets(rt_size);
        Renderer {
            world_rt,
            light_rt,
            rt_size,
            fx: Fx::new(),
            fades: WorldFades::default(),
            pixel_perfect,
            hour_offset,
            map_shown: None,
            continuous: None,
            last_view: None,
            cam_offset: Vec2::ZERO,
        }
    }

    /// World pixel shown at the top-left of the official view (camera follows the player).
    pub fn origin(game: &Game) -> Vec2 {
        let tl = game.me().map(|e| v(e.tile_tl())).unwrap_or(Vec2::ZERO);
        (tl - PLAYER_ANCHOR).round()
    }

    pub fn hour(&self) -> f32 {
        lighting::hour(self.hour_offset, get_time())
    }

    /// Loads neighbour maps (one per frame) and their textures, and handles map changes.
    fn update_continuous(&mut self, game: &Game, assets: &mut Assets, origin: Vec2, dt: f32) {
        let Some(number) = game.map.as_ref().map(|m| m.number) else { return };
        let cw = self.continuous.get_or_insert_with(|| ContinuousWorld::new(assets.root.clone()));
        let t0 = get_time();
        cw.update(number, 1);
        for n in cw.take_new() {
            if let Some(m) = cw.map(n) {
                let files = assets.preload_map(m);
                if std::env::var("AO_DEBUG").is_ok() {
                    eprintln!("neighbour map {n} '{}' textures={files} in {:.1} ms", m.name, (get_time() - t0) * 1000.0);
                }
            }
        }
        if Some(number) != self.map_shown {
            let crossing = cw.last_crossing.filter(|_| self.map_shown.is_some());
            // A crossing to a bordering map keeps the roofs; tile-keyed tree fades are reset.
            if crossing.is_none() {
                self.fades.clear();
            }
            self.cam_offset = Vec2::ZERO;
            if let (Some(d), Some((_, prev))) = (crossing, self.last_view) {
                // Where the camera was, in the new map's coordinates: keep it there and let
                // it settle (the server snaps the player, a half-done step would jump).
                let off = prev + vec2(d.0 as f32, d.1 as f32) * TILE - origin;
                if off.length() < 2.0 * TILE {
                    self.cam_offset = off;
                }
            }
            self.map_shown = Some(number);
        }
        self.cam_offset *= (1.0 - dt / CROSS_SETTLE_SECS).max(0.0);
        if self.cam_offset.length() < 0.5 {
            self.cam_offset = Vec2::ZERO;
        }
        self.last_view = Some((number, origin));
    }

    /// Draws the lit world into the off-screen target and blits it to `lay.stage`.
    pub fn draw_world(&mut self, game: &Game, assets: &mut Assets, lay: &Layout, t_ms: f32, dt: f32) {
        let base = dev_view_at().map(|(x, y)| vec2((x - 1) as f32, (y - 1) as f32) * TILE - PLAYER_ANCHOR).unwrap_or_else(|| Self::origin(game));
        self.update_continuous(game, assets, base, dt);
        let origin = (base + self.cam_offset).round();
        let size = lay.rt_size();
        if size != self.rt_size {
            (self.world_rt, self.light_rt) = new_targets(size);
            self.rt_size = size;
        }
        let off = lay.rt_offset();
        let window = Rect::new(off.x, off.y, size.x, size.y);
        let official = Rect::new(0.0, 0.0, RT_W, RT_H);
        let dungeon = game.map.as_ref().is_some_and(|m| m.zone.eq_ignore_ascii_case("DUNGEON"));
        let amb = lighting::ambient(self.hour(), dungeon);

        set_camera(&rt_camera(&self.light_rt, size, off));
        lighting::build_light_map(game, &self.fx, origin, amb, t_ms);

        set_camera(&rt_camera(&self.world_rt, size, off));
        clear_background(BLACK);
        if let Some(cw) = &self.continuous {
            world::draw_world(game, assets, &mut self.fades, cw, origin, window, t_ms, dt);
        }
        self.fx.draw(origin);
        // Both targets share the same orientation: no flip between them.
        gl_use_material(&self.fx.multiply);
        draw_texture_ex(&self.light_rt.texture, off.x, off.y, WHITE, DrawTextureParams { dest_size: Some(size), ..Default::default() });
        gl_use_default_material();
        world::draw_fog(window, official);
        self.fx.draw_overlay(origin, vec4(off.x, off.y, size.x, size.y));
        if game.casting || lay.view.contains(mouse_position().into()) {
            draw_target_square(game, lay, origin);
        }
        set_default_camera();

        // A Camera2D with positive zoom leaves the target upright: no flip_y here.
        let st = lay.stage;
        draw_texture_ex(&self.world_rt.texture, st.x, st.y, WHITE, DrawTextureParams { dest_size: Some(vec2(st.w, st.h)), ..Default::default() });
        if std::env::var_os("AO_NOHUD").is_none() {
            let t0 = crate::ui::hud::timing().then(get_time);
            if !crate::ui::hud::skip("names") {
                draw_names(game, lay, origin);
            }
            if !crate::ui::hud::skip("frame") {
                screen_frame(lay);
            }
            if let Some(t0) = t0 {
                crate::ui::hud::add_cost(get_time() - t0);
            }
        }
    }
}

/// Thin ornamental frame along the screen edge and a soft vignette: the world is the
/// window, the HUD floats on it.
fn screen_frame(lay: &Layout) {
    let (sw, sh, u) = (screen_width(), screen_height(), lay.u);
    let edge = Color::new(0.0, 0.0, 0.0, 0.0);
    let dark = Color::new(0.02, 0.015, 0.01, 0.55);
    let d = 70.0 * u;
    let outer = Rect::new(0.0, 0.0, sw, sh);
    let inner = Rect::new(d, d, sw - 2.0 * d, sh - 2.0 * d);
    gradient_ring(inner, edge, outer, dark);
    let gold = Color::new(0.62, 0.48, 0.24, 0.85);
    let i = 3.0 * u;
    draw_rectangle_lines(i, i, sw - 2.0 * i, sh - 2.0 * i, 1.5 * u, gold);
    draw_rectangle_lines(i + 4.0 * u, i + 4.0 * u, sw - 2.0 * i - 8.0 * u, sh - 2.0 * i - 8.0 * u, 1.0, Color::new(0.42, 0.32, 0.17, 0.5));
    let s = 26.0 * u;
    crate::ui::theme::corner(vec2(i + 2.0 * u, i + 2.0 * u), 1.0, 1.0, s);
    crate::ui::theme::corner(vec2(sw - i - 2.0 * u, i + 2.0 * u), -1.0, 1.0, s);
    crate::ui::theme::corner(vec2(i + 2.0 * u, sh - i - 2.0 * u), 1.0, -1.0, s);
    crate::ui::theme::corner(vec2(sw - i - 2.0 * u, sh - i - 2.0 * u), -1.0, -1.0, s);
}

/// Gradient between two nested rectangles (vertex colours, one mesh).
pub(crate) fn gradient_ring(a: Rect, ca: Color, b: Rect, cb: Color) {
    let ai = [vec2(a.x, a.y), vec2(a.right(), a.y), vec2(a.right(), a.bottom()), vec2(a.x, a.bottom())];
    let bo = [vec2(b.x, b.y), vec2(b.right(), b.y), vec2(b.right(), b.bottom()), vec2(b.x, b.bottom())];
    let mut vertices = Vec::with_capacity(8);
    vertices.extend(ai.iter().map(|p| Vertex::new(p.x, p.y, 0.0, 0.0, 0.0, ca)));
    vertices.extend(bo.iter().map(|p| Vertex::new(p.x, p.y, 0.0, 0.0, 0.0, cb)));
    let mut indices = Vec::with_capacity(24);
    for i in 0..4u16 {
        let j = (i + 1) % 4;
        indices.extend_from_slice(&[i, j, 4 + j, i, 4 + j, 4 + i]);
    }
    draw_mesh(&Mesh { vertices, indices, texture: None });
}

/// Development only: `AO_VIEW_AT=x,y` points the camera at that tile of the current map
/// (screenshots of map borders without walking there; the player stays where it is).
fn dev_view_at() -> Option<(i32, i32)> {
    static AT: std::sync::OnceLock<Option<(i32, i32)>> = std::sync::OnceLock::new();
    *AT.get_or_init(|| {
        let s = std::env::var("AO_VIEW_AT").ok()?;
        let (x, y) = s.split_once(',')?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    })
}

fn draw_target_square(game: &Game, lay: &Layout, origin: Vec2) {
    let Some(wp) = lay.screen_to_world(mouse_position().into(), origin) else { return };
    let (tx, ty) = ((wp.x / TILE).floor(), (wp.y / TILE).floor());
    let sp = vec2(tx * TILE, ty * TILE) - origin;
    let (c, w) = if game.casting { (Color::new(1.0, 0.8, 0.2, 0.9), 2.0) } else { (Color::new(1.0, 1.0, 1.0, 0.25), 1.0) };
    draw_rectangle_lines(sp.x, sp.y, TILE, TILE, w, c);
}

/// Colour of a player's name by faction (`e_Facciones`): citizens blue, the royal army
/// brighter, criminals red, the chaos side darker red; unknown players neutral blue.
pub fn faction_color(status: Option<u8>) -> Color {
    match status {
        Some(0) => Color::new(1.0, 0.38, 0.32, 1.0),
        Some(2) | Some(4) => Color::new(0.95, 0.25, 0.35, 1.0),
        Some(3) | Some(5) => Color::new(0.55, 0.78, 1.0, 1.0),
        _ => Color::new(0.48, 0.68, 1.0, 1.0),
    }
}

/// Health shown by the bars over each character, per entity: (shown, trailing "recent
/// damage" value, time of the last change). Keyed by entity id, not by position.
#[derive(Clone, Copy)]
struct BarAnim {
    shown: f32,
    lag: f32,
    hit_at: f64,
}

thread_local! {
    static BARS: std::cell::RefCell<std::collections::HashMap<u32, BarAnim>> = Default::default();
}

/// Names (with guild tag and faction colour) and thin health bars in screen space, crisp at
/// any scale (official view only). The hovered character also shows its name if it is a
/// creature.
fn draw_names(game: &Game, lay: &Layout, origin: Vec2) {
    use crate::ui::theme::FontKind;
    use crate::ui::widgets::{measure_with, text_with};
    let now = get_time();
    let dt = get_frame_time().min(0.1);
    let mouse: Vec2 = mouse_position().into();
    let faction = &game.local.social.faction;
    let tags = &game.local.social.guild.tags;
    BARS.with(|b| {
        let mut bars = b.borrow_mut();
        bars.retain(|id, _| game.ents.contains_key(id));
        for (id, e) in &game.ents {
            let p = lay.world_to_screen(v(e.world_pos()), origin);
            if !lay.view.contains(p) {
                continue;
            }
            // Invisible or hidden: others see no name and no bar (the body is not drawn either).
            let hidden = e.status & (ao_protocol::status::INVISIBLE | ao_protocol::status::HIDDEN) != 0;
            if hidden && *id != game.my_id {
                continue;
            }
            let s = lay.scale;
            let size = (14.0 * s.max(1.0)).min(20.0 * lay.u);
            let dead = e.status & ao_protocol::status::DEAD != 0;
            let hovered = Rect::new(p.x - 16.0 * s, p.y - 50.0 * s, 32.0 * s, 54.0 * s).contains(mouse);
            if e.kind == EntityKind::Player || hovered {
                let mut c = match e.kind {
                    EntityKind::Player if *id == game.my_id => faction_color(faction.known.then_some(faction.status)),
                    EntityKind::Player => faction_color(faction.others.get(id).copied()),
                    EntityKind::Creature => Color::new(1.0, 0.82, 0.55, 1.0),
                };
                if dead {
                    c = Color::new(0.6, 0.6, 0.62, 0.9);
                }
                let y = p.y + 14.0 * s + size * 0.2;
                let d = measure_with(FontKind::Body, &e.name, size);
                if hovered {
                    // A soft dark pill behind the hovered name.
                    let r = Rect::new(p.x - d.width / 2.0 - 6.0, y - size * 0.9, d.width + 12.0, size * 1.2);
                    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.45));
                }
                text_with(FontKind::Body, &e.name, p.x - d.width / 2.0, y, size, c);
                if let Some(tag) = tags.get(id).filter(|t| !t.is_empty()) {
                    let ts = size * 0.8;
                    let t = format!("«{tag}»");
                    let td = measure_with(FontKind::Title, &t, ts);
                    text_with(FontKind::Title, &t, p.x - td.width / 2.0, y + size * 1.0, ts, Color::new(0.92, 0.76, 0.44, 0.95));
                }
            }
            // Health bar: hidden at full health, with a trailing "recent damage" segment.
            let f = if e.max_hp > 0 { (e.hp.max(0) as f32 / e.max_hp as f32).clamp(0.0, 1.0) } else { 1.0 };
            let a = bars.entry(*id).or_insert(BarAnim { shown: f, lag: f, hit_at: -10.0 });
            if (f - a.shown).abs() > 0.001 {
                if f < a.shown {
                    a.hit_at = now;
                } else {
                    a.lag = f;
                }
                a.shown = f;
            }
            if now - a.hit_at > 0.45 {
                a.lag = (a.lag - dt * 0.9).max(a.shown);
            }
            if (f < 1.0 || a.lag > f + 0.001) && !dead {
                let w = 34.0 * s;
                let h = (3.5 * s).max(3.0);
                let r = Rect::new((p.x - w / 2.0).round(), (p.y - 58.0 * s).round(), w, h);
                draw_rectangle(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, Color::new(0.0, 0.0, 0.0, 0.75));
                draw_rectangle(r.x, r.y, r.w * a.lag, r.h, Color::new(1.0, 0.85, 0.55, 0.85));
                let col = if f > 0.5 { Color::new(0.35, 0.8, 0.25, 1.0) } else if f > 0.25 { Color::new(0.95, 0.7, 0.15, 1.0) } else { Color::new(0.92, 0.16, 0.10, 1.0) };
                draw_rectangle(r.x, r.y, r.w * f, r.h, col);
                draw_rectangle(r.x, r.y, r.w * f, r.h * 0.4, Color::new(1.0, 1.0, 1.0, 0.25));
                let flash = (1.0 - (now - a.hit_at) as f32 / 0.25).clamp(0.0, 1.0);
                if flash > 0.0 {
                    draw_rectangle(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, Color::new(1.0, 1.0, 1.0, 0.5 * flash));
                }
            }
        }
    });
}

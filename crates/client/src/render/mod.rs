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

    /// Draws the lit world into the off-screen target and blits it to `lay.world`.
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
        draw_texture_ex(&self.world_rt.texture, lay.world.x, lay.world.y, WHITE, DrawTextureParams { dest_size: Some(vec2(lay.world.w, lay.world.h)), ..Default::default() });
        draw_rectangle_lines(lay.world.x - 2.0, lay.world.y - 2.0, lay.world.w + 4.0, lay.world.h + 4.0, 2.0 * lay.u, Color::new(0.62, 0.48, 0.24, 1.0));
        draw_names(game, lay, origin);
    }
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

/// Names and HP bars in screen space, crisp at any scale (official view only).
fn draw_names(game: &Game, lay: &Layout, origin: Vec2) {
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
        let size = (14.0 * lay.scale.max(1.0)).min(22.0 * lay.u);
        if e.kind == EntityKind::Player {
            let d = crate::ui::widgets::measure(&e.name, size);
            let c = if *id == game.my_id { Color::new(0.55, 0.8, 1.0, 1.0) } else { Color::new(0.4, 0.6, 1.0, 1.0) };
            crate::ui::widgets::text(&e.name, p.x - d.width / 2.0, p.y + 16.0 * lay.scale, size, c);
        }
        if e.hp < e.max_hp {
            let w = 30.0 * lay.scale;
            let y = p.y - 60.0 * lay.scale;
            draw_rectangle(p.x - w / 2.0, y, w, 4.0 * lay.scale, Color::new(0.0, 0.0, 0.0, 0.7));
            draw_rectangle(p.x - w / 2.0, y, w * (e.hp.max(0) as f32 / e.max_hp as f32), 4.0 * lay.scale, Color::new(0.9, 0.15, 0.1, 1.0));
        }
    }
}

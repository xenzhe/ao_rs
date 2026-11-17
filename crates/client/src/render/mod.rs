//! Rendering. The world is drawn into an off-screen target with the official 23x18 tile
//! window, lit by a light map, then scaled to the screen; names and bars are drawn
//! afterwards in screen space so they stay crisp at any resolution.
//! - [`layout`]: where everything goes on screen
//! - [`world`]: map layers and characters
//! - [`lighting`]: day/night and light map
//! - [`fx`]: particles, projectiles, lightning, floating text
pub mod characters;
pub mod fx;
pub mod layout;
pub mod lighting;
pub mod weather;
pub mod world;

use crate::assets::Assets;
use crate::game::{v, Game};
use ao_protocol::EntityKind;
use fx::Fx;
use layout::{Layout, RT_H, RT_W, TILE};
use macroquad::prelude::*;
use world::WorldFades;

/// Player tile's top-left inside the world render target (centre column, middle row).
const PLAYER_ANCHOR: Vec2 = Vec2::new((ao_rules::VIEW_TILES_W / 2) as f32 * TILE, (ao_rules::VIEW_TILES_H / 2) as f32 * TILE - 16.0);

pub struct Renderer {
    world_rt: RenderTarget,
    light_rt: RenderTarget,
    pub fx: Fx,
    pub fades: WorldFades,
    pub pixel_perfect: bool,
    pub hour_offset: f32,
    map_shown: Option<u16>,
}

fn rt_camera(rt: &RenderTarget) -> Camera2D {
    Camera2D { zoom: vec2(2.0 / RT_W, 2.0 / RT_H), target: vec2(RT_W / 2.0, RT_H / 2.0), render_target: Some(rt.clone()), ..Default::default() }
}

impl Renderer {
    pub fn new(pixel_perfect: bool, hour_offset: f32) -> Renderer {
        let world_rt = render_target(RT_W as u32, RT_H as u32);
        world_rt.texture.set_filter(FilterMode::Nearest);
        let light_rt = render_target(RT_W as u32, RT_H as u32);
        light_rt.texture.set_filter(FilterMode::Linear);
        Renderer { world_rt, light_rt, fx: Fx::new(), fades: WorldFades::default(), pixel_perfect, hour_offset, map_shown: None }
    }

    /// World pixel shown at the top-left of the world view (camera follows the player).
    pub fn origin(game: &Game) -> Vec2 {
        let tl = game.me().map(|e| v(e.tile_tl())).unwrap_or(Vec2::ZERO);
        (tl - PLAYER_ANCHOR).round()
    }

    pub fn hour(&self) -> f32 {
        lighting::hour(self.hour_offset, get_time())
    }

    /// Draws the lit world into the off-screen target and blits it to `lay.world`.
    pub fn draw_world(&mut self, game: &Game, assets: &mut Assets, lay: &Layout, t_ms: f32, dt: f32) {
        let map_number = game.map.as_ref().map(|m| m.number);
        if map_number != self.map_shown {
            self.fades.clear();
            self.map_shown = map_number;
        }
        let origin = Self::origin(game);
        let dungeon = game.map.as_ref().is_some_and(|m| m.zone.eq_ignore_ascii_case("DUNGEON"));
        let amb = lighting::ambient(self.hour(), dungeon);

        set_camera(&rt_camera(&self.light_rt));
        lighting::build_light_map(game, &self.fx, origin, amb, t_ms);

        set_camera(&rt_camera(&self.world_rt));
        clear_background(BLACK);
        world::draw_world(game, assets, &mut self.fades, origin, t_ms, dt);
        self.fx.draw(origin);
        // Both targets share the same orientation: no flip between them.
        gl_use_material(&self.fx.multiply);
        draw_texture_ex(&self.light_rt.texture, 0.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(vec2(RT_W, RT_H)), ..Default::default() });
        gl_use_default_material();
        self.fx.draw_overlay(origin, vec4(0.0, 0.0, RT_W, RT_H));
        if game.casting || lay.world.contains(mouse_position().into()) {
            draw_target_square(game, lay, origin);
        }
        set_default_camera();

        // A Camera2D with positive zoom leaves the target upright: no flip_y here.
        draw_texture_ex(&self.world_rt.texture, lay.world.x, lay.world.y, WHITE, DrawTextureParams { dest_size: Some(vec2(lay.world.w, lay.world.h)), ..Default::default() });
        draw_rectangle_lines(lay.world.x - 2.0, lay.world.y - 2.0, lay.world.w + 4.0, lay.world.h + 4.0, 2.0 * lay.u, Color::new(0.62, 0.48, 0.24, 1.0));
        draw_names(game, lay, origin);
    }
}

fn draw_target_square(game: &Game, lay: &Layout, origin: Vec2) {
    let Some(wp) = lay.screen_to_world(mouse_position().into(), origin) else { return };
    let (tx, ty) = ((wp.x / TILE).floor(), (wp.y / TILE).floor());
    let sp = vec2(tx * TILE, ty * TILE) - origin;
    let (c, w) = if game.casting { (Color::new(1.0, 0.8, 0.2, 0.9), 2.0) } else { (Color::new(1.0, 1.0, 1.0, 0.25), 1.0) };
    draw_rectangle_lines(sp.x, sp.y, TILE, TILE, w, c);
}

/// Names and HP bars in screen space, crisp at any scale.
fn draw_names(game: &Game, lay: &Layout, origin: Vec2) {
    for (id, e) in &game.ents {
        let p = lay.world_to_screen(v(e.world_pos()), origin);
        if !lay.world.contains(p) {
            continue;
        }
        let size = (14.0 * lay.scale.max(1.0)).min(22.0 * lay.u);
        if e.kind == EntityKind::Player {
            let d = measure_text(&e.name, None, size as u16, 1.0);
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

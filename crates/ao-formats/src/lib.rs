//! Readers for Argentum Online 2.0 resources. Data only: no networking, no rendering,
//! no game rules, so any tool (client, server, map editor, converters) can reuse it.
//!
//! - `csm` — maps (`Mapas/mapaN.csm`)
//! - `graphics` — GRH table, bodies, heads, trees (`init/*.ini|dat`)
//! - `ini` — the INI dialect used by AO
//! - `paths` — locating the `Recursos` folder
pub mod csm;
pub mod dat;
pub mod graphics;
pub mod ini;
pub mod paths;

pub use csm::AoMap;
pub use graphics::GraphicsDb;
pub use paths::recursos_dir;

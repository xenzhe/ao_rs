//! Prints a summary of an AO map: `cargo run -p ao-formats --example csm_dump -- 1`
use ao_formats::csm::*;
use ao_formats::recursos_dir;

fn main() {
    let n: u16 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let m = AoMap::load(&recursos_dir(), n).expect("load map");
    println!("map {} '{}' safe={} zone={} terrain={} trailing={}", m.number, m.name, m.safe, m.zone, m.terrain, m.trailing);
    for l in 0..LAYERS {
        println!("  layer {} tiles={}", l + 1, m.layers[l].iter().filter(|g| **g > 0).count());
    }
    let water = m.blocked.iter().filter(|b| **b & FLAG_WATER != 0).count();
    let full = m.blocked.iter().filter(|b| **b & BLOCK_ALL == BLOCK_ALL).count();
    println!("  water={water} fully_blocked={full} lights={} particles={} npcs={}", m.lights.len(), m.particles.len(), m.npcs.len());
    for e in m.exits.iter().take(8) {
        println!("  exit ({},{}) -> map {} ({},{})", e.x, e.y, e.map, e.dest_x, e.dest_y);
    }
    println!("  exits total {}", m.exits.len());
}

//! Lists the maps with entry restrictions (`restrict_mode`, min/max level).
//! `cargo run --example map_rules -- <recursos>`
fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    for n in 1..=900u16 {
        let Ok(m) = ao_formats::AoMap::load(&root, n) else { continue };
        if m.restrict != 0 || m.min_level != 0 || m.max_level != 0 {
            println!("{n:>4} {:<32} restrict {:>3} levels {}..{}", m.name, m.restrict, m.min_level, m.max_level);
        }
    }
}

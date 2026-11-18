//! Lists the NPCs (and optionally the objects of one obj.dat type) of a map.
//! `cargo run --example map_npcs -- <recursos> <map> [filter]`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(&a[1]);
    let n: u16 = a[2].parse().unwrap();
    let filter = a.get(3).map(|s| s.to_lowercase()).unwrap_or_default();
    let npcs = ao_formats::dat::npcs::load(&root).unwrap().data;
    let objs = ao_formats::dat::obj::load(&root).unwrap().data;
    let map = ao_formats::AoMap::load(&root, n).unwrap();
    for m in &map.npcs {
        let name = npcs.get(m.npc as u32).map(|d| format!("{} {:?}", d.name, d.npc_type)).unwrap_or_default();
        if name.to_lowercase().contains(&filter) {
            println!("npc {} '{}' at {},{}", m.npc, name, m.x, m.y);
        }
    }
    if !filter.is_empty() {
        for o in ao_formats::dat::craft::map_objects(&root, n) {
            let Some(d) = objs.get(o.obj as u32) else { continue };
            if format!("{:?} {}", d.obj_type, d.name).to_lowercase().contains(&filter) {
                println!("obj {} '{}' at {},{}", o.obj, d.name, o.x, o.y);
            }
        }
    }
}

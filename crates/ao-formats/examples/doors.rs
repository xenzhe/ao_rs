//! Lists the door objects of a map and what the map layers hold on their tiles.
//! `cargo run --example doors -- <recursos> <map>`
use ao_formats::dat::obj::ObjType;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(&a[1]);
    let n: u16 = a[2].parse().unwrap();
    let objs = ao_formats::dat::obj::load(&root).unwrap().data;
    let map = ao_formats::csm::AoMap::load(&root, n).unwrap();
    let on = ao_formats::dat::craft::map_objects(&root, n);
    let mut types = std::collections::BTreeMap::new();
    for o in &on {
        let Some(d) = objs.get(o.obj as u32) else { continue };
        *types.entry(format!("{:?}", d.obj_type)).or_insert(0) += 1;
        if d.obj_type == ObjType::Doors {
            let t = ao_formats::csm::tile(o.x, o.y);
            println!(
                "door obj {} '{}' at {},{} grh {} sub {} | blocked {:#x} below {:#x} | layers {:?}",
                o.obj,
                d.name,
                o.x,
                o.y,
                d.grh,
                d.subtype,
                map.blocked[t],
                map.blocked[ao_formats::csm::tile(o.x, o.y + 1)],
                [map.layers[0][t], map.layers[1][t], map.layers[2][t], map.layers[3][t]]
            );
        }
    }
    println!("{types:?}");
}

//! Lists the teleport objects of every map and whether their tile has a map exit.
//! `cargo run --example teleports -- <recursos>`
use ao_formats::dat::obj::ObjType;

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let objs = ao_formats::dat::obj::load(&root).unwrap().data;
    let (mut with, mut without) = (0, 0);
    let mut subtypes = std::collections::BTreeMap::new();
    for n in 1..=900u16 {
        let Ok(m) = ao_formats::AoMap::load(&root, n) else { continue };
        for o in ao_formats::dat::craft::map_objects(&root, n) {
            let Some(d) = objs.get(o.obj as u32).filter(|d| d.obj_type == ObjType::Teleport) else { continue };
            *subtypes.entry((d.subtype, o.obj, d.name.clone())).or_insert(0) += 1;
            match m.exit(o.x, o.y) {
                Some(e) => {
                    with += 1;
                    if with <= 5 {
                        println!("map {n} ({},{}) '{}' -> {} ({},{})", o.x, o.y, d.name, e.map, e.dest_x, e.dest_y);
                    }
                }
                None => {
                    without += 1;
                    if without <= 5 {
                        println!("map {n} ({},{}) '{}' WITHOUT exit", o.x, o.y, d.name);
                    }
                }
            }
        }
    }
    println!("with exit {with}, without {without}");
    for ((s, id, name), c) in subtypes {
        println!("subtype {s} obj {id} '{name}' x{c}");
    }
}

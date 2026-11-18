//! Counts the tree-like map objects of every map by obj.dat index.
//! `cargo run --example trees -- <recursos>`
use ao_formats::dat::obj::ObjType;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(&a[1]);
    let objs = ao_formats::dat::obj::load(&root).unwrap().data;
    let mut count = std::collections::BTreeMap::new();
    for m in 1..=900u16 {
        for o in ao_formats::dat::craft::map_objects(&root, m) {
            let Some(d) = objs.get(o.obj as u32) else { continue };
            if matches!(d.obj_type, ObjType::Trees | ObjType::OreDeposit) {
                let e = count.entry((format!("{:?}", d.obj_type), o.obj, d.name.clone())).or_insert((0, 0u16));
                e.0 += 1;
                e.1 = m;
            }
        }
    }
    for ((t, id, name), (n, last)) in count {
        println!("{t} {id} '{name}' x{n} (e.g. map {last})");
    }
}

//! Doors placed on maps whose key is an inventory key (`Clave` >= 1000), with that key.
//! `cargo run --example key_doors -- <recursos>`
use ao_formats::dat::obj::ObjType;

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let objs = ao_formats::dat::obj::load(&root).unwrap().data;
    let mut shown = 0;
    for n in 1..=900u16 {
        for o in ao_formats::dat::craft::map_objects(&root, n) {
            let Some(d) = objs.get(o.obj as u32).filter(|d| d.obj_type == ObjType::Doors && d.key_code >= 1000) else { continue };
            let key = objs.iter().find(|(_, k)| k.obj_type == ObjType::Keys && k.key_code == d.key_code).map(|(i, k)| (i, k.name.clone()));
            println!("map {n} ({},{}) door {} '{}' clave {} locked {} key {:?}", o.x, o.y, o.obj, d.name, d.key_code, d.key_obj, key);
            shown += 1;
            if shown > 12 {
                return;
            }
        }
    }
}

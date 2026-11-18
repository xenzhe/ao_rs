//! Audit of a converted resource folder (TSAO by default): finds the kind of bug a player
//! would hit walking around instead of waiting for them to report it.
//! `cargo run -p ao-formats --example world_audit -- [recursos]`
//!
//! - map exits outside the walkable border, or to maps/tiles that do not exist;
//! - map graphics that do not resolve, or whose frame falls outside its image;
//! - NPCs placed on maps that npcs.dat lacks, or whose body/head has no graphic;
//! - objects whose inventory graphic does not resolve;
//! - sound ids the engine plays that the folder lacks.
use ao_formats::csm::{AoMap, MAP_SIZE};
use ao_formats::graphics::GraphicsDb;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "X:/dev/tsao_rec".into()));
    let db = GraphicsDb::load(&root);
    let mut sizes: HashMap<u32, Option<(u32, u32)>> = HashMap::new();
    let mut size = |f: u32| -> Option<(u32, u32)> {
        *sizes.entry(f).or_insert_with(|| image_size(&root.join("Graficos").join(format!("{f}.png"))))
    };
    let mut problems: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut add = |k: &'static str, s: String| problems.entry(k).or_default().push(s);

    let mut maps = BTreeMap::new();
    for e in std::fs::read_dir(root.join("Mapas")).unwrap().flatten() {
        let n = e.file_name().to_string_lossy().to_lowercase();
        if let Some(num) = n.strip_prefix("mapa").and_then(|s| s.strip_suffix(".csm")).and_then(|s| s.parse::<u16>().ok()) {
            match AoMap::load(&root, num) {
                Ok(m) => {
                    maps.insert(num, m);
                }
                Err(e) => add("mapa ilegible", format!("{num}: {e:?}")),
            }
        }
    }
    let npcs = ao_formats::dat::npcs::load(&root).map(|l| l.data).ok();
    let objs = ao_formats::dat::obj::load(&root).map(|l| l.data).ok();
    let mut bad_grh: BTreeMap<u32, u32> = BTreeMap::new();
    for (num, m) in &maps {
        let (x0, x1, y0, y1) = m.walk_bounds.unwrap_or((12, 89, 10, 91));
        for ex in &m.exits {
            if !(x0..=x1).contains(&ex.x) || !(y0..=y1).contains(&ex.y) {
                add("salida fuera del borde caminable", format!("mapa {num} ({}, {}) -> {}", ex.x, ex.y, ex.map));
            }
            match maps.get(&ex.map) {
                None => add("salida a un mapa que no existe", format!("mapa {num} ({}, {}) -> {}", ex.x, ex.y, ex.map)),
                Some(d) => {
                    if !ao_formats::csm::on_map(ex.dest_x, ex.dest_y) || d.blocked(ex.dest_x, ex.dest_y) & ao_formats::csm::BLOCK_ALL == ao_formats::csm::BLOCK_ALL {
                        add("salida a una casilla bloqueada", format!("mapa {num} ({}, {}) -> {} ({}, {})", ex.x, ex.y, ex.map, ex.dest_x, ex.dest_y));
                    }
                }
            }
        }
        for l in 0..ao_formats::csm::LAYERS {
            for i in 0..(MAP_SIZE * MAP_SIZE) as usize {
                let g = m.layers[l][i];
                if g == 0 {
                    continue;
                }
                let ok = db.grh(g).is_some_and(|gr| {
                    gr.frames.iter().all(|f| db.grh(*f).is_some_and(|fr| size(fr.file).is_some_and(|(w, h)| fr.sx + fr.w <= w as f32 && fr.sy + fr.h <= h as f32)))
                });
                if !ok {
                    *bad_grh.entry(g).or_default() += 1;
                }
            }
        }
        if let Some(npcs) = &npcs {
            for n in &m.npcs {
                match npcs.get(n.npc as u32) {
                    None => add("NPC en mapa que no existe en npcs.dat", format!("mapa {num} ({}, {}) NPC {}", n.x, n.y, n.npc)),
                    Some(d) => {
                        if d.body > 0 && !db.bodies.contains_key(&(d.body as u16)) {
                            add("NPC sin gráfico de cuerpo", format!("NPC {} {} cuerpo {}", n.npc, d.name, d.body));
                        }
                        if d.head > 0 && !db.heads.contains_key(&(d.head as u16)) {
                            add("NPC sin gráfico de cabeza", format!("NPC {} {} cabeza {}", n.npc, d.name, d.head));
                        }
                    }
                }
            }
        }
    }
    for (g, n) in &bad_grh {
        add("gráfico de mapa roto (no existe o se sale de su imagen)", format!("grh {g} en {n} casillas"));
    }
    if let Some(objs) = &objs {
        for (i, o) in objs.iter() {
            if o.grh > 0 && db.grh(o.grh as u32).is_none() {
                add("objeto sin gráfico", format!("obj {i} {} grh {}", o.name, o.grh));
            }
        }
    }
    for s in [2u16, 10, 23, 24, 50, 191, 194] {
        if !root.join("wav").join(format!("{s}.wav")).exists() {
            add("sonido del motor que falta", format!("{s}.wav"));
        }
    }
    println!("Auditoría de {}: {} mapas", root.display(), maps.len());
    if problems.is_empty() {
        println!("Sin problemas.");
    }
    for (k, v) in &problems {
        println!("\n## {k}: {}", v.len());
        for s in v.iter().take(12) {
            println!("  {s}");
        }
        if v.len() > 12 {
            println!("  ... y {} más", v.len() - 12);
        }
    }
}

fn image_size(p: &std::path::Path) -> Option<(u32, u32)> {
    // PNG: width and height are big-endian at bytes 16..24.
    let b = std::fs::read(p).ok()?;
    (b.len() > 24 && &b[1..4] == b"PNG").then(|| (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap())))
}

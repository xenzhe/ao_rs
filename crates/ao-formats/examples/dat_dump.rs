//! Summary of the official data files: `cargo run --release -p ao-formats --example dat_dump`
//! (add `-- -v` to print every warning, `-- <dir>` to read another `Recursos` folder).
use ao_formats::dat::balance::{self, Class, Race};
use ao_formats::dat::{npcs, obj, spells, Warning};
use ao_formats::recursos_dir;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn warnings(name: &str, w: &[Warning], verbose: bool) {
    println!("  warnings: {}", w.len());
    let mut by_key: BTreeMap<&str, usize> = BTreeMap::new();
    for x in w {
        *by_key.entry(x.key.as_str()).or_default() += 1;
    }
    let mut top: Vec<_> = by_key.into_iter().collect();
    top.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    for (key, n) in top.iter().take(8) {
        println!("    {n:>5} x {}", if key.is_empty() { "(section-level)" } else { key });
    }
    if verbose {
        for x in w {
            println!("    {x}");
        }
    }
    let _ = name;
}

fn main() {
    let mut verbose = false;
    let mut root = recursos_dir();
    for a in std::env::args().skip(1) {
        if a == "-v" {
            verbose = true;
        } else {
            root = PathBuf::from(a);
        }
    }
    println!("Recursos: {}", root.display());

    match obj::load(&root) {
        Ok(l) => {
            let t = &l.data;
            println!("obj.dat: {} objects (NumOBJs={}, highest id {})", t.len(), t.declared, t.max_id());
            let mut by_type: BTreeMap<i32, usize> = BTreeMap::new();
            for (_, o) in t.iter() {
                *by_type.entry(o.obj_type.id()).or_default() += 1;
            }
            let list: Vec<String> = by_type.iter().map(|(k, v)| format!("{k}:{v}")).collect();
            println!("  by ObjType: {}", list.join(" "));
            let named = t.iter().filter(|(_, o)| !o.name.is_empty()).count();
            let recipes = t.iter().filter(|(_, o)| o.recipe.is_some()).count();
            println!("  named: {named}, with recipe: {recipes}");
            warnings("obj", &l.warnings, verbose);
        }
        Err(e) => println!("obj.dat: {e}"),
    }

    match npcs::load(&root) {
        Ok(l) => {
            let t = &l.data;
            println!("npcs.dat: {} NPCs (NumNPCs={}, highest id {})", t.len(), t.declared, t.max_id());
            let mut by_type: BTreeMap<i32, usize> = BTreeMap::new();
            let mut hostile = 0;
            let mut vendors = 0;
            let mut casters = 0;
            for (_, n) in t.iter() {
                *by_type.entry(n.npc_type.id()).or_default() += 1;
                hostile += (n.hostile > 0) as usize;
                vendors += (n.trades > 0) as usize;
                casters += (!n.spells.is_empty()) as usize;
            }
            let list: Vec<String> = by_type.iter().map(|(k, v)| format!("{k}:{v}")).collect();
            println!("  by NpcType: {}", list.join(" "));
            println!("  hostile: {hostile}, vendors: {vendors}, casters: {casters}");
            warnings("npcs", &l.warnings, verbose);
        }
        Err(e) => println!("npcs.dat: {e}"),
    }

    match spells::load(&root) {
        Ok(l) => {
            let t = &l.data;
            println!("Hechizos.dat: {} spells (NumeroHechizos={}, highest id {})", t.len(), t.declared, t.max_id());
            let mut by_type: BTreeMap<i32, usize> = BTreeMap::new();
            for (_, s) in t.iter() {
                *by_type.entry(s.spell_type.id()).or_default() += 1;
            }
            let list: Vec<String> = by_type.iter().map(|(k, v)| format!("{k}:{v}")).collect();
            println!("  by Tipo: {}", list.join(" "));
            warnings("spells", &l.warnings, verbose);
        }
        Err(e) => println!("Hechizos.dat: {e}"),
    }

    match balance::load(&root) {
        Ok(l) => {
            let b = &l.data;
            println!("Balance.dat: {} classes, {} races, {} levels of experience", Class::ALL.len(), Race::ALL.len(), b.exp_level_up.len());
            for c in Class::ALL {
                let m = b.class(c);
                println!("  {:<10} life {:>5.2} mana0 {:>5.2} evasion {:.2} melee {:.2}", c.spanish_name(), m.life, m.initial_mana, m.evasion, m.melee_attack);
            }
            for r in Race::ALL {
                let m = b.race(r);
                println!("  {:<12} STR {:+} AGI {:+} INT {:+} CHA {:+} CON {:+}", r.spanish_name(), m.strength, m.agility, m.intelligence, m.charisma, m.constitution);
            }
            warnings("balance", &l.warnings, verbose);
        }
        Err(e) => println!("Balance.dat: {e}"),
    }
}

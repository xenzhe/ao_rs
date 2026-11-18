//! Counts tile triggers over every map and shows one example tile of each.
//! `cargo run --example triggers -- <recursos>`
fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let mut seen: std::collections::BTreeMap<u16, (usize, (u16, i32, i32))> = Default::default();
    for n in 1..=900u16 {
        let Ok(m) = ao_formats::AoMap::load(&root, n) else { continue };
        for y in 1..=100 {
            for x in 1..=100 {
                let t = m.trigger(x, y);
                if t != 0 {
                    let e = seen.entry(t).or_insert((0, (n, x, y)));
                    e.0 += 1;
                }
            }
        }
    }
    for (t, (count, at)) in seen {
        println!("trigger {t:>3}: {count:>6} tiles, e.g. {at:?}");
    }
}

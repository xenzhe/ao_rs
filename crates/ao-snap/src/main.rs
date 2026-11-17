//! `ao-snap --map 1 --at 50,50 --out x.png [--scale 2] [--size 23,18] [--t MS] [--entity body,head,heading@x,y ...]`
use ao_formats::csm::AoMap;
use ao_snap::{find_recursos, Snapshot, SnapEntity};

fn parse_pair(s: &str, what: &str) -> Result<(i32, i32), String> {
    let (a, b) = s.split_once(',').ok_or_else(|| format!("{what}: expected a,b"))?;
    Ok((a.trim().parse().map_err(|_| format!("{what}: bad number"))?, b.trim().parse().map_err(|_| format!("{what}: bad number"))?))
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (mut map_n, mut at, mut out, mut scale, mut size, mut t_ms) = (1u16, (50, 50), String::from("snap.png"), 1u32, (23, 18), 0f32);
    let mut entities = Vec::new();
    while let Some(a) = args.next() {
        let mut val = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match a.as_str() {
            "--map" => map_n = val("--map")?.parse().map_err(|_| "--map: bad number")?,
            "--at" => at = parse_pair(&val("--at")?, "--at")?,
            "--out" => out = val("--out")?,
            "--scale" => scale = val("--scale")?.parse().map_err(|_| "--scale: bad number")?,
            "--size" => size = parse_pair(&val("--size")?, "--size")?,
            "--t" => t_ms = val("--t")?.parse().map_err(|_| "--t: bad number")?,
            "--entity" => entities.push(SnapEntity::parse(&val("--entity")?)?),
            "-h" | "--help" => {
                println!("ao-snap --map N --at X,Y --out FILE.png [--scale K] [--size W,H] [--t MS] [--entity body,head,heading@x,y]...");
                return Ok(());
            }
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    let root = find_recursos().ok_or("AO20 resources not found (set AO_RECURSOS)")?;
    let map = AoMap::load(&root, map_n).map_err(|e| format!("map {map_n}: {e:?}"))?;
    let started = std::time::Instant::now();
    let mut snap = Snapshot::new(root).with_scale(scale);
    let img = snap.render(&map, at, size, &entities, t_ms);
    img.save(&out).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("{out}: {}x{} in {:?}", img.width(), img.height(), started.elapsed());
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ao-snap: {e}");
        std::process::exit(1);
    }
}

//! P4 crafting with bots: characters prepared in a file database (skills, class, level,
//! the tool equipped, materials, learnt recipes) open a trade and build. Traps: building
//! without materials, without skill, without the recipe, an object outside the trade's
//! list, absurd amounts, far from the anvil, without the tool. Plus the official anvil of
//! Ullathorpe and the skill raised by use.
use ao_protocol::p4::craft::Trade;
use ao_protocol::{CraftC2S, C2S};
use ao_rules::craft::{IRON_INGOT, WOOD};
use ao_rules::{Dir, WALK_MS};
use ao_testkit::{Bot, TestServer};
use server::db::{Db, NewCharacter};
use std::path::{Path, PathBuf};

const PASS: &str = "secreta1";
/// The anvil of Ullathorpe (map 1), read from mapa1.csm.
const ANVIL: (i32, i32) = (34, 65);
const SAW: u16 = 198;
const HAMMER: u16 = 390;
const POT: u16 = 887;
const SEWING_BOX: u16 = 886;
// Skill indexes (e_Skill - 1).
const SMITH: usize = 19;
const CARPENTRY: usize = 20;
const ALCHEMY: usize = 21;
const TAILOR: usize = 22;
// Classes.
const WORKER: u8 = 9;

struct Pj<'a> {
    name: &'a str,
    class: u8,
    level: u32,
    skills: &'a [(usize, u8)],
    /// (obj, amount, equipped), slots 1..
    items: &'a [(u16, u32, bool)],
    spells: &'a [u16],
    pos: (i32, i32),
}

fn db_path(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ao_rs_p4_craft_{tag}_{}.db", std::process::id()));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", p.display()));
    }
    p
}

fn seed(path: &Path, pjs: &[Pj]) {
    let db = Db::open(path.to_str().unwrap()).unwrap();
    let raw = rusqlite::Connection::open(path).unwrap();
    for pj in pjs {
        let acc = db.create_account(&pj.name.to_lowercase(), PASS).unwrap();
        let c = db
            .create_character(acc.id, &NewCharacter { name: pj.name, race: 1, class: pj.class, gender: 1, body: 1, head: 1, hp: 30, mana: 0, map: 1, x: pj.pos.0, y: pj.pos.1 })
            .unwrap();
        let mut skills = [0u8; 24];
        for &(i, v) in pj.skills {
            skills[i] = v;
        }
        let skills = skills.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(",");
        raw.execute("UPDATE character SET level = ?1, skills = ?2, sta = 200, max_sta = 200 WHERE id = ?3", rusqlite::params![pj.level, skills, c.id]).unwrap();
        for (i, &(obj, amount, eq)) in pj.items.iter().enumerate() {
            raw.execute("INSERT INTO inventory_item (character_id, slot, obj, amount, equipped) VALUES (?1, ?2, ?3, ?4, ?5)", rusqlite::params![c.id, i as i64 + 1, obj, amount, eq as i64]).unwrap();
        }
        for (i, &s) in pj.spells.iter().enumerate() {
            raw.execute("INSERT INTO spell (character_id, slot, spell) VALUES (?1, ?2, ?3)", rusqlite::params![c.id, i as i64 + 1, s]).unwrap();
        }
    }
}

fn server(path: &Path) -> TestServer {
    let path = path.to_str().unwrap().to_string();
    TestServer::with(move |st| {
        st.npcs = false;
        st.start = Some((1, 50, 50));
        st.config.database = path;
    })
}

fn enter(s: &TestServer, name: &str) -> Bot {
    let mut b = s.connect();
    assert!(b.account_login(&name.to_lowercase(), PASS), "login {name}");
    b.play(name);
    b.wait_until("en el mundo", |b| b.game.me().is_some());
    s.settle();
    b.sync();
    b
}

fn craft(b: &mut Bot, m: CraftC2S) {
    b.send(&C2S::Craft(m));
    b.sync();
}

/// One build interval (`IntervaloTrabajarConstruir`).
fn step(s: &TestServer, b: &mut Bot) {
    s.advance(500);
    b.sync();
}

fn said(b: &Bot, text: &str) -> bool {
    b.console().any(|t| t.contains(text))
}

fn has(b: &Bot, obj: u16) -> u32 {
    b.game.local.count_of(obj)
}

fn listed(b: &Bot, obj: u16) -> bool {
    b.game.local.p4.craft.recipe(obj).is_some()
}

#[test]
fn carpenter_worker_builds_a_batch_and_the_traps_fail() {
    let path = db_path("carp");
    seed(&path, &[Pj { name: "Carpintero", class: WORKER, level: 30, skills: &[(CARPENTRY, 50)], items: &[(SAW, 1, true), (WOOD, 1000, false)], spells: &[], pos: (50, 50) }]);
    let s = server(&path);
    let mut b = enter(&s, "Carpintero");

    craft(&mut b, CraftC2S::Open { trade: Trade::Carpenter, station: None });
    let open = b.game.local.p4.craft.open.clone().expect("carpinteria abierta");
    assert_eq!(open.trade, Trade::Carpenter);
    assert_eq!(open.per_step, 5, "trabajador con 50: skill/10");
    assert!(listed(&b, 163) && listed(&b, 479), "cuchara y arco simple (15)");
    assert!(!listed(&b, 1869), "arco maestro pide 75");
    let bow = b.game.local.p4.craft.recipe(479).unwrap().clone();
    assert_eq!(bow.materials, vec![(WOOD, 200)]);

    // Three bows in one step (batch of 5): 600 wood, 2 stamina.
    let sta = b.game.local.sta;
    craft(&mut b, CraftC2S::Build { obj: 479, amount: 3 });
    assert_eq!(has(&b, 479), 3, "tres arcos");
    assert_eq!(has(&b, WOOD), 400);
    assert_eq!(b.game.local.sta, sta - 2);
    assert!(b.game.local.p4.craft.order.is_none(), "orden terminada");

    // Trap: not enough materials (5 bows = 1000 wood): nothing is spent. (The next step
    // waits for the 500 ms interval.)
    craft(&mut b, CraftC2S::Build { obj: 479, amount: 5 });
    step(&s, &mut b);
    assert!(said(&b, "No tenés suficiente madera."));
    assert_eq!((has(&b, 479), has(&b, WOOD)), (3, 400));

    // Trap: in the list but above the skill.
    craft(&mut b, CraftC2S::Build { obj: 1869, amount: 1 });
    step(&s, &mut b);
    assert!(said(&b, "No tenés los conocimientos suficientes"));
    assert_eq!(has(&b, 1869), 0);
    assert_eq!(has(&b, WOOD), 400);

    // Traps that only a lying client sends: an object of another trade (Daga +1 is the
    // blacksmith's), amount 0 and an absurd amount. Strikes, nothing built; enough of
    // them end in a kick.
    craft(&mut b, CraftC2S::Build { obj: 165, amount: 1 });
    craft(&mut b, CraftC2S::Build { obj: 479, amount: 0 });
    craft(&mut b, CraftC2S::Build { obj: 479, amount: u32::MAX });
    assert_eq!((has(&b, 165), has(&b, 479), has(&b, WOOD)), (0, 3, 400));
    assert!(b.game.me().is_some(), "tres strikes no expulsan");
    for _ in 0..6 {
        b.send(&C2S::Craft(CraftC2S::Build { obj: 479, amount: 0 }));
    }
    b.wait_until("expulsado por cantidades absurdas", |b| b.closed());
}

#[test]
fn carpenter_order_runs_one_step_per_interval_and_raises_the_skill() {
    let path = db_path("raise");
    // A level 1 worker (cap 3 points; skill 0: batch of 1): 30 spoons, one every 500 ms.
    // Only workers can hold the tools (obj.dat CP1..CP12 forbid every other class).
    seed(&path, &[Pj { name: "Aprendiz", class: WORKER, level: 1, skills: &[], items: &[(SAW, 1, true), (WOOD, 200, false)], spells: &[], pos: (50, 50) }]);
    let s = server(&path);
    let mut b = enter(&s, "Aprendiz");
    craft(&mut b, CraftC2S::Open { trade: Trade::Carpenter, station: None });
    assert_eq!(b.game.local.p4.craft.open.as_ref().unwrap().per_step, 1);
    craft(&mut b, CraftC2S::Build { obj: 163, amount: 30 });
    assert_eq!(has(&b, 163), 1, "el primer paso es inmediato");
    s.advance(400);
    b.sync();
    assert_eq!(has(&b, 163), 1, "intervalo de 500 ms");
    for _ in 0..29 {
        s.advance(500);
        b.sync();
    }
    assert_eq!(has(&b, 163), 30);
    assert_eq!(has(&b, WOOD), 200 - 90);
    let skill = b.game.local.skills[CARPENTRY];
    assert!((1..=3).contains(&skill), "sube por uso hasta el tope del nivel 1: {skill}");
    assert!(said(&b, "¡Has mejorado tu habilidad Carpintería en un punto!"));

    // Trap: new orders do not skip the interval (it is the player's, not the order's):
    // five orders at the same instant right after the last step build nothing yet, and
    // then one step's worth.
    for _ in 0..5 {
        b.send(&C2S::Craft(CraftC2S::Build { obj: 163, amount: 1 }));
    }
    b.sync();
    assert_eq!(has(&b, 163), 30, "el intervalo sigue corriendo");
    step(&s, &mut b);
    s.advance(1000);
    b.sync();
    assert_eq!(has(&b, 163), 31, "un solo paso");

    // Stop: an order in progress halts.
    craft(&mut b, CraftC2S::Build { obj: 163, amount: 5 });
    craft(&mut b, CraftC2S::Stop);
    s.advance(2000);
    b.sync();
    assert_eq!(has(&b, 163), 32, "solo el paso inmediato");

    // Without the saw in the hand the order cannot even open.
    let mut c = s.connect();
    c.login("SinSerrucho");
    c.wait_until("en el mundo", |b| b.game.me().is_some());
    craft(&mut c, CraftC2S::Open { trade: Trade::Carpenter, station: None });
    assert!(said(&c, "Antes de usar la herramienta deberías equipártela."));
    assert!(c.game.local.p4.craft.open.is_none());
}

#[test]
fn blacksmith_at_the_ullathorpe_anvil() {
    let path = db_path("smith");
    seed(
        &path,
        &[
            Pj { name: "Herrero", class: WORKER, level: 30, skills: &[(SMITH, 60)], items: &[(HAMMER, 1, true), (IRON_INGOT, 200, false)], spells: &[], pos: (ANVIL.0, ANVIL.1 + 2) },
            Pj { name: "Desarmado", class: WORKER, level: 30, skills: &[(SMITH, 60)], items: &[(SAW, 1, true), (IRON_INGOT, 200, false)], spells: &[], pos: (ANVIL.0 + 1, ANVIL.1 + 1) },
        ],
    );
    let s = server(&path);
    let mut b = enter(&s, "Herrero");
    let at = b.pos();
    let d = (at.0 - ANVIL.0).abs() + (at.1 - ANVIL.1).abs();
    assert!(d <= 2, "empieza junto al yunque: {at:?}");

    // Trap: a tile without an anvil.
    craft(&mut b, CraftC2S::Open { trade: Trade::Blacksmith, station: Some((at.0, at.1)) });
    assert!(said(&b, "No hay ningún yunque ahí."));
    // Trap: the blacksmith without a station.
    craft(&mut b, CraftC2S::Open { trade: Trade::Blacksmith, station: None });
    assert!(said(&b, "Debes hacer click derecho sobre el yunque."));
    assert!(b.game.local.p4.craft.open.is_none());

    craft(&mut b, CraftC2S::Open { trade: Trade::Blacksmith, station: Some(ANVIL) });
    assert_eq!(b.game.local.p4.craft.open.as_ref().map(|o| o.trade), Some(Trade::Blacksmith));
    assert!(listed(&b, 165), "Daga +1 (15)");
    assert!(listed(&b, 1278), "Casco de hierro: 20 <= round(60 / ModHerreria 1)");
    assert!(!listed(&b, 1761), "Casco reforzado: 63");
    assert!(!listed(&b, 5170), "runa: 63");

    // Two daggers: one now, one 500 ms later.
    craft(&mut b, CraftC2S::Build { obj: 165, amount: 2 });
    assert_eq!(has(&b, 165), 1);
    s.advance(500);
    b.sync();
    assert_eq!((has(&b, 165), has(&b, IRON_INGOT)), (2, 140));

    // Walking away from the anvil closes the forge and stops the order.
    step(&s, &mut b);
    craft(&mut b, CraftC2S::Build { obj: 165, amount: 3 });
    assert_eq!(has(&b, 165), 3);
    for _ in 0..4 {
        b.walk(Dir::Down);
        s.advance(WALK_MS);
        b.sync();
    }
    s.advance(600);
    b.sync();
    let far = b.pos();
    assert!((far.0 - ANVIL.0).abs() + (far.1 - ANVIL.1).abs() > 2, "se alejo: {far:?}");
    assert!(b.game.local.p4.craft.open.is_none(), "la forja se cierra");
    assert!(said(&b, "Estás muy lejos."));
    assert!(has(&b, 165) < 5, "la orden no termina lejos del yunque");
    // Trap: opening from afar.
    craft(&mut b, CraftC2S::Open { trade: Trade::Blacksmith, station: Some(ANVIL) });
    assert!(b.game.local.p4.craft.open.is_none());

    // Without the hammer (a saw in the hand).
    let mut c = enter(&s, "Desarmado");
    craft(&mut c, CraftC2S::Open { trade: Trade::Blacksmith, station: Some(ANVIL) });
    assert!(said(&c, "La herramienta que tienes no es la correcta"));
    assert!(c.game.local.p4.craft.open.is_none());
}

#[test]
fn alchemy_needs_the_recipe_and_tailoring_the_skill() {
    let path = db_path("alch");
    let potion_mats: &[(u16, u32, bool)] = &[(POT, 1, true), (4316, 5, false), (3075, 5, false), (4997, 5, false)];
    seed(
        &path,
        &[
            Pj { name: "Druida", class: WORKER, level: 30, skills: &[(ALCHEMY, 20)], items: potion_mats, spells: &[285], pos: (50, 50) },
            Pj { name: "Olvidadizo", class: WORKER, level: 30, skills: &[(ALCHEMY, 20)], items: potion_mats, spells: &[], pos: (52, 50) },
            Pj { name: "Sastre", class: WORKER, level: 30, skills: &[(TAILOR, 0)], items: &[(SEWING_BOX, 1, true), (414, 100, false)], spells: &[], pos: (54, 50) },
        ],
    );
    let s = server(&path);

    let mut a = enter(&s, "Druida");
    craft(&mut a, CraftC2S::Open { trade: Trade::Alchemy, station: None });
    assert!(listed(&a, 5997), "pocion de vida (0)");
    assert!(listed(&a, 5996), "pocion de mana: 20 <= 20 \\ ModAlquimia 1");
    let r = a.game.local.p4.craft.recipe(5997).unwrap().clone();
    assert_eq!(r.materials.len(), 3, "mortero, frasco y flor roja: {:?}", r.materials);
    craft(&mut a, CraftC2S::Build { obj: 5997, amount: 2 });
    s.advance(500);
    a.sync();
    assert_eq!(has(&a, 5997), 2);
    assert_eq!((has(&a, 4316), has(&a, 3075), has(&a, 4997)), (3, 3, 3));

    // Trap: the same potion without the recipe learnt.
    let mut o = enter(&s, "Olvidadizo");
    craft(&mut o, CraftC2S::Open { trade: Trade::Alchemy, station: None });
    craft(&mut o, CraftC2S::Build { obj: 5997, amount: 1 });
    assert!(said(&o, "no aprendiste la receta"));
    assert_eq!((has(&o, 5997), has(&o, 4316)), (0, 5));
    // Trap: a trade whose tool is not in the hand.
    craft(&mut o, CraftC2S::Open { trade: Trade::Tailor, station: None });
    assert_eq!(o.game.local.p4.craft.open.as_ref().map(|t| t.trade), Some(Trade::Alchemy));

    // Tailoring with skill 0: worked cloth yes, leather armor (25) no.
    let mut t = enter(&s, "Sastre");
    craft(&mut t, CraftC2S::Open { trade: Trade::Tailor, station: None });
    assert!(listed(&t, 3578) && !listed(&t, 30));
    craft(&mut t, CraftC2S::Build { obj: 30, amount: 1 });
    assert!(said(&t, "No tenés los conocimientos suficientes"));
    assert_eq!((has(&t, 30), has(&t, 414)), (0, 100));
    craft(&mut t, CraftC2S::Build { obj: 3578, amount: 1 });
    step(&s, &mut t);
    assert_eq!((has(&t, 3578), has(&t, 414)), (1, 99));
    // Build without anything open (closed window).
    craft(&mut t, CraftC2S::Close);
    craft(&mut t, CraftC2S::Build { obj: 3578, amount: 1 });
    assert!(said(&t, "No estás trabajando en ningún oficio."));
    assert_eq!(has(&t, 3578), 1);
}

//! TSAO pets, summons and creature fights over the wire with the converted Tierras Sagradas
//! data (`tools/tsao/build.cmd <dir>`, read from `AO_TSAO`; skipped when it is not there):
//! taming, following, dismissing, summoning elementals that fight for their owner and
//! vanish, the one-per-kind rule, companions, pets travelling with the owner, paralysis and
//! spell immunity on creatures, and the dragon slayer.
use ao_protocol::p4::skill::SkillC2S;
use ao_protocol::C2S;
use ao_testkit::{Bot, TestServer};
use std::path::PathBuf;

/// Afueras de Kahlimdor: open field where fighting is allowed.
const MAP: u16 = 1;
const TAMING: u8 = 24;
const CHARISMA: u8 = 4;
const HORMIGA: u16 = 76;
const GALLO: u16 = 2;
const CERDO: u16 = 3;
const INVOCAR_FUEGO: u16 = 26;
const INVOCAR_TIERRA: u16 = 28;
const INVOCAR_MASCOTA: u16 = 51;
const PARALIZAR: u16 = 9;
const RELAMPAGO: u16 = 12;
/// `ExclusivoClase=DRUIDA` of the elemental summons.
const DRUIDA: u8 = 6;

fn tsao() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var("AO_TSAO").unwrap_or_else(|_| "X:/dev/tsao_rec".into()));
    p.join("Mapas").join("mapa1.csm").exists().then_some(p)
}

fn server(rec: PathBuf, x: i32, y: i32) -> TestServer {
    TestServer::with(|st| {
        st.recursos = rec;
        st.start = Some((MAP, x, y));
    })
}

fn cmd(s: &TestServer, b: &mut Bot, line: &str) {
    b.say(line);
    s.advance(50);
    b.sync();
}

/// Past the cast interval, with full mana.
fn rest(s: &TestServer, b: &mut Bot) {
    s.advance(2000);
    cmd(s, b, "/llenar");
}

fn said(b: &Bot, needle: &str) -> bool {
    b.console().any(|t| t.contains(needle))
}

/// The creature called `name` this bot sees.
fn creature(b: &Bot, name: &str) -> Option<(u32, (i32, i32))> {
    b.creatures().find(|(_, e)| e.name.starts_with(name)).map(|(id, e)| (id, (e.x, e.y)))
}

fn dist(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

/// Walks `steps` tiles in a free direction.
fn stroll(s: &TestServer, b: &mut Bot, steps: i32) {
    let dir = b.free_dir(steps).expect("open ground");
    for _ in 0..steps {
        b.walk(dir);
        s.advance(300);
        b.sync();
    }
}

#[test]
fn tame_follow_and_dismiss() {
    let Some(rec) = tsao() else { return eprintln!("TSAO data not found, skipping") };
    let s = server(rec, 50, 50);
    let mut b = s.bot("Domador");
    // No skill: a rooster (Domable 30) can never be tamed (CHA 18 * 0 + 3 x 1..6 < 30).
    cmd(&s, &mut b, &format!("/criatura {GALLO} 0 2"));
    let (_, gallo) = creature(&b, "Gallo").expect("the rooster");
    for _ in 0..5 {
        b.send(&C2S::Skill(SkillC2S::Tame { x: gallo.0, y: gallo.1 }));
        s.advance(1600);
        b.sync();
    }
    assert!(said(&b, "No has logrado domar"), "{:?}", b.console().collect::<Vec<_>>());
    assert!(!said(&b, "aceptado"));

    // Skill 100: the ant (Domable 5) accepts. Too far first (the fix over TSAO's range).
    cmd(&s, &mut b, &format!("/skill {TAMING} 100"));
    cmd(&s, &mut b, &format!("/atributo {CHARISMA} 18"));
    cmd(&s, &mut b, &format!("/criatura {HORMIGA} 4 -3"));
    let (ant, at) = creature(&b, "Hormiga").expect("the ant");
    b.send(&C2S::Skill(SkillC2S::Tame { x: at.0, y: at.1 }));
    s.advance(1600);
    b.sync();
    assert!(said(&b, "demasiado lejos"));
    b.clear_log();
    cmd(&s, &mut b, &format!("/ir {MAP} {} {}", at.0 - 1, at.1));
    let at = b.ent(ant).map(|e| (e.x, e.y)).unwrap();
    b.send(&C2S::Skill(SkillC2S::Tame { x: at.0, y: at.1 }));
    s.advance(1600);
    b.sync();
    assert!(said(&b, "te ha aceptado como su amo"), "{:?}", b.console().collect::<Vec<_>>());

    // It follows: walk away and it comes after us.
    stroll(&s, &mut b, 8);
    s.advance(6000);
    b.sync();
    let pos = b.ent(ant).map(|e| (e.x, e.y)).expect("still here");
    assert!(dist(pos, b.pos()) <= 4, "the ant {pos:?} follows {:?}", b.pos());

    // /quitarmascota lets it go (the ant came from /criatura: it just leaves).
    cmd(&s, &mut b, "/quitarmascota");
    assert!(said(&b, "liberado"));
    s.advance(100);
    b.sync();
    assert!(b.ent(ant).is_none());
}

#[test]
fn summoned_elemental_fights_for_its_owner_then_vanishes() {
    let Some(rec) = tsao() else { return eprintln!("TSAO data not found, skipping") };
    let s = server(rec, 50, 50);
    let mut b = s.bot("Invocador");
    cmd(&s, &mut b, &format!("/clase {DRUIDA}"));
    cmd(&s, &mut b, &format!("/aprender {INVOCAR_FUEGO}"));
    cmd(&s, &mut b, &format!("/aprender {INVOCAR_TIERRA}"));
    let (x, y) = b.pos();
    b.cast_raw(INVOCAR_FUEGO, x + 2, y);
    s.advance(100);
    b.sync();
    let (fire, _) = creature(&b, "Elemental de fuego").unwrap_or_else(|| panic!("summoned: {:?}", b.console().collect::<Vec<_>>()));
    // One fire elemental at a time.
    rest(&s, &mut b);
    b.clear_log();
    b.cast_raw(INVOCAR_FUEGO, x - 2, y);
    s.advance(100);
    b.sync();
    assert!(said(&b, "Ya tienes invocada"));
    assert_eq!(b.creatures().filter(|(_, e)| e.name.starts_with("Elemental de fuego")).count(), 1);

    // A pig next to us: we hit it, the elemental burns it to death and we get the kill.
    cmd(&s, &mut b, &format!("/criatura {CERDO} 0 1"));
    let (pig, pp) = creature(&b, "Cerdo").expect("the pig");
    let me = b.pos();
    let dir = ao_rules::pathfinding::heading_to(me, pp);
    if dist(me, pp) != 1 {
        cmd(&s, &mut b, &format!("/ir {MAP} {} {}", pp.0, pp.1 - 1));
    }
    let dir = if dist(b.pos(), pp) == 1 { ao_rules::pathfinding::heading_to(b.pos(), pp) } else { dir };
    b.send(&C2S::Turn { dir });
    s.advance(50);
    b.send(&C2S::Melee { seq: 1_000 });
    s.advance(50);
    b.sync();
    for _ in 0..40 {
        s.advance(250);
        b.sync();
        if b.ent(pig).is_none() {
            break;
        }
    }
    assert!(b.ent(pig).is_none(), "the pig dies: {:?}", b.console().collect::<Vec<_>>());
    assert!(said(&b, "Elemental de fuego le ha causado"), "{:?}", b.console().collect::<Vec<_>>());

    // TiempoInvocacion: gone after [Pets] SummonSecs (120).
    s.advance_secs(121);
    b.sync();
    assert!(b.ent(fire).is_none(), "the elemental vanished");
}

#[test]
fn companion_toggles_and_summons_do_not_travel() {
    let Some(rec) = tsao() else { return eprintln!("TSAO data not found, skipping") };
    let s = server(rec, 50, 50);
    let mut b = s.bot("Viajero");
    cmd(&s, &mut b, &format!("/aprender {INVOCAR_MASCOTA}"));
    cmd(&s, &mut b, &format!("/aprender {INVOCAR_TIERRA}"));
    let (x, y) = b.pos();
    b.cast_raw(INVOCAR_MASCOTA, x + 1, y);
    s.advance(100);
    b.sync();
    let (pet, _) = creature(&b, "Drag").expect("the golden dragon companion");
    rest(&s, &mut b);
    b.cast_raw(INVOCAR_MASCOTA, x + 1, y);
    s.advance(100);
    b.sync();
    assert!(b.ent(pet).is_none() && said(&b, "se ha ido"), "casting it again sends it away");

    // A tamed ant comes along to the next map, the earth elemental does not.
    cmd(&s, &mut b, &format!("/skill {TAMING} 100"));
    cmd(&s, &mut b, &format!("/criatura {HORMIGA} 1 0"));
    let (_, at) = creature(&b, "Hormiga").expect("ant");
    b.send(&C2S::Skill(SkillC2S::Tame { x: at.0, y: at.1 }));
    rest(&s, &mut b);
    b.sync();
    assert!(said(&b, "aceptado"));
    b.cast_raw(INVOCAR_TIERRA, x, y + 2);
    s.advance(100);
    b.sync();
    assert!(creature(&b, "Elemental de tierra").is_some(), "{:?}", b.console().collect::<Vec<_>>());
    cmd(&s, &mut b, "/ir 2 50 50");
    s.advance(500);
    b.sync();
    assert_eq!(b.map(), 2);
    assert!(said(&b, "desvanecido"));
    assert!(creature(&b, "Elemental de tierra").is_none());
    let (_, ant) = creature(&b, "Hormiga").expect("the ant travelled");
    assert!(dist(ant, b.pos()) <= 3);
}

#[test]
fn paralysis_and_immunity_on_creatures() {
    let Some(rec) = tsao() else { return eprintln!("TSAO data not found, skipping") };
    let s = server(rec, 50, 50);
    let mut b = s.bot("Hechicero");
    cmd(&s, &mut b, &format!("/clase {DRUIDA}"));
    cmd(&s, &mut b, &format!("/aprender {PARALIZAR}"));
    cmd(&s, &mut b, &format!("/aprender {RELAMPAGO}"));
    cmd(&s, &mut b, &format!("/aprender {INVOCAR_FUEGO}"));
    // A wandering pig stays still for NpcParalysisSecs once paralysed.
    cmd(&s, &mut b, &format!("/criatura {CERDO} 3 0"));
    let (pig, _) = creature(&b, "Cerdo").expect("pig");
    // It wanders: aim where it is now, again if it stepped away meanwhile.
    for _ in 0..5 {
        b.clear_log();
        let pp = b.ent(pig).map(|e| (e.x, e.y)).unwrap();
        b.cast_raw(PARALIZAR, pp.0, pp.1);
        s.advance(50);
        b.sync();
        if !said(&b, "inválido") {
            break;
        }
        rest(&s, &mut b);
    }
    assert!(!said(&b, "inválido"), "{:?}", b.console().collect::<Vec<_>>());
    let p0 = b.ent(pig).map(|e| (e.x, e.y)).unwrap();
    for _ in 0..40 {
        s.advance(1000);
    }
    b.sync();
    assert_eq!(b.ent(pig).map(|e| (e.x, e.y)), Some(p0), "a paralysed pig does not move");

    // Elementals: immune to paralysis (AfectaParalisis) and to Relámpago (AfectaRelampago).
    let (x, y) = b.pos();
    rest(&s, &mut b);
    b.cast_raw(INVOCAR_FUEGO, x - 2, y);
    s.advance(100);
    b.sync();
    let (fire, _) = creature(&b, "Elemental de fuego").expect("elemental");
    for spell in [PARALIZAR, RELAMPAGO] {
        cast_on(&s, &mut b, spell, fire);
        assert!(said(&b, "inmune"), "{spell}: {:?}", b.console().collect::<Vec<_>>());
    }
}

/// Casts `spell` on the creature `id` where it stands, again if it stepped away.
fn cast_on(s: &TestServer, b: &mut Bot, spell: u16, id: u32) {
    for _ in 0..8 {
        rest(s, b);
        b.clear_log();
        let p = b.ent(id).map(|e| (e.x, e.y)).expect("still here");
        b.cast_raw(spell, p.0, p.1);
        s.advance(50);
        b.sync();
        if !said(b, "inválido") {
            return;
        }
    }
    panic!("could not aim at {id}");
}

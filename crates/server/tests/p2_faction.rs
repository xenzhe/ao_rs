//! P2 factions (ModFacciones.bas, PuedeAtacar, ContarMuerte, VolverCriminal) on ao-testkit:
//! citizens turning criminal, the citizen "seguro", the royal army refusing to hit
//! citizens, enlisting at the official recruiter with the requirements of
//! rangos_faccion.dat, ranks, leaving, kill counters and their persistence. Characters
//! that need a level or faction points are prepared in a database file before playing.
use ao_formats::dat::npcs::{self, NpcType};
use ao_formats::{recursos_dir, AoMap};
use ao_protocol::{FactionC2S, C2S, S2C};
use ao_rules::combat::ATTACK_INTERVAL_MS;
use ao_rules::faction as rules;
use ao_rules::{tile_distance, Dir};
use ao_testkit::{Bot, TestServer};
use server::db::Db;
use server::world::faction::FactionRecord;

/// Map 2: not a safe zone.
const FIELD: (u16, i32, i32) = (2, 50, 50);
const WARRIOR: u8 = 3;
const PASS: &str = "secreta1";
/// obj.dat 4340, the army's rank 1 reward (recompensas_faccion.dat `Recompensa23=1-4340`).
const UNIFORM: u16 = 4340;

/// A fresh database file for one test.
fn db_file(tag: &str) -> String {
    let p = std::env::temp_dir().join(format!("ao_p2_faction_{tag}_{}.db", std::process::id()));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", p.display()));
    }
    p.display().to_string()
}

fn server(at: (u16, i32, i32), hp: i32, db: &str, npcs: bool) -> TestServer {
    let db = db.to_string();
    TestServer::with(move |st| {
        st.config.database = db;
        st.npcs = npcs;
        st.start = Some(at);
        let b = &mut st.data.balance;
        (b.start_map, b.start_x, b.start_y) = at;
        b.start_hp = hp;
        b.hp_regen = 0;
        b.creatures_per_map = 0;
    })
}

/// Creates the account and the character (not playing yet).
fn create(s: &TestServer, name: &str) -> Bot {
    let mut c = s.connect();
    assert!(c.create_account(&name.to_lowercase(), PASS));
    assert!(c.create_character(name, 1, WARRIOR, 1, 1));
    c
}

/// Sets the saved level and faction of a created character.
fn seed(db: &str, name: &str, level: u32, f: impl FnOnce(&mut FactionRecord)) {
    let db = Db::open(db).unwrap();
    let acc = db.verify_account(&name.to_lowercase(), PASS).unwrap();
    let mut rec = db.load_character(acc.id, name).unwrap();
    rec.level = level;
    let state = db.load_state(rec.id).unwrap();
    db.save_full(&rec, &state).unwrap();
    let mut fr = db.load_faction(rec.id).unwrap();
    f(&mut fr);
    db.save_faction(rec.id, &fr).unwrap();
}

fn saved(db: &str, name: &str) -> FactionRecord {
    let db = Db::open(db).unwrap();
    let acc = db.verify_account(&name.to_lowercase(), PASS).unwrap();
    let rec = db.load_character(acc.id, name).unwrap();
    db.load_faction(rec.id).unwrap()
}

fn play(mut c: Bot, name: &str) -> Bot {
    c.play(name);
    c.wait_until("estado de faccion", |b| b.game.local.social.faction.known);
    c
}

/// With its official starting equipment on: armor lets stamina regenerate between swings.
fn armed(mut c: Bot) -> Bot {
    c.wait_until("equipo inicial", |b| b.game.local.inventory.iter().flatten().any(|i| i.equipped));
    c
}

/// Naked (no armor absorbing hits) so the victim dies quickly.
fn strip(mut c: Bot) -> Bot {
    let equipped: Vec<u8> = c.game.local.inventory.iter().enumerate().filter(|(_, s)| s.is_some_and(|i| i.equipped)).map(|(i, _)| (i + 1) as u8).collect();
    for slot in equipped {
        c.send(&C2S::EquipItem { slot });
    }
    c.wait_until("desnudo", |b| b.game.local.inventory.iter().flatten().all(|i| !i.equipped));
    c
}

fn says(b: &Bot, text: &str) -> bool {
    b.console().any(|t| t.contains(text))
}

fn face(bot: &mut Bot, to: (i32, i32)) {
    let me = bot.pos();
    bot.send(&C2S::Turn { dir: Dir::towards(to.0 - me.0, to.1 - me.1) });
    bot.sync();
}

/// `b` steps next to `a` (orthogonally) and `a` faces it.
fn adjacent(a: &mut Bot, b: &mut Bot) {
    let (pa, pb) = (a.pos(), b.pos());
    assert_eq!(tile_distance(pa, pb), 1, "spawned next to each other");
    if pa.0 != pb.0 && pa.1 != pb.1 {
        assert!(b.walk(Dir::towards(pa.0 - pb.0, 0)));
        b.sync();
    }
    a.sync();
    let to = b.pos();
    face(a, to);
}

/// Swings until the victim dies (or `max` swings). Returns true if it died.
fn fight(s: &TestServer, a: &mut Bot, v: &mut Bot, max: usize) -> bool {
    for _ in 0..max {
        if v.hp() == 0 {
            return true;
        }
        let seq = a.game.next_seq();
        a.send(&C2S::Melee { seq });
        s.advance(ATTACK_INTERVAL_MS);
        a.sync();
        v.sync();
    }
    v.hp() == 0
}

/// The first official recruiter of npcs.dat whose `Faccion` is royal (0) or not, with
/// a free tile next to it: (map, npc tile, free tile).
fn recruiter(royal: bool) -> (u16, (i32, i32), (i32, i32)) {
    let root = recursos_dir();
    let table = npcs::load(&root).unwrap().data;
    for number in 1..=400u16 {
        let Ok(map) = AoMap::load(&root, number) else { continue };
        for n in &map.npcs {
            let Some(d) = table.get(n.npc as u32) else { continue };
            if d.npc_type != NpcType::Recruiter || (d.faction == 0) != royal {
                continue;
            }
            let free = [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().map(|(dx, dy)| (n.x + dx, n.y + dy)).find(|&(x, y)| ao_rules::walk::standable(&map, x, y) && map.npcs.iter().all(|o| (o.x, o.y) != (x, y)));
            if let Some(free) = free {
                return (number, (n.x, n.y), free);
            }
        }
    }
    panic!("no official recruiter found");
}

/// Safe maps get their NPCs on the next world tick.
fn npc_at(s: &TestServer, b: &mut Bot, at: (i32, i32)) -> u32 {
    s.advance(100);
    if !b.try_wait_until(std::time::Duration::from_secs(2), |b| b.game.ents.values().any(|e| (e.x, e.y) == at)) {
        let ents: Vec<_> = b.game.ents.values().map(|e| (e.name.clone(), e.x, e.y)).collect();
        panic!("no recruiter at {at:?}; me {:?} map {}; {ents:?}", b.pos(), b.map());
    }
    b.game.ents.iter().find(|(_, e)| (e.x, e.y) == at).map(|(id, _)| *id).unwrap()
}

fn npc_said(b: &Bot, text: &str) -> bool {
    b.log().iter().any(|m| matches!(m, S2C::NpcSay { text: t, .. } if t.contains(text)))
}

#[test]
fn a_citizen_killing_a_citizen_turns_criminal() {
    let db = db_file("crim");
    let s = server(FIELD, 3, &db, false);
    let mut a = armed(play(create(&s, "Asesino"), "Asesino"));
    let mut v = strip(play(create(&s, "Inocente"), "Inocente"));
    assert_eq!(a.game.local.social.faction.status, rules::CITIZEN);
    assert!(a.game.local.social.faction.safe, "ConnectUser: citizens log in with the seguro on");
    a.send(&C2S::Faction(FactionC2S::ToggleSafe));
    a.wait_until("sin seguro", |b| !b.game.local.social.faction.safe);
    let aid = a.id();
    adjacent(&mut a, &mut v);
    assert!(fight(&s, &mut a, &mut v, 300), "the victim died");
    a.wait_until("criminal", |b| b.game.local.social.faction.status == rules::CRIMINAL);
    v.wait_until("la victima lo ve criminal", |b| b.game.local.social.faction.others.get(&aid) == Some(&rules::CRIMINAL));
    assert!(says(&a, "Ahora sos un criminal."));
    // Level 1 victims are newbies: ContarMuerte counts nothing.
    assert_eq!(a.game.local.social.faction.citizens_killed, 0);
    // Only citizens may toggle the seguro.
    a.send(&C2S::Faction(FactionC2S::ToggleSafe));
    a.sync();
    s.settle();
    a.sync();
    assert!(says(&a, "Solo los ciudadanos pueden cambiar el seguro."));
    drop(a);
    s.advance(100);
    assert_eq!(saved(&db, "Asesino").status, rules::CRIMINAL, "saved on logout");
}

/// An official arena: a tile with trigger 6 (`TRIGGER_FIGHT_ZONE`) whose 8 neighbours are
/// arena tiles too and free to stand on, so both fighters spawn inside.
fn arena() -> (u16, i32, i32) {
    let root = recursos_dir();
    for number in 1..=400u16 {
        let Ok(map) = AoMap::load(&root, number) else { continue };
        for y in 3..=98 {
            for x in 3..=98 {
                let ok = (-1..=1).all(|dy| (-1..=1).all(|dx| map.trigger(x + dx, y + dy) == ao_rules::combat::TRIGGER_FIGHT_ZONE && ao_rules::walk::standable(&map, x + dx, y + dy) && map.npcs.iter().all(|n| (n.x, n.y) != (x + dx, y + dy))));
                if ok {
                    return (number, x, y);
                }
            }
        }
    }
    panic!("no official arena found");
}

#[test]
fn nothing_counts_in_an_arena() {
    let at = arena();
    let db = db_file("arena");
    let s = server(at, 3, &db, false);
    let mut a = armed(play(create(&s, "Gladiador"), "Gladiador"));
    let mut v = strip(play(create(&s, "Rival"), "Rival"));
    adjacent(&mut a, &mut v);
    assert!(fight(&s, &mut a, &mut v, 300), "the rival died");
    s.settle();
    a.sync();
    assert_eq!(a.game.local.social.faction.status, rules::CITIZEN, "PeleaSegura: no criminal in an arena");
    assert!(!says(&a, "Ahora sos un criminal."));
}

#[test]
fn the_seguro_and_the_royal_army_refuse_attacks_on_citizens() {
    let db = db_file("seguro");
    let s = server(FIELD, 3, &db, false);
    let soldier = create(&s, "Soldado");
    seed(&db, "Soldado", 30, |f| {
        f.status = rules::ROYAL;
        f.royal_rank = 1;
        f.enlistments = 1;
    });
    let mut a = strip(play(soldier, "Soldado"));
    assert_eq!(a.game.local.social.faction.status, rules::ROYAL, "loaded from the database");
    assert_eq!(a.game.local.social.faction.rank, 1);
    assert_eq!(a.game.local.social.faction.title, "Recluta Real");
    let mut v = armed(play(create(&s, "Civil"), "Civil"));
    adjacent(&mut a, &mut v);
    let hp = v.hp();
    assert!(!fight(&s, &mut a, &mut v, 4));
    assert_eq!(v.hp(), hp);
    assert!(says(&a, "Los miembros del Ejercito Real tienen prohibido atacar ciudadanos."));

    // A citizen with the seguro on cannot hit the army either; off, he can (and turns criminal).
    let to = a.pos();
    face(&mut v, to);
    assert!(v.game.local.social.faction.safe, "logged in with the seguro on");
    let hp = a.hp();
    assert!(!fight(&s, &mut v, &mut a, 4));
    assert_eq!(a.hp(), hp);
    assert!(says(&v, "No podés atacar miembros del Ejercito Real, para hacerlo debes desactivar el seguro."));
    v.send(&C2S::Faction(FactionC2S::ToggleSafe));
    v.wait_until("sin seguro", |b| !b.game.local.social.faction.safe);
    assert!(fight(&s, &mut v, &mut a, 300), "the soldier died");
    v.wait_until("criminal", |b| b.game.local.social.faction.status == rules::CRIMINAL);
    assert!(a.kicked().is_none() && v.kicked().is_none());
}

#[test]
fn killing_counts_and_faction_points_are_saved() {
    let db = db_file("kills");
    let s = server(FIELD, 3, &db, false);
    // The starting equipment is for newbies (level 12 at most); victims up to level 12
    // do not count (ContarMuerte / EsNewbie).
    let killer = create(&s, "Bandido");
    seed(&db, "Bandido", 12, |f| f.status = rules::CRIMINAL);
    let mut a = armed(play(killer, "Bandido"));
    let victim = create(&s, "Aldeano");
    seed(&db, "Aldeano", 13, |_| {});
    let mut v = strip(play(victim, "Aldeano"));
    adjacent(&mut a, &mut v);
    assert!(fight(&s, &mut a, &mut v, 300), "the citizen died");
    // CalculateBaseFactionScore(12, 13) = 10 + 13 - 12 = 11 for a criminal killing a citizen.
    a.wait_until("contador", |b| b.game.local.social.faction.citizens_killed == 1);
    assert_eq!(a.game.local.social.faction.score, 11);
    assert!(says(&a, "Has ganado 11 puntos de facción por matar a Aldeano."));
    let f = saved(&db, "Bandido");
    assert_eq!((f.citizens_killed, f.criminals_killed, f.score), (1, 0, 11));
}

#[test]
fn enlisting_at_the_royal_recruiter() {
    let (map, npc, free) = recruiter(true);
    let db = db_file("armada");
    let s = server((map, free.0, free.1), 30, &db, true);

    // Level 1, no points: refused (EnlistarArmadaReal requirements).
    let mut novato = play(create(&s, "Novato"), "Novato");
    let nid = npc_at(&s, &mut novato, npc);
    novato.send(&C2S::Interact { tx: npc.0, ty: npc.1 });
    s.settle();
    novato.sync();
    assert!(npc_said(&novato, "Para unirte a las fuerzas de la Armada Real debes tener al menos 500 puntos"), "{:?}", novato.console().collect::<Vec<_>>());
    assert_eq!(novato.game.local.social.faction.status, rules::CITIZEN);
    // Leaving as a plain citizen turns criminal at once; criminals are not accepted.
    novato.send(&C2S::Faction(FactionC2S::Leave { npc: Some(nid) }));
    novato.wait_until("criminal", |b| b.game.local.social.faction.status == rules::CRIMINAL);
    novato.send(&C2S::Interact { tx: npc.0, ty: npc.1 });
    s.settle();
    novato.sync();
    assert!(npc_said(&novato, "No se permiten criminales en el Ejército Real."));
    drop(novato);

    // Enough points but no level.
    let joven = create(&s, "Joven");
    seed(&db, "Joven", 10, |f| f.score = 600);
    let mut joven = play(joven, "Joven");
    joven.send(&C2S::Interact { tx: npc.0, ty: npc.1 });
    s.settle();
    joven.sync();
    assert!(npc_said(&joven, "debes ser al menos de nivel 23"));
    drop(joven);

    // Level 25 with 600 points: enlisted at rank 1 (Recluta Real).
    let soldado = create(&s, "Soldado");
    seed(&db, "Soldado", 25, |f| f.score = 600);
    let mut a = play(soldado, "Soldado");
    let nid = npc_at(&s, &mut a, npc);
    a.send(&C2S::Interact { tx: npc.0, ty: npc.1 });
    a.wait_until("armada", |b| b.game.local.social.faction.status == rules::ROYAL);
    assert_eq!(a.game.local.social.faction.rank, 1);
    assert_eq!(a.game.local.social.faction.title, "Recluta Real");
    assert!(says(&a, "¡Ahora perteneces al Ejército Real!"));
    assert!(npc_said(&a, "Bienvenido al Ejercito Imperial"));
    let f = saved(&db, "Soldado");
    assert_eq!((f.status, f.royal_rank, f.royal_rewarded, f.enlistments, f.entry_level), (rules::ROYAL, 1, 1, 1, 25));
    // DarRecompensas: rank 1 of the army is obj 4340 "Uniforme Imperial (1ª Jerarquía)"
    // (Real=1, forbidden only for pirate, thief and worker).
    a.wait_until("uniforme", |b| b.game.local.inventory.iter().flatten().any(|i| i.obj == UNIFORM));
    // A soldier of rank 1 can wear it (FaccionPuedeUsarItem + JerarquiaPuedeUsarItem).
    let slot = a.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == UNIFORM)).unwrap() as u8 + 1;
    a.send(&C2S::EquipItem { slot });
    a.wait_until("uniforme puesto", |b| b.game.local.slot(slot).is_some_and(|i| i.equipped));

    // Talking again asks for the next rank (1500 points).
    a.send(&C2S::Interact { tx: npc.0, ty: npc.1 });
    s.settle();
    a.sync();
    assert!(npc_said(&a, "Te faltan 900 Puntos de faccion para subir de rango."));

    // Leaving needs the recruiter: a made-up NPC is ignored.
    a.send(&C2S::Faction(FactionC2S::Leave { npc: Some(987_654) }));
    a.send(&C2S::Faction(FactionC2S::Leave { npc: None }));
    s.settle();
    a.sync();
    assert!(says(&a, "Para salir del ejercito debes ir a visitar al rey."));
    assert_eq!(a.game.local.social.faction.status, rules::ROYAL);
    a.send(&C2S::Faction(FactionC2S::Leave { npc: Some(nid) }));
    a.wait_until("expulsado", |b| b.game.local.social.faction.status == rules::CITIZEN);
    // The console line may come in a later frame than the status.
    a.wait_until("aviso de expulsion", |b| says(b, "Has sido expulsado del Ejército Real."));
    a.wait_until("sin uniforme", |b| b.game.local.inventory.iter().flatten().all(|i| i.obj != UNIFORM));
    let objs = ao_formats::dat::obj::load(&recursos_dir()).unwrap().data;
    assert!(a.game.local.inventory.iter().flatten().all(|i| objs.get(i.obj as u32).is_none_or(|o| o.royal != 1)), "faction items are taken away");

    // Deserters are never accepted again (MAX_FACTION_ENLISTMENTS = 0).
    a.send(&C2S::Interact { tx: npc.0, ty: npc.1 });
    s.settle();
    a.sync();
    assert!(npc_said(&a, "Ya has desertado el Ejército Real."));
    assert!(a.kicked().is_none());
}

#[test]
fn the_chaos_recruiter_does_not_take_citizens() {
    let (map, npc, free) = recruiter(false);
    let db = db_file("caos");
    let s = server((map, free.0, free.1), 30, &db, true);
    let mut c = play(create(&s, "Cortes"), "Cortes");
    c.send(&C2S::Interact { tx: npc.0, ty: npc.1 });
    s.settle();
    c.sync();
    assert!(npc_said(&c, "Tu no eres bienvenido aquí asqueroso Ciudadano"));
    drop(c);

    let villano = create(&s, "Villano");
    seed(&db, "Villano", 25, |f| {
        f.status = rules::CRIMINAL;
        f.score = 600;
    });
    let mut v = play(villano, "Villano");
    v.send(&C2S::Interact { tx: npc.0, ty: npc.1 });
    v.wait_until("legion", |b| b.game.local.social.faction.status == rules::CHAOS);
    assert_eq!(v.game.local.social.faction.title, "Iniciado Oscuro");
    assert!(says(&v, "¡Ahora perteneces a la Legión Oscura!"));
}

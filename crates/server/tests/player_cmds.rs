//! Player slash commands of `ParseUserCommand` (systems/player_cmds.rs): forgiveness at the
//! priest (`/perdon` + `/donar`, `VolverCiudadano`), `/promedio`, `/seg`, `/onlineclan`.
use ao_protocol::{C2S, S2C};
use ao_rules::faction as rules;
use ao_testkit::{Bot, TestServer};
use server::db::Db;

/// Ullathorpe's priest (map 1).
const PRIEST: (i32, i32) = (78, 67);
const PASS: &str = "secreta1";

fn db_file(tag: &str) -> String {
    let p = std::env::temp_dir().join(format!("ao_player_cmds_{tag}_{}.db", std::process::id()));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", p.display()));
    }
    p.display().to_string()
}

fn says(b: &Bot, text: &str) -> bool {
    b.console().any(|t| t.contains(text)) || b.log().iter().any(|m| matches!(m, S2C::NpcSay { text: t, .. } if t.contains(text)))
}

#[test]
fn a_criminal_buys_forgiveness_from_the_priest() {
    let db = db_file("perdon");
    let dbc = db.clone();
    let s = TestServer::with(move |st| {
        st.config.database = dbc;
        st.start = Some((1, PRIEST.0, PRIEST.1 + 2));
        let b = &mut st.data.balance;
        (b.start_map, b.start_x, b.start_y) = (1, PRIEST.0, PRIEST.1 + 2);
        b.creatures_per_map = 0;
    });
    let mut c = s.connect();
    assert!(c.create_account("bandido", PASS));
    assert!(c.create_character("Bandido", 1, 3, 1, 1));
    {
        let d = Db::open(&db).unwrap();
        let acc = d.verify_account("bandido", PASS).unwrap();
        let rec = d.load_character(acc.id, "Bandido").unwrap();
        let mut state = d.load_state(rec.id).unwrap();
        state.gold = 20_000;
        d.save_full(&rec, &state).unwrap();
        let mut f = d.load_faction(rec.id).unwrap();
        f.status = rules::CRIMINAL;
        f.citizens_killed = 2;
        d.save_faction(rec.id, &f).unwrap();
    }
    c.play("Bandido");
    c.wait_until("criminal", |b| b.game.local.social.faction.known && b.game.local.social.faction.status == rules::CRIMINAL);

    c.say("/perdon");
    c.wait_until("sin objetivo", |b| says(b, "Primero tenés que seleccionar"));
    c.send(&C2S::Interact { tx: PRIEST.0, ty: PRIEST.1 });
    s.settle();
    c.say("/perdon");
    // 2 citizens killed * GoldMult 1 * 5000.
    c.wait_until("precio", |b| says(b, "donación de 10000"));
    c.say("/donar 5000");
    c.wait_until("avaro", |b| says(b, "persona avara"));
    assert_eq!(c.game.local.social.faction.status, rules::CRIMINAL);
    c.say("/donar 10000");
    c.wait_until("ciudadano", |b| b.game.local.social.faction.status == rules::CITIZEN);
    assert!(says(&c, "Has donado 10000"));
    c.say("/perdon");
    c.wait_until("ya libre", |b| says(b, "libre de pecados"));
}

#[test]
fn average_safe_toggle_and_clan_list() {
    let s = TestServer::start_at(1, 50, 50);
    let mut c = s.connect();
    assert!(c.create_account("promedio", PASS));
    assert!(c.create_character("Promedio", 1, 3, 1, 1));
    c.play("Promedio");
    c.wait_until("en el mundo", |b| b.game.me().is_some());
    c.say("/promedio");
    c.wait_until("vida esperada", |b| says(b, "Vida esperada") && says(b, "Vida actual"));
    c.say("/seg");
    c.wait_until("seguro", |b| says(b, "Has desactivado el seguro") || says(b, "Has activado el seguro"));
    c.say("/onlineclan");
    c.wait_until("sin clan", |b| says(b, "No pertenecés a ningún clan"));
    c.say("/apostar 10");
    c.wait_until("sin timbero", |b| says(b, "Primero tenés que seleccionar"));
}

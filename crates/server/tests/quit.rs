//! `/salir` (`HandleQuit` / `Cerrar_Usuario` / `CancelExit`): back to the character list,
//! at once in a safe map, after a 10 s countdown elsewhere (cancelled by walking).
use ao_protocol::S2C;
use ao_testkit::{Bot, TestServer};

fn left(b: &Bot) -> bool {
    b.log().iter().any(|m| matches!(m, S2C::LeftWorld))
}

fn player(s: &TestServer, name: &str) -> Bot {
    let mut c = s.connect();
    assert!(c.create_account(&name.to_lowercase(), "secreta1"));
    assert!(c.create_character(name, 1, 1, 1, 1));
    c.play(name);
    c.wait_until("en el mundo", |b| b.game.me().is_some());
    c
}

#[test]
fn quitting_counts_down_outside_safe_maps_and_walking_cancels_it() {
    // Map 2 (forest north of Ullathorpe) is not a safe map.
    let s = TestServer::with(|st| {
        st.start = Some((2, 50, 50));
        let b = &mut st.data.balance;
        (b.start_map, b.start_x, b.start_y) = (2, 50, 50);
        st.data.balance.creatures_per_map = 0;
        st.npcs = false;
    });
    let mut b = player(&s, "Viajero");
    b.say("/salir");
    b.wait_until("cuenta atras", |b| b.console().any(|l| l.contains("Cerrando el juego en 10")));
    s.advance(3000);
    b.sync();
    let dir = b.free_dir(1).expect("room to walk");
    assert!(b.walk(dir));
    b.wait_until("cancelada", |b| b.console().any(|l| l.contains("cancelado la salida")));
    s.advance(12_000);
    b.sync();
    assert!(!left(&b), "a cancelled quit keeps the character in");

    b.say("/salir");
    for _ in 0..12 {
        s.advance(1000);
    }
    b.wait_until("fuera del mundo", left);
    assert!(!b.closed(), "the account stays logged in");
    b.wait_until("lista de personajes", |b| b.log().iter().rev().take(3).any(|m| matches!(m, S2C::CharacterList { .. })));
    // And the same character can come back in.
    b.play("Viajero");
    b.wait_until("de vuelta", |b| b.game.me().is_some() && b.map() == 2);
}

#[test]
fn quitting_in_a_safe_map_is_immediate() {
    let s = TestServer::start_at(1, 50, 50);
    let mut b = player(&s, "Ciudadano");
    b.say("/salir");
    b.wait_until("fuera al instante", left);
}

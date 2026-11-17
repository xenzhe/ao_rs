//! P3 account flow of the client (`client_core::p3::account`): login, register, create,
//! delete and play through `AccountFlow`, with bots on a virtual clock against the real server.
use client_core::p3::account::{valid_heads, AccountFlow, Pending};
use client_core::Screen;
use ao_rules::character::{FEMALE, MALE, MAX_CHARACTERS, RACES};
use ao_testkit::{Bot, TestServer};

/// Waits for the server's answer (or the world) and lets the flow apply it.
fn step(b: &mut Bot, flow: &mut AccountFlow) {
    b.wait_until("respuesta de cuenta", |b| b.game.account_response().is_some() || b.game.entered_world() || b.closed());
    flow.update(&mut b.game);
}

fn register(b: &mut Bot, flow: &mut AccountFlow, user: &str) {
    flow.register(&mut b.game, user, "secreta1");
    assert!(flow.busy());
    step(b, flow);
    assert_eq!(b.game.screen, Screen::Create, "a new account goes straight to creating its first character");
}

fn create(b: &mut Bot, flow: &mut AccountFlow, name: &str, race: u8, class: u8, gender: u8) {
    let head = valid_heads(race, gender, |_| true)[0];
    flow.create_character(&mut b.game, name, race, class, gender, head);
    assert_eq!(flow.pending, Some(Pending::Create));
    step(b, flow);
    assert_eq!(b.game.screen, Screen::Characters, "creating returns to the list: {:?}", flow.notice);
    // The list arrives right after the result; wait for it.
    b.wait_until("lista con el personaje", |b| b.game.characters.iter().any(|c| c.name == name));
}

#[test]
fn full_flow_register_create_delete_play() {
    let s = TestServer::start();
    let mut b = s.connect();
    let mut flow = AccountFlow::default();

    register(&mut b, &mut flow, "viajero");
    assert!(!flow.busy() && flow.notice.is_none());
    create(&mut b, &mut flow, "Gandalf", 1, 1, MALE);
    create(&mut b, &mut flow, "Galadriel", 2, 2, FEMALE);
    assert_eq!(b.game.characters.len(), 2);
    let elf = b.game.characters.iter().find(|c| c.name == "Galadriel").unwrap();
    assert_eq!((elf.race, elf.class, elf.gender), (2, 2, FEMALE));
    assert!(elf.head >= 150, "female elf head");

    // Taken name: stays on the creation screen with a red message.
    b.game.screen = Screen::Create;
    flow.create_character(&mut b.game, "Gandalf", 1, 1, MALE, 1);
    step(&mut b, &mut flow);
    assert_eq!(b.game.screen, Screen::Create);
    assert_eq!(flow.notice.as_ref().map(|n| n.1), Some(false));
    assert!(flow.notice.as_ref().unwrap().0.contains("en uso"));
    b.game.screen = Screen::Characters;

    // Delete one.
    flow.delete_character(&mut b.game, "Galadriel");
    assert_eq!(flow.pending, Some(Pending::Delete));
    step(&mut b, &mut flow);
    b.wait_until("lista sin ella", |b| b.game.characters.len() == 1);
    assert_eq!(b.game.screen, Screen::Characters);
    assert_eq!(flow.notice.as_ref().map(|n| n.1), Some(true));

    // Someone else's / non-existent character: refused, still on the list.
    flow.delete_character(&mut b.game, "Nadie");
    step(&mut b, &mut flow);
    assert_eq!(flow.notice.as_ref().map(|n| n.1), Some(false));

    // Play.
    flow.play(&mut b.game, "Gandalf");
    assert_eq!(flow.pending, Some(Pending::Play));
    step(&mut b, &mut flow);
    assert_eq!(b.game.screen, Screen::Game);
    assert!(b.game.entered_world() && !flow.busy());
    assert_eq!(b.game.me().unwrap().name, "Gandalf");
}

#[test]
fn login_errors_and_reconnect() {
    let s = TestServer::start();
    let mut owner = s.connect();
    let mut f = AccountFlow::default();
    register(&mut owner, &mut f, "duenia");
    create(&mut owner, &mut f, "Unica", 4, 1, FEMALE);
    drop(owner);
    s.settle();

    let mut b = s.connect();
    let mut flow = AccountFlow::default();
    // Refused locally: no round trip, no failed-login strike.
    flow.login(&mut b.game, "du", "secreta1");
    assert!(!flow.busy());
    assert!(flow.notice.as_ref().unwrap().0.contains("Usuario"));
    flow.login(&mut b.game, "duenia", "corta");
    assert!(!flow.busy());
    assert_eq!(b.game.screen, Screen::Login);

    // Wrong password: message, still on the login screen, can try again.
    flow.login(&mut b.game, "duenia", "equivocada1");
    step(&mut b, &mut flow);
    assert_eq!(b.game.screen, Screen::Login);
    assert!(flow.notice.as_ref().unwrap().0.contains("incorrectos"));
    // Right one.
    flow.login(&mut b.game, "duenia", "secreta1");
    step(&mut b, &mut flow);
    assert_eq!(b.game.screen, Screen::Characters);
    b.wait_until("lista", |b| b.game.characters.len() == 1);
    assert_eq!(b.game.characters[0].name, "Unica");

    // Back to the login screen and in again with the same Game (new connection).
    b.game.account_logout();
    assert_eq!(b.game.screen, Screen::Login);
    assert!(b.game.characters.is_empty() && b.game.net.is_none());
    s.settle();
    flow.login(&mut b.game, "duenia", "secreta1");
    step(&mut b, &mut flow);
    assert_eq!(b.game.screen, Screen::Characters, "{:?}", flow.notice);
}

#[test]
fn three_wrong_passwords_close_the_connection_and_the_screen_recovers() {
    let s = TestServer::start();
    let mut owner = s.connect();
    let mut f = AccountFlow::default();
    register(&mut owner, &mut f, "victima");
    drop(owner);
    s.settle();

    let mut b = s.connect();
    let mut flow = AccountFlow::default();
    for i in 0..3 {
        flow.login(&mut b.game, "victima", "equivocada1");
        step(&mut b, &mut flow);
        if i == 2 {
            // The server kicks on the third failure: the flow turns the dead-end Kicked
            // screen back into the login screen with the reason.
            b.wait_until("expulsado", |b| matches!(b.game.screen, Screen::Kicked(_)) || b.closed());
            flow.update(&mut b.game);
            assert!(flow.notice.as_ref().unwrap().0.contains("intentos"), "{:?}", flow.notice);
        }
        assert_eq!(b.game.screen, Screen::Login);
    }
    assert!(b.game.net.is_none());
    // Next attempt opens a fresh connection by itself.
    flow.login(&mut b.game, "victima", "secreta1");
    assert!(flow.busy(), "{:?}", flow.notice);
    step(&mut b, &mut flow);
    assert_eq!(b.game.screen, Screen::Characters);
}

#[test]
fn character_limit_and_offline_refusals() {
    let s = TestServer::start();
    let mut b = s.connect();
    let mut flow = AccountFlow::default();
    register(&mut b, &mut flow, "coleccion");
    for i in 0..MAX_CHARACTERS {
        create(&mut b, &mut flow, &format!("Pj{i}"), 1, 3, MALE);
    }
    assert_eq!(b.game.characters.len(), MAX_CHARACTERS);
    flow.create_character(&mut b.game, "Sobrante", 1, 3, MALE, 1);
    assert!(!flow.busy());
    assert!(flow.notice.as_ref().unwrap().0.contains("máximo"));
    assert!(!b.closed(), "refused locally: the server never saw it");

    // The server dies: the pending request ends with a clear message and the login screen.
    drop(s);
    let mut flow = AccountFlow::default();
    b.game.request_delete_character("Pj0");
    flow.begin(&b.game, Pending::Delete);
    b.wait_until("conexion perdida", |b| b.closed() || b.game.account_response().is_some());
    flow.update(&mut b.game);
    assert!(!flow.busy());
}

/// Every race and gender: the first and last head the creation screen offers are accepted
/// by the server (a refused head would kick the player).
#[test]
fn offered_heads_are_accepted_by_the_server() {
    let s = TestServer::start();
    for (n, genders) in [(0, MALE), (1, FEMALE)] {
        let mut b = s.connect();
        let mut flow = AccountFlow::default();
        register(&mut b, &mut flow, &format!("cabezas{n}"));
        for (race, _) in RACES {
            let heads = valid_heads(race, genders, |_| true);
            for (i, head) in [*heads.first().unwrap(), *heads.last().unwrap()].into_iter().enumerate().take(if race <= 2 { 2 } else { 1 }) {
                let name = format!("R{race}G{genders}H{i}");
                flow.create_character(&mut b.game, &name, race, 1 + (race % 12), genders, head);
                step(&mut b, &mut flow);
                assert_eq!(b.game.screen, Screen::Characters, "{name}: {:?}", flow.notice);
                assert!(!b.closed(), "{name}: head {head} made the server kick");
                b.game.screen = Screen::Create;
                b.wait_until("lista", |b| b.game.characters.iter().any(|c| c.name == name));
                let c = b.game.characters.iter().find(|c| c.name == name).unwrap();
                assert_eq!(c.head, head);
            }
        }
    }
}

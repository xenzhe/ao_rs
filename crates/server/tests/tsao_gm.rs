//! TSAO game master tools over the wire: Shift + click / minimap teleport (the client sends
//! `/TELEP YO map x y`), and the TSAO commands /GO, /IRCERCA, /BLOQ, /TRIGGER, /EXP, /KILL,
//! /LIBERAR, with ranks (a Consejero cannot use Dios commands, a player none).
use ao_protocol::p4::gm::GmC2S;
use ao_protocol::C2S;
use ao_testkit::{Bot, TestServer};

fn server() -> TestServer {
    std::env::set_var("AO_GM_SEED", "Admin:admin,Conse:consejero");
    TestServer::start_at(1, 50, 50)
}

fn cmd(s: &TestServer, b: &mut Bot, line: &str) {
    b.send(&C2S::Gm(GmC2S::Command { line: line.into() }));
    s.advance(50);
    b.sync();
}

fn near(a: (i32, i32), b: (i32, i32)) -> bool {
    (a.0 - b.0).abs() <= 3 && (a.1 - b.1).abs() <= 3
}

fn said(b: &Bot, t: &str) -> bool {
    b.console().any(|l| l.contains(t))
}

#[test]
fn gm_teleport_and_tsao_commands() {
    let s = server();
    let mut admin = s.bot("Admin");
    let mut conse = s.bot("Conse");
    let mut user = s.bot("Pepe");

    // What Shift + click sends.
    cmd(&s, &mut admin, "/TELEP YO 1 60 60");
    admin.wait_until("teletransportado", |b| near(b.pos(), (60, 60)));
    // A Consejero may move himself, not use a Dios command.
    cmd(&s, &mut conse, "/TELEP YO 1 40 40");
    conse.wait_until("consejero se mueve", |b| near(b.pos(), (40, 40)));
    cmd(&s, &mut conse, "/EXP 5");
    assert!(said(&conse, "deshabilitado para tu cargo"), "denied");
    // A player cannot use any (the line is just chat, not a command).
    cmd(&s, &mut user, "/TELEP YO 1 70 70");
    assert!(!near(user.pos(), (70, 70)));

    cmd(&s, &mut admin, "/IRCERCA Pepe");
    admin.wait_until("cerca de Pepe", |b| (b.pos().0 - 50).abs() <= 3 && (b.pos().1 - 50).abs() <= 3);
    cmd(&s, &mut admin, "/TRIGGER 4");
    admin.wait_until("trigger", |b| said(b, "Trigger en"));
    cmd(&s, &mut admin, "/EXP 3");
    user.wait_until("anuncio", |b| said(b, "Multiplicador de experiencia: x3"));
    cmd(&s, &mut admin, "/KILL Pepe");
    user.wait_until("muerto", |b| b.hp() <= 0);
    cmd(&s, &mut admin, "/GO 2");
    admin.wait_until("mapa 2", |b| b.map() == 2);
}

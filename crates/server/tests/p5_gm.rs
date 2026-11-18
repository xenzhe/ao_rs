//! Phase 5, GM and moderation: temporary bans, inspecting players, moderation commands,
//! the help queue (SOS and reports) and the map / world commands, each with the VB rank.
use ao_protocol::p4::gm::GmRank;
use ao_protocol::{status, ChannelC2S, GmC2S, GmS2C, C2S, S2C};
use ao_rules::Dir;
use ao_testkit::{Bot, TestServer};

const CITY: (u16, i32, i32) = (1, 50, 50);
const DENIED: &str = "deshabilitado para tu cargo";

fn server() -> TestServer {
    std::env::set_var("AO_GM_SEED", "Admin:admin,Dios:dios,Semi:semidios,Conse:consejero");
    TestServer::with(|st| {
        st.npcs = false;
        st.start = Some(CITY);
    })
}

fn said(b: &Bot, needle: &str) -> bool {
    b.console().any(|l| l.contains(needle))
}

fn cmd(s: &TestServer, b: &mut Bot, line: &str) {
    b.say(line);
    s.settle();
    b.sync();
}

fn bot_of(s: &TestServer, rank: GmRank) -> Bot {
    s.bot(match rank {
        GmRank::User => "Pepe",
        GmRank::Consejero => "Conse",
        GmRank::SemiDios => "Semi",
        GmRank::Dios => "Dios",
        GmRank::Admin => "Admin",
    })
}

fn below(r: GmRank) -> GmRank {
    match r {
        GmRank::Admin => GmRank::Dios,
        GmRank::Dios => GmRank::SemiDios,
        GmRank::SemiDios => GmRank::Consejero,
        _ => GmRank::User,
    }
}

/// Every new command with its minimum rank. A plain user gets no answer at all and the
/// rank just below gets the VB "Comando deshabilitado para tu cargo", and nothing happens.
#[test]
fn every_command_needs_its_vb_rank() {
    let table: &[(&str, GmRank)] = &[
        ("/SHOW SOS", GmRank::Consejero),
        ("/ATENDER 1", GmRank::Consejero),
        ("/CR 5", GmRank::Consejero),
        ("/BANT Beto@x@1", GmRank::SemiDios),
        ("/STAT Beto", GmRank::SemiDios),
        ("/INV Beto", GmRank::SemiDios),
        ("/BOV Beto", GmRank::SemiDios),
        ("/SKILLS Beto", GmRank::SemiDios),
        ("/NICK2IP Beto", GmRank::SemiDios),
        ("/IP2NICK 127.0.0.1", GmRank::SemiDios),
        ("/ONLINEGM", GmRank::SemiDios),
        ("/ONLINEMAP", GmRank::SemiDios),
        ("/LIMPIAR", GmRank::SemiDios),
        ("/ESTUPIDO Beto", GmRank::Dios),
        ("/NOESTUPIDO Beto", GmRank::Dios),
        ("/BORRARPENA Beto@1@x", GmRank::Dios),
        ("/ECHARTODOSPJS", GmRank::Dios),
        ("/GRABAR", GmRank::Dios),
        ("/BORRAR SOS", GmRank::Dios),
        ("/CT 1 30 30", GmRank::Dios),
        ("/DT", GmRank::Dios),
        ("/MODMAPINFO PK 1", GmRank::Dios),
        ("/MOTDCAMBIA hackeado", GmRank::Dios),
        ("/GLOBAL", GmRank::Dios),
        ("/BAL Beto", GmRank::Admin),
    ];
    let s = server();
    let mut beto = s.bot("Beto");
    let mut pepe = bot_of(&s, GmRank::User);
    let mut gms: Vec<(GmRank, Bot)> = [GmRank::Consejero, GmRank::SemiDios, GmRank::Dios].into_iter().map(|r| (r, bot_of(&s, r))).collect();
    for (line, need) in table {
        pepe.clear_log();
        cmd(&s, &mut pepe, line);
        assert!(pepe.console().all(|l| !l.contains("Uso:") && !l.contains(DENIED) && !l.contains("Servidor »")), "{line}: a user learnt something: {:?}", pepe.console().collect::<Vec<_>>());
        let low = below(*need);
        if let Some((_, gm)) = gms.iter_mut().find(|(r, _)| *r == low) {
            gm.clear_log();
            cmd(&s, gm, line);
            assert!(said(gm, DENIED), "{line}: {low:?} was not refused: {:?}", gm.console().collect::<Vec<_>>());
        }
    }
    s.advance(10_000);
    beto.sync();
    assert!(beto.kicked().is_none() && !beto.closed(), "Beto was not banned nor kicked");
    assert_eq!(beto.map(), CITY.0);
    assert!(!beto.log().iter().any(|m| matches!(m, S2C::Status { flags, .. } if flags & status::DUMB != 0)), "nobody made Beto dumb");
    // Nor the MOTD nor the global chat changed.
    drop(pepe);
    let pepe = bot_of(&s, GmRank::User);
    assert!(!said(&pepe, "hackeado"));
}

#[test]
fn a_temporary_ban_keeps_the_character_out_and_needs_a_higher_rank() {
    let s = server();
    let mut semi = bot_of(&s, GmRank::SemiDios);
    let mut malo = s.connect();
    assert!(malo.create_account("cuentatemp", "secreta1"));
    assert!(malo.create_character("Temporal", 1, 3, 1, 1));
    malo.play("Temporal");

    cmd(&s, &mut semi, "/bant Temporal@flood@0");
    assert!(said(&semi, "Los días van de 1 a 255"));
    cmd(&s, &mut semi, "/bant Nadie@flood@2");
    assert!(said(&semi, "El personaje no existe"));
    cmd(&s, &mut semi, "/bant Dios@flood@2");
    assert!(said(&semi, "mayor o igual rango"));

    cmd(&s, &mut semi, "/bant Temporal@flood@2");
    malo.wait_until("baneado", |b| b.kicked().is_some_and(|r| r.contains("durante 2 días más")));
    semi.wait_until("aviso", |b| said(b, "Temporal fue baneado por flood durante los próximos 2 días"));
    let mut again = s.connect();
    assert!(again.account_login("cuentatemp", "secreta1"));
    again.send(&C2S::PlayCharacter { name: "Temporal".into() });
    again.wait_until("rechazado", |b| b.log().iter().any(|m| matches!(m, S2C::AccountResult { ok: false, message } if message.contains("durante 2 días más"))));
    cmd(&s, &mut semi, "/penas Temporal");
    assert!(said(&semi, "ban temporal por Semi 2880 min: flood"));
    cmd(&s, &mut semi, "/info Temporal");
    assert!(said(&semi, "baneado durante 2 días más: flood"));

    // /UNBAN (Dios) lifts it before its time.
    let mut dios = bot_of(&s, GmRank::Dios);
    drop(again);
    s.settle();
    s.advance(100);
    cmd(&s, &mut dios, "/unban Temporal");
    let mut back = s.connect();
    assert!(back.account_login("cuentatemp", "secreta1"));
    back.play("Temporal");
    assert!(back.kicked().is_none());
}

#[test]
fn inspecting_players_shows_their_data_and_hides_higher_gms() {
    let s = server();
    let mut semi = bot_of(&s, GmRank::SemiDios);
    let mut dios = bot_of(&s, GmRank::Dios);
    let mut admin = bot_of(&s, GmRank::Admin);
    let mut beto = s.bot("Beto");
    cmd(&s, &mut admin, "/dar Beto@12@500@x");
    beto.wait_until("oro", |b| b.game.local.gold >= 500);

    cmd(&s, &mut semi, "/stat Beto");
    assert!(said(&semi, "Pj: Beto") && said(&semi, "Oro en billetera: 500"));
    cmd(&s, &mut semi, "/inv Beto");
    assert!(said(&semi, "Beto: tiene"));
    cmd(&s, &mut semi, "/bov Beto");
    assert!(said(&semi, "Beto (bóveda): tiene 0 objetos"));
    cmd(&s, &mut semi, "/skills Beto");
    assert!(said(&semi, "Skills libres:"));
    cmd(&s, &mut semi, "/stat Nadie");
    assert!(said(&semi, "no está online"));
    cmd(&s, &mut admin, "/bal Beto");
    assert!(said(&admin, "El usuario Beto tiene 0 en el banco"));

    // IPs: a SemiDios only sees plain users; a Dios sees everyone.
    cmd(&s, &mut semi, "/nick2ip Beto");
    assert!(said(&semi, "El ip de Beto es 127.0.0.1"));
    let list = semi.console().find(|l| l.starts_with("Los personajes con ip")).unwrap().to_string();
    assert!(list.contains("Beto") && !list.contains("Dios") && !list.contains("Admin"), "{list}");
    semi.clear_log();
    cmd(&s, &mut semi, "/nick2ip Dios");
    assert!(said(&semi, "No hay ningún personaje con ese nick"));
    cmd(&s, &mut dios, "/ip2nick 127.0.0.1");
    let list = dios.console().find(|l| l.starts_with("Los personajes con ip")).unwrap().to_string();
    assert!(list.contains("Semi") && list.contains("Admin"), "{list}");

    // Online lists: Dioses and Admins only for a Dios or above.
    cmd(&s, &mut semi, "/onlinegm");
    let l = semi.console().last().unwrap().to_string();
    assert!(l.contains("Semi") && !l.contains("Dios") && !l.contains("Admin"), "{l}");
    cmd(&s, &mut dios, "/onlinegm");
    assert!(dios.console().last().unwrap().contains("Admin"));
    cmd(&s, &mut semi, "/onlinemap");
    assert!(said(&semi, "Usuarios en el mapa 1:") && semi.console().last().unwrap().contains("Beto"));
}

#[test]
fn moderation_dumb_penalties_kick_all_and_save() {
    let s = server();
    let mut dios = bot_of(&s, GmRank::Dios);
    let mut admin = bot_of(&s, GmRank::Admin);
    let mut beto = s.bot("Beto");
    let mut ana = s.bot("Ana");

    cmd(&s, &mut dios, "/estupido Beto");
    beto.wait_until("estupido", |b| b.log().iter().any(|m| matches!(m, S2C::Status { flags, .. } if flags & status::DUMB != 0)));
    s.advance(600_000);
    beto.sync();
    let last = beto.log().iter().rev().find_map(|m| if let S2C::Status { id, flags } = m { (*id == beto.id()).then_some(*flags) } else { None });
    assert!(last.is_some_and(|f| f & status::DUMB != 0), "lasts until a GM lifts it");
    cmd(&s, &mut dios, "/noestupido Beto");
    beto.wait_until("ya no", |b| b.log().iter().rev().find_map(|m| if let S2C::Status { id, flags } = m { (*id == b.id()).then_some(*flags) } else { None }).is_some_and(|f| f & status::DUMB == 0));
    cmd(&s, &mut dios, "/estupido Admin");
    assert!(said(&dios, "mayor o igual rango"));

    cmd(&s, &mut dios, "/advertencia Beto@spam");
    cmd(&s, &mut dios, "/borrarpena Beto@1@era una broma");
    assert!(said(&dios, "Pena modificada"));
    cmd(&s, &mut dios, "/borrarpena Beto@9@x");
    assert!(said(&dios, "no tiene la pena 9"));
    cmd(&s, &mut dios, "/penas Beto");
    assert!(said(&dios, "advertencia por Dios: Dios: <era una broma>"));

    cmd(&s, &mut dios, "/grabar");
    assert!(said(&dios, "Se han guardado todos los personajes"));

    cmd(&s, &mut dios, "/echartodospjs");
    beto.wait_until("echado", |b| b.kicked().is_some());
    ana.wait_until("echada", |b| b.kicked().is_some());
    s.advance(100);
    admin.sync();
    assert!(admin.kicked().is_none() && !admin.closed(), "GMs stay");
    assert!(said(&dios, "Se echó a 2 personajes"));
}

fn sos(b: &Bot) -> Vec<ao_protocol::p4::gm::GmSosRow> {
    b.log().iter().rev().find_map(|m| if let S2C::Gm(GmS2C::Sos { entries }) = m { Some(entries.clone()) } else { None }).unwrap_or_default()
}

#[test]
fn help_requests_and_reports_reach_the_gms_and_get_attended() {
    let s = server();
    let mut conse = bot_of(&s, GmRank::Consejero);
    let mut pepe = s.bot("Pepe");
    let mut beto = s.bot("Beto");

    cmd(&s, &mut pepe, "/gm");
    assert!(said(&pepe, "Uso: /GM texto"));
    cmd(&s, &mut pepe, "/gm Me quedé trabado en una pared");
    assert!(said(&pepe, "Tu mensaje fue recibido por el equipo de soporte"));
    conse.wait_until("aviso", |b| said(b, "nuevo mensaje de soporte de Pepe"));
    cmd(&s, &mut pepe, "/gm otra vez");
    assert!(said(&pepe, "una consulta cada 5 minutos"));
    cmd(&s, &mut pepe, "/gm Inventado@x");
    assert!(said(&pepe, "Tipos de consulta"));

    // A report carries what the reported player said.
    beto.say("sos un manco");
    s.settle();
    cmd(&s, &mut pepe, "/denunciar Beto");
    assert!(said(&pepe, "Tu denuncia fue recibida"));
    conse.wait_until("denuncia", |b| said(b, "nueva denuncia de parte de Pepe"));
    // Reporting someone offline answers the same and queues nothing; GMs cannot be reported.
    cmd(&s, &mut pepe, "/denunciar Fantasma");
    assert_eq!(pepe.console().filter(|l| l.contains("Tu denuncia fue recibida")).count(), 2);
    cmd(&s, &mut pepe, "/denunciar Conse");
    assert!(said(&pepe, "No podés denunciar a un administrador"));

    conse.send(&C2S::Gm(GmC2S::Refresh));
    conse.wait_until("lista", |b| sos(b).len() == 2);
    let rows = sos(&conse);
    assert_eq!((rows[0].name.as_str(), rows[0].kind.as_str()), ("Pepe", "Consulta regular"));
    assert!(rows[1].kind == "Denuncia a Beto" && rows[1].text.contains("sos un manco"), "{:?}", rows[1]);
    assert!(rows.iter().all(|r| r.online));
    cmd(&s, &mut conse, "/show sos");
    assert!(said(&conse, "Pedidos de ayuda (2)"));

    cmd(&s, &mut conse, &format!("/atender {}", rows[0].id));
    pepe.wait_until("atendido", |b| said(b, "Conse atendió tu consulta"));
    cmd(&s, &mut conse, &format!("/atender {}", rows[0].id));
    assert!(said(&conse, "No hay ningún pedido abierto"));
    // Clearing the queue is for a Dios.
    let mut dios = bot_of(&s, GmRank::Dios);
    assert!(said(&dios, "Hay 1 pedidos de ayuda sin atender"), "told on entering");
    cmd(&s, &mut dios, "/borrar sos");
    assert!(said(&dios, "Se borraron 1 pedidos"));
    conse.send(&C2S::Gm(GmC2S::Refresh));
    conse.wait_until("vacía", |b| sos(b).is_empty());
}

#[test]
fn world_commands_clean_teleport_motd_global_and_countdown() {
    let s = server();
    let mut dios = bot_of(&s, GmRank::Dios);
    let mut semi = bot_of(&s, GmRank::SemiDios);
    let mut admin = bot_of(&s, GmRank::Admin);

    // What players drop is gone with /LIMPIAR.
    cmd(&s, &mut admin, "/ci 12 50");
    let ground = |b: &Bot| b.log().iter().filter(|m| matches!(m, S2C::GroundItem { .. })).count();
    admin.wait_until("oro", |b| b.game.local.gold >= 50);
    admin.send(&C2S::DropGold { amount: 50 });
    admin.wait_until("tirado", |b| ground(b) > 0);
    cmd(&s, &mut semi, "/limpiar");
    assert!(said(&semi, "Se han limpiado los items del suelo (1)"));
    admin.wait_until("limpio", |b| b.log().iter().any(|m| matches!(m, S2C::GroundItemRemoved { .. })));

    // A teleport above the GM takes whoever walks on it.
    cmd(&s, &mut dios, "/telep YO 1 60 60");
    dios.wait_until("movido", |b| (b.pos().0 - 60).abs() <= 2 && (b.pos().1 - 60).abs() <= 2);
    let here = dios.pos();
    cmd(&s, &mut dios, "/ct 1 30 30");
    assert!(said(&dios, "Teleport creado"), "{:?}", dios.console().collect::<Vec<_>>());
    assert!(dios.walk(Dir::Up), "the tile above is walkable");
    dios.wait_until("teletransportado", |b| (b.pos().0 - 30).abs() <= 1 && (b.pos().1 - 30).abs() <= 1);
    cmd(&s, &mut dios, &format!("/telep YO 1 {} {}", here.0, here.1));
    dios.wait_until("vuelta", |b| b.pos() == here);
    cmd(&s, &mut dios, "/dt");
    assert!(said(&dios, "Teleport destruido"));
    s.advance(1000);
    dios.sync();
    assert!(dios.walk(Dir::Up));
    s.advance(500);
    dios.sync();
    assert_eq!(dios.pos(), (here.0, here.1 - 1), "the exit is gone");

    // MOTD: shown on entering and with /MOTD.
    cmd(&s, &mut dios, "/motdcambia Bienvenidos|Respeten las reglas");
    let mut pepe = s.bot("Pepe");
    assert!(said(&pepe, "Bienvenidos") && said(&pepe, "Respeten las reglas"));
    pepe.clear_log();
    cmd(&s, &mut pepe, "/motd");
    assert!(said(&pepe, "Bienvenidos"));
    cmd(&s, &mut dios, "/motdcambia -");
    pepe.clear_log();
    cmd(&s, &mut pepe, "/motd");
    assert!(said(&pepe, "No hay mensaje del día"));

    // /GLOBAL really turns the global chat off and on (the VB only announced it).
    cmd(&s, &mut dios, "/global");
    pepe.wait_until("aviso", |b| said(b, "Chat general deshabilitado"));
    pepe.send(&C2S::Channel(ChannelC2S::Global { text: "hola".into() }));
    s.settle();
    pepe.sync();
    assert!(said(&pepe, "El global se encuentra Desactivado"));
    cmd(&s, &mut dios, "/global");
    pepe.wait_until("aviso", |b| said(b, "Chat general habilitado"));

    // Map info: the GM's current map, here PK.
    cmd(&s, &mut dios, "/modmapinfo PK 1");
    assert!(said(&dios, "Mapa 1: Seguro = No"));
    cmd(&s, &mut dios, "/modmapinfo RESTRINGIR NEWBIE");
    assert!(said(&dios, "Newbie = Sí"));

    // Countdown to everyone.
    let conse = bot_of(&s, GmRank::Consejero);
    drop(conse);
    let mut conse = bot_of(&s, GmRank::Consejero);
    cmd(&s, &mut conse, "/cr 3");
    pepe.wait_until("empieza", |b| said(b, "cuenta regresiva desde: 3 segundos"));
    s.advance(3_100);
    pepe.wait_until("ya", |b| said(b, "¡Ya!"));
    assert!(said(&pepe, "2 segundos...!") && said(&pepe, "1 segundos...!"));
}

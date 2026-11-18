//! Phase 6 over the network: the new GM commands with their VB rank, a deathmatch lobby
//! joined with `/PARTICIPAR` and the players' event commands.
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

#[test]
fn every_new_command_needs_its_vb_rank() {
    let table: &[(&str, &str)] = &[
        ("/TELEPLOC", "Pepe"),
        ("/SLOT Beto@1", "Pepe"),
        ("/CREAREVENTO DEATHMATCH 2 1 47", "Conse"),
        ("/CONFIGLOBBY LIST", "Conse"),
        ("/CANCELAREVENTO", "Conse"),
        ("/FINALIZAREVENTO", "Conse"),
        ("/SEGUIR", "Conse"),
        ("/CC", "Conse"),
        ("/SPAWN 500", "Conse"),
        ("/TALKAS hola", "Conse"),
        ("/TRABAJANDO", "Conse"),
        ("/OCULTANDO", "Conse"),
        ("/LLUVIA", "Conse"),
        ("/NIEVE", "Conse"),
        ("/NIEBLA", "Conse"),
        ("/RESETINV", "Conse"),
        ("/MASSDEST", "Conse"),
        ("/RMATA", "Conse"),
        ("/CE INVASION BANDER", "Semi"),
        ("/EVENTOFACCIONARIO", "Semi"),
        ("/ANAME Beto@Roberto", "Semi"),
        ("/FORCEMIDI 3", "Semi"),
        ("/FORCEWAV 3", "Semi"),
        ("/FORCEWAVMAP 3", "Semi"),
        ("/SETDESC hola", "Semi"),
        ("/MASSKILL", "Semi"),
        ("/GENIO", "Semi"),
        ("/DOBACKUP", "Semi"),
        ("/CERRARCLIENTE Beto", "Dios"),
        ("/CREAREVENTO 2@10@2", "Dios"),
    ];
    let s = server();
    let mut beto = s.bot("Beto");
    let mut bots: Vec<Bot> = ["Pepe", "Conse", "Semi", "Dios"].iter().map(|n| s.bot(n)).collect();
    for (line, low) in table {
        let b = bots.iter_mut().find(|b| b.name() == *low).unwrap();
        b.clear_log();
        cmd(&s, b, line);
        if *low == "Pepe" {
            assert!(b.console().all(|l| !l.contains(DENIED)), "{line}: a user learnt it exists");
        } else {
            assert!(said(b, DENIED) || said(b, "Solo Administradores"), "{line}: {low} was not refused: {:?}", b.console().collect::<Vec<_>>());
        }
    }
    beto.sync();
    assert!(beto.kicked().is_none(), "Beto was not kicked");
    assert_eq!(beto.name(), "Beto", "nor renamed");
}

#[test]
fn players_join_a_deathmatch_with_participar() {
    let s = server();
    let mut dios = s.bot("Dios");
    let mut ana = s.bot("Ana");
    let mut beto = s.bot("Beto");
    cmd(&s, &mut dios, "/crearevento deathmatch 2 1 47");
    assert!(said(&dios, "Sala 1 creada"));
    ana.sync();
    assert!(said(&ana, "Se creó un evento de tipo"), "announced to everyone");
    cmd(&s, &mut ana, "/salas");
    assert!(said(&ana, "Sala 1: DeathMatch"));
    cmd(&s, &mut ana, "/participar");
    assert!(said(&ana, "Has Ingresado al evento."));
    ana.wait_until("summoned to the arena", |b| b.map() == 49);
    cmd(&s, &mut beto, "/participar");
    beto.wait_until("summoned too", |b| b.map() == 49);
    s.advance_secs(13);
    ana.sync();
    assert!(said(&ana, "¡Que inicie la masacre!"));
    cmd(&s, &mut dios, "/cancelarevento");
    s.advance_secs(12);
    ana.wait_until("back home", |b| b.map() == CITY.0);
    beto.wait_until("back home", |b| b.map() == CITY.0);
    cmd(&s, &mut ana, "/evento");
    assert!(said(&ana, "Actualmente no hay ningún evento en curso"));
}

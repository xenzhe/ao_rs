//! World clock (`WorldTime.bas`): every player gets the same hour from the server, the day
//! lasts `DuracionDia` (120 real minutes) and it starts at 08:00 with the server.
use ao_protocol::S2C;
use ao_testkit::TestServer;

fn clock(b: &ao_testkit::Bot) -> Option<(u64, u64)> {
    b.log().iter().find_map(|m| match m {
        S2C::WorldTime { elapsed_ms, day_ms } => Some((*elapsed_ms, *day_ms)),
        _ => None,
    })
}

#[test]
fn players_share_the_server_clock() {
    let s = TestServer::start();
    let mut a = s.bot("Madrugador");
    a.wait_until("hora", |b| clock(b).is_some());
    let (e1, day) = clock(&a).unwrap();
    assert_eq!(day, 120 * 60_000, "a 120-minute day");
    assert!((day / 3..day / 3 + 5_000).contains(&e1), "starts at 08:00: {e1}");
    s.advance(30 * 60_000);
    let mut b = s.bot("Trasnochador");
    b.wait_until("hora", |b| clock(b).is_some());
    let (e2, _) = clock(&b).unwrap();
    assert_eq!(e2 - e1, 30 * 60_000, "half an hour later for everyone");
}

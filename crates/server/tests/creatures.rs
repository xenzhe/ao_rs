//! A non-safe map gets populated with creatures from data/creatures.toml, and a safe one
//! (Ullathorpe) does not. Own test binary because it sets $AO_START.
use ao_protocol::*;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn creatures_seen(addr: std::net::SocketAddr, name: &str) -> (u16, Vec<(String, u16)>) {
    let mut sock = TcpStream::connect(addr).await.unwrap();
    sock.write_all(&encode(&C2S::Login { name: name.into(), version: PROTOCOL_VERSION })).await.unwrap();
    let mut reader = FrameReader::default();
    let mut buf = [0u8; 65536];
    let (mut map, mut seen) = (0, Vec::new());
    let deadline = tokio::time::Instant::now() + Duration::from_millis(800);
    while tokio::time::Instant::now() < deadline {
        let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(200), sock.read(&mut buf)).await else { continue };
        if n == 0 {
            break;
        }
        reader.push(&buf[..n]);
        while let Some(Ok(m)) = reader.next::<S2C>(MAX_SERVER_FRAME) {
            match m {
                S2C::EnterMap { map: m, .. } => map = m,
                S2C::Spawn { name, kind: EntityKind::Creature, body, .. } => seen.push((name, body)),
                _ => {}
            }
        }
    }
    (map, seen)
}

#[tokio::test]
async fn non_safe_maps_get_creatures_from_data() {
    std::env::set_var("AO_START", "2,50,50");
    let settings = server::Settings::defaults();
    let expected = settings.data.balance.creatures_per_map;
    let defs = settings.data.creatures.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(server::run(listener, settings));

    let (map, seen) = creatures_seen(addr, "Cazador").await;
    assert_eq!(map, 2);
    assert!(seen.len() * 10 >= expected * 7, "expected about {expected} creatures, got {}", seen.len());
    for (name, body) in &seen {
        assert!(defs.iter().any(|d| &d.name == name && d.body == *body), "{name} must come from creatures.toml");
    }
}

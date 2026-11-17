//! Walks through a real Ullathorpe exit and checks the server moves the player to the
//! destination map. Own test binary because it sets $AO_START for the whole process.
use ao_formats::{recursos_dir, AoMap};
use ao_protocol::*;
use ao_rules::{walk, Dir};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn walking_into_an_exit_changes_map() {
    let map = AoMap::load(&recursos_dir(), 1).expect("AO maps");
    let exit = map
        .exits
        .iter()
        .find(|e| walk::standable(&map, e.x, e.y + 1) && walk::can_enter(&map, e.x, e.y, Dir::Up))
        .copied()
        .expect("exit reachable from the south");
    std::env::set_var("AO_START", format!("1,{},{}", exit.x, exit.y + 1));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(server::run(listener, server::Settings::defaults()));
    let mut sock = TcpStream::connect(addr).await.unwrap();
    sock.write_all(&encode(&C2S::Login { name: "Viajero".into(), version: PROTOCOL_VERSION })).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    sock.write_all(&encode(&C2S::Walk { dir: Dir::Up, seq: 1 })).await.unwrap();

    let mut reader = FrameReader::default();
    let mut buf = [0u8; 65536];
    let mut maps = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(300), sock.read(&mut buf)).await else { continue };
        if n == 0 {
            break;
        }
        reader.push(&buf[..n]);
        while let Some(Ok(m)) = reader.next::<S2C>(MAX_SERVER_FRAME) {
            if let S2C::EnterMap { map, x, y } = m {
                maps.push((map, x, y));
            }
        }
    }
    assert_eq!(maps.first().map(|m| m.0), Some(1), "login enters the start map");
    let last = maps.last().unwrap();
    assert_eq!(last.0, exit.map, "the exit must take us to its destination map");
    assert!((last.1 - exit.dest_x).abs() <= 2 && (last.2 - exit.dest_y).abs() <= 2, "near the destination tile");
}

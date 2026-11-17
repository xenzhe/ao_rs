//! Networking: accepts connections, decodes frames with size and rate limits, and forwards
//! them to the world as events. Nothing here touches game state.
use crate::clock::{Clock, GameTime};
use crate::config::SecurityConfig;
use ao_protocol::{C2S, FrameError, FrameReader, MAX_CLIENT_FRAME};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tracing::info;

pub type ConnId = u32;

pub enum Event {
    Connected { id: ConnId, addr: SocketAddr, tx: UnboundedSender<Vec<u8>> },
    Frame { id: ConnId, msg: C2S },
    Violation { id: ConnId, reason: &'static str },
    Disconnected { id: ConnId },
}

/// Sending an empty buffer through a connection's channel closes it.
pub const CLOSE: Vec<u8> = Vec::new();

/// The flood limit counts frames per second of game time (the world's clock), so it is
/// the same in production and under a test's virtual clock.
pub async fn accept_loop(listener: TcpListener, events: UnboundedSender<Event>, security: SecurityConfig, clock: Clock) {
    let mut next_id: ConnId = 1;
    loop {
        let Ok((sock, addr)) = listener.accept().await else { continue };
        let _ = sock.set_nodelay(true);
        let id = next_id;
        next_id += 1;
        info!(id, %addr, "conexion");
        tokio::spawn(connection(sock, addr, id, events.clone(), security.max_msgs_per_sec, clock.clone()));
    }
}

async fn connection(sock: TcpStream, addr: SocketAddr, id: ConnId, ev: UnboundedSender<Event>, max_per_sec: u32, clock: Clock) {
    let (mut rd, mut wr) = sock.into_split();
    let (out_tx, mut out_rx) = unbounded_channel::<Vec<u8>>();
    let _ = ev.send(Event::Connected { id, addr, tx: out_tx });

    let writer = tokio::spawn(async move {
        while let Some(bytes) = out_rx.recv().await {
            if bytes.is_empty() || wr.write_all(&bytes).await.is_err() {
                break;
            }
        }
        let _ = wr.shutdown().await;
    });

    let mut reader = FrameReader::default();
    let mut buf = [0u8; 2048];
    let mut window = GameTime::ZERO;
    let mut count = 0u32;
    'outer: loop {
        let n = match rd.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        reader.push(&buf[..n]);
        while let Some(frame) = reader.next::<C2S>(MAX_CLIENT_FRAME) {
            let violation = match frame {
                Ok(msg) => {
                    let now = clock.now();
                    if now.since(window) >= 1000 {
                        window = now;
                        count = 0;
                    }
                    count += 1;
                    if count > max_per_sec {
                        Some("flood de paquetes")
                    } else {
                        let _ = ev.send(Event::Frame { id, msg });
                        None
                    }
                }
                Err(FrameError::TooBig) => Some("paquete demasiado grande"),
                Err(FrameError::Malformed) => Some("paquete malformado"),
            };
            if let Some(reason) = violation {
                let _ = ev.send(Event::Violation { id, reason });
                break 'outer;
            }
        }
    }
    let _ = ev.send(Event::Disconnected { id });
    let _ = writer.await;
}

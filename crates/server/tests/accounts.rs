//! Accounts with several characters: full flow over TCP against a real server, plus the
//! ways a client could try to abuse it. (Will move to ao-testkit with the virtual clock.)
use ao_formats::{recursos_dir, AoMap};
use ao_protocol::*;
use ao_rules::{walk, Dir, WALK_MS};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};

struct Conn {
    sock: TcpStream,
    reader: FrameReader,
}

impl Conn {
    async fn open(addr: &str) -> Conn {
        Conn { sock: TcpStream::connect(addr).await.unwrap(), reader: FrameReader::default() }
    }
    async fn send(&mut self, m: C2S) {
        self.sock.write_all(&encode(&m)).await.unwrap();
    }
    /// Messages for `ms`, and whether the server closed the connection.
    async fn drain(&mut self, ms: u64) -> (Vec<S2C>, bool) {
        let mut out = Vec::new();
        let mut buf = [0u8; 65536];
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            while let Some(Ok(m)) = self.reader.next::<S2C>(MAX_SERVER_FRAME) {
                out.push(m);
            }
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                return (out, false);
            }
            match timeout(left, self.sock.read(&mut buf)).await {
                Ok(Ok(0)) | Ok(Err(_)) => return (out, true),
                Ok(Ok(n)) => self.reader.push(&buf[..n]),
                Err(_) => return (out, false),
            }
        }
    }
}

fn result(msgs: &[S2C]) -> Option<(bool, String)> {
    msgs.iter().rev().find_map(|m| if let S2C::AccountResult { ok, message } = m { Some((*ok, message.clone())) } else { None })
}

fn list(msgs: &[S2C]) -> Option<Vec<CharacterSummary>> {
    msgs.iter().rev().find_map(|m| if let S2C::CharacterList { characters } = m { Some(characters.clone()) } else { None })
}

fn entered(msgs: &[S2C]) -> Option<(u16, i32, i32)> {
    msgs.iter().find_map(|m| if let S2C::EnterMap { map, x, y } = m { Some((*map, *x, *y)) } else { None })
}

async fn start() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(server::run(listener, server::Settings::defaults()));
    addr
}

fn create(name: &str, race: u8, class: u8) -> C2S {
    C2S::CreateCharacter { name: name.into(), race, class, gender: 1, head: 1 }
}

#[tokio::test]
async fn account_with_two_characters_persists_position() {
    let addr = start().await;
    let mut c = Conn::open(&addr).await;
    c.send(C2S::CreateAccount { user: "xenzhe".into(), password: "secreta1".into(), version: PROTOCOL_VERSION }).await;
    let (msgs, _) = c.drain(400).await;
    assert_eq!(result(&msgs).map(|r| r.0), Some(true), "{msgs:?}");
    assert_eq!(list(&msgs).map(|l| l.len()), Some(0));

    c.send(create("Gandalf", 1, 1)).await;
    c.send(create("Aragorn", 6, 3)).await;
    let (msgs, _) = c.drain(300).await;
    let chars = list(&msgs).expect("list after creating");
    assert_eq!(chars.len(), 2);
    let orc = chars.iter().find(|c| c.name == "Aragorn").unwrap();
    assert_eq!(orc.body, 582, "official starting body for a male orc");

    // Play, take two legit steps, leave.
    c.send(C2S::PlayCharacter { name: "Gandalf".into() }).await;
    let (msgs, _) = c.drain(300).await;
    let (map, x, y) = entered(&msgs).expect("enters the world");
    let m = AoMap::load(&recursos_dir(), map).unwrap();
    let dir = [Dir::Right, Dir::Left, Dir::Down, Dir::Up]
        .into_iter()
        .find(|d| {
            let (dx, dy) = d.delta();
            (1..=2).all(|k| walk::can_enter(&m, x + k * dx, y + k * dy, *d) && m.exit(x + k * dx, y + k * dy).is_none())
        })
        .unwrap();
    for seq in 1..=2 {
        c.send(C2S::Walk { dir, seq }).await;
        sleep(Duration::from_millis(WALK_MS + 10)).await;
    }
    drop(c);
    sleep(Duration::from_millis(200)).await;

    // Back in: the character is where we left it.
    let mut c = Conn::open(&addr).await;
    c.send(C2S::AccountLogin { user: "XENZHE".into(), password: "secreta1".into(), version: PROTOCOL_VERSION }).await;
    let (msgs, _) = c.drain(400).await;
    assert_eq!(list(&msgs).map(|l| l.len()), Some(2));
    c.send(C2S::PlayCharacter { name: "Gandalf".into() }).await;
    let (msgs, _) = c.drain(300).await;
    let (dx, dy) = dir.delta();
    assert_eq!(entered(&msgs), Some((map, x + 2 * dx, y + 2 * dy)), "position saved on logout");
}

#[tokio::test]
async fn wrong_passwords_close_the_connection() {
    let addr = start().await;
    let mut c = Conn::open(&addr).await;
    c.send(C2S::CreateAccount { user: "victima".into(), password: "secreta1".into(), version: PROTOCOL_VERSION }).await;
    c.drain(400).await;
    drop(c);

    let mut attacker = Conn::open(&addr).await;
    for guess in ["123456", "password", "qwerty12"] {
        attacker.send(C2S::AccountLogin { user: "victima".into(), password: guess.into(), version: PROTOCOL_VERSION }).await;
    }
    let (msgs, closed) = attacker.drain(1500).await;
    assert!(closed, "3 failed logins close the connection");
    assert!(msgs.iter().all(|m| !matches!(m, S2C::CharacterList { .. })), "never sees the character list");
}

#[tokio::test]
async fn account_cannot_be_open_twice_nor_touch_other_characters() {
    let addr = start().await;
    let mut a = Conn::open(&addr).await;
    a.send(C2S::CreateAccount { user: "duenio".into(), password: "secreta1".into(), version: PROTOCOL_VERSION }).await;
    a.send(create("Propio", 1, 1)).await;
    a.drain(500).await;

    let mut b = Conn::open(&addr).await;
    b.send(C2S::AccountLogin { user: "duenio".into(), password: "secreta1".into(), version: PROTOCOL_VERSION }).await;
    let (msgs, _) = b.drain(400).await;
    assert_eq!(result(&msgs).map(|r| r.0), Some(false), "same account twice is refused");

    let mut other = Conn::open(&addr).await;
    other.send(C2S::CreateAccount { user: "ajeno".into(), password: "secreta1".into(), version: PROTOCOL_VERSION }).await;
    other.drain(400).await;
    other.send(C2S::PlayCharacter { name: "Propio".into() }).await;
    other.send(C2S::DeleteCharacter { name: "Propio".into() }).await;
    let (msgs, _) = other.drain(300).await;
    assert!(entered(&msgs).is_none(), "cannot play a character of another account");
    a.send(C2S::DeleteCharacter { name: "Nada".into() }).await;
    let (msgs, _) = a.drain(300).await;
    assert_eq!(result(&msgs).map(|r| r.0), Some(false));
}

#[tokio::test]
async fn forged_creation_data_is_kicked() {
    let addr = start().await;
    let mut c = Conn::open(&addr).await;
    c.send(C2S::CreateAccount { user: "tramposo".into(), password: "secreta1".into(), version: PROTOCOL_VERSION }).await;
    c.drain(400).await;
    c.send(create("Hacker", 9, 1)).await; // race 9 does not exist
    let (_, closed) = c.drain(400).await;
    assert!(closed);

    let mut d = Conn::open(&addr).await;
    d.send(create("SinCuenta", 1, 1)).await;
    let (_, closed) = d.drain(400).await;
    assert!(closed, "creating a character without an account is kicked");
}

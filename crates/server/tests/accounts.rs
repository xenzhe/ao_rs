//! Accounts with several characters on ao-testkit (virtual clock): the P0 exit scenario
//! plus the ways a client could try to abuse accounts.
use ao_protocol::{C2S, PROTOCOL_VERSION};
use ao_rules::WALK_MS;
use ao_testkit::TestServer;

/// P0 exit criterion: create an account with two characters, play, walk, leave, come
/// back and find everything persisted.
#[test]
fn account_with_two_characters_persists() {
    let s = TestServer::start();
    let mut c = s.connect();
    assert!(c.create_account("xenzhe", "secreta1"));
    assert!(c.characters().is_empty());
    assert!(c.create_character("Gandalf", 1, 1, 1, 1));
    assert!(c.create_character("Aragorn", 6, 3, 1, 1));
    assert_eq!(c.characters().len(), 2);
    let orc = c.characters().iter().find(|x| x.name == "Aragorn").unwrap();
    assert_eq!(orc.body, 582, "official starting body for a male orc (DarCuerpo)");

    // Official creation: a human mage starts with Constitution as hit points, Intelligence x
    // ManaInicial as mana, its EquipamientoInicial items and spells.
    c.play("Gandalf");
    c.wait_until("atributos iniciales", |b| b.game.local.attributes != [0; 5]);
    let attrs = c.game.local.attributes;
    assert_eq!(c.game.stats.max_hp.get(), attrs[3] as i32, "MaxHp = Constitucion");
    assert!(c.game.stats.max_mana.get() > 100, "mage mana from intelligence");
    drop(c);
    s.settle();
    let mut c = s.connect();
    assert!(c.account_login("xenzhe", "secreta1"));

    c.play("Gandalf");
    let dir = c.free_dir(2).expect("room to walk");
    for _ in 0..2 {
        assert!(c.walk(dir));
        s.advance(WALK_MS);
    }
    let (map, pos) = (c.map(), c.pos());
    drop(c);
    s.settle();

    let mut c = s.connect();
    assert!(c.account_login("XENZHE", "secreta1"), "user names ignore case");
    assert_eq!(c.characters().len(), 2);
    c.play("Gandalf");
    assert_eq!((c.map(), c.pos()), (map, pos), "position saved on logout");
}

#[test]
fn wrong_passwords_close_the_connection() {
    let s = TestServer::start();
    let mut owner = s.connect();
    assert!(owner.create_account("victima", "secreta1"));
    drop(owner);

    let mut attacker = s.connect();
    for guess in ["123456", "password", "qwerty12"] {
        assert!(!attacker.account_login("victima", guess));
    }
    attacker.wait_until("cerrada tras 3 fallos", |b| b.closed());
    assert!(attacker.characters().is_empty(), "never sees the character list");
}

#[test]
fn account_cannot_be_open_twice_nor_touch_other_characters() {
    let s = TestServer::start();
    let mut a = s.connect();
    assert!(a.create_account("duenio", "secreta1"));
    assert!(a.create_character("Propio", 1, 1, 1, 1));

    let mut b = s.connect();
    assert!(!b.account_login("duenio", "secreta1"), "same account twice is refused");

    let mut other = s.connect();
    assert!(other.create_account("ajeno", "secreta1"));
    other.send(&C2S::PlayCharacter { name: "Propio".into() });
    assert!(!other.account_request(&C2S::DeleteCharacter { name: "Propio".into() }).0);
    assert!(!other.in_map(), "cannot play a character of another account");
    assert!(!a.account_request(&C2S::DeleteCharacter { name: "Nada".into() }).0);
}

#[test]
fn forged_creation_data_is_kicked() {
    let s = TestServer::start();
    let mut c = s.connect();
    assert!(c.create_account("tramposo", "secreta1"));
    c.send(&C2S::CreateCharacter { name: "Hacker".into(), race: 9, class: 1, gender: 1, head: 1 });
    c.wait_until("expulsado por raza falsa", |b| b.closed());

    let mut d = s.connect();
    d.send(&C2S::CreateCharacter { name: "SinCuenta".into(), race: 1, class: 1, gender: 1, head: 1 });
    d.wait_until("expulsado sin cuenta", |b| b.closed());

    let mut e = s.connect();
    assert!(!e.account_request(&C2S::CreateAccount { user: "viejo".into(), password: "secreta1".into(), version: PROTOCOL_VERSION - 1 }).0, "old clients are refused");
}

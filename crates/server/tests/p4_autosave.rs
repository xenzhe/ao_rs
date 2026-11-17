//! Autosave: an online character is written to the database every 5 minutes, without
//! logging out (a crash loses at most one interval).
use ao_testkit::TestServer;

#[test]
fn online_characters_are_saved_every_five_minutes() {
    let path = std::env::temp_dir().join(format!("ao_rs_autosave_{}.db", std::process::id()));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
    }
    let db = path.to_str().unwrap().to_string();
    let s = TestServer::with(move |st| st.config.database = db);
    let mut b = s.connect();
    assert!(b.create_account("guardado", "secreta1"));
    assert!(b.create_character("Ahorrador", 1, 3, 1, 1));
    b.play("Ahorrador");
    b.wait_until("en el mundo", |b| b.game.me().is_some());

    // Earn gold while online (development command).
    b.send(&ao_protocol::C2S::Say { text: "/dar 12 777".into() });
    b.wait_until("oro", |b| b.game.local.gold >= 777);
    let gold = b.game.local.gold;
    let saved = || -> u32 {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.query_row("SELECT gold FROM character WHERE name = 'Ahorrador'", [], |r| r.get(0)).unwrap()
    };
    assert!(saved() < gold, "not saved yet");

    s.advance(5 * 60_000 + 100);
    b.sync();
    assert!(b.kicked().is_none());
    assert_eq!(saved(), gold, "saved while still online");
}

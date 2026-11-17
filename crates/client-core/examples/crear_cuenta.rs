//! Creates an account and a character on a running server through the real account flow:
//! `cargo run -p client-core --example crear_cuenta -- <user> <password> <character> [server]`.
//! (Tests and manual play; the server must be running.)
use client_core::p3::account::valid_heads;
use client_core::Game;
use std::time::{Duration, Instant};

fn wait(game: &mut Game, what: &str) -> (bool, String) {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        game.poll();
        if let Some(r) = game.account_response() {
            return r;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("sin respuesta del servidor: {what}");
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [user, pass, name] = [&a[0], &a[1], &a[2]];
    let server = a.get(3).cloned().unwrap_or_else(|| "127.0.0.1:7666".into());
    let mut g = Game::new(String::new(), server, ao_formats::recursos_dir());
    g.account_connect().expect("conectar");
    g.console.clear();
    g.account_register(user, pass).expect("datos de cuenta");
    let (ok, msg) = wait(&mut g, "cuenta");
    if !ok {
        // Already registered: log in instead.
        println!("registro: {msg}; entrando");
        g.console.clear();
        g.account_login(user, pass).expect("login");
        let (ok, msg) = wait(&mut g, "login");
        assert!(ok, "login: {msg}");
    }
    // Human male paladin (race 1, class 7), first valid head.
    let head = valid_heads(1, 1, |_| true)[0];
    g.console.clear();
    g.request_create_character(name, 1, 7, 1, head).expect("personaje");
    let (ok, msg) = wait(&mut g, "personaje");
    println!("personaje {name}: {} {msg}", if ok { "creado" } else { "NO creado:" });
}

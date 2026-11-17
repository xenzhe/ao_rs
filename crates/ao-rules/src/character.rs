//! Account and character creation rules (official AO20 values), shared by the server
//! (enforcement) and the client (creation screen).

/// Max characters per account (Declares.bas `MAX_PERSONAJES`).
pub const MAX_CHARACTERS: usize = 10;

pub const RACES: [(u8, &str); 6] = [(1, "Humano"), (2, "Elfo"), (3, "Drow"), (4, "Gnomo"), (5, "Enano"), (6, "Orco")];

pub const CLASSES: [(u8, &str); 12] = [
    (1, "Mago"),
    (2, "Clerigo"),
    (3, "Guerrero"),
    (4, "Asesino"),
    (5, "Bardo"),
    (6, "Druida"),
    (7, "Paladin"),
    (8, "Cazador"),
    (9, "Trabajador"),
    (10, "Pirata"),
    (11, "Ladron"),
    (12, "Bandido"),
];

pub const MALE: u8 = 1;
pub const FEMALE: u8 = 2;

pub fn valid_race(r: u8) -> bool {
    RACES.iter().any(|(id, _)| *id == r)
}

pub fn valid_class(c: u8) -> bool {
    CLASSES.iter().any(|(id, _)| *id == c)
}

pub fn valid_gender(g: u8) -> bool {
    g == MALE || g == FEMALE
}

/// Starting clothed body by race and gender (TCP.bas `DarCuerpo`).
pub fn starting_body(race: u8, gender: u8) -> u16 {
    match (race, gender) {
        (1, _) => 1,
        (2, _) => 2,
        (3, _) => 3,
        (4, _) | (5, _) => 300,
        (6, FEMALE) => 581,
        (6, _) => 582,
        _ => 1,
    }
}

/// Body with no armour equipped (General.bas `SetNakedBody`).
pub fn naked_body(race: u8, gender: u8) -> u16 {
    let base = if gender == FEMALE { 3006 } else { 3000 };
    let offset = match race {
        3 => 1, // Drow
        2 => 2, // Elfo
        4 => 3, // Gnomo
        5 => 4, // Enano
        6 => 5, // Orco
        _ => 0, // Humano
    };
    base + offset
}

/// Account user names: 3-20 letters, digits or underscores.
pub fn valid_account_name(user: &str) -> bool {
    (3..=20).contains(&user.chars().count()) && user.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Passwords: 6-64 characters, no control characters.
pub fn valid_password(pass: &str) -> bool {
    (6..=64).contains(&pass.chars().count()) && !pass.chars().any(|c| c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_bodies() {
        assert_eq!(starting_body(1, MALE), 1);
        assert_eq!(starting_body(6, MALE), 582);
        assert_eq!(starting_body(6, FEMALE), 581);
        assert_eq!(naked_body(1, MALE), 3000);
        assert_eq!(naked_body(3, FEMALE), 3007);
        assert_eq!(naked_body(6, FEMALE), 3011);
    }

    #[test]
    fn validation() {
        assert!(valid_race(6) && !valid_race(7) && !valid_race(0));
        assert!(valid_class(12) && !valid_class(13));
        assert!(valid_account_name("xen_zhe") && !valid_account_name("ab") && !valid_account_name("con espacio"));
        assert!(valid_password("secreta1") && !valid_password("corta"));
    }
}

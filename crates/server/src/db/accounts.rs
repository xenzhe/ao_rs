//! Accounts and characters.
use super::{now_unix, Db, DbError, DbResult};
use ao_rules::character::MAX_CHARACTERS;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand_core::OsRng;
use argon2::Argon2;
use rusqlite::{params, OptionalExtension, Row};

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub id: i64,
    pub user: String,
}

/// Persistent part of a character.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterRecord {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub body: u16,
    pub head: u16,
    pub level: u32,
    pub exp: u32,
    pub hp: i32,
    pub max_hp: i32,
    pub mana: i32,
    pub max_mana: i32,
    pub map: u16,
    pub x: i32,
    pub y: i32,
}

/// The rest of a character: attributes, skills, vitals, gold, inventory, vault, spells.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterState {
    pub attributes: [u8; 5],
    pub skills: Vec<u8>,
    pub skill_points: u16,
    pub sta: i32,
    pub max_sta: i32,
    pub food: i32,
    pub water: i32,
    pub gold: u32,
    pub bank_gold: u32,
    /// (slot 1-based, obj, amount, equipped)
    pub inventory: Vec<(u8, u16, u32, bool)>,
    /// (slot 1-based, obj, amount)
    pub bank: Vec<(u8, u16, u32)>,
    /// (slot 1-based, Hechizos.dat id)
    pub spells: Vec<(u8, u16)>,
}

impl Default for CharacterState {
    fn default() -> Self {
        CharacterState {
            attributes: [18; 5],
            skills: vec![0; 24],
            skill_points: 0,
            sta: 40,
            max_sta: 40,
            food: 100,
            water: 100,
            gold: 0,
            bank_gold: 0,
            inventory: Vec::new(),
            bank: Vec::new(),
            spells: Vec::new(),
        }
    }
}

fn csv<T: ToString>(v: &[T]) -> String {
    v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
}

fn parse_csv(s: &str) -> Vec<u8> {
    s.split(',').filter_map(|p| p.trim().parse().ok()).collect()
}

/// Everything needed to insert a new character (already validated by the caller).
pub struct NewCharacter<'a> {
    pub name: &'a str,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub body: u16,
    pub head: u16,
    pub hp: i32,
    pub mana: i32,
    pub map: u16,
    pub x: i32,
    pub y: i32,
}

const CHAR_COLUMNS: &str = "id, account_id, name, race, class, gender, body, head, level, exp, hp, max_hp, mana, max_mana, map, x, y";

fn char_from_row(r: &Row) -> rusqlite::Result<CharacterRecord> {
    Ok(CharacterRecord {
        id: r.get(0)?,
        account_id: r.get(1)?,
        name: r.get(2)?,
        race: r.get(3)?,
        class: r.get(4)?,
        gender: r.get(5)?,
        body: r.get(6)?,
        head: r.get(7)?,
        level: r.get(8)?,
        exp: r.get(9)?,
        hp: r.get(10)?,
        max_hp: r.get(11)?,
        mana: r.get(12)?,
        max_mana: r.get(13)?,
        map: r.get(14)?,
        x: r.get(15)?,
        y: r.get(16)?,
    })
}

fn hash_password(pass: &str) -> DbResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(pass.as_bytes(), &salt).map(|h| h.to_string()).map_err(|e| DbError::Sql(e.to_string()))
}

impl Db {
    pub fn create_account(&self, user: &str, pass: &str) -> DbResult<Account> {
        let hash = hash_password(pass)?;
        self.conn.execute("INSERT INTO account (user, pass_hash, created_at) VALUES (?1, ?2, ?3)", params![user, hash, now_unix()])?;
        Ok(Account { id: self.conn.last_insert_rowid(), user: user.to_string() })
    }

    /// Checks the password. Unknown user and wrong password are the same error from the
    /// outside (`WrongPassword`), so the login screen cannot be used to discover accounts.
    pub fn verify_account(&self, user: &str, pass: &str) -> DbResult<Account> {
        let row: Option<(i64, String, String, i64)> = self
            .conn
            .query_row("SELECT id, user, pass_hash, banned FROM account WHERE user = ?1", params![user], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .optional()?;
        let Some((id, user, hash, banned)) = row else {
            // Spend the same time as a real check.
            let _ = hash_password(pass);
            return Err(DbError::WrongPassword);
        };
        let parsed = PasswordHash::new(&hash).map_err(|e| DbError::Sql(e.to_string()))?;
        if Argon2::default().verify_password(pass.as_bytes(), &parsed).is_err() {
            return Err(DbError::WrongPassword);
        }
        if banned != 0 {
            return Err(DbError::Banned);
        }
        self.conn.execute("UPDATE account SET last_login = ?1 WHERE id = ?2", params![now_unix(), id])?;
        Ok(Account { id, user })
    }

    pub fn characters(&self, account_id: i64) -> DbResult<Vec<CharacterRecord>> {
        let mut st = self.conn.prepare(&format!("SELECT {CHAR_COLUMNS} FROM character WHERE account_id = ?1 AND deleted = 0 ORDER BY id"))?;
        let rows = st.query_map(params![account_id], char_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn create_character(&self, account_id: i64, c: &NewCharacter) -> DbResult<CharacterRecord> {
        if self.characters(account_id)?.len() >= MAX_CHARACTERS {
            return Err(DbError::TooManyCharacters);
        }
        self.conn.execute(
            "INSERT INTO character (account_id, name, race, class, gender, body, head, hp, max_hp, mana, max_mana, map, x, y, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9, ?10, ?11, ?12, ?13)",
            params![account_id, c.name, c.race, c.class, c.gender, c.body, c.head, c.hp, c.mana, c.map, c.x, c.y, now_unix()],
        )?;
        self.load_character(account_id, c.name)
    }

    /// Only characters of this account (a client cannot load someone else's).
    pub fn load_character(&self, account_id: i64, name: &str) -> DbResult<CharacterRecord> {
        Ok(self.conn.query_row(&format!("SELECT {CHAR_COLUMNS} FROM character WHERE account_id = ?1 AND name = ?2 AND deleted = 0"), params![account_id, name], char_from_row)?)
    }

    /// Soft delete; the name becomes free again.
    pub fn delete_character(&self, account_id: i64, name: &str) -> DbResult<()> {
        let c = self.load_character(account_id, name)?;
        self.conn.execute("UPDATE character SET deleted = 1, name = name || '#' || id WHERE id = ?1", params![c.id])?;
        Ok(())
    }

    pub fn load_state(&self, char_id: i64) -> DbResult<CharacterState> {
        let (attrs, skills, skill_points, sta, max_sta, food, water, gold, bank_gold): (String, String, u16, i32, i32, i32, i32, u32, u32) = self.conn.query_row(
            "SELECT attributes, skills, skill_points, sta, max_sta, food, water, gold, bank_gold FROM character WHERE id = ?1",
            params![char_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
        )?;
        let mut s = CharacterState { skill_points, sta, max_sta, food, water, gold, bank_gold, ..CharacterState::default() };
        let a = parse_csv(&attrs);
        if a.len() == 5 {
            s.attributes.copy_from_slice(&a);
        }
        let k = parse_csv(&skills);
        if k.len() == 24 {
            s.skills = k;
        }
        let mut st = self.conn.prepare("SELECT slot, obj, amount, equipped FROM inventory_item WHERE character_id = ?1 ORDER BY slot")?;
        s.inventory = st.query_map(params![char_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? != 0)))?.collect::<rusqlite::Result<_>>()?;
        let mut st = self.conn.prepare("SELECT slot, obj, amount FROM bank_item WHERE character_id = ?1 ORDER BY slot")?;
        s.bank = st.query_map(params![char_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut st = self.conn.prepare("SELECT slot, spell FROM spell WHERE character_id = ?1 ORDER BY slot")?;
        s.spells = st.query_map(params![char_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(s)
    }

    /// Saves the row and the full state atomically (a crash never leaves half a save).
    pub fn save_full(&self, c: &CharacterRecord, s: &CharacterState) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        self.save_character(c)?;
        tx.execute(
            "UPDATE character SET attributes = ?2, skills = ?3, skill_points = ?4, sta = ?5, max_sta = ?6, food = ?7, water = ?8, gold = ?9, bank_gold = ?10 WHERE id = ?1",
            params![c.id, csv(&s.attributes), csv(&s.skills), s.skill_points, s.sta, s.max_sta, s.food, s.water, s.gold, s.bank_gold],
        )?;
        tx.execute("DELETE FROM inventory_item WHERE character_id = ?1", params![c.id])?;
        tx.execute("DELETE FROM bank_item WHERE character_id = ?1", params![c.id])?;
        tx.execute("DELETE FROM spell WHERE character_id = ?1", params![c.id])?;
        for (slot, obj, amount, equipped) in &s.inventory {
            tx.execute("INSERT INTO inventory_item (character_id, slot, obj, amount, equipped) VALUES (?1, ?2, ?3, ?4, ?5)", params![c.id, slot, obj, amount, *equipped as i64])?;
        }
        for (slot, obj, amount) in &s.bank {
            tx.execute("INSERT INTO bank_item (character_id, slot, obj, amount) VALUES (?1, ?2, ?3, ?4)", params![c.id, slot, obj, amount])?;
        }
        for (slot, spell) in &s.spells {
            tx.execute("INSERT INTO spell (character_id, slot, spell) VALUES (?1, ?2, ?3)", params![c.id, slot, spell])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn save_character(&self, c: &CharacterRecord) -> DbResult<()> {
        let n = self.conn.execute(
            "UPDATE character SET body = ?2, head = ?3, level = ?4, exp = ?5, hp = ?6, max_hp = ?7, mana = ?8, max_mana = ?9, \
             map = ?10, x = ?11, y = ?12, last_played = ?13 WHERE id = ?1 AND deleted = 0",
            params![c.id, c.body, c.head, c.level, c.exp, c.hp, c.max_hp, c.mana, c.max_mana, c.map, c.x, c.y, now_unix()],
        )?;
        if n == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_char(name: &str) -> NewCharacter<'_> {
        NewCharacter { name, race: 1, class: 1, gender: 1, body: 1, head: 1, hp: 100, mana: 120, map: 1, x: 50, y: 50 }
    }

    #[test]
    fn accounts_and_passwords() {
        let db = Db::open(":memory:").unwrap();
        assert_eq!(db.schema_version(), crate::db::MIGRATIONS.len());
        let a = db.create_account("Xenzhe", "secreta1").unwrap();
        assert_eq!(db.create_account("xenzhe", "otra1234").unwrap_err(), DbError::Taken, "names ignore case");
        assert_eq!(db.verify_account("XENZHE", "secreta1").unwrap().id, a.id);
        assert_eq!(db.verify_account("xenzhe", "mala1234").unwrap_err(), DbError::WrongPassword);
        assert_eq!(db.verify_account("nadie", "secreta1").unwrap_err(), DbError::WrongPassword, "unknown user looks the same");
    }

    #[test]
    fn characters_lifecycle() {
        let db = Db::open(":memory:").unwrap();
        let a = db.create_account("uno", "secreta1").unwrap();
        let b = db.create_account("dos", "secreta1").unwrap();
        db.create_character(a.id, &new_char("Gandalf")).unwrap();
        assert_eq!(db.create_character(b.id, &new_char("gandalf")).unwrap_err(), DbError::Taken, "names are global");
        assert_eq!(db.load_character(b.id, "Gandalf").unwrap_err(), DbError::NotFound, "not yours");

        let mut c = db.load_character(a.id, "Gandalf").unwrap();
        c.level = 5;
        c.x = 61;
        db.save_character(&c).unwrap();
        assert_eq!(db.load_character(a.id, "Gandalf").unwrap(), c);

        db.delete_character(a.id, "Gandalf").unwrap();
        assert!(db.characters(a.id).unwrap().is_empty());
        db.create_character(b.id, &new_char("Gandalf")).expect("deleted names are free again");
    }

    #[test]
    fn full_state_roundtrip() {
        let db = Db::open(":memory:").unwrap();
        assert_eq!(db.schema_version(), crate::db::MIGRATIONS.len());
        let a = db.create_account("estado", "secreta1").unwrap();
        let c = db.create_character(a.id, &new_char("Lleno")).unwrap();
        assert_eq!(db.load_state(c.id).unwrap(), CharacterState::default(), "new characters start with defaults");
        let mut skills = vec![0; 24];
        skills[3] = 50;
        let s = CharacterState {
            attributes: [19, 20, 18, 15, 21],
            skills,
            skill_points: 7,
            sta: 33,
            max_sta: 60,
            food: 80,
            water: 70,
            gold: 1500,
            bank_gold: 99_000,
            inventory: vec![(1, 2, 1, true), (5, 30, 250, false)],
            bank: vec![(1, 201, 3)],
            spells: vec![(1, 1), (2, 12)],
        };
        db.save_full(&c, &s).unwrap();
        assert_eq!(db.load_state(c.id).unwrap(), s);
        // Saving again replaces (an item moved out of slot 5 disappears from it).
        let s2 = CharacterState { inventory: vec![(1, 2, 1, true)], ..s.clone() };
        db.save_full(&c, &s2).unwrap();
        assert_eq!(db.load_state(c.id).unwrap().inventory, s2.inventory);
    }

    #[test]
    fn max_ten_characters() {
        let db = Db::open(":memory:").unwrap();
        let a = db.create_account("muchos", "secreta1").unwrap();
        for i in 0..MAX_CHARACTERS {
            db.create_character(a.id, &new_char(&format!("Pj{i}"))).unwrap();
        }
        assert_eq!(db.create_character(a.id, &new_char("Pj99")).unwrap_err(), DbError::TooManyCharacters);
    }

    #[test]
    fn file_database_persists_and_migrates_once() {
        let dir = std::env::temp_dir().join(format!("ao_db_test_{}", std::process::id()));
        let path = dir.join("t.db");
        let _ = std::fs::remove_file(&path);
        {
            let db = Db::open(path.to_str().unwrap()).unwrap();
            db.create_account("persiste", "secreta1").unwrap();
        }
        let db = Db::open(path.to_str().unwrap()).unwrap();
        assert_eq!(db.schema_version(), crate::db::MIGRATIONS.len());
        assert!(db.verify_account("persiste", "secreta1").is_ok());
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }
}

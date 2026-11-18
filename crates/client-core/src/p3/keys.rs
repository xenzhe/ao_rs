//! Rebindable game keys (TSAO `Teclas.tsao` / `BindKeys`), saved in `config/keys.ini` as
//! `action=KeyName`. Graphics-free: keys are names ("Up", "W", "LeftControl", "F6"); the
//! client maps them to its key codes.
use super::hud::config_dir;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Attack,
    PickUp,
    Drop,
    Use,
    Equip,
    Meditate,
    Safe,
    Hide,
    Steal,
    Tame,
    Inventory,
    Spells,
    Character,
    Guild,
    Party,
    Quests,
    WorldMap,
    Options,
    Events,
    Auction,
}

impl Action {
    pub const ALL: [Action; 24] = [
        Action::MoveUp,
        Action::MoveDown,
        Action::MoveLeft,
        Action::MoveRight,
        Action::Attack,
        Action::PickUp,
        Action::Drop,
        Action::Use,
        Action::Equip,
        Action::Meditate,
        Action::Safe,
        Action::Hide,
        Action::Steal,
        Action::Tame,
        Action::Inventory,
        Action::Spells,
        Action::Character,
        Action::Guild,
        Action::Party,
        Action::Quests,
        Action::WorldMap,
        Action::Options,
        Action::Events,
        Action::Auction,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Action::MoveUp => "move_up",
            Action::MoveDown => "move_down",
            Action::MoveLeft => "move_left",
            Action::MoveRight => "move_right",
            Action::Attack => "attack",
            Action::PickUp => "pick_up",
            Action::Drop => "drop",
            Action::Use => "use",
            Action::Equip => "equip",
            Action::Meditate => "meditate",
            Action::Safe => "safe",
            Action::Hide => "hide",
            Action::Steal => "steal",
            Action::Tame => "tame",
            Action::Inventory => "inventory",
            Action::Spells => "spells",
            Action::Character => "character",
            Action::Guild => "guild",
            Action::Party => "party",
            Action::Quests => "quests",
            Action::WorldMap => "world_map",
            Action::Options => "options",
            Action::Events => "events",
            Action::Auction => "auction",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::MoveUp => "Caminar arriba",
            Action::MoveDown => "Caminar abajo",
            Action::MoveLeft => "Caminar a la izquierda",
            Action::MoveRight => "Caminar a la derecha",
            Action::Attack => "Atacar",
            Action::PickUp => "Agarrar del suelo",
            Action::Drop => "Tirar objeto",
            Action::Use => "Usar objeto",
            Action::Equip => "Equipar objeto",
            Action::Meditate => "Meditar",
            Action::Safe => "Seguro",
            Action::Hide => "Ocultarse",
            Action::Steal => "Robar (y clic)",
            Action::Tame => "Domar (y clic)",
            Action::Inventory => "Inventario",
            Action::Spells => "Hechizos",
            Action::Character => "Personaje",
            Action::Guild => "Clan",
            Action::Party => "Grupo",
            Action::Quests => "Misiones",
            Action::WorldMap => "Mapa",
            Action::Options => "Opciones",
            Action::Events => "Eventos",
            Action::Auction => "Subastas",
        }
    }

    /// Movement keys repeat while held; the rest fire once per press.
    pub fn is_move(self) -> bool {
        matches!(self, Action::MoveUp | Action::MoveDown | Action::MoveLeft | Action::MoveRight)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBinds {
    keys: Vec<(Action, String)>,
}

impl Default for KeyBinds {
    /// Arrows to walk, the AO keys for actions (TSAO defaults), the Numic window keys.
    fn default() -> Self {
        Self::arrows()
    }
}

impl KeyBinds {
    pub fn arrows() -> KeyBinds {
        use Action::*;
        let k = [
            (MoveUp, "Up"),
            (MoveDown, "Down"),
            (MoveLeft, "Left"),
            (MoveRight, "Right"),
            (Attack, "LeftControl"),
            (PickUp, "A"),
            (Drop, "T"),
            (Use, "U"),
            (Equip, "E"),
            (Meditate, "F6"),
            (Safe, "S"),
            (Hide, "B"),
            (Steal, "J"),
            (Tame, "N"),
            (Inventory, "I"),
            (Spells, "H"),
            (Character, "C"),
            (Guild, "G"),
            (Party, "P"),
            (Quests, "Q"),
            (WorldMap, "M"),
            (Options, "O"),
            (Events, "V"),
            (Auction, "K"),
        ];
        KeyBinds { keys: k.iter().map(|(a, s)| (*a, s.to_string())).collect() }
    }

    /// WASD to walk; what used those letters moves out of the way.
    pub fn wasd() -> KeyBinds {
        let mut k = Self::arrows();
        for (a, key) in [(Action::MoveUp, "W"), (Action::MoveLeft, "A"), (Action::MoveDown, "S"), (Action::MoveRight, "D"), (Action::PickUp, "R"), (Action::Safe, "Z")] {
            k.set(a, key);
        }
        k
    }

    pub fn key(&self, a: Action) -> &str {
        self.keys.iter().find(|(x, _)| *x == a).map_or("", |(_, k)| k.as_str())
    }

    /// Binds `a` to `key`; an action that had that key swaps to `a`'s old key, so no two
    /// actions share one.
    pub fn set(&mut self, a: Action, key: &str) {
        let old = self.key(a).to_string();
        for (x, k) in self.keys.iter_mut() {
            if *x != a && k == key {
                *k = old.clone();
            }
        }
        if let Some((_, k)) = self.keys.iter_mut().find(|(x, _)| *x == a) {
            *k = key.to_string();
        }
    }

    pub fn parse(text: &str) -> KeyBinds {
        let mut k = KeyBinds::default();
        for line in text.lines() {
            let Some((id, key)) = line.split_once('=') else { continue };
            if let Some(a) = Action::ALL.iter().find(|a| a.id() == id.trim()) {
                let key = key.trim();
                if !key.is_empty() {
                    k.set(*a, key);
                }
            }
        }
        k
    }

    pub fn to_text(&self) -> String {
        Action::ALL.iter().map(|a| format!("{}={}\n", a.id(), self.key(*a))).collect()
    }

    pub fn path() -> PathBuf {
        config_dir().join("keys.ini")
    }

    pub fn load() -> KeyBinds {
        std::fs::read_to_string(Self::path()).map(|t| Self::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) -> bool {
        super::hud::write_config(&Self::path(), &self.to_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_never_share_a_key() {
        for k in [KeyBinds::arrows(), KeyBinds::wasd()] {
            let mut seen: Vec<&str> = Action::ALL.iter().map(|a| k.key(*a)).collect();
            seen.sort();
            let n = seen.len();
            seen.dedup();
            assert_eq!(seen.len(), n, "{k:?}");
        }
    }

    #[test]
    fn rebinding_swaps_and_round_trips() {
        let mut k = KeyBinds::arrows();
        k.set(Action::Meditate, "A"); // A was pick up
        assert_eq!((k.key(Action::Meditate), k.key(Action::PickUp)), ("A", "F6"));
        assert_eq!(KeyBinds::parse(&k.to_text()), k);
        assert_eq!(KeyBinds::parse("move_up=W\nbogus=X\n").key(Action::MoveUp), "W");
    }
}

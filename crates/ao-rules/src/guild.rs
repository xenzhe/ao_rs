//! Pure rules of guilds (alignment, ranks, limits). OWNER: Opus A (docs/P2_CONTRATO.md).
//!
//! Ported from the official AO20 server: `modGuilds.bas` (PuedeFundarUnClan,
//! m_EstadoPermiteEntrar, GuildNameValido, CheckClanExp, MiembrosPermite...) and the
//! guild levels of `Recursos/Dat/Clanes.dat` (loaded by `FileIO.bas` LoadGuildsConfig).
use ao_formats::ini::Ini;

/// `PuedeFundarUnClan`: minimum level to found a guild.
pub const FOUND_MIN_LEVEL: u32 = 23;
/// `PuedeFundarUnClan`: minimum `liderazgo` skill.
pub const FOUND_MIN_LEADERSHIP: u8 = 50;
/// `e_Skill.liderazgo = 11` (1-based in VB6) as a 0-based index of the skills array.
pub const SKILL_LEADERSHIP: usize = 10;
/// `PuedeFundarUnClan` / `HandleCreateNewGuild`: the four foundation gems (obj.dat
/// 407 green, 408 red, 409 blue, 412 polar), one of each, consumed when founding.
pub const FOUNDATION_GEMS: [u16; 4] = [407, 408, 409, 412];
/// `modGuilds.MAXASPIRANTES`.
pub const MAX_APPLICANTS: usize = 10;
/// `modGuilds.MAX_GUILDS`.
pub const MAX_GUILDS: usize = 1000;
/// `Declares.LimiteNewbie` (`EsNewbie`: level <= 12 cannot join a guild).
pub const NEWBIE_MAX_LEVEL: u32 = 12;
/// Not in VB6 (no limit there): longest guild name accepted.
pub const MAX_NAME_LEN: usize = 30;
/// Not in VB6: caps for free text (description/codex, news, application, reasons).
pub const MAX_DESC_LEN: usize = 400;
pub const MAX_NEWS_LEN: usize = 600;
pub const MAX_APPLICATION_LEN: usize = 200;

/// `e_ALINEACION_GUILD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Neutral = 0,
    Armada = 1,
    Chaos = 2,
    Citizen = 3,
    Criminal = 4,
}

impl Alignment {
    pub fn from_u8(v: u8) -> Option<Alignment> {
        Some(match v {
            0 => Alignment::Neutral,
            1 => Alignment::Armada,
            2 => Alignment::Chaos,
            3 => Alignment::Citizen,
            4 => Alignment::Criminal,
            _ => return None,
        })
    }

    /// `Alineacion2String`.
    pub fn name(self) -> &'static str {
        match self {
            Alignment::Neutral => "Neutral",
            Alignment::Armada => "Armada Real",
            Alignment::Chaos => "Legión Oscura",
            Alignment::Citizen => "Ciudadano",
            Alignment::Criminal => "Criminal",
        }
    }
}

/// `e_Facciones` as returned by `Status(UserIndex)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactionStatus {
    Criminal,
    Citizen,
    Armada,
    Chaos,
    /// `consejo` (Consejo de Banderbill, leaders of the Armada).
    Council,
    /// `concilio` (Concilio de las Sombras, leaders of the Legion).
    Conclave,
}

/// `m_EstadoPermiteEntrar`: may a character with `status` belong to a guild of `a`?
pub fn status_allows(a: Alignment, status: FactionStatus) -> bool {
    use FactionStatus::*;
    match a {
        Alignment::Neutral => matches!(status, Citizen | Criminal),
        Alignment::Armada => matches!(status, Armada | Council),
        Alignment::Chaos => matches!(status, Chaos | Conclave),
        Alignment::Citizen => matches!(status, Citizen | Armada),
        Alignment::Criminal => matches!(status, Criminal | Chaos),
    }
}

/// Everything `PuedeFundarUnClan` looks at, gathered by the server.
#[derive(Clone, Debug)]
pub struct Founder {
    pub in_guild: bool,
    pub level: u32,
    pub leadership: u8,
    /// Has one of each `FOUNDATION_GEMS`.
    pub has_gems: bool,
    /// `flags.Seguro`.
    pub safe_on: bool,
    pub status: FactionStatus,
}

/// `PuedeFundarUnClan`: why `f` may not found a guild of alignment `a` (official texts).
pub fn found_refusal(f: &Founder, a: Alignment) -> Option<&'static str> {
    use FactionStatus::*;
    if f.in_guild {
        return Some("Ya perteneces a un clan, no podés fundar otro.");
    }
    if f.level < FOUND_MIN_LEVEL || f.leadership < FOUND_MIN_LEADERSHIP || !f.has_gems {
        return Some("Para fundar un clan debes ser Nivel 23, tener 50 puntos en liderazgo y tener en tu inventario las Gemas de Fundación Verde, Roja, Azul y Polar.");
    }
    if a == Alignment::Citizen && !f.safe_on {
        return Some("Para fundar un clan ciudadano deberás tener activado el seguro.");
    }
    let s = f.status;
    match a {
        Alignment::Neutral if matches!(s, Chaos | Armada | Council | Conclave) => Some("Para fundar un clan neutral deberás ser ciudadano o criminal."),
        Alignment::Armada if !matches!(s, Armada | Council) => Some("Para fundar un clan de la Armada Real deberás pertenecer a la misma."),
        Alignment::Chaos if !matches!(s, Chaos | Conclave) => Some("Para fundar un clan de la Legión Oscura deberás pertenecer a la misma."),
        Alignment::Citizen if !matches!(s, Citizen | Armada) => Some("Para fundar un clan ciudadano deberás ser ciudadano."),
        Alignment::Criminal if !matches!(s, Criminal | Chaos) => Some("Para fundar un clan criminal deberás ser criminal o legión oscura."),
        _ => None,
    }
}

/// `GuildNameValido`: only letters a-z (any case), spaces and `ÿ`. Added here: not empty,
/// no leading/trailing or double spaces, at most `MAX_NAME_LEN` characters.
pub fn valid_name(name: &str) -> bool {
    let n = name.chars().count();
    n > 0
        && n <= MAX_NAME_LEN
        && name.trim() == name
        && !name.contains("  ")
        && name.chars().all(|c| c.is_ascii_alphabetic() || c == ' ' || c == 'ÿ')
}

/// Free text from the client: no control characters, capped length, trimmed.
pub fn clean_text(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

/// `e_RELACIONES_GUILD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    War = -1,
    Peace = 0,
    Allied = 1,
}

impl Relation {
    pub fn from_i64(v: i64) -> Relation {
        match v {
            -1 => Relation::War,
            1 => Relation::Allied,
            _ => Relation::Peace,
        }
    }
}

/// Guild levels of `Clanes.dat` (`[GUILDLEVELS]`, `[GUILDEXP]`, `[MEMBERSBYLEVEL]`,
/// `[GUILDPRICEACCEPTMEMBER]`). Index 0 = level 1.
#[derive(Clone, Debug, PartialEq)]
pub struct GuildLevels {
    pub max_level: u8,
    pub exp: Vec<u32>,
    pub members: Vec<u8>,
    pub accept_price: Vec<u32>,
}

impl GuildLevels {
    pub fn parse(ini: &Ini) -> GuildLevels {
        let get = |sec: &str, key: String| -> u32 {
            ini.section(sec).and_then(|s| s.get(&key.to_ascii_lowercase())).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0)
        };
        let max_level = get("GUILDLEVELS", "MaxGuildLevel".into()).clamp(1, 255) as u8;
        let range = 1..=max_level as u32;
        GuildLevels {
            max_level,
            exp: range.clone().map(|i| get("GUILDEXP", format!("GuildExpLevel{i}"))).collect(),
            members: range.clone().map(|i| get("MEMBERSBYLEVEL", format!("GuildMembersLevel{i}")).min(255) as u8).collect(),
            accept_price: range.map(|i| get("GUILDPRICEACCEPTMEMBER", format!("PriceAcceptMemberGuildLevel{i}"))).collect(),
        }
    }

    pub fn load(recursos: &std::path::Path) -> GuildLevels {
        GuildLevels::parse(&Ini::load(&recursos.join("Dat").join("Clanes.dat")))
    }

    fn at<T: Copy + Default>(v: &[T], level: u8) -> T {
        level.checked_sub(1).and_then(|i| v.get(i as usize)).copied().unwrap_or_default()
    }

    /// `MiembrosPermite`.
    pub fn members_allowed(&self, level: u8) -> usize {
        Self::at(&self.members, level) as usize
    }

    /// `PriceAcceptMemberGuild(level)`.
    pub fn accept_price(&self, level: u8) -> u32 {
        Self::at(&self.accept_price, level)
    }

    /// `GetRequiredExpForGuildLevel`.
    pub fn exp_required(&self, level: u8) -> u32 {
        Self::at(&self.exp, level)
    }

    /// `CheckClanExp`: new (level, exp) after the guild earns `gain`. At most one level per
    /// call, like the original; exp is 0 at the maximum level.
    pub fn add_exp(&self, level: u8, exp: u32, gain: u32) -> (u8, u32) {
        if level >= self.max_level {
            return (level, exp);
        }
        let need = self.exp_required(level);
        let mut exp = exp.saturating_add(gain);
        let mut level = level;
        if exp >= need {
            exp -= need;
            level += 1;
            if level >= self.max_level {
                exp = 0;
            }
        }
        (level, exp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn founder() -> Founder {
        Founder { in_guild: false, level: 23, leadership: 50, has_gems: true, safe_on: true, status: FactionStatus::Citizen }
    }

    #[test]
    fn founding_requirements() {
        assert_eq!(found_refusal(&founder(), Alignment::Neutral), None);
        assert_eq!(found_refusal(&founder(), Alignment::Citizen), None);
        assert!(found_refusal(&Founder { level: 22, ..founder() }, Alignment::Neutral).is_some());
        assert!(found_refusal(&Founder { leadership: 49, ..founder() }, Alignment::Neutral).is_some());
        assert!(found_refusal(&Founder { has_gems: false, ..founder() }, Alignment::Neutral).is_some());
        assert!(found_refusal(&Founder { in_guild: true, ..founder() }, Alignment::Neutral).unwrap().starts_with("Ya perteneces"));
        assert!(found_refusal(&Founder { safe_on: false, ..founder() }, Alignment::Citizen).unwrap().contains("seguro"));
        assert!(found_refusal(&founder(), Alignment::Armada).is_some());
        assert!(found_refusal(&founder(), Alignment::Chaos).is_some());
        assert!(found_refusal(&founder(), Alignment::Criminal).is_some());
        let crim = Founder { status: FactionStatus::Criminal, ..founder() };
        assert_eq!(found_refusal(&crim, Alignment::Criminal), None);
        assert_eq!(found_refusal(&Founder { status: FactionStatus::Armada, ..founder() }, Alignment::Armada), None);
        assert!(found_refusal(&Founder { status: FactionStatus::Armada, ..founder() }, Alignment::Neutral).is_some());
    }

    #[test]
    fn alignment_table() {
        use FactionStatus::*;
        assert!(status_allows(Alignment::Neutral, Criminal) && status_allows(Alignment::Neutral, Citizen) && !status_allows(Alignment::Neutral, Armada));
        assert!(status_allows(Alignment::Armada, Council) && !status_allows(Alignment::Armada, Citizen));
        assert!(status_allows(Alignment::Chaos, Conclave) && !status_allows(Alignment::Chaos, Criminal));
        assert!(status_allows(Alignment::Citizen, Armada) && !status_allows(Alignment::Citizen, Criminal));
        assert!(status_allows(Alignment::Criminal, Chaos) && !status_allows(Alignment::Criminal, Citizen));
        assert_eq!(Alignment::from_u8(5), None);
        assert_eq!(Alignment::from_u8(2).unwrap().name(), "Legión Oscura");
    }

    #[test]
    fn names() {
        assert!(valid_name("Los Pibes"));
        assert!(valid_name("ABC"));
        for bad in ["", " Pibes", "Pibes ", "Los  Pibes", "Pibes1", "Pib-es", "Ñandú", "abcdefghijabcdefghijabcdefghijx"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        assert_eq!(clean_text("  ho\nla \u{7} ", 10), "hola");
        assert_eq!(clean_text("abcdef", 3), "abc");
    }

    #[test]
    fn official_levels() {
        let l = GuildLevels::load(&ao_formats::recursos_dir());
        assert_eq!(l.max_level, 8);
        assert_eq!(l.members_allowed(1), 3);
        assert_eq!(l.members_allowed(5), 9, "trailing tab in GuildMembersLevel5");
        assert_eq!(l.members_allowed(8), 12);
        assert_eq!(l.accept_price(1), 500);
        assert_eq!(l.exp_required(1), 750);
        assert_eq!(l.exp_required(8), 0);
        assert_eq!(l.members_allowed(0), 0);
        assert_eq!(l.add_exp(1, 700, 100), (2, 50));
        assert_eq!(l.add_exp(1, 0, 100_000), (2, 99_250), "one level per call");
        assert_eq!(l.add_exp(7, 47_000, 2000), (8, 0));
        assert_eq!(l.add_exp(8, 0, 5000), (8, 0));
    }
}

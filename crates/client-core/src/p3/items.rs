//! Client actions and state for inventory, bank, shop and trade actions (`impl Game` blocks, graphics-free and
//! testable with bots). OWNER: Sonnet 2 (docs/P3_CONTRATO.md).
//!
//! Every action first checks what the server would check (the slot holds something, the
//! amount is not more than what is there, the gold is enough...) so an honest player never
//! sends a packet that would count as cheating; a refusal explains itself in the console.
//! The decisions are `plan_*` functions that return the message to send (or the reason), so
//! they are testable without a connection; the `Game` methods send the plan.
//!
//! Also here: the pure helpers the item windows share (what a double click does, which
//! equipment row an object belongs to, tooltips, parsing a typed quantity).
use crate::local::{ItemSlot, BANK_SLOTS, INVENTORY_SLOTS};
use crate::social::trade::PlayerTrade;
use crate::{Ent, Game};
use ao_formats::dat::obj::{ObjData, ObjType};
use ao_protocol::{EntityKind, Tone, TradeC2S, C2S};
use ao_rules::trade as rules;

/// First slots of the inventory without a backpack (AO20: 24, 42 with it).
pub const BASE_SLOTS: usize = 24;
/// `MAX_OFFER_ITEMS`: entries of one side of a player trade.
pub const MAX_OFFER_ITEMS: usize = rules::MAX_OFFER_ITEMS;

// ---------------------------------------------------------------- object helpers

/// What a double click on an object does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primary {
    Use,
    Equip,
}

/// Equipment is put on (and taken off) with `EquipItem`; everything else is used.
pub fn primary_action(t: ObjType) -> Primary {
    use ObjType::*;
    match t {
        Weapon | Armor | Shield | Helmet | Amulets | RingAccessory | Backpack | Arrows | WorkingTools | MagicalInstrument | MusicalInstruments | Saddles | SkinsWings | SkinsArmours | SkinsShields | SkinsHelmets | SkinsWeapons | SkinsBoats | SkinsSpells => Primary::Equip,
        _ => Primary::Use,
    }
}

/// The row of the equipment strip an equipped object shows in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipCat {
    Weapon,
    Shield,
    Helmet,
    Armor,
    Ring,
    Amulet,
    Other,
}

impl EquipCat {
    pub const ALL: [EquipCat; 7] = [EquipCat::Weapon, EquipCat::Shield, EquipCat::Helmet, EquipCat::Armor, EquipCat::Ring, EquipCat::Amulet, EquipCat::Other];

    pub fn label(self) -> &'static str {
        match self {
            EquipCat::Weapon => "Arma",
            EquipCat::Shield => "Escudo",
            EquipCat::Helmet => "Casco",
            EquipCat::Armor => "Armadura",
            EquipCat::Ring => "Anillo",
            EquipCat::Amulet => "Amuleto",
            EquipCat::Other => "Otro",
        }
    }

    /// Caption short enough for an empty slot of the equipment strip.
    pub fn short(self) -> &'static str {
        match self {
            EquipCat::Armor => "Cuerpo",
            EquipCat::Amulet => "Collar",
            c => c.label(),
        }
    }
}

/// Equipment strip category of an object type (None: it is not equipment).
pub fn equip_category(t: ObjType) -> Option<EquipCat> {
    use ObjType::*;
    Some(match t {
        Weapon => EquipCat::Weapon,
        Shield => EquipCat::Shield,
        Helmet => EquipCat::Helmet,
        Armor => EquipCat::Armor,
        RingAccessory => EquipCat::Ring,
        Amulets => EquipCat::Amulet,
        _ if primary_action(t) == Primary::Equip => EquipCat::Other,
        _ => return None,
    })
}

/// Spanish name of an object type, for tooltips.
pub fn type_name(t: ObjType) -> &'static str {
    use ObjType::*;
    match t {
        UseOnce => "Consumible",
        Weapon => "Arma",
        Armor => "Armadura",
        Trees => "Árbol",
        GoldCoin => "Oro",
        Doors => "Puerta",
        Backpack => "Mochila",
        SignBoards => "Cartel",
        Keys => "Llave",
        Potions => "Poción",
        Drinks => "Bebida",
        Wood => "Madera",
        Shield => "Escudo",
        Helmet => "Casco",
        WorkingTools => "Herramienta",
        Decorations => "Decoración",
        Amulets => "Amuleto",
        Minerals => "Mineral",
        Parchment => "Pergamino",
        MusicalInstruments => "Instrumento musical",
        BlacksmithMaterial => "Material de herrería",
        MagicalInstrument => "Instrumento mágico",
        Ships => "Barca",
        Arrows => "Munición",
        EmptyBottle => "Botella vacía",
        FullBottle => "Botella llena",
        RingAccessory => "Anillo",
        PassageTicket => "Pasaje",
        Map => "Mapa",
        Saddles => "Montura",
        RecallStones => "Piedra de recuerdo",
        Mail => "Correo",
        Chest => "Cofre",
        Quest => "Misión",
        UsableOnTarget => "Usable sobre objetivo",
        Plants => "Planta",
        ElementalRune => "Runa elemental",
        _ => "Objeto",
    }
}

/// Lines of the tooltip of an object: name first, then type, numbers and requirements.
/// `my_level` marks a level requirement the player does not meet.
pub fn tooltip_lines(o: &ObjData, amount: u32, equipped: bool, my_level: i32) -> Vec<String> {
    let mut v = vec![if equipped { format!("{} (equipado)", o.name) } else { o.name.clone() }];
    if amount > 1 {
        v.push(format!("Cantidad: {amount}"));
    }
    v.push(type_name(o.obj_type).to_string());
    use ObjType::*;
    match o.obj_type {
        Weapon => {
            v.push(format!("Daño: {}-{}", o.min_hit, o.max_hit));
            if o.two_handed != 0 {
                v.push("A dos manos".into());
            }
        }
        Armor | Shield | Helmet => {
            v.push(format!("Defensa: {}-{}", o.min_def, o.max_def));
            if o.magic_resistance > 0 {
                v.push(format!("Resistencia mágica: {}", o.magic_resistance));
            }
        }
        Arrows => v.push(format!("Daño: {}-{}", o.min_hit, o.max_hit)),
        Potions => {
            if o.max_modifier > 0 {
                v.push(format!("Efecto: {}-{}", o.min_modifier, o.max_modifier));
            }
        }
        UseOnce | Drinks => {
            if o.min_hunger > 0 {
                v.push(format!("Hambre: +{}", o.min_hunger));
            }
            if o.min_thirst > 0 {
                v.push(format!("Sed: +{}", o.min_thirst));
            }
            if o.min_hp > 0 {
                v.push(format!("Vida: +{}-{}", o.min_hp, o.max_hp.max(o.min_hp)));
            }
            if o.min_stamina > 0 {
                v.push(format!("Energía: +{}", o.min_stamina));
            }
        }
        _ => {}
    }
    if o.value > 0 {
        v.push(format!("Valor: {} oro", o.value));
    }
    if o.newbie != 0 {
        v.push("Objeto de novato".into());
    }
    if o.non_droppable != 0 {
        v.push("No se puede tirar".into());
    }
    if o.min_level > 1 {
        v.push(if my_level < o.min_level { format!("Requiere nivel {} (te falta)", o.min_level) } else { format!("Requiere nivel {}", o.min_level) });
    }
    if o.min_skill > 0 {
        v.push(format!("Habilidad mínima: {}", o.min_skill));
    }
    if o.male_only {
        v.push("Solo hombres".into());
    }
    if o.female_only {
        v.push("Solo mujeres".into());
    }
    if !o.forbidden_classes.is_empty() {
        let names: Vec<&str> = o.forbidden_classes.iter().map(|c| c.spanish_name()).collect();
        v.push(format!("No pueden usarlo: {}", names.join(", ")));
    }
    v
}

/// A typed quantity: digits only, at least 1, at most `max` (`None` when empty / zero).
pub fn parse_qty(text: &str, max: u32) -> Option<u32> {
    let n: u64 = text.trim().parse().ok()?;
    (n > 0 && max > 0).then(|| n.min(max as u64) as u32)
}

/// Gold a merchant pays for `amount` units of `o` (0 when it does not buy it).
pub fn sale_value(o: &ObjData, amount: u32) -> u64 {
    if o.no_tradeable_to_npc {
        return 0;
    }
    rules::sale_price(o.value, o.newbie != 0, amount)
}

// ---------------------------------------------------------------- categories, rarity, slots

/// What an object is for, as the bag shows it: filter tabs and frame colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Weapon,
    Armor,
    Potion,
    Food,
    Material,
    Magic,
    Tool,
    Other,
}

impl Category {
    pub const ALL: [Category; 8] = [Category::Weapon, Category::Armor, Category::Potion, Category::Food, Category::Material, Category::Magic, Category::Tool, Category::Other];

    /// Plural caption (filter tabs).
    pub fn label(self) -> &'static str {
        match self {
            Category::Weapon => "Armas",
            Category::Armor => "Armaduras",
            Category::Potion => "Pociones",
            Category::Food => "Comida y bebida",
            Category::Material => "Materiales",
            Category::Magic => "Magia y joyas",
            Category::Tool => "Útiles",
            Category::Other => "Varios",
        }
    }
}

/// The category an object type belongs to.
pub fn category(t: ObjType) -> Category {
    use ObjType::*;
    match t {
        Weapon | Arrows => Category::Weapon,
        Armor | Shield | Helmet => Category::Armor,
        Potions => Category::Potion,
        UseOnce | Drinks | FullBottle | EmptyBottle => Category::Food,
        Wood | Minerals | BlacksmithMaterial | Plants => Category::Material,
        Amulets | RingAccessory | MagicalInstrument | ElementalRune | Parchment | RecallStones | SkinsSpells => Category::Magic,
        WorkingTools | Backpack | Keys | MusicalInstruments | Saddles | Ships | Map | Mail | PassageTicket => Category::Tool,
        _ => Category::Other,
    }
}

/// How special an object is, from its price (obj.dat has no rarity field). Only things that
/// are worn or cast can be special; a bag of wood is never "legendary".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rarity {
    Common,
    Fine,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    pub fn label(self) -> &'static str {
        match self {
            Rarity::Common => "Común",
            Rarity::Fine => "Fino",
            Rarity::Rare => "Raro",
            Rarity::Epic => "Épico",
            Rarity::Legendary => "Legendario",
        }
    }
}

pub fn rarity(o: &ObjData) -> Rarity {
    if !matches!(category(o.obj_type), Category::Weapon | Category::Armor | Category::Magic) || o.obj_type == ObjType::Arrows {
        return Rarity::Common;
    }
    match o.value {
        v if v >= 40_000 => Rarity::Legendary,
        v if v >= 12_000 => Rarity::Epic,
        v if v >= 3_000 => Rarity::Rare,
        v if v >= 500 => Rarity::Fine,
        _ => Rarity::Common,
    }
}

/// A place of the doll where one object is worn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EquipSlot {
    Helmet,
    Amulet,
    Armor,
    Ring,
    Weapon,
    Shield,
    Ammo,
    Tool,
    Pack,
}

impl EquipSlot {
    pub const ALL: [EquipSlot; 9] = [EquipSlot::Helmet, EquipSlot::Amulet, EquipSlot::Armor, EquipSlot::Ring, EquipSlot::Weapon, EquipSlot::Shield, EquipSlot::Ammo, EquipSlot::Tool, EquipSlot::Pack];

    pub fn label(self) -> &'static str {
        match self {
            EquipSlot::Helmet => "Casco",
            EquipSlot::Amulet => "Collar",
            EquipSlot::Armor => "Armadura",
            EquipSlot::Ring => "Anillo",
            EquipSlot::Weapon => "Arma",
            EquipSlot::Shield => "Escudo",
            EquipSlot::Ammo => "Munición",
            EquipSlot::Tool => "Herramienta",
            EquipSlot::Pack => "Mochila",
        }
    }
}

/// The doll slot a wearable object goes to (None: not wearable, or an exotic kind that has
/// no slot of its own: instruments, mounts, skins).
pub fn equip_slot(t: ObjType) -> Option<EquipSlot> {
    use ObjType::*;
    Some(match t {
        Helmet => EquipSlot::Helmet,
        Amulets => EquipSlot::Amulet,
        Armor => EquipSlot::Armor,
        RingAccessory => EquipSlot::Ring,
        Weapon => EquipSlot::Weapon,
        Shield => EquipSlot::Shield,
        Arrows => EquipSlot::Ammo,
        WorkingTools => EquipSlot::Tool,
        Backpack => EquipSlot::Pack,
        _ => return None,
    })
}

/// A line of rol for objects whose obj.dat has no description of its own.
pub fn role_phrase(t: ObjType) -> &'static str {
    use ObjType::*;
    match t {
        Weapon => "Acero que ha visto más de una pelea.",
        Armor => "Protege el cuerpo de lo que el mundo te lance.",
        Shield => "Detrás de él, hasta el más cobarde respira.",
        Helmet => "Lo que importa se guarda bajo el casco.",
        Potions => "Un trago que separa la vida de la muerte.",
        UseOnce => "Para llenar el estómago antes del próximo viaje.",
        Drinks => "El agua es lo primero que se echa en falta.",
        Amulets => "Un talismán que pesa más de lo que parece.",
        RingAccessory => "Un aro pequeño con un poder callado.",
        Backpack => "Más sitio para cargar con tu historia.",
        Arrows => "Vuelan rectas si la mano es firme.",
        WorkingTools => "Los oficios los hacen manos callosas.",
        Minerals => "Piedra madre, a la espera de la forja.",
        Wood => "Madera noble, lista para el taller.",
        BlacksmithMaterial => "Metal que espera su turno en el yunque.",
        Parchment => "Palabras viejas que aún saben morder.",
        Keys => "Una puerta cerrada es una promesa.",
        MagicalInstrument => "Canaliza lo que la voz sola no alcanza.",
        EmptyBottle | FullBottle => "Un frasco: lo que valga lo pone su contenido.",
        _ => "Un objeto como tantos, hasta que alguien lo necesita.",
    }
}

// ---------------------------------------------------------------- rich tooltips

/// The player the tooltip speaks to (to mark in red what he does not meet).
#[derive(Clone, Debug, Default)]
pub struct Viewer {
    pub level: i32,
    /// `e_Facciones` of the player, when the server already told.
    pub faction: Option<u8>,
    /// Skill levels, index 0 = skill id 1.
    pub skills: Vec<u8>,
}

/// Tooltip of an object as lines for the item windows. Conventions (drawn by
/// `inventory::tooltip_at`): the first line is the name; `~` role-playing text; `!` a
/// requirement the viewer does not meet (red); `$` price line (gold); `?` hint (dim); the
/// rest are data.
pub fn rich_tooltip(o: &ObjData, amount: u32, equipped: bool, v: &Viewer) -> Vec<String> {
    use ObjType::*;
    let mut out = vec![o.name.clone()];
    let rar = rarity(o);
    let mut kind = type_name(o.obj_type).to_string();
    if rar > Rarity::Common {
        kind = format!("{kind} · {}", rar.label());
    }
    if equipped {
        kind = format!("{kind} · equipado");
    }
    out.push(kind);
    let desc = o.text.trim();
    // obj.dat's text is often just the numbers again ("Daño 1/3"): only prose is used.
    let flavour = if !matches!(o.obj_type, SignBoards | Doors | Mail) && !desc.is_empty() && desc.chars().count() <= 140 && !desc.chars().any(|c| c.is_ascii_digit()) { desc.to_string() } else { role_phrase(o.obj_type).to_string() };
    out.push(format!("~{flavour}"));
    if amount > 1 {
        out.push(format!("Cantidad: {amount}"));
    }
    match o.obj_type {
        Weapon => {
            out.push(format!("Daño: {}-{}", o.min_hit, o.max_hit));
            if o.two_handed != 0 {
                out.push("A dos manos".into());
            }
        }
        Armor | Shield | Helmet => {
            out.push(format!("Defensa: {}-{}", o.min_def, o.max_def));
            if o.magic_resistance > 0 {
                out.push(format!("Resistencia mágica: {}", o.magic_resistance));
            }
        }
        Arrows => out.push(format!("Daño: {}-{}", o.min_hit, o.max_hit)),
        Potions => {
            if o.max_modifier > 0 {
                out.push(format!("Efecto: {}-{}", o.min_modifier, o.max_modifier));
            }
        }
        UseOnce | Drinks => {
            if o.min_hunger > 0 {
                out.push(format!("Hambre: +{}", o.min_hunger));
            }
            if o.min_thirst > 0 {
                out.push(format!("Sed: +{}", o.min_thirst));
            }
            if o.min_hp > 0 {
                out.push(format!("Vida: +{}-{}", o.min_hp, o.max_hp.max(o.min_hp)));
            }
            if o.min_stamina > 0 {
                out.push(format!("Energía: +{}", o.min_stamina));
            }
        }
        _ => {}
    }
    if o.non_droppable != 0 {
        out.push("No se puede tirar".into());
    }
    if o.newbie != 0 {
        out.push("Objeto de novato: no se vende".into());
    }
    // ---- requirements: red when not met ----
    let mut req = |met: bool, text: String| out.push(if met { text } else { format!("!{text}") });
    if o.min_level > 1 {
        req(v.level >= o.min_level, format!("Requiere nivel {}{}", o.min_level, if v.level < o.min_level { format!(" (tienes {})", v.level) } else { String::new() }));
    }
    if let Some(sr) = o.skill_requirement {
        let have = v.skills.get(sr.skill.id() as usize - 1).copied().unwrap_or(0) as i32;
        req(have >= sr.level, format!("Requiere {} {}", sr.skill.spanish_name().unwrap_or("habilidad"), sr.level));
    }
    if o.royal != 0 {
        req(v.faction.is_none_or(ao_rules::faction::is_royal), "Solo para el Ejército Real".into());
    }
    if o.chaos != 0 {
        req(v.faction.is_none_or(ao_rules::faction::is_chaos), "Solo para la Legión Oscura".into());
    }
    // The client does not know the own class, race or sex: shown, never marked red.
    if o.male_only {
        out.push("Solo hombres".into());
    }
    if o.female_only {
        out.push("Solo mujeres".into());
    }
    if !o.forbidden_classes.is_empty() {
        let names: Vec<&str> = o.forbidden_classes.iter().map(|c| c.spanish_name()).collect();
        out.push(format!("Vetado a: {}", names.join(", ")));
    }
    if !o.forbidden_races.is_empty() {
        let names: Vec<&str> = o.forbidden_races.iter().map(|r| r.spanish_name()).collect();
        out.push(format!("Vetado a razas: {}", names.join(", ")));
    }
    if o.value > 0 {
        let sale = sale_value(o, 1);
        out.push(if sale > 0 { format!("$Valor {} · el mercader paga {}", o.value, sale) } else { format!("$Valor {}", o.value) });
    }
    out
}

// ---------------------------------------------------------------- organising the bag

/// How "Ordenar" arranges the bag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortMode {
    /// By kind (weapons, armour, jewels, potions...), then by name.
    Type,
    Name,
    /// Most valuable first.
    Value,
}

/// What an object is sorted by (the window builds it from obj.dat).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortKey {
    pub group: u16,
    pub name: String,
    pub value: i64,
    pub obj: u16,
}

/// Order of the kinds in a "by type" sort.
const TYPE_ORDER: [ObjType; 22] = [
    ObjType::Weapon,
    ObjType::Arrows,
    ObjType::Armor,
    ObjType::Shield,
    ObjType::Helmet,
    ObjType::RingAccessory,
    ObjType::Amulets,
    ObjType::MagicalInstrument,
    ObjType::ElementalRune,
    ObjType::Parchment,
    ObjType::Potions,
    ObjType::UseOnce,
    ObjType::Drinks,
    ObjType::FullBottle,
    ObjType::EmptyBottle,
    ObjType::WorkingTools,
    ObjType::Backpack,
    ObjType::Minerals,
    ObjType::Wood,
    ObjType::BlacksmithMaterial,
    ObjType::Plants,
    ObjType::Keys,
];

pub fn sort_key(obj: u16, o: Option<&ObjData>) -> SortKey {
    match o {
        Some(o) => SortKey { group: TYPE_ORDER.iter().position(|t| *t == o.obj_type).unwrap_or(TYPE_ORDER.len()) as u16, name: o.name.to_lowercase(), value: o.value as i64, obj },
        None => SortKey { group: 999, name: String::new(), value: 0, obj },
    }
}

fn cmp_keys(mode: SortMode, a: &SortKey, b: &SortKey) -> std::cmp::Ordering {
    match mode {
        SortMode::Type => (a.group, &a.name, a.obj).cmp(&(b.group, &b.name, b.obj)),
        SortMode::Name => (&a.name, a.obj).cmp(&(&b.name, b.obj)),
        SortMode::Value => (std::cmp::Reverse(a.value), &a.name, a.obj).cmp(&(std::cmp::Reverse(b.value), &b.name, b.obj)),
    }
}

/// What the server does with `SwapSlots { a, b }` (0-based cells): the same object merges
/// into `b` up to `MAX_STACK` (the rest stays in `a`), anything else swaps. Equipment
/// follows its object, so the flag travels with it. Mirror of `Server::on_swap_slots`.
pub fn apply_swap(cells: &mut [Option<ItemSlot>], a: usize, b: usize) {
    use ao_rules::items::MAX_STACK;
    match (cells[a], cells[b]) {
        (Some(x), Some(y)) if x.obj == y.obj => {
            let total = x.amount + y.amount;
            if total > MAX_STACK {
                cells[b] = Some(ItemSlot { amount: MAX_STACK, ..y });
                cells[a] = Some(ItemSlot { amount: total - MAX_STACK, ..x });
            } else {
                // The equipment field that pointed at `a` now points at `b`.
                cells[b] = Some(ItemSlot { amount: total, equipped: y.equipped || x.equipped, ..y });
                cells[a] = None;
            }
        }
        _ => cells.swap(a, b),
    }
}

/// Joins the loose stacks of every object (equipped ones stay), the rest going to a second
/// stack when the limit is reached. Appends the `SwapSlots` to `plan` and applies them to `cells`.
fn merge_stacks(cells: &mut [Option<ItemSlot>], plan: &mut Vec<(u8, u8)>) {
    use ao_rules::items::MAX_STACK;
    let mut objs: Vec<u16> = Vec::new();
    for it in cells.iter().flatten().filter(|i| !i.equipped) {
        if !objs.contains(&it.obj) {
            objs.push(it.obj);
        }
    }
    for obj in objs {
        let idx: Vec<usize> = (0..cells.len()).filter(|i| cells[*i].is_some_and(|c| c.obj == obj && !c.equipped)).collect();
        let Some(&first) = idx.first() else { continue };
        let mut dest = first;
        for &src in &idx[1..] {
            if cells[dest].is_none_or(|c| c.amount >= MAX_STACK) {
                dest = src;
                continue;
            }
            // Source and target are occupied, so the server always accepts it.
            apply_swap(cells, src, dest);
            plan.push((src as u8 + 1, dest as u8 + 1));
            if cells[src].is_some() {
                dest = src;
            }
        }
    }
}

/// Only the joining of equal stacks ("Juntar"): the `SwapSlots` for it.
pub fn plan_merge(slots: &[Option<ItemSlot>]) -> Vec<(u8, u8)> {
    let mut cells = slots.to_vec();
    let mut plan = Vec::new();
    merge_stacks(&mut cells, &mut plan);
    plan
}

/// The `SwapSlots` (1-based `a`, `b`) that leave the bag organised, as few as possible:
/// first stacks of the same object are joined (`merge`; equipped ones stay as they are),
/// then every object goes to its place with one swap per misplaced cell (a cycle of `n`
/// objects costs `n - 1`; an empty cell is a free place). Only slots the server accepts
/// are used: the source is occupied and the target is unlocked (`usable`) or occupied.
/// Two stacks of the same object are never swapped with each other (the server would
/// merge them), so a rare leftover (a second full stack) may stay out of order.
pub fn plan_organize(slots: &[Option<ItemSlot>], usable: usize, mode: SortMode, key: &dyn Fn(u16) -> SortKey, merge: bool) -> Vec<(u8, u8)> {
    let mut cells: Vec<Option<ItemSlot>> = slots.to_vec();
    let mut plan: Vec<(u8, u8)> = Vec::new();
    let legal = |cells: &[Option<ItemSlot>], a: usize, b: usize| a != b && cells[a].is_some() && (b < usable || cells[b].is_some());
    if merge {
        merge_stacks(&mut cells, &mut plan);
    }
    let mut want: Vec<(SortKey, usize, ItemSlot)> = cells.iter().enumerate().filter_map(|(i, c)| c.map(|c| (key(c.obj), i, c))).collect();
    want.sort_by(|(ka, ia, a), (kb, ib, b)| cmp_keys(mode, ka, kb).then(b.amount.cmp(&a.amount)).then(ia.cmp(ib)));
    for (i, (_, _, w)) in want.iter().enumerate() {
        if cells[i] == Some(*w) {
            continue;
        }
        if cells[i].is_some_and(|c| c.obj == w.obj) {
            continue;
        }
        let Some(j) = (i + 1..cells.len()).find(|j| cells[*j] == Some(*w)) else { continue };
        if !legal(&cells, j, i) {
            continue;
        }
        apply_swap(&mut cells, j, i);
        plan.push((j as u8 + 1, i as u8 + 1));
    }
    plan
}

// ---------------------------------------------------------------- plans

type Plan = Result<C2S, String>;

fn refuse<T>(why: &str) -> Result<T, String> {
    Err(why.to_string())
}

impl Game {
    fn dispatch(&mut self, plan: Plan) -> bool {
        match plan {
            Ok(m) => {
                self.send(&m);
                true
            }
            Err(why) => {
                if !why.is_empty() {
                    self.say(why, Tone::Warn);
                }
                false
            }
        }
    }

    fn held(&self, slot: u8) -> Result<ItemSlot, String> {
        self.local.slot(slot).ok_or_else(|| "Esa casilla está vacía.".to_string())
    }

    // ---- inventory ----

    pub fn plan_use(&self, slot: u8) -> Plan {
        self.held(slot).map(|_| C2S::UseItem { slot })
    }

    pub fn plan_equip(&self, slot: u8) -> Plan {
        self.held(slot).map(|_| C2S::EquipItem { slot })
    }

    pub fn plan_drop(&self, slot: u8, amount: u32) -> Plan {
        let it = self.held(slot)?;
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::DropItem { slot, amount: amount.min(it.amount) })
    }

    pub fn plan_drop_gold(&self, amount: u32) -> Plan {
        if self.local.gold == 0 {
            return refuse("No tienes oro.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::DropGold { amount: amount.min(self.local.gold) })
    }

    pub fn plan_swap(&self, a: u8, b: u8) -> Plan {
        let ok = |s: u8| (1..=INVENTORY_SLOTS as u8).contains(&s);
        if a == b || !ok(a) || !ok(b) {
            return refuse("");
        }
        self.held(a)?;
        Ok(C2S::SwapSlots { a, b })
    }

    pub fn inv_use(&mut self, slot: u8) -> bool {
        let p = self.plan_use(slot);
        self.dispatch(p)
    }

    pub fn inv_equip(&mut self, slot: u8) -> bool {
        let p = self.plan_equip(slot);
        self.dispatch(p)
    }

    /// Throws `amount` units (clamped to what the slot holds) on the floor.
    pub fn inv_drop(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_drop(slot, amount);
        self.dispatch(p)
    }

    pub fn inv_drop_gold(&mut self, amount: u32) -> bool {
        let p = self.plan_drop_gold(amount);
        self.dispatch(p)
    }

    pub fn inv_swap(&mut self, a: u8, b: u8) -> bool {
        let p = self.plan_swap(a, b);
        self.dispatch(p)
    }

    pub fn inv_pick_up(&mut self) {
        self.send(&C2S::PickUp);
    }

    /// Who the tooltips talk to: level, faction (once known) and skills.
    pub fn viewer(&self) -> Viewer {
        let f = &self.local.social.faction;
        Viewer { level: self.stats.level as i32, faction: f.known.then_some(f.status), skills: self.local.skills.clone() }
    }

    /// The swaps "Ordenar" would send now (`usable` = unlocked slots).
    pub fn plan_sort(&self, usable: usize, mode: SortMode, key: &dyn Fn(u16) -> SortKey, merge: bool) -> Vec<(u8, u8)> {
        plan_organize(&self.local.inventory, usable, mode, key, merge)
    }

    /// Joins equal stacks only ("Juntar"); returns how many `SwapSlots` were sent.
    pub fn inv_merge(&mut self) -> usize {
        let plan = plan_merge(&self.local.inventory);
        for (a, b) in &plan {
            self.send(&C2S::SwapSlots { a: *a, b: *b });
        }
        plan.len()
    }

    /// Organises the bag by sending the planned `SwapSlots`; returns how many were sent.
    pub fn inv_organize(&mut self, usable: usize, mode: SortMode, key: &dyn Fn(u16) -> SortKey, merge: bool) -> usize {
        let plan = self.plan_sort(usable, mode, key, merge);
        for (a, b) in &plan {
            self.send(&C2S::SwapSlots { a: *a, b: *b });
        }
        plan.len()
    }

    // ---- vault ----

    fn bank_slot(&self, slot: u8) -> Result<ItemSlot, String> {
        let b = self.local.bank.as_ref().ok_or("La bóveda está cerrada.")?;
        b.slots.get((slot as usize).wrapping_sub(1)).copied().flatten().ok_or_else(|| "Esa casilla de la bóveda está vacía.".to_string())
    }

    pub fn plan_deposit(&self, slot: u8, amount: u32) -> Plan {
        if self.local.bank.is_none() {
            return refuse("La bóveda está cerrada.");
        }
        let it = self.held(slot)?;
        if it.equipped {
            return refuse("Quítate el objeto antes de guardarlo.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::BankDeposit { slot, amount: amount.min(it.amount) })
    }

    pub fn plan_withdraw(&self, slot: u8, amount: u32) -> Plan {
        let it = self.bank_slot(slot)?;
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::BankWithdraw { slot, amount: amount.min(it.amount) })
    }

    pub fn plan_deposit_gold(&self, amount: u32) -> Plan {
        if self.local.bank.is_none() {
            return refuse("La bóveda está cerrada.");
        }
        if self.local.gold == 0 {
            return refuse("No llevas oro.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::BankDepositGold { amount: amount.min(self.local.gold) })
    }

    pub fn plan_withdraw_gold(&self, amount: u32) -> Plan {
        let held = self.local.bank.as_ref().ok_or("La bóveda está cerrada.")?.gold;
        if held == 0 {
            return refuse("No hay oro en la bóveda.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::BankWithdrawGold { amount: amount.min(held) })
    }

    pub fn bank_deposit(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_deposit(slot, amount);
        self.dispatch(p)
    }

    pub fn bank_withdraw(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_withdraw(slot, amount);
        self.dispatch(p)
    }

    pub fn bank_deposit_gold(&mut self, amount: u32) -> bool {
        let p = self.plan_deposit_gold(amount);
        self.dispatch(p)
    }

    pub fn bank_withdraw_gold(&mut self, amount: u32) -> bool {
        let p = self.plan_withdraw_gold(amount);
        self.dispatch(p)
    }

    pub fn bank_close(&mut self) {
        self.send(&C2S::BankClose);
    }

    /// Item of the open vault at `slot` (1-based).
    pub fn bank_item(&self, slot: u8) -> Option<ItemSlot> {
        self.bank_slot(slot).ok()
    }

    // ---- merchant ----

    /// Most units of merchant slot `slot` the purse allows (stock and gold).
    pub fn shop_max_buy(&self, slot: u8) -> u32 {
        let Some(it) = self.local.social.trade.shop.as_ref().and_then(|s| s.items.get(&slot)) else { return 0 };
        let by_gold = self.local.gold.checked_div(it.price).unwrap_or(it.amount);
        it.amount.min(by_gold)
    }

    pub fn plan_buy(&self, slot: u8, amount: u32) -> Plan {
        let shop = self.local.social.trade.shop.as_ref().ok_or("El comerciante no está atendiendo.")?;
        let it = shop.items.get(&slot).ok_or("El comerciante ya no tiene eso.")?;
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        let amount = amount.min(it.amount);
        // The server rounds the whole line, not each unit: never refuse for less than that.
        let lowest = (it.price as u64 * amount as u64).saturating_sub(amount as u64 - 1);
        if (self.local.gold as u64) < lowest {
            return refuse("No tienes oro suficiente.");
        }
        Ok(C2S::Trade(TradeC2S::Buy { slot, amount }))
    }

    pub fn plan_sell(&self, slot: u8, amount: u32) -> Plan {
        if self.local.social.trade.shop.is_none() {
            return refuse("El comerciante no está atendiendo.");
        }
        let it = self.held(slot)?;
        if it.equipped {
            return refuse("Quítate el objeto antes de venderlo.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        Ok(C2S::Trade(TradeC2S::Sell { slot, amount: amount.min(it.amount) }))
    }

    pub fn shop_buy(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_buy(slot, amount);
        self.dispatch(p)
    }

    pub fn shop_sell(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_sell(slot, amount);
        self.dispatch(p)
    }

    pub fn shop_close(&mut self) {
        self.send(&C2S::Trade(TradeC2S::CloseShop));
    }

    // ---- trade between players ----

    /// Players within trading range (Manhattan), nearest first: (entity id, name).
    pub fn nearby_players(&self) -> Vec<(u32, String)> {
        let Some(me) = self.me() else { return Vec::new() };
        let mut v: Vec<(i32, &u32, &Ent)> = self
            .ents
            .iter()
            .filter(|(id, e)| **id != self.my_id && e.kind == EntityKind::Player && (e.x - me.x).abs() + (e.y - me.y).abs() <= rules::PLAYER_TRADE_RANGE)
            .map(|(id, e)| ((e.x - me.x).abs() + (e.y - me.y).abs(), id, e))
            .collect();
        v.sort_by_key(|(d, id, _)| (*d, **id));
        v.into_iter().map(|(_, id, e)| (*id, e.name.clone())).collect()
    }

    pub fn plan_propose(&self, target: u32) -> Plan {
        if self.local.social.trade.player.is_some() {
            return refuse("Ya estás comerciando.");
        }
        let Some(e) = self.ents.get(&target).filter(|e| target != self.my_id && e.kind == EntityKind::Player) else { return refuse("Ese jugador no está a la vista.") };
        let Some(me) = self.me() else { return refuse("") };
        if (e.x - me.x).abs() + (e.y - me.y).abs() > rules::PLAYER_TRADE_RANGE {
            return refuse("Estás demasiado lejos para comerciar.");
        }
        Ok(C2S::Trade(TradeC2S::Propose { target }))
    }

    /// Asks `target` (entity id) to trade.
    pub fn trade_propose(&mut self, target: u32) -> bool {
        let p = self.plan_propose(target);
        if self.dispatch(p) {
            if let Some(name) = self.ents.get(&target).map(|e| e.name.clone()) {
                self.say(format!("Le propones comerciar a {name}."), Tone::Info);
            }
            return true;
        }
        false
    }

    /// Hook for a click on the world: proposes a trade to the player standing on tile
    /// `(tx, ty)`. Returns whether there was a player there to ask.
    pub fn trade_with_tile(&mut self, tx: i32, ty: i32) -> bool {
        match self.player_at(tx, ty) {
            Some(id) => {
                self.trade_propose(id);
                true
            }
            None => false,
        }
    }

    /// Answers the pending invitation with our own `Propose`.
    pub fn plan_accept_invite(&self) -> Plan {
        let (from, _) = self.local.social.trade.invited_by.clone().ok_or("Nadie te ha invitado a comerciar.")?;
        Ok(C2S::Trade(TradeC2S::Propose { target: from }))
    }

    pub fn plan_decline(&self) -> Plan {
        self.local.social.trade.invited_by.as_ref().ok_or("Nadie te ha invitado a comerciar.")?;
        Ok(C2S::Trade(TradeC2S::Decline))
    }

    /// Accepts the pending invitation (answers it with our own `Propose`).
    pub fn trade_accept_invite(&mut self) -> bool {
        let p = self.plan_accept_invite();
        self.dispatch(p)
    }

    pub fn trade_decline(&mut self) -> bool {
        let p = self.plan_decline();
        let ok = self.dispatch(p);
        self.local.social.trade.invited_by = None;
        ok
    }

    fn trade_open(&self) -> Result<&PlayerTrade, String> {
        self.local.social.trade.player.as_ref().ok_or_else(|| "No estás comerciando.".to_string())
    }

    /// Units of inventory slot `slot` not yet in the own offer.
    pub fn offerable(&self, slot: u8) -> u32 {
        let Some(it) = self.local.slot(slot) else { return 0 };
        if it.equipped {
            return 0;
        }
        let offered: u32 = self.local.social.trade.player.as_ref().map_or(0, |t| t.mine.items.iter().filter(|(o, _)| *o == it.obj).map(|(_, n)| *n).sum());
        // Another slot with the same object may hold the rest of the units.
        let total = self.local.count_of(it.obj);
        it.amount.min(total.saturating_sub(offered))
    }

    pub fn plan_offer(&self, slot: u8, amount: u32) -> Plan {
        let t = self.trade_open()?;
        let it = self.held(slot)?;
        if it.equipped {
            return refuse("Quítate el objeto antes de ofrecerlo.");
        }
        if amount == 0 {
            return refuse("Indica una cantidad.");
        }
        let free = self.offerable(slot);
        if free == 0 {
            return refuse("Ya has ofrecido todo lo que tienes de eso.");
        }
        if t.mine.items.len() >= MAX_OFFER_ITEMS && !t.mine.items.iter().any(|(o, _)| *o == it.obj) {
            return refuse("No caben más objetos distintos en la oferta.");
        }
        Ok(C2S::Trade(TradeC2S::Offer { slot, amount: amount.min(free) }))
    }

    pub fn plan_offer_gold(&self, amount: u32) -> Plan {
        self.trade_open()?;
        Ok(C2S::Trade(TradeC2S::OfferGold { amount: amount.min(self.local.gold) }))
    }

    pub fn plan_retract(&self, index: u8) -> Plan {
        let t = self.trade_open()?;
        if (index as usize) >= t.mine.items.len() {
            return refuse("");
        }
        Ok(C2S::Trade(TradeC2S::Retract { index }))
    }

    /// First confirmation of the current revision. Only while it is not confirmed yet.
    pub fn plan_confirm(&self) -> Plan {
        let t = self.trade_open()?;
        if t.i_confirmed {
            return refuse("");
        }
        if t.mine.items.is_empty() && t.mine.gold == 0 && t.theirs.items.is_empty() && t.theirs.gold == 0 {
            return refuse("La oferta está vacía.");
        }
        Ok(C2S::Trade(TradeC2S::Confirm { rev: t.rev }))
    }

    /// Second confirmation: only once both sides confirmed the same revision.
    pub fn plan_accept(&self) -> Plan {
        let t = self.trade_open()?;
        if !(t.i_confirmed && t.they_confirmed) || t.i_accepted {
            return refuse("");
        }
        Ok(C2S::Trade(TradeC2S::Accept { rev: t.rev }))
    }

    pub fn trade_offer(&mut self, slot: u8, amount: u32) -> bool {
        let p = self.plan_offer(slot, amount);
        self.dispatch(p)
    }

    pub fn trade_offer_gold(&mut self, amount: u32) -> bool {
        let p = self.plan_offer_gold(amount);
        self.dispatch(p)
    }

    pub fn trade_retract(&mut self, index: u8) -> bool {
        let p = self.plan_retract(index);
        self.dispatch(p)
    }

    pub fn trade_confirm(&mut self) -> bool {
        let p = self.plan_confirm();
        self.dispatch(p)
    }

    pub fn trade_accept(&mut self) -> bool {
        let p = self.plan_accept();
        self.dispatch(p)
    }

    pub fn trade_cancel(&mut self) {
        self.send(&C2S::Trade(TradeC2S::Cancel));
    }
}

/// Where the double confirmation of a player trade stands, for the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradePhase {
    /// Nobody confirmed: offers are being built.
    Building,
    /// Only I confirmed: waiting for them.
    WaitingThem,
    /// Only they confirmed: waiting for me.
    WaitingMe,
    /// Both confirmed: the second step (accept) is open.
    BothConfirmed,
    /// I accepted, waiting for their acceptance.
    WaitingAccept,
    /// They accepted, waiting for mine.
    TheyAccepted,
}

pub fn trade_phase(t: &PlayerTrade) -> TradePhase {
    match (t.i_confirmed, t.they_confirmed, t.i_accepted, t.they_accepted) {
        (true, true, true, _) => TradePhase::WaitingAccept,
        (true, true, false, true) => TradePhase::TheyAccepted,
        (true, true, _, _) => TradePhase::BothConfirmed,
        (true, false, ..) => TradePhase::WaitingThem,
        (false, true, ..) => TradePhase::WaitingMe,
        _ => TradePhase::Building,
    }
}

/// Inventory slots the player can really use: 24, or all of them with a backpack on.
pub fn usable_slots(has_backpack: bool) -> usize {
    if has_backpack {
        INVENTORY_SLOTS
    } else {
        BASE_SLOTS
    }
}

/// Unused here but shared by windows: vault capacity.
pub const VAULT_SLOTS: usize = BANK_SLOTS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::trade::{Offer, Shop, ShopItem};
    use std::path::PathBuf;

    fn game() -> Game {
        let mut g = Game::new("T".into(), "127.0.0.1:1".into(), PathBuf::new());
        g.local.inventory[0] = Some(ItemSlot { obj: 38, amount: 10, equipped: false });
        g.local.inventory[1] = Some(ItemSlot { obj: 3, amount: 1, equipped: true });
        g.local.gold = 500;
        g
    }

    fn msg(p: Plan) -> String {
        format!("{:?}", p)
    }

    #[test]
    fn inventory_plans_clamp_and_refuse() {
        let g = game();
        assert!(matches!(g.plan_use(1), Ok(C2S::UseItem { slot: 1 })));
        assert!(g.plan_use(5).is_err(), "empty slot");
        assert!(matches!(g.plan_drop(1, 99), Ok(C2S::DropItem { slot: 1, amount: 10 })), "clamped to the stack");
        assert!(g.plan_drop(1, 0).is_err());
        assert!(matches!(g.plan_drop_gold(9999), Ok(C2S::DropGold { amount: 500 })));
        assert!(matches!(g.plan_swap(1, 7), Ok(C2S::SwapSlots { a: 1, b: 7 })));
        assert!(g.plan_swap(1, 1).is_err() && g.plan_swap(1, 0).is_err() && g.plan_swap(1, 43).is_err(), "never a packet the server counts as cheating");
        assert!(g.plan_swap(9, 1).is_err(), "empty source");
    }

    #[test]
    fn vault_plans_need_an_open_vault_and_loose_items() {
        let mut g = game();
        assert!(g.plan_deposit(1, 1).is_err(), "closed vault");
        g.local.bank = Some(crate::local::Bank { slots: vec![None; BANK_SLOTS], gold: 40 });
        g.local.bank.as_mut().unwrap().slots[2] = Some(ItemSlot { obj: 38, amount: 5, equipped: false });
        assert!(matches!(g.plan_deposit(1, 4), Ok(C2S::BankDeposit { slot: 1, amount: 4 })));
        assert!(g.plan_deposit(2, 1).is_err(), "equipped item stays on");
        assert!(matches!(g.plan_withdraw(3, 50), Ok(C2S::BankWithdraw { slot: 3, amount: 5 })));
        assert!(g.plan_withdraw(4, 1).is_err());
        assert!(matches!(g.plan_deposit_gold(10_000), Ok(C2S::BankDepositGold { amount: 500 })));
        assert!(matches!(g.plan_withdraw_gold(10_000), Ok(C2S::BankWithdrawGold { amount: 40 })));
    }

    #[test]
    fn shop_plans_check_stock_and_purse() {
        let mut g = game();
        assert!(g.plan_buy(1, 1).is_err(), "no shop open");
        let mut shop = Shop::default();
        shop.items.insert(1, ShopItem { obj: 38, amount: 3, price: 100 });
        g.local.social.trade.shop = Some(shop);
        assert!(matches!(g.plan_buy(1, 2), Ok(C2S::Trade(TradeC2S::Buy { slot: 1, amount: 2 }))));
        assert!(matches!(g.plan_buy(1, 9), Ok(C2S::Trade(TradeC2S::Buy { slot: 1, amount: 3 }))), "clamped to the stock");
        assert_eq!(g.shop_max_buy(1), 3);
        g.local.gold = 150;
        assert!(g.plan_buy(1, 2).is_err(), "not enough gold");
        assert_eq!(g.shop_max_buy(1), 1);
        assert!(g.plan_buy(7, 1).is_err(), "no such slot");
        assert!(matches!(g.plan_sell(1, 3), Ok(C2S::Trade(TradeC2S::Sell { slot: 1, amount: 3 }))));
        assert!(g.plan_sell(2, 1).is_err(), "equipped");
    }

    #[test]
    fn offers_respect_the_units_already_on_the_table() {
        let mut g = game();
        assert!(g.plan_offer(1, 1).is_err(), "no trade open");
        g.local.social.trade.player = Some(PlayerTrade { with: 2, name: "Otro".into(), rev: 3, ..Default::default() });
        assert!(matches!(g.plan_offer(1, 4), Ok(C2S::Trade(TradeC2S::Offer { slot: 1, amount: 4 }))));
        g.local.social.trade.player.as_mut().unwrap().mine = Offer { items: vec![(38, 8)], gold: 0 };
        assert_eq!(g.offerable(1), 2);
        assert!(matches!(g.plan_offer(1, 9), Ok(C2S::Trade(TradeC2S::Offer { slot: 1, amount: 2 }))));
        assert!(matches!(g.plan_offer_gold(10_000), Ok(C2S::Trade(TradeC2S::OfferGold { amount: 500 }))));
        assert!(g.plan_retract(0).is_ok() && g.plan_retract(1).is_err());
        assert!(g.plan_offer(2, 1).is_err(), "equipped");
        g.local.social.trade.player.as_mut().unwrap().mine.items = (1..=6).map(|o| (o, 1)).collect();
        assert!(g.plan_offer(1, 1).is_err(), "seven different entries do not fit");
    }

    #[test]
    fn double_confirmation_order() {
        let mut g = game();
        g.local.social.trade.player = Some(PlayerTrade { with: 2, rev: 7, ..Default::default() });
        assert!(g.plan_confirm().is_err(), "an empty offer is not confirmed");
        g.local.social.trade.player.as_mut().unwrap().mine.gold = 10;
        assert!(msg(g.plan_confirm()).contains("rev: 7"));
        assert!(g.plan_accept().is_err(), "accept needs both confirmations");
        let t = g.local.social.trade.player.as_mut().unwrap();
        t.i_confirmed = true;
        assert_eq!(trade_phase(t), TradePhase::WaitingThem);
        t.they_confirmed = true;
        assert_eq!(trade_phase(t), TradePhase::BothConfirmed);
        assert!(g.plan_confirm().is_err(), "no second Confirm");
        assert!(matches!(g.plan_accept(), Ok(C2S::Trade(TradeC2S::Accept { rev: 7 }))));
        let t = g.local.social.trade.player.as_mut().unwrap();
        t.i_accepted = true;
        assert_eq!(trade_phase(t), TradePhase::WaitingAccept);
        assert!(g.plan_accept().is_err());
    }

    #[test]
    fn quantities_and_primary_actions() {
        assert_eq!(parse_qty("12", 100), Some(12));
        assert_eq!(parse_qty("500", 100), Some(100), "clamped");
        assert_eq!(parse_qty("0", 100), None);
        assert_eq!(parse_qty("", 100), None);
        assert_eq!(parse_qty("5", 0), None);
        assert_eq!(primary_action(ObjType::Weapon), Primary::Equip);
        assert_eq!(primary_action(ObjType::Potions), Primary::Use);
        assert_eq!(equip_category(ObjType::Helmet), Some(EquipCat::Helmet));
        assert_eq!(equip_category(ObjType::Backpack), Some(EquipCat::Other));
        assert_eq!(equip_category(ObjType::Potions), None);
        assert_eq!(usable_slots(false), 24);
        assert_eq!(usable_slots(true), 42);
    }

    #[test]
    fn tooltip_shows_numbers_and_unmet_requirements() {
        let o = ObjData { name: "Espada".into(), obj_type: ObjType::Weapon, min_hit: 4, max_hit: 9, value: 300, min_level: 5, ..Default::default() };
        let t = tooltip_lines(&o, 1, true, 2);
        assert_eq!(t[0], "Espada (equipado)");
        assert!(t.contains(&"Daño: 4-9".to_string()) && t.contains(&"Valor: 300 oro".to_string()));
        assert!(t.iter().any(|l| l.contains("nivel 5") && l.contains("te falta")));
        let t = tooltip_lines(&o, 3, false, 9);
        assert!(t.contains(&"Cantidad: 3".to_string()) && !t.iter().any(|l| l.contains("te falta")));
        assert_eq!(sale_value(&o, 3), 300);
    }

    // ---- organising ----

    fn obj_key(obj: u16) -> SortKey {
        // A fake obj.dat: kind = obj / 100, name = "n<obj>", value = obj * 10.
        SortKey { group: obj / 100, name: format!("n{obj:05}"), value: obj as i64 * 10, obj }
    }

    fn it(obj: u16, amount: u32) -> Option<ItemSlot> {
        Some(ItemSlot { obj, amount, equipped: false })
    }

    /// Runs a plan on a model of the bag exactly like the server would.
    fn run(slots: &[Option<ItemSlot>], plan: &[(u8, u8)]) -> Vec<Option<ItemSlot>> {
        let mut c = slots.to_vec();
        for (a, b) in plan {
            apply_swap(&mut c, *a as usize - 1, *b as usize - 1);
        }
        c
    }

    fn totals(c: &[Option<ItemSlot>]) -> std::collections::BTreeMap<u16, u32> {
        let mut m = std::collections::BTreeMap::new();
        for i in c.iter().flatten() {
            *m.entry(i.obj).or_insert(0) += i.amount;
        }
        m
    }

    fn bag(items: &[(usize, u16, u32)]) -> Vec<Option<ItemSlot>> {
        let mut v = vec![None; INVENTORY_SLOTS];
        for (i, o, n) in items {
            v[*i] = it(*o, *n);
        }
        v
    }

    #[test]
    fn organise_by_type_packs_and_orders() {
        let b = bag(&[(3, 250, 1), (9, 110, 4), (10, 105, 1), (20, 5, 2), (41, 101, 7)]);
        let plan = plan_organize(&b, 42, SortMode::Type, &obj_key, true);
        let r = run(&b, &plan);
        let objs: Vec<u16> = r.iter().flatten().map(|i| i.obj).collect();
        assert_eq!(objs, vec![5, 101, 105, 110, 250], "ordered by (group, name)");
        assert!(r[..5].iter().all(|c| c.is_some()) && r[5..].iter().all(|c| c.is_none()), "packed at the start");
        assert_eq!(totals(&r), totals(&b), "nothing is lost");
        assert!(plan.len() <= 5, "one swap per misplaced object at most");
    }

    #[test]
    fn organise_value_and_name_modes() {
        let b = bag(&[(0, 120, 1), (1, 900, 1), (2, 300, 1)]);
        let v = run(&b, &plan_organize(&b, 24, SortMode::Value, &obj_key, true));
        assert_eq!(v.iter().flatten().map(|i| i.obj).collect::<Vec<_>>(), vec![900, 300, 120], "most valuable first");
        let n = run(&b, &plan_organize(&b, 24, SortMode::Name, &obj_key, true));
        assert_eq!(n.iter().flatten().map(|i| i.obj).collect::<Vec<_>>(), vec![120, 300, 900]);
    }

    #[test]
    fn an_organised_bag_needs_no_swaps() {
        let b = bag(&[(0, 101, 3), (1, 105, 1), (2, 250, 9)]);
        assert!(plan_organize(&b, 24, SortMode::Type, &obj_key, true).is_empty());
        let again = run(&bag(&[(5, 250, 9), (2, 101, 3)]), &plan_organize(&bag(&[(5, 250, 9), (2, 101, 3)]), 24, SortMode::Type, &obj_key, true));
        assert!(plan_organize(&again, 24, SortMode::Type, &obj_key, true).is_empty(), "idempotent");
    }

    #[test]
    fn merging_stacks_respects_the_stack_limit_and_equipment() {
        use ao_rules::items::MAX_STACK;
        let b = bag(&[(0, 7, 5000), (4, 7, 4000), (9, 7, 3000), (11, 8, 2), (12, 8, 3)]);
        let r = run(&b, &plan_organize(&b, 24, SortMode::Type, &obj_key, true));
        assert_eq!(totals(&r), totals(&b));
        let sevens: Vec<u32> = r.iter().flatten().filter(|i| i.obj == 7).map(|i| i.amount).collect();
        assert_eq!(sevens, vec![MAX_STACK, 2000], "a full stack and the remainder");
        let eights: Vec<u32> = r.iter().flatten().filter(|i| i.obj == 8).map(|i| i.amount).collect();
        assert_eq!(eights, vec![5]);
        // Equipped stacks are not joined with loose ones.
        let mut e = bag(&[(0, 7, 10), (3, 7, 20)]);
        e[0].as_mut().unwrap().equipped = true;
        let r = run(&e, &plan_organize(&e, 24, SortMode::Type, &obj_key, true));
        assert_eq!(r.iter().flatten().count(), 2);
        assert!(r.iter().flatten().any(|i| i.equipped && i.amount == 10));
        // Without `merge` nothing is joined.
        let m = bag(&[(0, 7, 5), (3, 7, 6)]);
        assert!(plan_organize(&m, 24, SortMode::Type, &obj_key, false).is_empty());
    }

    #[test]
    fn organise_never_sends_a_packet_the_server_refuses() {
        // No backpack: 24 usable slots, one object stranded in a locked slot.
        let b = bag(&[(30, 101, 1), (2, 250, 1), (5, 105, 1)]);
        let plan = plan_organize(&b, 24, SortMode::Type, &obj_key, true);
        let mut c = b.clone();
        for (a, bb) in &plan {
            let (a, bb) = (*a as usize - 1, *bb as usize - 1);
            assert!(c[a].is_some(), "the source is occupied");
            assert!(bb < 24 || c[bb].is_some(), "never into an empty locked slot");
            assert_ne!(a, bb);
            apply_swap(&mut c, a, bb);
        }
        assert!(c[..24].iter().filter(|x| x.is_some()).count() == 3, "the stranded object was pulled in");
        // A full bag with a backpack is fine too.
        let full: Vec<Option<ItemSlot>> = (0..42).map(|i| it(300 - i as u16, 1)).collect();
        let plan = plan_organize(&full, 42, SortMode::Name, &obj_key, true);
        let r = run(&full, &plan);
        let names: Vec<u16> = r.iter().flatten().map(|i| i.obj).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        assert!(plan.len() <= 41);
    }

    /// Fewest legal swaps from `start` to `goal` over the server's model (breadth first).
    fn shortest(start: Vec<Option<ItemSlot>>, goal: &[Option<ItemSlot>], usable: usize) -> usize {
        use std::collections::{HashSet, VecDeque};
        let mut seen: HashSet<Vec<Option<(u16, u32, bool)>>> = HashSet::new();
        let flat = |v: &[Option<ItemSlot>]| v.iter().map(|c| c.map(|i| (i.obj, i.amount, i.equipped))).collect::<Vec<_>>();
        let mut q = VecDeque::from([(start.clone(), 0usize)]);
        seen.insert(flat(&start));
        while let Some((s, d)) = q.pop_front() {
            if s == goal {
                return d;
            }
            for a in 0..s.len() {
                for b in 0..s.len() {
                    if a == b || s[a].is_none() || (b >= usable && s[b].is_none()) {
                        continue;
                    }
                    let mut n = s.clone();
                    apply_swap(&mut n, a, b);
                    if seen.insert(flat(&n)) {
                        q.push_back((n, d + 1));
                    }
                }
            }
        }
        usize::MAX
    }

    #[test]
    fn organise_uses_the_fewest_swaps() {
        // Every arrangement of 4 distinct objects in 6 cells (a small deterministic sweep).
        let objs = [101u16, 102, 103, 104];
        let mut perms: Vec<Vec<usize>> = Vec::new();
        for a in 0..6 {
            for b in 0..6 {
                for c in 0..6 {
                    for d in 0..6 {
                        let p = vec![a, b, c, d];
                        let mut s = p.clone();
                        s.sort();
                        s.dedup();
                        if s.len() == 4 {
                            perms.push(p);
                        }
                    }
                }
            }
        }
        for (n, p) in perms.iter().enumerate().filter(|(n, _)| n % 7 == 0) {
            let mut b: Vec<Option<ItemSlot>> = vec![None; 6];
            for (k, pos) in p.iter().enumerate() {
                b[*pos] = it(objs[k], 1);
            }
            let plan = plan_organize(&b, 6, SortMode::Name, &obj_key, false);
            let goal: Vec<Option<ItemSlot>> = (0..6).map(|i| objs.get(i).and_then(|o| it(*o, 1))).collect();
            assert_eq!(run(&b, &plan), goal, "arrangement {n} ends sorted");
            assert_eq!(plan.len(), shortest(b.clone(), &goal, 6), "arrangement {n}: {p:?} is not the shortest");
        }
    }

    #[test]
    fn categories_rarity_and_slots() {
        assert_eq!(category(ObjType::Potions), Category::Potion);
        assert_eq!(category(ObjType::Shield), Category::Armor);
        assert_eq!(category(ObjType::Arrows), Category::Weapon);
        assert_eq!(equip_slot(ObjType::Amulets), Some(EquipSlot::Amulet));
        assert_eq!(equip_slot(ObjType::Potions), None);
        let o = |t, v| ObjData { obj_type: t, value: v, ..Default::default() };
        assert_eq!(rarity(&o(ObjType::Weapon, 50)), Rarity::Common);
        assert_eq!(rarity(&o(ObjType::Weapon, 5000)), Rarity::Rare);
        assert_eq!(rarity(&o(ObjType::Armor, 50_000)), Rarity::Legendary);
        assert_eq!(rarity(&o(ObjType::Wood, 50_000)), Rarity::Common, "materials are never special");
    }

    #[test]
    fn rich_tooltip_marks_what_the_player_does_not_meet() {
        let o = ObjData { name: "Espada Real".into(), obj_type: ObjType::Weapon, min_hit: 4, max_hit: 9, value: 3000, min_level: 12, royal: 1, text: "Para combatir.".into(), ..Default::default() };
        let weak = Viewer { level: 5, faction: Some(ao_rules::faction::CHAOS), skills: vec![] };
        let t = rich_tooltip(&o, 1, false, &weak);
        assert_eq!(t[0], "Espada Real");
        assert!(t[1].contains("Arma") && t[1].contains("Raro"));
        assert_eq!(t[2], "~Para combatir.");
        assert!(t.contains(&"Daño: 4-9".to_string()));
        assert!(t.iter().any(|l| l.starts_with('!') && l.contains("nivel 12") && l.contains("tienes 5")));
        assert!(t.iter().any(|l| l.starts_with('!') && l.contains("Ejército Real")));
        assert!(t.iter().any(|l| l.starts_with('$') && l.contains("3000") && l.contains("1000")));
        let strong = Viewer { level: 20, faction: Some(ao_rules::faction::ROYAL), skills: vec![] };
        assert!(!rich_tooltip(&o, 1, true, &strong).iter().any(|l| l.starts_with('!')));
        let none = ObjData { name: "Pan".into(), obj_type: ObjType::UseOnce, ..Default::default() };
        assert!(rich_tooltip(&none, 3, false, &strong)[2].starts_with('~'), "a role phrase when obj.dat has no text");
    }

    #[test]
    fn nearby_players_are_the_ones_in_trading_range() {
        let mut g = game();
        g.my_id = 1;
        let ent = |name: &str, kind, x, y| Ent::new(name.into(), kind, 1, 1, x, y, ao_rules::Dir::Down, 10, 10);
        g.ents.insert(1, ent("Yo", EntityKind::Player, 10, 10));
        g.ents.insert(2, ent("Cerca", EntityKind::Player, 11, 12));
        g.ents.insert(3, ent("Lejos", EntityKind::Player, 20, 10));
        g.ents.insert(4, ent("Npc", EntityKind::Creature, 10, 11));
        assert_eq!(g.nearby_players(), vec![(2, "Cerca".to_string())]);
        assert!(g.plan_propose(2).is_ok());
        assert!(g.plan_propose(3).is_err() && g.plan_propose(4).is_err() && g.plan_propose(1).is_err());
    }
}

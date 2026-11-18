//! One loaded map: its static data, its NPCs and the index of players on it.
//! Everything that happens on a map only touches its instance, which is what will let
//! each instance run on its own task later (one actor per map). OWNER: NPC system (P1).
use super::entities::{EntityId, Npc};
use crate::clock::GameTime;
use ao_formats::AoMap;
use std::collections::{HashMap, HashSet};

/// Tiles per side of the NPC grid (map tiles are 1..=100).
const SIDE: i32 = 101;

pub struct MapInstance {
    pub map: AoMap,
    pub npcs: HashMap<EntityId, Npc>,
    /// Players currently on this map (positions live in the player).
    pub players: HashSet<EntityId>,
    /// Objects on the floor. OWNER: inventory system (P1, Opus C).
    pub ground: super::inventory::GroundItems,
    /// The official NPCs of the .csm have been created (`CargarMapaFormatoCSM`).
    pub populated: bool,
    /// Next run of the AI loop on this map (`modNpcAiLoop`, every 100 ms).
    pub next_ai: GameTime,
    /// NPC ids in creation order: stable iteration (determinism) without sorting each tick.
    order: Vec<EntityId>,
    /// `MapData(x, y).NpcIndex`: which NPC stands on each tile (0 = none).
    grid: Vec<EntityId>,
    /// Doors whose object differs from the map file (opened or closed by players): tile
    /// -> obj.dat index. Their blocks are already applied to `map.blocked`.
    pub doors: HashMap<(i32, i32), u16>,
}

fn cell(x: i32, y: i32) -> Option<usize> {
    ((0..SIDE).contains(&x) && (0..SIDE).contains(&y)).then(|| (y * SIDE + x) as usize)
}

impl MapInstance {
    pub fn new(map: AoMap) -> Self {
        MapInstance {
            map,
            npcs: HashMap::new(),
            players: HashSet::new(),
            ground: Default::default(),
            populated: false,
            next_ai: GameTime::ZERO,
            order: Vec::new(),
            grid: vec![0; (SIDE * SIDE) as usize],
            doors: HashMap::new(),
        }
    }

    /// Living NPC on a tile (O(1)).
    pub fn npc_at(&self, x: i32, y: i32) -> Option<EntityId> {
        let id = *self.grid.get(cell(x, y)?)?;
        (id != 0 && self.npcs.get(&id).is_some_and(|n| n.alive() && n.x == x && n.y == y)).then_some(id)
    }

    /// NPC ids in creation order.
    pub fn npc_ids(&self) -> &[EntityId] {
        &self.order
    }

    pub fn add_npc(&mut self, id: EntityId, npc: Npc) {
        self.place(id, npc.x, npc.y);
        self.npcs.insert(id, npc);
        self.order.push(id);
    }

    pub fn remove_npc(&mut self, id: EntityId) -> Option<Npc> {
        let n = self.npcs.remove(&id)?;
        self.unplace(id, n.x, n.y);
        self.order.retain(|o| *o != id);
        Some(n)
    }

    /// Moves an NPC on the grid and in its struct.
    pub fn move_npc(&mut self, id: EntityId, x: i32, y: i32) {
        let Some(n) = self.npcs.get_mut(&id) else { return };
        let (ox, oy) = (n.x, n.y);
        n.x = x;
        n.y = y;
        self.unplace(id, ox, oy);
        self.place(id, x, y);
    }

    pub(crate) fn unplace(&mut self, id: EntityId, x: i32, y: i32) {
        if let Some(c) = cell(x, y) {
            if self.grid[c] == id {
                self.grid[c] = 0;
            }
        }
    }

    fn place(&mut self, id: EntityId, x: i32, y: i32) {
        if let Some(c) = cell(x, y) {
            self.grid[c] = id;
        }
    }
}

//! One loaded map: its static data, its creatures and the index of players on it.
//! Everything that happens on a map only touches its instance, which is what will let
//! each instance run on its own task later (one actor per map).
use super::entities::{EntityId, Npc};
use ao_formats::AoMap;
use std::collections::{HashMap, HashSet};

pub struct MapInstance {
    pub map: AoMap,
    pub npcs: HashMap<EntityId, Npc>,
    /// Players currently on this map (positions live in the player).
    pub players: HashSet<EntityId>,
}

impl MapInstance {
    pub fn new(map: AoMap) -> Self {
        MapInstance { map, npcs: HashMap::new(), players: HashSet::new() }
    }

    pub fn npc_at(&self, x: i32, y: i32) -> Option<EntityId> {
        self.npcs.iter().find(|(_, n)| n.alive() && n.x == x && n.y == y).map(|(id, _)| *id)
    }
}

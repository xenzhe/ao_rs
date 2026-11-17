//! Event system: events: duels, tournaments, auctions, invasions, castles. OWNER: Opus D (docs/P4_CONTRATO.md).
use crate::clock::GameTime;
use crate::world::World;
use ao_protocol::EventC2S;

impl World {
    /// (`on_event` is the network event loop; this is the game-events system.)
    pub(crate) fn on_game_event(&mut self, id: u32, msg: EventC2S) {
        let _ = id;
        match msg {}
    }

    /// HOOK (combat): does `id` keep its items when dying (duels, tournaments, arenas)?
    pub(crate) fn event_keeps_items_on_death(&self, id: u32) -> bool {
        let _ = id;
        false
    }

    /// HOOK (combat): `killer` (a player, or None) killed the player `victim`, after the
    /// death is processed. Events decide rounds, winners, respawns...
    pub(crate) fn event_on_player_killed(&mut self, killer: Option<u32>, victim: u32) {
        let _ = (killer, victim);
    }

    /// Every world tick.
    pub(crate) fn tick_event(&mut self, now: GameTime) {
        let _ = now;
    }

    /// When the player enters the world (after everything else is sent).
    pub(crate) fn event_on_enter(&mut self, id: u32) {
        let _ = id;
    }

    /// Right before the player leaves the world, still present.
    pub(crate) fn event_on_leave(&mut self, id: u32) {
        let _ = id;
    }
}

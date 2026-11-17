//! What the client knows about craft (from `S2C::Craft`) and its actions (`impl Game`).
//! OWNER: Opus B (docs/P4_CONTRATO.md).
use crate::local::Local;
use crate::state::Game;
use ao_protocol::p4::craft::{Recipe, Trade};
use ao_protocol::{CraftC2S, CraftS2C, C2S};

/// The open trade, as the server sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTrade {
    pub trade: Trade,
    pub recipes: Vec<Recipe>,
    pub per_step: u32,
    pub station: Option<(i32, i32)>,
}

/// A running order (`done` of `total`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Order {
    pub obj: u16,
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Default)]
pub struct CraftState {
    pub open: Option<OpenTrade>,
    pub order: Option<Order>,
    /// Why the last order stopped (shown in the window until the next order).
    pub last_stop: Option<String>,
    /// Units built since the window opened (for the window's feedback line).
    pub built: Vec<(u16, u32)>,
    /// Set when the server opens a trade: the UI opens `WindowId::Crafting` and clears it
    /// (the integrator hooks this in `ui::auto_windows`).
    pub open_request: bool,
    /// Set when the server closes the trade: the UI closes the window and clears it.
    pub close_request: bool,
    /// Work sounds to play (`Crafted.sound`), drained by the frontend.
    pub sounds: Vec<u16>,
}

impl CraftState {
    pub fn apply(&mut self, msg: &CraftS2C) {
        match msg {
            CraftS2C::Open { trade, recipes, per_step, station } => {
                let fresh = self.open.as_ref().is_none_or(|o| o.trade != *trade);
                self.open = Some(OpenTrade { trade: *trade, recipes: recipes.clone(), per_step: *per_step, station: *station });
                if fresh {
                    self.open_request = true;
                    self.close_request = false;
                    self.built.clear();
                }
            }
            CraftS2C::Recipes { trade, recipes, per_step } => {
                if let Some(o) = self.open.as_mut().filter(|o| o.trade == *trade) {
                    o.recipes = recipes.clone();
                    o.per_step = *per_step;
                }
            }
            CraftS2C::Progress { obj, done, total } => {
                self.order = (done < total).then_some(Order { obj: *obj, done: *done, total: *total });
                self.last_stop = None;
            }
            CraftS2C::Stopped { reason } => {
                self.order = None;
                self.last_stop = Some(reason.clone());
            }
            CraftS2C::Crafted { obj, amount, sound } => {
                match self.built.iter_mut().find(|(o, _)| o == obj) {
                    Some(b) => b.1 += amount,
                    None => self.built.push((*obj, *amount)),
                }
                // Until the frontend drains it, keep only the latest few.
                if self.sounds.len() >= 16 {
                    self.sounds.remove(0);
                }
                self.sounds.push(*sound);
            }
            CraftS2C::Closed => {
                self.open = None;
                self.order = None;
                self.close_request = true;
                self.open_request = false;
            }
        }
    }

    pub fn recipe(&self, obj: u16) -> Option<&Recipe> {
        self.open.as_ref()?.recipes.iter().find(|r| r.obj == obj)
    }
}

/// How many units of a recipe the inventory covers (materials per unit; `u32::MAX` when it
/// needs nothing).
pub fn affordable(local: &Local, r: &Recipe) -> u32 {
    r.materials.iter().map(|(obj, n)| if *n == 0 { u32::MAX } else { local.count_of(*obj) / n }).min().unwrap_or(u32::MAX)
}

impl Game {
    /// Opens a trade (the tool must be equipped; the blacksmith needs the anvil tile).
    pub fn craft_open(&mut self, trade: Trade, station: Option<(i32, i32)>) {
        self.send(&C2S::Craft(CraftC2S::Open { trade, station }));
    }

    /// Starts an order. Refused locally when nothing is open or the amount is out of range
    /// (the server checks everything again).
    pub fn craft_build(&mut self, obj: u16, amount: u32) -> Result<(), String> {
        let st = &self.local.p4.craft;
        if st.open.is_none() {
            return Err("No estás trabajando en ningún oficio.".into());
        }
        if st.recipe(obj).is_none() {
            return Err("No sabés construir eso.".into());
        }
        if amount == 0 || amount > ao_rules::craft::MAX_ORDER {
            return Err("Cantidad inválida.".into());
        }
        self.send(&C2S::Craft(CraftC2S::Build { obj, amount }));
        Ok(())
    }

    pub fn craft_stop(&mut self) {
        self.send(&C2S::Craft(CraftC2S::Stop));
    }

    /// The player closed the window.
    pub fn craft_close(&mut self) {
        let st = &mut self.local.p4.craft;
        st.open = None;
        st.order = None;
        self.send(&C2S::Craft(CraftC2S::Close));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::ItemSlot;

    fn recipe() -> Recipe {
        Recipe { obj: 479, skill: 15, materials: vec![(58, 200)] }
    }

    #[test]
    fn open_progress_stop_close() {
        let mut s = CraftState::default();
        s.apply(&CraftS2C::Open { trade: Trade::Carpenter, recipes: vec![recipe()], per_step: 1, station: None });
        assert!(s.open_request && s.recipe(479).is_some());
        s.open_request = false;
        // A refresh of the same trade (skill raised) does not re-open the window.
        s.apply(&CraftS2C::Open { trade: Trade::Carpenter, recipes: vec![recipe()], per_step: 2, station: None });
        assert!(!s.open_request);
        s.apply(&CraftS2C::Progress { obj: 479, done: 1, total: 3 });
        assert_eq!(s.order, Some(Order { obj: 479, done: 1, total: 3 }));
        s.apply(&CraftS2C::Crafted { obj: 479, amount: 1, sound: 42 });
        s.apply(&CraftS2C::Crafted { obj: 479, amount: 2, sound: 42 });
        assert_eq!(s.built, vec![(479, 3)]);
        s.apply(&CraftS2C::Progress { obj: 479, done: 3, total: 3 });
        assert_eq!(s.order, None);
        s.apply(&CraftS2C::Stopped { reason: "x".into() });
        assert_eq!(s.last_stop.as_deref(), Some("x"));
        s.apply(&CraftS2C::Closed);
        assert!(s.open.is_none() && s.close_request);
    }

    #[test]
    fn affordable_counts_the_scarcest_material() {
        let mut l = Local::new();
        l.inventory[0] = Some(ItemSlot { obj: 58, amount: 450, equipped: false });
        l.inventory[3] = Some(ItemSlot { obj: 58, amount: 150, equipped: false });
        assert_eq!(affordable(&l, &recipe()), 3);
        let two = Recipe { materials: vec![(58, 200), (2781, 1)], ..recipe() };
        assert_eq!(affordable(&l, &two), 0);
    }
}

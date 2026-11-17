//! What the client knows about work (from `S2C::Work`) and its actions (`impl Game`).
//! OWNER: Opus A (docs/P4_CONTRATO.md).
//!
//! The gesture, like AO20: using (double click / U) the equipped tool asks for a target
//! (`WorkRequestTarget`), then a left click on the world tile starts the work there. For
//! smelting, using an ore stack asks for the target and the click goes on the forge.
//! The frontend (input.rs, integrator) keeps the pending [`Targeting`] and on the next world
//! click calls [`Game::plan_work`] / [`Game::plan_smelt`]; walking needs no stop message
//! (the server ends the work when the player moved), `plan_stop_work` is for an explicit
//! cancel (Esc).
use crate::Game;
use ao_formats::csm::FLAG_WATER;
use ao_formats::dat::obj::{ObjData, ObjType};
use ao_protocol::p4::work::WorkKind;
use ao_protocol::{WorkC2S, WorkS2C, C2S};
use ao_rules::work::{self as rules, Job};

/// What the next world click does after using a tool or an ore stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Targeting {
    Tool(Job),
    Smelt { slot: u8 },
}

#[derive(Debug, Default)]
pub struct WorkState {
    /// Running work (set only when the server confirms it).
    pub active: Option<(WorkKind, i32, i32)>,
    /// Last "+N" (obj, amount): the frontend shows it over the character.
    pub last_gathered: Option<(u16, u32)>,
    pub gathered_total: u32,
    /// Successful attempts (a low-level Worker may succeed and still get 0, like the VB).
    pub hits: u32,
    pub misses: u32,
}

impl WorkState {
    pub fn apply(&mut self, msg: &WorkS2C) {
        match *msg {
            WorkS2C::Started { kind, x, y } => self.active = Some((kind, x, y)),
            WorkS2C::Stopped => self.active = None,
            WorkS2C::Gathered { obj, amount } => {
                self.last_gathered = Some((obj, amount));
                self.gathered_total += amount;
                self.hits += 1;
            }
            WorkS2C::Missed => self.misses += 1,
        }
    }
}

type Plan = Result<C2S, String>;

/// Which target a tool asks for (`UseInvItem`), or why not.
pub fn targeting_for_tool(tool: &ObjData) -> Option<Targeting> {
    (tool.obj_type == ObjType::WorkingTools).then(|| Job::for_tool(tool.subtype)).flatten().map(Targeting::Tool)
}

impl Game {
    /// Start the job of the equipped `tool` on (x, y). Same checks as the server where the
    /// client can know them (distance, water); the tree/deposit itself is checked there.
    pub fn plan_work(&self, tool: Option<&ObjData>, x: i32, y: i32) -> Plan {
        let me = self.me().ok_or("No estás en el mundo.")?;
        let tool = tool.ok_or("Antes de usar la herramienta deberías equipártela.")?;
        let Some(Targeting::Tool(job)) = targeting_for_tool(tool) else {
            return Err("Esa herramienta no sirve para esto.".into());
        };
        let (px, py) = (me.x, me.y);
        let kind = match job {
            Job::Chop => WorkKind::Chop,
            Job::Mine => WorkKind::Mine,
            _ => WorkKind::Fish,
        };
        if matches!(kind, WorkKind::Chop | WorkKind::Mine) && rules::distance(px, py, x, y) > rules::CLOSE_DISTANCE {
            return Err("Esta demasiado lejos.".into());
        }
        if kind == WorkKind::Fish {
            if let Some(m) = &self.map {
                if m.blocked(x, y) & FLAG_WATER == 0 {
                    return Err("Zona de pesca no Autorizada. Busca otro lugar para hacerlo.".into());
                }
            }
        }
        Ok(C2S::Work(WorkC2S::Start { kind, x, y }))
    }

    /// Smelt the ore of `slot` in the forge at (x, y).
    pub fn plan_smelt(&self, slot: u8, x: i32, y: i32) -> Plan {
        let me = self.me().ok_or("No estás en el mundo.")?;
        if self.local.slot(slot).is_none() {
            return Err("Esa casilla está vacía.".into());
        }
        if rules::distance(me.x, me.y, x, y) > rules::MEDIUM_DISTANCE {
            return Err("Esta demasiado lejos.".into());
        }
        Ok(C2S::Work(WorkC2S::Start { kind: WorkKind::Smelt { slot }, x, y }))
    }

    pub fn plan_stop_work(&self) -> Plan {
        if self.local.p4.work.active.is_none() {
            return Err("No estás trabajando.".into());
        }
        Ok(C2S::Work(WorkC2S::Stop))
    }
}

//! Gameplay systems, one file each, all as `impl World` blocks. A new feature (inventory,
//! trade, guilds...) is a new file here plus its messages in `ao-protocol`.
mod account;
mod bank;
mod chat;
mod interact;
mod inventory;
mod melee;
mod progression;
mod combat;
mod magic;
pub(crate) mod eot;
mod eot_extra;
mod magic_extra;
mod npc_guard;
mod movement;
mod npc_ai;
mod npc_tactics;
mod npc_owner;
mod regen;
mod session;
// P2 (docs/P2_CONTRATO.md)
mod faction;
mod guild;
mod party;
mod quest;
mod social;
mod trade;
pub(crate) mod weather;
// P4 (docs/P4_CONTRATO.md)
mod work;
mod craft;
mod doors;
mod maprules;
mod speed;
mod campfire;
pub(crate) mod cards;
pub(crate) mod skins;
mod gm;
mod gm_mod;
mod gm_tsao;
mod event;
mod travel;
mod mail;
mod surgeon;
mod tsao;
pub(crate) mod thief;
pub(crate) mod tsao_progress;
pub(crate) mod pets;
pub(crate) mod creatures;

use crate::world::World;
use ao_protocol::C2S;

impl World {
    /// Routes a decoded client message to its system.
    pub fn handle(&mut self, id: u32, msg: C2S) {
        let logged = self.player(id).is_some();
        match msg {
            C2S::CreateAccount { .. } | C2S::AccountLogin { .. } | C2S::CreateCharacter { .. } | C2S::DeleteCharacter { .. } | C2S::PlayCharacter { .. } => {
                self.on_account(id, msg)
            }
            C2S::Login { name, version } => self.on_login(id, name, version),
            _ if !logged => self.kick(id, "paquete antes del login"),
            C2S::Walk { dir, seq } => self.on_walk(id, dir, seq),
            C2S::Turn { dir } => self.on_turn(id, dir),
            C2S::Melee { seq } => self.on_melee(id, seq),
            C2S::Shoot { tx, ty, seq } => self.on_shoot(id, tx, ty, seq),
            C2S::Cast { spell, tx, ty, seq } => self.on_cast(id, spell, tx, ty, seq),
            C2S::Meditate => self.on_meditate(id),
            C2S::UseItem { slot } => self.on_use_item(id, slot),
            C2S::EquipItem { slot } => self.on_equip_item(id, slot),
            C2S::DropItem { slot, amount } => self.on_drop_item(id, slot, amount),
            C2S::DropGold { amount } => self.on_drop_gold(id, amount),
            C2S::PickUp => self.on_pick_up(id),
            C2S::SwapSlots { a, b } => self.on_swap_slots(id, a, b),
            C2S::Interact { tx, ty } => self.on_interact(id, tx, ty),
            C2S::BankDeposit { slot, amount } => self.on_bank_deposit(id, slot, amount),
            C2S::BankWithdraw { slot, amount } => self.on_bank_withdraw(id, slot, amount),
            C2S::BankDepositGold { amount } => self.on_bank_deposit_gold(id, amount),
            C2S::BankWithdrawGold { amount } => self.on_bank_withdraw_gold(id, amount),
            C2S::BankClose => self.on_bank_close(id),
            C2S::AssignSkill { skill, points } => self.on_assign_skill(id, skill, points),
            C2S::Say { text } => self.on_say(id, text),
            C2S::Trade(m) => self.on_trade(id, m),
            C2S::Guild(m) => self.on_guild(id, m),
            C2S::Party(m) => self.on_party(id, m),
            C2S::Faction(m) => self.on_faction(id, m),
            C2S::Quest(m) => self.on_quest(id, m),
            C2S::Channel(m) => self.on_channel(id, m),
            C2S::Work(m) => self.on_work(id, m),
            C2S::Craft(m) => self.on_craft(id, m),
            C2S::Gm(m) => self.on_gm(id, m),
            C2S::Event(m) => self.on_game_event(id, m),
            C2S::Travel(m) => self.on_travel(id, m),
            C2S::Mail(m) => self.on_mail(id, m),
            C2S::Surgeon(m) => self.on_surgeon(id, m),
            C2S::Tsao(m) => self.on_tsao(id, m),
            C2S::Skill(m) => self.on_skill(id, m),
            C2S::UseItemAt { slot, tx, ty } => self.on_use_item_at(id, slot, tx, ty),
        }
    }
}

//! Active buffs, debuffs and cooldowns of the own character (`S2C::Buff`), as the VB
//! client keeps them in `BuffList` / `DeBuffList` / `CDList` (`ModCooldown.bas`).
use ao_protocol::buff_kind;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActiveBuff {
    /// `ClientEffectTypeId`: row of `init/Effects.ini`.
    pub icon: u16,
    pub uid: i32,
    pub ends: Instant,
    pub total_ms: u32,
    pub kind: u8,
    pub stacks: u16,
}

impl ActiveBuff {
    pub fn remaining_ms(&self, now: Instant) -> u64 {
        self.ends.saturating_duration_since(now).as_millis() as u64
    }

    /// Fraction of the time left, 0..1 (1 when the total is unknown).
    pub fn left_fraction(&self, now: Instant) -> f32 {
        if self.total_ms == 0 {
            return 1.0;
        }
        (self.remaining_ms(now) as f32 / self.total_ms as f32).clamp(0.0, 1.0)
    }

    /// Debuffs (and informative debuffs) go to the second row.
    pub fn is_debuff(&self) -> bool {
        matches!(self.kind, buff_kind::DEBUFF | buff_kind::INFO_DEBUFF)
    }

    pub fn is_cooldown(&self) -> bool {
        self.kind == buff_kind::COOLDOWN
    }
}

#[derive(Debug, Default)]
pub struct Buffs {
    pub list: Vec<ActiveBuff>,
}

impl Buffs {
    /// `AddOrUpdateEffect` / `RemoveEffect`: same icon and uid = the same entry.
    pub fn apply(&mut self, icon: u16, uid: i32, remaining_ms: u32, total_ms: u32, kind: u8, stacks: u16, now: Instant) {
        self.list.retain(|b| !(b.icon == icon && b.uid == uid));
        if remaining_ms > 0 {
            self.list.push(ActiveBuff { icon, uid, ends: now + Duration::from_millis(remaining_ms as u64), total_ms, kind, stacks });
        }
    }

    /// `UpdateEffectTime`: drops what ran out.
    pub fn expire(&mut self, now: Instant) {
        self.list.retain(|b| b.ends > now);
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_icon_and_uid_replaces_zero_removes_and_time_expires() {
        let now = Instant::now();
        let mut b = Buffs::default();
        b.apply(4, 7, 10_000, 10_000, buff_kind::BUFF, 0, now);
        b.apply(4, 8, 5_000, 5_000, buff_kind::DEBUFF, 0, now);
        b.apply(4, 7, 8_000, 10_000, buff_kind::BUFF, 2, now);
        assert_eq!(b.list.len(), 2);
        assert_eq!(b.list.iter().find(|x| x.uid == 7).unwrap().stacks, 2);
        assert!(b.list.iter().find(|x| x.uid == 8).unwrap().is_debuff());
        b.apply(4, 8, 0, 0, buff_kind::DEBUFF, 0, now);
        assert_eq!(b.list.len(), 1);
        b.expire(now + Duration::from_millis(9_000));
        assert!(b.list.is_empty());
    }
}

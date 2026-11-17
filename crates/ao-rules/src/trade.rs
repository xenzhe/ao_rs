//! Pure rules of trade (prices, commerce rules). OWNER: Opus B (docs/P2_CONTRATO.md).
//!
//! Port of `modSistemaComercio` (Comercio.bas): `Descuento`, `SalePrice`, the price lines of
//! `Comercio` and `UpdateNpcInv`. The VB number types are kept: `Descuento` is a Single and
//! the buy price is `Ceil(Valor / Descuento * Cantidad)`; the sale price is
//! `Fix(CSng(Valor / REDUCTOR_PRECIOVENTA) * Cantidad)`.

/// `REDUCTOR_PRECIOVENTA`: merchants pay a third of the value.
pub const SALE_DIVISOR: f64 = 3.0;
/// `MAXORO` (Declares.bas): most gold a character can carry.
pub const MAX_GOLD: u32 = 90_000_000;
/// `e_Skill.Comerciar` (9) as a 0-based skill index.
pub const SKILL_COMMERCE: usize = 8;
/// `itemsAenviar(1 To 6)`: most different entries in one player's offer.
pub const MAX_OFFER_ITEMS: usize = 6;
/// Max Manhattan distance between two players to start a trade (HandleCommerceStart).
pub const PLAYER_TRADE_RANGE: i32 = 3;

/// `Descuento`: `1 + Comerciar / 100`, as a VB Single.
pub fn discount(commerce_skill: u8) -> f32 {
    1.0 + commerce_skill as f32 / 100.0
}

/// Price of `amount` units bought from a merchant (`Comercio`, Compra).
pub fn buy_price(value: i32, commerce_skill: u8, amount: u32) -> u64 {
    let v = value.max(0) as f64 / discount(commerce_skill) as f64 * amount as f64;
    v.ceil() as u64
}

/// Unit price shown in the merchant window (`UpdateNpcInv`: `Ceil(Valor / Desc)`).
pub fn unit_buy_price(value: i32, commerce_skill: u8) -> u32 {
    buy_price(value, commerce_skill, 1).min(u32::MAX as u64) as u32
}

/// Gold paid for `amount` units sold to a merchant (`SalePrice` * Cantidad, `Fix`).
/// Newbie objects are worth nothing (`ItemNewbie`); the caller refuses them anyway.
/// The `worker_sale_bonus` feature (off by default) is not ported.
pub fn sale_price(value: i32, newbie: bool, amount: u32) -> u64 {
    if newbie {
        return 0;
    }
    let unit = (value.max(0) as f64 / SALE_DIVISOR) as f32;
    (unit as f64 * amount as f64).trunc() as u64
}

/// Gold after earning `earned`, capped at `MAXORO` like `Comercio` (Venta).
pub fn add_gold(gold: u32, earned: u64) -> u32 {
    (gold as u64 + earned).min(MAX_GOLD as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buying_rounds_up_and_the_skill_discounts() {
        assert_eq!(buy_price(100, 0, 1), 100);
        assert_eq!(buy_price(100, 0, 3), 300);
        // 100 / 1.5 = 66.67 -> 67 each, but the VB rounds the whole line: 200.
        assert_eq!(buy_price(100, 50, 3), 200);
        assert_eq!(unit_buy_price(100, 50), 67);
        // Skill 100 halves prices.
        assert_eq!(buy_price(1000, 100, 1), 500);
        assert_eq!(buy_price(1, 100, 1), 1, "never free");
        assert_eq!(buy_price(0, 0, 10), 0);
        // Large lines do not overflow.
        assert_eq!(buy_price(i32::MAX, 0, 10_000), i32::MAX as u64 * 10_000);
    }

    #[test]
    fn selling_pays_a_third_truncated() {
        assert_eq!(sale_price(100, false, 1), 33);
        assert_eq!(sale_price(100, false, 3), 99, "33.333 * 3 = 99.99 -> 99");
        assert_eq!(sale_price(2, false, 1), 0);
        assert_eq!(sale_price(300, false, 10), 1000);
        assert_eq!(sale_price(300, true, 10), 0, "newbie objects are worth nothing");
    }

    #[test]
    fn gold_is_capped() {
        assert_eq!(add_gold(10, 5), 15);
        assert_eq!(add_gold(MAX_GOLD - 1, 1_000_000), MAX_GOLD);
        assert_eq!(add_gold(u32::MAX, u64::MAX / 2), MAX_GOLD);
    }
}

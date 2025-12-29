//! Market Maker
//!
//! Implements market making from dol/nexus.dol lines 340-448

use super::types::*;
use crate::core::Credits;

/// Calculate the bid/ask spread for market making.
///
/// Formula:
/// spread = base_spread
///        + volatility * volatility_factor
///        + inventory_imbalance * inventory_factor
///        + entropy * entropy_spread_factor
///
/// From dol/nexus.dol lines 406-448
pub fn calculate_spread(
    order_book: &OrderBook,
    config: &MarketMakerConfig,
    local_entropy: f64,
    price_history: &[Credits],
) -> f64 {
    let mut spread = config.minimum_spread;

    // Volatility adjustment
    if price_history.len() > 1 {
        let mean_price: f64 =
            price_history.iter().map(|p| p.amount as f64).sum::<f64>() / price_history.len() as f64;

        if mean_price > 0.0 {
            let variance: f64 = price_history
                .iter()
                .map(|p| (p.amount as f64 - mean_price).powi(2))
                .sum::<f64>()
                / price_history.len() as f64;

            let volatility = variance.sqrt() / mean_price;
            spread += volatility * config.volatility_factor;
        }
    }

    // Inventory adjustment
    let current_inventory = order_book.total_inventory();
    if config.target_inventory > 0 {
        let inventory_imbalance = (current_inventory as i64 - config.target_inventory as i64).abs()
            as f64
            / config.target_inventory as f64;
        spread += inventory_imbalance * config.inventory_factor;
    }

    // Entropy adjustment
    spread += local_entropy * config.entropy_spread_factor;

    spread
}

/// Calculate bid price given mid price and spread
pub fn calculate_bid_price(mid_price: Credits, spread: f64) -> Credits {
    let bid = (mid_price.amount as f64 * (1.0 - spread / 2.0)).floor() as u64;
    Credits::new(bid)
}

/// Calculate ask price given mid price and spread
pub fn calculate_ask_price(mid_price: Credits, spread: f64) -> Credits {
    let ask = (mid_price.amount as f64 * (1.0 + spread / 2.0)).ceil() as u64;
    Credits::new(ask)
}

/// Market maker that provides liquidity for resources
pub struct MarketMaker {
    config: MarketMakerConfig,
}

impl MarketMaker {
    pub fn new(config: MarketMakerConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self {
            config: MarketMakerConfig::default(),
        }
    }

    /// Calculate bid and ask prices for a resource
    pub fn quote(
        &self,
        order_book: &OrderBook,
        mid_price: Credits,
        local_entropy: f64,
        price_history: &[Credits],
    ) -> (Credits, Credits) {
        let spread = calculate_spread(order_book, &self.config, local_entropy, price_history);

        let bid = calculate_bid_price(mid_price, spread);
        let ask = calculate_ask_price(mid_price, spread);

        (bid, ask)
    }

    /// Calculate expected revenue from market making
    /// spread_revenue = volume * spread / 2
    pub fn expected_revenue(&self, volume: u64, spread: f64) -> Credits {
        let revenue = (volume as f64 * spread / 2.0).floor() as u64;
        Credits::new(revenue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_spread_minimum() {
        let book = OrderBook::new(ResourceType::Cpu);
        // Set target_inventory to 0 so empty book matches target
        let config = MarketMakerConfig {
            target_inventory: 0,
            ..Default::default()
        };

        // With no volatility, inventory at target, and no entropy
        let spread = calculate_spread(&book, &config, 0.0, &[]);
        assert!((spread - config.minimum_spread).abs() < 0.001);
    }

    #[test]
    fn test_calculate_spread_with_entropy() {
        let book = OrderBook::new(ResourceType::Cpu);
        let config = MarketMakerConfig::default();

        let base_spread = calculate_spread(&book, &config, 0.0, &[]);
        let high_entropy_spread = calculate_spread(&book, &config, 5.0, &[]);

        assert!(high_entropy_spread > base_spread);
        let expected_increase = 5.0 * config.entropy_spread_factor;
        assert!((high_entropy_spread - base_spread - expected_increase).abs() < 0.001);
    }

    #[test]
    fn test_calculate_spread_with_volatility() {
        let book = OrderBook::new(ResourceType::Cpu);
        let config = MarketMakerConfig::default();

        // Volatile price history
        let prices = vec![
            Credits::new(100),
            Credits::new(150),
            Credits::new(80),
            Credits::new(120),
        ];

        let spread_with_volatility = calculate_spread(&book, &config, 0.0, &prices);
        let spread_no_volatility = calculate_spread(&book, &config, 0.0, &[]);

        assert!(spread_with_volatility > spread_no_volatility);
    }

    #[test]
    fn test_bid_ask_prices() {
        let mid = Credits::new(100);
        let spread = 0.02; // 2%

        let bid = calculate_bid_price(mid, spread);
        let ask = calculate_ask_price(mid, spread);

        // Bid should be mid * (1 - spread/2) = 100 * 0.99 = 99
        assert_eq!(bid.amount, 99);
        // Ask should be mid * (1 + spread/2) = 100 * 1.01 = 101
        assert_eq!(ask.amount, 101);
    }

    #[test]
    fn test_market_maker_quote() {
        let book = OrderBook::new(ResourceType::Cpu);
        let mm = MarketMaker::with_default_config();

        let (bid, ask) = mm.quote(&book, Credits::new(1000), 0.0, &[]);

        assert!(bid.amount < 1000);
        assert!(ask.amount > 1000);
        assert!(ask.amount > bid.amount);
    }

    #[test]
    fn test_expected_revenue() {
        let mm = MarketMaker::with_default_config();

        let revenue = mm.expected_revenue(100, 0.02);
        // 100 * 0.02 / 2 = 1
        assert_eq!(revenue.amount, 1);

        let revenue = mm.expected_revenue(10000, 0.05);
        // 10000 * 0.05 / 2 = 250
        assert_eq!(revenue.amount, 250);
    }
}

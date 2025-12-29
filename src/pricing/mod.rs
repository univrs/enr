//! ENR Pricing Module
//!
//! Implements pricing models from dol/pricing.dol
//!
//! Three pricing models:
//! - Fixed: Static prices for predictable workloads
//! - Dynamic: Entropy-adjusted prices
//! - Auction: Market-based price discovery

use crate::core::Credits;
use crate::entropy::{EntropyAccount, EntropyWeights};
use serde::{Deserialize, Serialize};

/// Pricing model types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PricingModel {
    /// Static prices for predictable workloads
    Fixed,
    /// Entropy-adjusted prices
    Dynamic,
    /// Market-based price discovery
    Auction,
}

impl Default for PricingModel {
    fn default() -> Self {
        PricingModel::Dynamic
    }
}

/// Fixed price configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixedPriceConfig {
    pub cpu_per_cycle: u64,
    pub memory_per_mb: u64,
    pub storage_per_gb: u64,
    pub bandwidth_per_mb: u64,
}

impl Default for FixedPriceConfig {
    fn default() -> Self {
        Self {
            cpu_per_cycle: 1,
            memory_per_mb: 10,
            storage_per_gb: 100,
            bandwidth_per_mb: 5,
        }
    }
}

/// Dynamic price configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DynamicPriceConfig {
    pub base_price: Credits,
    pub entropy_weights: EntropyWeights,
    pub min_multiplier: f64,
    pub max_multiplier: f64,
}

impl Default for DynamicPriceConfig {
    fn default() -> Self {
        Self {
            base_price: Credits::new(100),
            entropy_weights: EntropyWeights::default(),
            min_multiplier: 1.0,
            max_multiplier: 5.0,
        }
    }
}

/// Calculate fixed price
pub fn calculate_fixed_price(
    config: &FixedPriceConfig,
    cpu_cycles: u64,
    memory_mb: u64,
    storage_gb: u64,
    bandwidth_mb: u64,
) -> Credits {
    let total = cpu_cycles * config.cpu_per_cycle
        + memory_mb * config.memory_per_mb
        + storage_gb * config.storage_per_gb
        + bandwidth_mb * config.bandwidth_per_mb;
    Credits::new(total)
}

/// Calculate dynamic price with entropy adjustment
pub fn calculate_dynamic_price(
    config: &DynamicPriceConfig,
    entropy: &EntropyAccount,
) -> Credits {
    use crate::entropy::entropy_price_multiplier;

    let multiplier = entropy_price_multiplier(entropy)
        .clamp(config.min_multiplier, config.max_multiplier);

    let adjusted = (config.base_price.amount as f64 * multiplier) as u64;
    Credits::new(adjusted)
}

/// Price quote for a resource request
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceQuote {
    pub base_price: Credits,
    pub entropy_adjustment: Credits,
    pub total_price: Credits,
    pub entropy_multiplier: f64,
    pub model: PricingModel,
}

impl PriceQuote {
    pub fn fixed(price: Credits) -> Self {
        Self {
            base_price: price,
            entropy_adjustment: Credits::ZERO,
            total_price: price,
            entropy_multiplier: 1.0,
            model: PricingModel::Fixed,
        }
    }

    pub fn dynamic(base: Credits, multiplier: f64) -> Self {
        let total = Credits::new((base.amount as f64 * multiplier) as u64);
        let adjustment = if total.amount > base.amount {
            Credits::new(total.amount - base.amount)
        } else {
            Credits::ZERO
        };

        Self {
            base_price: base,
            entropy_adjustment: adjustment,
            total_price: total,
            entropy_multiplier: multiplier,
            model: PricingModel::Dynamic,
        }
    }
}

/// Pricer for generating quotes
pub struct Pricer {
    fixed_config: FixedPriceConfig,
    dynamic_config: DynamicPriceConfig,
    default_model: PricingModel,
}

impl Default for Pricer {
    fn default() -> Self {
        Self::new()
    }
}

impl Pricer {
    pub fn new() -> Self {
        Self {
            fixed_config: FixedPriceConfig::default(),
            dynamic_config: DynamicPriceConfig::default(),
            default_model: PricingModel::Dynamic,
        }
    }

    pub fn with_fixed_config(mut self, config: FixedPriceConfig) -> Self {
        self.fixed_config = config;
        self
    }

    pub fn with_dynamic_config(mut self, config: DynamicPriceConfig) -> Self {
        self.dynamic_config = config;
        self
    }

    pub fn quote_fixed(
        &self,
        cpu_cycles: u64,
        memory_mb: u64,
        storage_gb: u64,
        bandwidth_mb: u64,
    ) -> PriceQuote {
        let price = calculate_fixed_price(
            &self.fixed_config,
            cpu_cycles,
            memory_mb,
            storage_gb,
            bandwidth_mb,
        );
        PriceQuote::fixed(price)
    }

    pub fn quote_dynamic(&self, entropy: &EntropyAccount) -> PriceQuote {
        use crate::entropy::entropy_price_multiplier;

        let multiplier = entropy_price_multiplier(entropy)
            .clamp(self.dynamic_config.min_multiplier, self.dynamic_config.max_multiplier);

        PriceQuote::dynamic(self.dynamic_config.base_price, multiplier)
    }

    pub fn quote(&self, entropy: Option<&EntropyAccount>) -> PriceQuote {
        match (self.default_model, entropy) {
            (PricingModel::Dynamic, Some(e)) => self.quote_dynamic(e),
            (PricingModel::Dynamic, None) => {
                PriceQuote::dynamic(self.dynamic_config.base_price, 1.0)
            }
            _ => self.quote_fixed(0, 0, 0, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_pricing() {
        let config = FixedPriceConfig::default();
        let price = calculate_fixed_price(&config, 100, 10, 1, 50);

        // 100*1 + 10*10 + 1*100 + 50*5 = 100 + 100 + 100 + 250 = 550
        assert_eq!(price.amount, 550);
    }

    #[test]
    fn test_dynamic_pricing() {
        let config = DynamicPriceConfig::default();
        let low_entropy = EntropyAccount::zero();
        let price = calculate_dynamic_price(&config, &low_entropy);

        // Low entropy should give close to base price
        assert!(price.amount >= config.base_price.amount);
        assert!(price.amount <= (config.base_price.amount as f64 * 1.5) as u64);
    }

    #[test]
    fn test_price_quote() {
        let quote = PriceQuote::fixed(Credits::new(100));
        assert_eq!(quote.total_price.amount, 100);
        assert_eq!(quote.entropy_multiplier, 1.0);

        let quote = PriceQuote::dynamic(Credits::new(100), 2.0);
        assert_eq!(quote.total_price.amount, 200);
        assert_eq!(quote.entropy_adjustment.amount, 100);
    }

    #[test]
    fn test_pricer() {
        let pricer = Pricer::new();
        let quote = pricer.quote_fixed(100, 10, 1, 50);
        assert!(quote.total_price.amount > 0);
    }
}

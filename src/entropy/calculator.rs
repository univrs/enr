//! ENR Entropy Calculator
//!
//! Implements entropy calculation from dol/entropy.dol lines 87-311

use super::types::*;

/// Entropy calculation constants from dol/entropy.dol lines 28-62

// Network entropy factors (α coefficients)
pub const HOP_ENTROPY_BASE: f64 = 0.1;
pub const LATENCY_ENTROPY_FACTOR: f64 = 0.01;
pub const LOSS_ENTROPY_FACTOR: f64 = 5.0;
pub const SATURATION_ENTROPY_FACTOR: f64 = 2.0;

// Compute entropy factors (β coefficients)
pub const CYCLE_ENTROPY_FACTOR: f64 = 0.000001;
pub const MEMORY_ENTROPY_FACTOR: f64 = 0.0000001;
pub const CONTEXT_SWITCH_FACTOR: f64 = 0.01;
pub const CACHE_MISS_FACTOR: f64 = 1.0;

// Storage entropy factors (γ coefficients)
pub const SIZE_ENTROPY_FACTOR: f64 = 0.00000001;
pub const REPLICA_DIVERGENCE_FACTOR: f64 = 3.0;
pub const FRAGMENTATION_FACTOR: f64 = 2.0;
pub const COMPACTION_DEBT_FACTOR: f64 = 1.5;

// Temporal entropy factors (δ coefficients)
pub const STALENESS_ENTROPY_FACTOR: f64 = 0.001;
pub const CLOCK_DRIFT_FACTOR: f64 = 0.001;
pub const ORDERING_UNCERTAINTY_FACTOR: f64 = 2.0;
pub const VERSION_DIVERGENCE_FACTOR: f64 = 2.5;

// Maximum entropy bounds
pub const MAX_NETWORK_ENTROPY: f64 = 10.0;
pub const MAX_COMPUTE_ENTROPY: f64 = 10.0;
pub const MAX_STORAGE_ENTROPY: f64 = 10.0;
pub const MAX_TEMPORAL_ENTROPY: f64 = 10.0;

// Price multiplier thresholds
pub const LOW_ENTROPY_THRESHOLD: f64 = 2.0;
pub const MED_ENTROPY_THRESHOLD: f64 = 5.0;
pub const HIGH_ENTROPY_THRESHOLD: f64 = 8.0;
pub const MAX_ENTROPY_MULTIPLIER: f64 = 5.0;

/// Calculate network entropy from path characteristics.
///
/// Formula: Sₙ = α₁·hops + α₂·latency_var + α₃·loss_prob + α₄·saturation
///
/// From dol/entropy.dol lines 87-107
pub fn calculate_network_entropy(input: &NetworkEntropyInput) -> f64 {
    let entropy = (input.hops as f64) * HOP_ENTROPY_BASE
        + input.latency_variance_ms * LATENCY_ENTROPY_FACTOR
        + input.packet_loss_probability * LOSS_ENTROPY_FACTOR
        + input.bandwidth_saturation * SATURATION_ENTROPY_FACTOR;

    entropy.min(MAX_NETWORK_ENTROPY)
}

/// Calculate compute entropy from resource usage.
///
/// Formula: Sᶜ = β₁·cpu_cycles + β₂·mem + β₃·ctx_switches + β₄·cache_miss
///
/// From dol/entropy.dol lines 129-149
pub fn calculate_compute_entropy(input: &ComputeEntropyInput) -> f64 {
    let entropy = (input.cpu_cycles as f64) * CYCLE_ENTROPY_FACTOR
        + (input.memory_bytes as f64) * MEMORY_ENTROPY_FACTOR
        + (input.context_switches as f64) * CONTEXT_SWITCH_FACTOR
        + input.cache_miss_rate * CACHE_MISS_FACTOR;

    entropy.min(MAX_COMPUTE_ENTROPY)
}

/// Calculate storage entropy from data characteristics.
///
/// Formula: Sˢ = γ₁·size + γ₂·replica_div + γ₃·fragmentation + γ₄·debt
///
/// From dol/entropy.dol lines 182-202
pub fn calculate_storage_entropy(input: &StorageEntropyInput) -> f64 {
    let entropy = (input.size_bytes as f64) * SIZE_ENTROPY_FACTOR
        + input.replica_divergence * REPLICA_DIVERGENCE_FACTOR
        + input.fragmentation_ratio * FRAGMENTATION_FACTOR
        + input.compaction_debt * COMPACTION_DEBT_FACTOR;

    entropy.min(MAX_STORAGE_ENTROPY)
}

/// Calculate temporal entropy from time-related factors.
///
/// Formula: Sᵗ = δ₁·staleness + δ₂·clock_drift + δ₃·ordering + δ₄·version_div
///
/// From dol/entropy.dol lines 235-255
pub fn calculate_temporal_entropy(input: &TemporalEntropyInput) -> f64 {
    let entropy = input.staleness_seconds * STALENESS_ENTROPY_FACTOR
        + input.clock_drift_ms * CLOCK_DRIFT_FACTOR
        + input.ordering_uncertainty * ORDERING_UNCERTAINTY_FACTOR
        + input.version_divergence * VERSION_DIVERGENCE_FACTOR;

    entropy.min(MAX_TEMPORAL_ENTROPY)
}

/// Calculate weighted sum of all entropy components.
///
/// Formula: S_total = wₙ·Sₙ + wᶜ·Sᶜ + wˢ·Sˢ + wᵗ·Sᵗ
///
/// From dol/entropy.dol lines 261-278
pub fn weighted_entropy_sum(account: &EntropyAccount, weights: &EntropyWeights) -> f64 {
    account.network * weights.network_weight
        + account.compute * weights.compute_weight
        + account.storage * weights.storage_weight
        + account.temporal * weights.temporal_weight
}

/// Calculate price multiplier based on total entropy.
///
/// Uses piecewise linear function with increasing slope for higher entropy.
///
/// From dol/entropy.dol lines 280-311
pub fn entropy_price_multiplier(account: &EntropyAccount) -> f64 {
    let total = weighted_entropy_sum(account, &EntropyWeights::default());

    let multiplier = if total < LOW_ENTROPY_THRESHOLD {
        // Low entropy: gentle increase [0, 2) -> [1.0, 1.1)
        1.0 + (total * 0.05)
    } else if total < MED_ENTROPY_THRESHOLD {
        // Medium entropy: moderate increase [2, 5) -> [1.1, 1.4)
        1.1 + ((total - 2.0) * 0.1)
    } else if total < HIGH_ENTROPY_THRESHOLD {
        // High entropy: steeper increase [5, 8) -> [1.4, 2.0)
        1.4 + ((total - 5.0) * 0.2)
    } else {
        // Very high entropy: aggressive increase [8, 10] -> [2.0, 5.0]
        // At total=8: 2.0, at total=10: 5.0
        2.0 + ((total - 8.0) * 1.5)
    };

    multiplier.min(MAX_ENTROPY_MULTIPLIER)
}

/// Standard entropy calculator implementation
#[derive(Debug, Clone, Default)]
pub struct EntropyCalculator {
    pub weights: EntropyWeights,
}

impl EntropyCalculator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_weights(weights: EntropyWeights) -> Self {
        Self { weights }
    }

    /// Calculate entropy for all components
    pub fn calculate(
        &self,
        network: Option<&NetworkEntropyInput>,
        compute: Option<&ComputeEntropyInput>,
        storage: Option<&StorageEntropyInput>,
        temporal: Option<&TemporalEntropyInput>,
    ) -> EntropyAccount {
        EntropyAccount {
            network: network.map(calculate_network_entropy).unwrap_or(0.0),
            compute: compute.map(calculate_compute_entropy).unwrap_or(0.0),
            storage: storage.map(calculate_storage_entropy).unwrap_or(0.0),
            temporal: temporal.map(calculate_temporal_entropy).unwrap_or(0.0),
        }
    }

    /// Calculate weighted sum
    pub fn weighted_sum(&self, account: &EntropyAccount) -> f64 {
        weighted_entropy_sum(account, &self.weights)
    }

    /// Calculate price multiplier
    pub fn price_multiplier(&self, account: &EntropyAccount) -> f64 {
        entropy_price_multiplier(account)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_entropy() {
        let input = NetworkEntropyInput {
            hops: 3,
            latency_variance_ms: 10.0,
            packet_loss_probability: 0.01,
            bandwidth_saturation: 0.5,
        };
        let entropy = calculate_network_entropy(&input);
        // 3*0.1 + 10*0.01 + 0.01*5.0 + 0.5*2.0 = 0.3 + 0.1 + 0.05 + 1.0 = 1.45
        assert!((entropy - 1.45).abs() < 0.001);
    }

    #[test]
    fn test_compute_entropy() {
        let input = ComputeEntropyInput {
            cpu_cycles: 1_000_000,
            memory_bytes: 10_000_000,
            context_switches: 50,
            cache_miss_rate: 0.1,
        };
        let entropy = calculate_compute_entropy(&input);
        // 1M*1e-6 + 10M*1e-7 + 50*0.01 + 0.1*1.0 = 1.0 + 1.0 + 0.5 + 0.1 = 2.6
        assert!((entropy - 2.6).abs() < 0.001);
    }

    #[test]
    fn test_storage_entropy() {
        let input = StorageEntropyInput {
            size_bytes: 100_000_000,
            replica_divergence: 0.1,
            fragmentation_ratio: 0.2,
            compaction_debt: 1.0,
        };
        let entropy = calculate_storage_entropy(&input);
        // 100M*1e-8 + 0.1*3 + 0.2*2 + 1.0*1.5 = 1.0 + 0.3 + 0.4 + 1.5 = 3.2
        assert!((entropy - 3.2).abs() < 0.001);
    }

    #[test]
    fn test_temporal_entropy() {
        let input = TemporalEntropyInput {
            staleness_seconds: 100.0,
            clock_drift_ms: 50.0,
            ordering_uncertainty: 0.2,
            version_divergence: 1.0,
        };
        let entropy = calculate_temporal_entropy(&input);
        // 100*0.001 + 50*0.001 + 0.2*2.0 + 1.0*2.5 = 0.1 + 0.05 + 0.4 + 2.5 = 3.05
        assert!((entropy - 3.05).abs() < 0.001);
    }

    #[test]
    fn test_entropy_capped_at_max() {
        let input = NetworkEntropyInput {
            hops: 100,
            latency_variance_ms: 1000.0,
            packet_loss_probability: 1.0,
            bandwidth_saturation: 1.0,
        };
        let entropy = calculate_network_entropy(&input);
        assert!(entropy <= MAX_NETWORK_ENTROPY);
        assert_eq!(entropy, 10.0);
    }

    #[test]
    fn test_weighted_sum() {
        let account = EntropyAccount {
            network: 2.0,
            compute: 3.0,
            storage: 4.0,
            temporal: 5.0,
        };
        let weights = EntropyWeights::default();
        let sum = weighted_entropy_sum(&account, &weights);
        // 2.0*0.3 + 3.0*0.3 + 4.0*0.2 + 5.0*0.2 = 0.6 + 0.9 + 0.8 + 1.0 = 3.3
        assert!((sum - 3.3).abs() < 0.001);
    }

    #[test]
    fn test_price_multiplier_ranges() {
        // Low entropy
        let low = EntropyAccount {
            network: 0.5,
            compute: 0.5,
            storage: 0.5,
            temporal: 0.5,
        };
        let mult_low = entropy_price_multiplier(&low);
        assert!(mult_low >= 1.0 && mult_low < 1.1);

        // High entropy
        let high = EntropyAccount {
            network: 10.0,
            compute: 10.0,
            storage: 10.0,
            temporal: 10.0,
        };
        let mult_high = entropy_price_multiplier(&high);
        assert_eq!(mult_high, MAX_ENTROPY_MULTIPLIER);
    }

    #[test]
    fn test_calculator() {
        let calc = EntropyCalculator::new();

        let network = NetworkEntropyInput {
            hops: 2,
            latency_variance_ms: 5.0,
            packet_loss_probability: 0.0,
            bandwidth_saturation: 0.3,
        };

        let account = calc.calculate(Some(&network), None, None, None);
        assert!(account.network > 0.0);
        assert_eq!(account.compute, 0.0);
        assert_eq!(account.storage, 0.0);
        assert_eq!(account.temporal, 0.0);
    }
}

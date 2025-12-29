//! ENR Entropy Types
//!
//! Rust implementation of genes from dol/entropy.dol

use serde::{Deserialize, Serialize};

/// EntropyAccount - from dol/core.dol line 166
/// Tracks entropy across four dimensions
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct EntropyAccount {
    /// Network entropy (hops, latency, loss)
    pub network: f64,
    /// Compute entropy (CPU, memory churn)
    pub compute: f64,
    /// Storage entropy (size, fragmentation)
    pub storage: f64,
    /// Temporal entropy (staleness, drift)
    pub temporal: f64,
}

impl EntropyAccount {
    /// Maximum entropy per component
    pub const MAX_COMPONENT: f64 = 10.0;

    /// Constraint: bounded from dol/core.dol line 185
    pub fn is_valid(&self) -> bool {
        self.network >= 0.0
            && self.network <= Self::MAX_COMPONENT
            && self.compute >= 0.0
            && self.compute <= Self::MAX_COMPONENT
            && self.storage >= 0.0
            && self.storage <= Self::MAX_COMPONENT
            && self.temporal >= 0.0
            && self.temporal <= Self::MAX_COMPONENT
    }

    pub fn zero() -> Self {
        Self::default()
    }

    /// Clamp all values to valid range
    pub fn clamped(self) -> Self {
        Self {
            network: self.network.clamp(0.0, Self::MAX_COMPONENT),
            compute: self.compute.clamp(0.0, Self::MAX_COMPONENT),
            storage: self.storage.clamp(0.0, Self::MAX_COMPONENT),
            temporal: self.temporal.clamp(0.0, Self::MAX_COMPONENT),
        }
    }
}

/// EntropyWeights - from dol/core.dol line 202
/// Weights for combining entropy components (must sum to 1.0)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntropyWeights {
    pub network_weight: f64,
    pub compute_weight: f64,
    pub storage_weight: f64,
    pub temporal_weight: f64,
}

impl Default for EntropyWeights {
    fn default() -> Self {
        Self {
            network_weight: 0.3,
            compute_weight: 0.3,
            storage_weight: 0.2,
            temporal_weight: 0.2,
        }
    }
}

impl EntropyWeights {
    /// Constraint: weights_sum_to_one from dol/core.dol line 213
    pub fn is_valid(&self) -> bool {
        let sum = self.network_weight + self.compute_weight + self.storage_weight + self.temporal_weight;
        (sum - 1.0).abs() < 0.001
    }

    pub fn new(network: f64, compute: f64, storage: f64, temporal: f64) -> Option<Self> {
        let weights = Self {
            network_weight: network,
            compute_weight: compute,
            storage_weight: storage,
            temporal_weight: temporal,
        };
        if weights.is_valid() {
            Some(weights)
        } else {
            None
        }
    }
}

/// NetworkEntropyInput - from dol/entropy.dol line 69
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct NetworkEntropyInput {
    /// Number of network hops
    pub hops: u32,
    /// Latency jitter in milliseconds
    pub latency_variance_ms: f64,
    /// Probability of packet loss [0, 1]
    pub packet_loss_probability: f64,
    /// Bandwidth utilization [0, 1]
    pub bandwidth_saturation: f64,
}

impl NetworkEntropyInput {
    /// Constraint: valid_ranges from dol/entropy.dol line 79
    pub fn is_valid(&self) -> bool {
        self.packet_loss_probability >= 0.0
            && self.packet_loss_probability <= 1.0
            && self.bandwidth_saturation >= 0.0
            && self.bandwidth_saturation <= 1.0
    }
}

/// ComputeEntropyInput - from dol/entropy.dol line 114
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ComputeEntropyInput {
    /// CPU cycles consumed
    pub cpu_cycles: u64,
    /// Memory allocation in bytes
    pub memory_bytes: u64,
    /// Context switch count
    pub context_switches: u32,
    /// Cache miss ratio [0, 1]
    pub cache_miss_rate: f64,
}

impl ComputeEntropyInput {
    /// Constraint: valid_range from dol/entropy.dol line 124
    pub fn is_valid(&self) -> bool {
        self.cache_miss_rate >= 0.0 && self.cache_miss_rate <= 1.0
    }
}

/// StorageEntropyInput - from dol/entropy.dol line 156
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct StorageEntropyInput {
    /// Data size in bytes
    pub size_bytes: u64,
    /// 0.0 = perfect sync, 1.0 = fully diverged
    pub replica_divergence: f64,
    /// 0.0 = contiguous, 1.0 = fully fragmented
    pub fragmentation_ratio: f64,
    /// Pending compaction work [0, 10]
    pub compaction_debt: f64,
}

impl StorageEntropyInput {
    /// Constraint: valid_ranges from dol/entropy.dol line 166
    pub fn is_valid(&self) -> bool {
        self.replica_divergence >= 0.0
            && self.replica_divergence <= 1.0
            && self.fragmentation_ratio >= 0.0
            && self.fragmentation_ratio <= 1.0
            && self.compaction_debt >= 0.0
    }
}

/// TemporalEntropyInput - from dol/entropy.dol line 209
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct TemporalEntropyInput {
    /// Time since last update
    pub staleness_seconds: f64,
    /// Clock drift in milliseconds
    pub clock_drift_ms: f64,
    /// 0.0 = certain, 1.0 = uncertain
    pub ordering_uncertainty: f64,
    /// Vector clock divergence [0, 10]
    pub version_divergence: f64,
}

impl TemporalEntropyInput {
    /// Constraint: valid_ranges from dol/entropy.dol line 219
    pub fn is_valid(&self) -> bool {
        self.staleness_seconds >= 0.0
            && self.clock_drift_ms >= 0.0
            && self.ordering_uncertainty >= 0.0
            && self.ordering_uncertainty <= 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_account_valid() {
        let valid = EntropyAccount {
            network: 5.0,
            compute: 3.0,
            storage: 2.0,
            temporal: 1.0,
        };
        assert!(valid.is_valid());

        let invalid = EntropyAccount {
            network: 11.0,
            compute: 3.0,
            storage: 2.0,
            temporal: 1.0,
        };
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_entropy_weights_valid() {
        let default = EntropyWeights::default();
        assert!(default.is_valid());

        let custom = EntropyWeights::new(0.25, 0.25, 0.25, 0.25);
        assert!(custom.is_some());
        assert!(custom.unwrap().is_valid());

        let invalid = EntropyWeights::new(0.5, 0.5, 0.5, 0.5);
        assert!(invalid.is_none());
    }

    #[test]
    fn test_clamped() {
        let account = EntropyAccount {
            network: 15.0,
            compute: -1.0,
            storage: 5.0,
            temporal: 10.0,
        };
        let clamped = account.clamped();
        assert!(clamped.is_valid());
        assert_eq!(clamped.network, 10.0);
        assert_eq!(clamped.compute, 0.0);
        assert_eq!(clamped.storage, 5.0);
        assert_eq!(clamped.temporal, 10.0);
    }
}

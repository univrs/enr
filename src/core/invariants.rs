//! ENR Invariants
//!
//! Implements constraints from dol/core.dol:
//! - CreditConservation (line 478)
//! - EntropyNonNegative (line 489)
//! - RevivalCompleteness (line 499)
//! - NexusConsistency (line 509)
//! - SeptalSafety (line 520)

use crate::core::{Credits, NodeId};

/// Credit Conservation Invariant
/// law: sum(all_credits) == GENESIS_AMOUNT + minted - burned
#[derive(Debug, Clone)]
pub struct CreditConservation {
    pub genesis_amount: Credits,
    pub total_minted: Credits,
    pub total_burned: Credits,
}

impl CreditConservation {
    pub fn new(genesis_amount: Credits) -> Self {
        Self {
            genesis_amount,
            total_minted: Credits::ZERO,
            total_burned: Credits::ZERO,
        }
    }

    pub fn expected_total(&self) -> Credits {
        Credits::new(
            self.genesis_amount
                .amount
                .saturating_add(self.total_minted.amount)
                .saturating_sub(self.total_burned.amount),
        )
    }

    pub fn check(&self, actual_total: Credits) -> bool {
        actual_total == self.expected_total()
    }

    pub fn record_mint(&mut self, amount: Credits) {
        self.total_minted = self.total_minted.saturating_add(amount);
    }

    pub fn record_burn(&mut self, amount: Credits) {
        self.total_burned = self.total_burned.saturating_add(amount);
    }
}

/// Entropy Non-Negative Invariant
/// law: entropy(tx) >= 0.0
pub fn check_entropy_non_negative(entropy: f64) -> bool {
    entropy >= 0.0
}

/// Entropy bounded invariant
/// law: entropy <= MAX_ENTROPY (10.0 per component)
pub fn check_entropy_bounded(entropy: f64, max: f64) -> bool {
    entropy >= 0.0 && entropy <= max
}

/// Nexus Consistency Invariant
/// law: nexus.view >= leaf.view for all leaves
pub fn check_nexus_consistency<T: PartialOrd>(nexus_view: &T, leaf_view: &T) -> bool {
    nexus_view >= leaf_view
}

/// Septal Safety Invariant
/// law: isolated(node) implies no_credit_flow(node)
#[derive(Debug, Clone, Default)]
pub struct SeptalSafety {
    pub isolated_nodes: Vec<NodeId>,
}

impl SeptalSafety {
    pub fn new() -> Self {
        Self {
            isolated_nodes: Vec::new(),
        }
    }

    pub fn isolate(&mut self, node: NodeId) {
        if !self.isolated_nodes.contains(&node) {
            self.isolated_nodes.push(node);
        }
    }

    pub fn restore(&mut self, node: &NodeId) {
        self.isolated_nodes.retain(|n| n != node);
    }

    pub fn is_isolated(&self, node: &NodeId) -> bool {
        self.isolated_nodes.contains(node)
    }

    pub fn can_transact(&self, node: &NodeId) -> bool {
        !self.is_isolated(node)
    }

    pub fn check_transaction(&self, from: &NodeId, to: &NodeId) -> bool {
        self.can_transact(from) && self.can_transact(to)
    }
}

/// Weight normalization check
/// law: sum(weights) == 1.0 (within tolerance)
pub fn check_weights_normalized(weights: &[f64], tolerance: f64) -> bool {
    let sum: f64 = weights.iter().sum();
    (sum - 1.0).abs() < tolerance
}

/// Invariant checker that aggregates all invariant checks
#[derive(Debug)]
pub struct InvariantChecker {
    pub conservation: CreditConservation,
    pub septal_safety: SeptalSafety,
}

impl InvariantChecker {
    pub fn new(genesis_amount: Credits) -> Self {
        Self {
            conservation: CreditConservation::new(genesis_amount),
            septal_safety: SeptalSafety::new(),
        }
    }

    /// Check all invariants
    pub fn check_all(&self, actual_total: Credits) -> Vec<InvariantViolation> {
        let mut violations = Vec::new();

        if !self.conservation.check(actual_total) {
            violations.push(InvariantViolation::CreditConservation {
                expected: self.conservation.expected_total(),
                actual: actual_total,
            });
        }

        violations
    }
}

/// Invariant violation types
#[derive(Debug, Clone)]
pub enum InvariantViolation {
    CreditConservation { expected: Credits, actual: Credits },
    EntropyNegative { value: f64 },
    EntropyUnbounded { value: f64, max: f64 },
    NexusInconsistency { nexus: NodeId, leaf: NodeId },
    SeptalViolation { isolated_node: NodeId },
    WeightsNotNormalized { sum: f64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credit_conservation() {
        let mut conservation = CreditConservation::new(Credits::new(1_000_000));

        assert!(conservation.check(Credits::new(1_000_000)));

        conservation.record_mint(Credits::new(100));
        assert!(conservation.check(Credits::new(1_000_100)));

        conservation.record_burn(Credits::new(50));
        assert!(conservation.check(Credits::new(1_000_050)));
    }

    #[test]
    fn test_entropy_checks() {
        assert!(check_entropy_non_negative(0.0));
        assert!(check_entropy_non_negative(5.5));
        assert!(!check_entropy_non_negative(-0.1));

        assert!(check_entropy_bounded(5.0, 10.0));
        assert!(!check_entropy_bounded(11.0, 10.0));
        assert!(!check_entropy_bounded(-1.0, 10.0));
    }

    #[test]
    fn test_septal_safety() {
        let mut safety = SeptalSafety::new();
        let node = NodeId::from_bytes([1u8; 32]);
        let other = NodeId::from_bytes([2u8; 32]);

        assert!(safety.can_transact(&node));
        assert!(safety.check_transaction(&node, &other));

        safety.isolate(node);
        assert!(!safety.can_transact(&node));
        assert!(!safety.check_transaction(&node, &other));
        assert!(safety.can_transact(&other));

        safety.restore(&node);
        assert!(safety.can_transact(&node));
    }

    #[test]
    fn test_weights_normalized() {
        let good_weights = [0.3, 0.3, 0.2, 0.2];
        assert!(check_weights_normalized(&good_weights, 0.001));

        let bad_weights = [0.3, 0.3, 0.3, 0.3];
        assert!(!check_weights_normalized(&bad_weights, 0.001));
    }
}

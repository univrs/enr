//! Septal Healing
//!
//! Recovery mechanisms for isolated nodes

use super::gate::{HealthStatus, SeptalGate, SeptalGateConfig, SeptalGateState};
use super::woronin::WoroninManager;
use crate::core::{NodeId, Timestamp};

/// Health checker trait
pub trait HealthChecker {
    fn check_health(&self, node: &NodeId) -> HealthStatus;
}

/// Healing manager for managing recovery attempts
pub struct HealingManager<H: HealthChecker> {
    health_checker: H,
    last_check: std::collections::HashMap<NodeId, Timestamp>,
    check_interval_ms: u64,
}

impl<H: HealthChecker> HealingManager<H> {
    pub fn new(health_checker: H, check_interval_ms: u64) -> Self {
        Self {
            health_checker,
            last_check: std::collections::HashMap::new(),
            check_interval_ms,
        }
    }

    /// Check if it's time for a health check
    pub fn should_check(&self, node: &NodeId) -> bool {
        match self.last_check.get(node) {
            Some(last) => {
                let now = Timestamp::now();
                now.millis >= last.millis + self.check_interval_ms
            }
            None => true,
        }
    }

    /// Perform health check and attempt recovery if appropriate
    pub fn attempt_recovery(
        &mut self,
        gate: &mut SeptalGate,
        woronin: &mut WoroninManager,
        config: &SeptalGateConfig,
    ) -> RecoveryResult {
        let node = gate.node;

        // Only attempt recovery for closed gates
        if !gate.state.is_closed() && !gate.state.is_half_open() {
            return RecoveryResult::NotNeeded;
        }

        // Check if it's time for a health check
        if !self.should_check(&node) {
            return RecoveryResult::TooSoon;
        }

        // Record check time
        self.last_check.insert(node, Timestamp::now());

        // Check health
        let health = self.health_checker.check_health(&node);

        match gate.state {
            SeptalGateState::Closed => {
                // Try to transition to half-open
                if gate.attempt_half_open() {
                    RecoveryResult::EnteredHalfOpen
                } else {
                    RecoveryResult::StillClosed
                }
            }
            SeptalGateState::HalfOpen => {
                // Test recovery
                if health.is_healthy && !health.should_isolate(config) {
                    gate.recover();
                    woronin.deactivate(&node);
                    RecoveryResult::Recovered
                } else {
                    gate.fail_recovery();
                    RecoveryResult::RecoveryFailed
                }
            }
            SeptalGateState::Open => RecoveryResult::NotNeeded,
        }
    }
}

/// Result of a recovery attempt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryResult {
    /// Node was not isolated, no recovery needed
    NotNeeded,
    /// Too soon to check again
    TooSoon,
    /// Node is still closed (waiting for timeout)
    StillClosed,
    /// Node entered half-open state for testing
    EnteredHalfOpen,
    /// Recovery test passed, node is now open
    Recovered,
    /// Recovery test failed, back to closed
    RecoveryFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockHealthChecker {
        healthy: bool,
    }

    impl HealthChecker for MockHealthChecker {
        fn check_health(&self, _node: &NodeId) -> HealthStatus {
            HealthStatus {
                is_healthy: self.healthy,
                timeout_score: if self.healthy { 0.0 } else { 1.0 },
                credit_score: if self.healthy { 0.0 } else { 1.0 },
                reputation_score: if self.healthy { 0.0 } else { 1.0 },
                last_check: Timestamp::now(),
            }
        }
    }

    #[test]
    fn test_recovery_not_needed() {
        let node = NodeId::from_bytes([1u8; 32]);
        let mut gate = SeptalGate::new(node);
        let mut woronin = WoroninManager::new();
        let config = SeptalGateConfig::default();
        let checker = MockHealthChecker { healthy: true };
        let mut manager = HealingManager::new(checker, 0);

        let result = manager.attempt_recovery(&mut gate, &mut woronin, &config);
        assert_eq!(result, RecoveryResult::NotNeeded);
    }

    #[test]
    fn test_recovery_successful() {
        let node = NodeId::from_bytes([1u8; 32]);
        let mut gate = SeptalGate::new(node);
        gate.state = SeptalGateState::HalfOpen;

        let mut woronin = WoroninManager::new();
        woronin.activate(node, "test");

        let config = SeptalGateConfig::default();
        let checker = MockHealthChecker { healthy: true };
        let mut manager = HealingManager::new(checker, 0);

        let result = manager.attempt_recovery(&mut gate, &mut woronin, &config);
        assert_eq!(result, RecoveryResult::Recovered);
        assert!(gate.state.is_open());
        assert!(!woronin.is_isolated(&node));
    }

    #[test]
    fn test_recovery_failed() {
        let node = NodeId::from_bytes([1u8; 32]);
        let mut gate = SeptalGate::new(node);
        gate.state = SeptalGateState::HalfOpen;

        let mut woronin = WoroninManager::new();
        woronin.activate(node, "test");

        let config = SeptalGateConfig::default();
        let checker = MockHealthChecker { healthy: false };
        let mut manager = HealingManager::new(checker, 0);

        let result = manager.attempt_recovery(&mut gate, &mut woronin, &config);
        assert_eq!(result, RecoveryResult::RecoveryFailed);
        assert!(gate.state.is_closed());
    }
}

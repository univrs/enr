//! Septal Gate (Circuit Breaker)
//!
//! Implements septal gate from dol/septal.dol

use crate::core::{NodeId, Timestamp, Duration};
use serde::{Deserialize, Serialize};

/// Constants from dol/septal.dol lines 36-43
pub const FAILURE_THRESHOLD: u32 = 5;
pub const RECOVERY_TIMEOUT_MS: u64 = 60_000; // 60 seconds
pub const HALF_OPEN_TEST_INTERVAL_MS: u64 = 10_000; // 10 seconds
pub const ISOLATION_THRESHOLD: f64 = 0.7;
pub const PING_TIMEOUT_MS: u64 = 5_000;
pub const HEALTH_CHECK_INTERVAL_MS: u64 = 10_000;

/// SeptalGateState - from dol/core.dol line 398
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SeptalGateState {
    /// Normal operation
    Open,
    /// Testing recovery
    HalfOpen,
    /// Isolated (Woronin active)
    Closed,
}

impl Default for SeptalGateState {
    fn default() -> Self {
        SeptalGateState::Open
    }
}

impl SeptalGateState {
    pub fn is_open(&self) -> bool {
        matches!(self, SeptalGateState::Open)
    }

    pub fn is_closed(&self) -> bool {
        matches!(self, SeptalGateState::Closed)
    }

    pub fn is_half_open(&self) -> bool {
        matches!(self, SeptalGateState::HalfOpen)
    }

    /// Check if traffic is allowed
    pub fn allows_traffic(&self) -> bool {
        matches!(self, SeptalGateState::Open | SeptalGateState::HalfOpen)
    }
}

/// SeptalGate - from dol/core.dol line 406
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeptalGate {
    pub node: NodeId,
    pub state: SeptalGateState,
    pub failure_count: u32,
    pub last_failure: Option<Timestamp>,
    pub isolation_start: Option<Timestamp>,
}

impl SeptalGate {
    pub fn new(node: NodeId) -> Self {
        Self {
            node,
            state: SeptalGateState::Open,
            failure_count: 0,
            last_failure: None,
            isolation_start: None,
        }
    }

    /// Record a failure
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Timestamp::now());
    }

    /// Reset failure count on success
    pub fn record_success(&mut self) {
        self.failure_count = 0;
    }

    /// Check if should trip (close) the gate
    pub fn should_trip(&self) -> bool {
        self.failure_count >= FAILURE_THRESHOLD
    }

    /// Trip (close) the gate
    pub fn trip(&mut self) {
        self.state = SeptalGateState::Closed;
        self.isolation_start = Some(Timestamp::now());
    }

    /// Attempt transition to half-open
    pub fn attempt_half_open(&mut self) -> bool {
        if self.state != SeptalGateState::Closed {
            return false;
        }

        if let Some(start) = self.isolation_start {
            let now = Timestamp::now();
            if now.millis >= start.millis + RECOVERY_TIMEOUT_MS {
                self.state = SeptalGateState::HalfOpen;
                return true;
            }
        }
        false
    }

    /// Complete recovery (transition to open)
    pub fn recover(&mut self) {
        self.state = SeptalGateState::Open;
        self.failure_count = 0;
        self.isolation_start = None;
    }

    /// Fail recovery (transition back to closed)
    pub fn fail_recovery(&mut self) {
        self.state = SeptalGateState::Closed;
        self.isolation_start = Some(Timestamp::now());
    }
}

/// SeptalGateConfig - from dol/core.dol line 426
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeptalGateConfig {
    pub timeout_weight: f64,
    pub timeout_threshold: Duration,
    pub credit_default_weight: f64,
    pub credit_default_threshold: u64,
    pub reputation_weight: f64,
    pub reputation_threshold: f64,
}

impl Default for SeptalGateConfig {
    fn default() -> Self {
        Self {
            timeout_weight: 0.4,
            timeout_threshold: Duration::seconds(30),
            credit_default_weight: 0.3,
            credit_default_threshold: 100,
            reputation_weight: 0.3,
            reputation_threshold: 0.5,
        }
    }
}

impl SeptalGateConfig {
    /// Constraint: weights_sum from dol/core.dol line 438
    pub fn is_valid(&self) -> bool {
        let sum = self.timeout_weight + self.credit_default_weight + self.reputation_weight;
        (sum - 1.0).abs() < 0.001
    }
}

/// HealthStatus - from dol/core.dol line 455
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub timeout_score: f64,
    pub credit_score: f64,
    pub reputation_score: f64,
    pub last_check: Timestamp,
}

impl HealthStatus {
    /// Calculate weighted score
    pub fn weighted_score(&self, config: &SeptalGateConfig) -> f64 {
        self.timeout_score * config.timeout_weight
            + self.credit_score * config.credit_default_weight
            + self.reputation_score * config.reputation_weight
    }

    /// Check if should isolate based on config
    /// From dol/septal.dol lines 99-117
    pub fn should_isolate(&self, config: &SeptalGateConfig) -> bool {
        self.weighted_score(config) >= ISOLATION_THRESHOLD
    }
}

/// SeptalGateTransition - from dol/septal.dol line 123
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeptalGateTransition {
    pub from_state: SeptalGateState,
    pub to_state: SeptalGateState,
    pub reason: String,
    pub timestamp: Timestamp,
}

/// Determine state transition for a septal gate
/// From dol/septal.dol lines 134-215
pub fn transition_gate(
    gate: &mut SeptalGate,
    health: &HealthStatus,
    config: &SeptalGateConfig,
) -> Option<SeptalGateTransition> {
    match gate.state {
        SeptalGateState::Open => {
            if health.should_isolate(config) {
                gate.record_failure();

                if gate.should_trip() {
                    let transition = SeptalGateTransition {
                        from_state: SeptalGateState::Open,
                        to_state: SeptalGateState::Closed,
                        reason: format!(
                            "Failure threshold exceeded ({} failures)",
                            gate.failure_count
                        ),
                        timestamp: Timestamp::now(),
                    };
                    gate.trip();
                    return Some(transition);
                }
            } else {
                gate.record_success();
            }
            None
        }
        SeptalGateState::Closed => {
            if gate.attempt_half_open() {
                Some(SeptalGateTransition {
                    from_state: SeptalGateState::Closed,
                    to_state: SeptalGateState::HalfOpen,
                    reason: "Recovery timeout elapsed, testing recovery".to_string(),
                    timestamp: Timestamp::now(),
                })
            } else {
                None
            }
        }
        SeptalGateState::HalfOpen => {
            if health.is_healthy {
                let transition = SeptalGateTransition {
                    from_state: SeptalGateState::HalfOpen,
                    to_state: SeptalGateState::Open,
                    reason: "Recovery test passed".to_string(),
                    timestamp: Timestamp::now(),
                };
                gate.recover();
                Some(transition)
            } else {
                let transition = SeptalGateTransition {
                    from_state: SeptalGateState::HalfOpen,
                    to_state: SeptalGateState::Closed,
                    reason: "Recovery test failed".to_string(),
                    timestamp: Timestamp::now(),
                };
                gate.fail_recovery();
                Some(transition)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gate_state_transitions() {
        let node = NodeId::from_bytes([1u8; 32]);
        let mut gate = SeptalGate::new(node);

        assert!(gate.state.is_open());
        assert!(gate.state.allows_traffic());

        // Record failures
        for _ in 0..FAILURE_THRESHOLD {
            gate.record_failure();
        }
        assert!(gate.should_trip());

        gate.trip();
        assert!(gate.state.is_closed());
        assert!(!gate.state.allows_traffic());
    }

    #[test]
    fn test_health_status_isolation() {
        let config = SeptalGateConfig::default();

        let healthy = HealthStatus {
            is_healthy: true,
            timeout_score: 0.0,
            credit_score: 0.0,
            reputation_score: 0.0,
            last_check: Timestamp::now(),
        };
        assert!(!healthy.should_isolate(&config));

        let unhealthy = HealthStatus {
            is_healthy: false,
            timeout_score: 1.0,
            credit_score: 1.0,
            reputation_score: 1.0,
            last_check: Timestamp::now(),
        };
        assert!(unhealthy.should_isolate(&config));
    }

    #[test]
    fn test_config_validation() {
        let config = SeptalGateConfig::default();
        assert!(config.is_valid());

        let invalid = SeptalGateConfig {
            timeout_weight: 0.5,
            credit_default_weight: 0.5,
            reputation_weight: 0.5,
            ..Default::default()
        };
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_transition_open_to_closed() {
        let node = NodeId::from_bytes([1u8; 32]);
        let mut gate = SeptalGate::new(node);
        let config = SeptalGateConfig::default();

        // Set gate to threshold - 1
        gate.failure_count = FAILURE_THRESHOLD - 1;

        let unhealthy = HealthStatus {
            is_healthy: false,
            timeout_score: 1.0,
            credit_score: 1.0,
            reputation_score: 1.0,
            last_check: Timestamp::now(),
        };

        let transition = transition_gate(&mut gate, &unhealthy, &config);
        assert!(transition.is_some());
        let t = transition.unwrap();
        assert_eq!(t.from_state, SeptalGateState::Open);
        assert_eq!(t.to_state, SeptalGateState::Closed);
    }
}

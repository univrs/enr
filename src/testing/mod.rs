//! ENR Testing Module
//!
//! Chaos testing framework from ENR-ARCHITECTURE.md Section 12
//!
//! Six chaos scenarios:
//! 1. NexusFailure: Primary nexus goes down
//! 2. NetworkPartition: Network split into isolated groups
//! 3. CreditExhaustion: Node runs out of credits
//! 4. CascadeFailure: Multiple sequential node failures
//! 5. ByzantineNexus: Nexus node behaves maliciously
//! 6. EntropySpike: Sudden entropy surge

use crate::core::{Credits, NodeId, Timestamp};
use serde::{Deserialize, Serialize};

/// Chaos scenario types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChaosScenario {
    /// Primary nexus goes down
    NexusFailure,
    /// Network split into isolated groups
    NetworkPartition,
    /// Node runs out of credits
    CreditExhaustion,
    /// Multiple sequential node failures
    CascadeFailure,
    /// Nexus node behaves maliciously
    ByzantineNexus,
    /// Sudden entropy surge
    EntropySpike,
}

/// Chaos event
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChaosEvent {
    pub scenario: ChaosScenario,
    pub affected_nodes: Vec<NodeId>,
    pub timestamp: Timestamp,
    pub duration_ms: Option<u64>,
    pub parameters: std::collections::HashMap<String, String>,
}

impl ChaosEvent {
    pub fn new(scenario: ChaosScenario) -> Self {
        Self {
            scenario,
            affected_nodes: Vec::new(),
            timestamp: Timestamp::now(),
            duration_ms: None,
            parameters: std::collections::HashMap::new(),
        }
    }

    pub fn with_nodes(mut self, nodes: Vec<NodeId>) -> Self {
        self.affected_nodes = nodes;
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }
}

/// Test cluster configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestClusterConfig {
    pub node_count: usize,
    pub nexus_count: usize,
    pub initial_credits: Credits,
    pub enable_chaos: bool,
    pub failure_rate: f64,
}

impl Default for TestClusterConfig {
    fn default() -> Self {
        Self {
            node_count: 100,
            nexus_count: 10,
            initial_credits: Credits::new(10000),
            enable_chaos: true,
            failure_rate: 0.1, // 10% failure rate
        }
    }
}

/// Simulated node for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedNode {
    pub id: NodeId,
    pub credits: Credits,
    pub is_nexus: bool,
    pub is_alive: bool,
    pub is_isolated: bool,
}

impl SimulatedNode {
    pub fn new(id: NodeId, credits: Credits, is_nexus: bool) -> Self {
        Self {
            id,
            credits,
            is_nexus,
            is_alive: true,
            is_isolated: false,
        }
    }

    pub fn fail(&mut self) {
        self.is_alive = false;
    }

    pub fn recover(&mut self) {
        self.is_alive = true;
    }

    pub fn isolate(&mut self) {
        self.is_isolated = true;
    }

    pub fn reconnect(&mut self) {
        self.is_isolated = false;
    }
}

/// Test cluster for simulation
pub struct TestCluster {
    pub config: TestClusterConfig,
    pub nodes: Vec<SimulatedNode>,
    pub chaos_events: Vec<ChaosEvent>,
}

impl TestCluster {
    pub fn new(config: TestClusterConfig) -> Self {
        let mut nodes = Vec::with_capacity(config.node_count);

        // Create nodes
        for i in 0..config.node_count {
            let mut id_bytes = [0u8; 32];
            id_bytes[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            let id = NodeId::from_bytes(id_bytes);
            let is_nexus = i < config.nexus_count;
            nodes.push(SimulatedNode::new(id, config.initial_credits, is_nexus));
        }

        Self {
            config,
            nodes,
            chaos_events: Vec::new(),
        }
    }

    /// Get alive nodes
    pub fn alive_nodes(&self) -> Vec<&SimulatedNode> {
        self.nodes.iter().filter(|n| n.is_alive).collect()
    }

    /// Get alive nexus nodes
    pub fn alive_nexuses(&self) -> Vec<&SimulatedNode> {
        self.nodes
            .iter()
            .filter(|n| n.is_alive && n.is_nexus)
            .collect()
    }

    /// Failure rate in the cluster
    pub fn failure_rate(&self) -> f64 {
        let failed = self.nodes.iter().filter(|n| !n.is_alive).count();
        failed as f64 / self.nodes.len() as f64
    }

    /// Inject chaos event
    pub fn inject_chaos(&mut self, event: ChaosEvent) {
        match event.scenario {
            ChaosScenario::NexusFailure => {
                for node_id in &event.affected_nodes {
                    if let Some(node) = self.nodes.iter_mut().find(|n| n.id == *node_id) {
                        node.fail();
                    }
                }
            }
            ChaosScenario::NetworkPartition => {
                for node_id in &event.affected_nodes {
                    if let Some(node) = self.nodes.iter_mut().find(|n| n.id == *node_id) {
                        node.isolate();
                    }
                }
            }
            ChaosScenario::CreditExhaustion => {
                for node_id in &event.affected_nodes {
                    if let Some(node) = self.nodes.iter_mut().find(|n| n.id == *node_id) {
                        node.credits = Credits::ZERO;
                    }
                }
            }
            ChaosScenario::CascadeFailure => {
                // Fail nodes sequentially
                for node_id in &event.affected_nodes {
                    if let Some(node) = self.nodes.iter_mut().find(|n| n.id == *node_id) {
                        node.fail();
                    }
                }
            }
            ChaosScenario::ByzantineNexus => {
                // Mark nexus as byzantine (for now just fail)
                for node_id in &event.affected_nodes {
                    if let Some(node) = self.nodes.iter_mut().find(|n| n.id == *node_id) {
                        node.fail();
                    }
                }
            }
            ChaosScenario::EntropySpike => {
                // Entropy spike doesn't directly affect nodes
            }
        }
        self.chaos_events.push(event);
    }

    /// Check invariants
    pub fn check_invariants(&self) -> Vec<InvariantViolation> {
        let mut violations = Vec::new();

        // Check that we have at least one alive nexus
        if self.alive_nexuses().is_empty() && !self.nodes.is_empty() {
            violations.push(InvariantViolation::NoAliveNexus);
        }

        // Check failure rate doesn't exceed 50%
        if self.failure_rate() > 0.5 {
            violations.push(InvariantViolation::ExcessiveFailures {
                rate: self.failure_rate(),
            });
        }

        violations
    }
}

/// Invariant violations
#[derive(Debug, Clone, PartialEq)]
pub enum InvariantViolation {
    NoAliveNexus,
    ExcessiveFailures { rate: f64 },
    CreditConservationViolation { expected: Credits, actual: Credits },
    CascadeDetected { failed_count: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_creation() {
        let config = TestClusterConfig {
            node_count: 10,
            nexus_count: 2,
            ..Default::default()
        };
        let cluster = TestCluster::new(config);

        assert_eq!(cluster.nodes.len(), 10);
        assert_eq!(cluster.alive_nexuses().len(), 2);
    }

    #[test]
    fn test_chaos_injection() {
        let config = TestClusterConfig {
            node_count: 10,
            nexus_count: 2,
            ..Default::default()
        };
        let mut cluster = TestCluster::new(config);

        let nexus_id = cluster.nodes[0].id;
        let event = ChaosEvent::new(ChaosScenario::NexusFailure).with_nodes(vec![nexus_id]);

        cluster.inject_chaos(event);

        assert!(!cluster.nodes[0].is_alive);
        assert_eq!(cluster.alive_nexuses().len(), 1);
    }

    #[test]
    fn test_invariant_check() {
        let config = TestClusterConfig {
            node_count: 2,
            nexus_count: 1,
            ..Default::default()
        };
        let mut cluster = TestCluster::new(config);

        // Initially no violations
        assert!(cluster.check_invariants().is_empty());

        // Fail the only nexus
        let nexus_id = cluster.nodes[0].id;
        cluster.inject_chaos(ChaosEvent::new(ChaosScenario::NexusFailure).with_nodes(vec![nexus_id]));

        // Now we should have a violation
        let violations = cluster.check_invariants();
        assert!(violations.iter().any(|v| matches!(v, InvariantViolation::NoAliveNexus)));
    }
}

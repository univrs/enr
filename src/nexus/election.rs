//! Nexus Election Algorithm
//!
//! Implements nexus election from dol/nexus.dol lines 275-338

use super::types::*;
use crate::core::NodeId;

/// Nexus eligibility thresholds - from dol/nexus.dol lines 30-32
pub const MIN_NEXUS_UPTIME: f64 = 0.95;
pub const MIN_NEXUS_BANDWIDTH: u64 = 10_000_000;
pub const MIN_NEXUS_REPUTATION: f64 = 0.7;

/// Nexus capacity - from dol/nexus.dol lines 35-36
pub const MIN_LEAVES_PER_NEXUS: u32 = 5;
pub const MAX_LEAVES_PER_NEXUS: u32 = 50;

/// Election weights - from dol/nexus.dol lines 39-42
pub const UPTIME_WEIGHT: f64 = 0.3;
pub const BANDWIDTH_WEIGHT: f64 = 0.3;
pub const REPUTATION_WEIGHT: f64 = 0.2;
pub const CONNECTIVITY_WEIGHT: f64 = 0.2;

/// Normalize bandwidth to [0, 1] for scoring.
/// 100 Mbps = 1.0
/// From dol/nexus.dol lines 252-260
pub fn normalize_bandwidth(bandwidth: u64) -> f64 {
    let max_expected = 100_000_000.0;
    (bandwidth as f64 / max_expected).min(1.0)
}

/// Normalize connectivity to [0, 1] for scoring.
/// Prefer nodes with moderate connectivity.
/// From dol/nexus.dol lines 262-273
pub fn normalize_connectivity(leaf_count: u32) -> f64 {
    let optimal = (MIN_LEAVES_PER_NEXUS + MAX_LEAVES_PER_NEXUS) / 2;
    let distance = (leaf_count as i32 - optimal as i32).unsigned_abs();
    let max_distance = MAX_LEAVES_PER_NEXUS as f64;

    1.0 - (distance as f64 / max_distance)
}

/// Calculate election score for a candidate
pub fn calculate_election_score(candidate: &NexusCandidate) -> f64 {
    candidate.uptime * UPTIME_WEIGHT
        + normalize_bandwidth(candidate.bandwidth) * BANDWIDTH_WEIGHT
        + candidate.reputation * REPUTATION_WEIGHT
        + normalize_connectivity(candidate.current_leaf_count) * CONNECTIVITY_WEIGHT
}

/// Node metrics for election
pub trait NodeMetrics {
    fn get_uptime(&self, node: &NodeId) -> f64;
    fn get_bandwidth(&self, node: &NodeId) -> u64;
    fn get_reputation(&self, node: &NodeId) -> f64;
    fn get_connection_count(&self, node: &NodeId) -> u32;
}

/// Check if a node meets minimum nexus eligibility requirements
pub fn is_nexus_eligible(uptime: f64, bandwidth: u64, reputation: f64) -> bool {
    uptime >= MIN_NEXUS_UPTIME
        && bandwidth >= MIN_NEXUS_BANDWIDTH
        && reputation >= MIN_NEXUS_REPUTATION
}

/// Nexus elector
pub struct NexusElector<M: NodeMetrics> {
    metrics: M,
}

impl<M: NodeMetrics> NexusElector<M> {
    pub fn new(metrics: M) -> Self {
        Self { metrics }
    }

    /// Build candidate from node
    fn build_candidate(&self, node: NodeId) -> NexusCandidate {
        NexusCandidate {
            node,
            uptime: self.metrics.get_uptime(&node),
            bandwidth: self.metrics.get_bandwidth(&node),
            reputation: self.metrics.get_reputation(&node),
            current_leaf_count: self.metrics.get_connection_count(&node),
            election_score: 0.0,
        }
    }

    /// Elect a nexus for the given region
    /// From dol/nexus.dol lines 275-338
    pub fn elect(&self, region: &Region) -> Option<NodeId> {
        if region.nodes.is_empty() {
            return None;
        }

        // Step 1: Gather qualified candidates
        let mut candidates: Vec<NexusCandidate> = region
            .nodes
            .iter()
            .map(|n| self.build_candidate(*n))
            .filter(|c| is_nexus_eligible(c.uptime, c.bandwidth, c.reputation))
            .collect();

        // Step 2: If no qualified candidates, use best available by reputation
        if candidates.is_empty() {
            let mut all_candidates: Vec<NexusCandidate> = region
                .nodes
                .iter()
                .map(|n| self.build_candidate(*n))
                .collect();

            all_candidates.sort_by(|a, b| {
                b.reputation
                    .partial_cmp(&a.reputation)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            candidates = all_candidates.into_iter().take(3).collect();
        }

        if candidates.is_empty() {
            return None;
        }

        // Step 3: Score candidates
        for candidate in &mut candidates {
            candidate.election_score = calculate_election_score(candidate);
        }

        // Step 4: Elect highest scorer
        candidates
            .into_iter()
            .max_by(|a, b| {
                a.election_score
                    .partial_cmp(&b.election_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|c| c.node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockMetrics {
        uptimes: HashMap<NodeId, f64>,
        bandwidths: HashMap<NodeId, u64>,
        reputations: HashMap<NodeId, f64>,
        connections: HashMap<NodeId, u32>,
    }

    impl NodeMetrics for MockMetrics {
        fn get_uptime(&self, node: &NodeId) -> f64 {
            *self.uptimes.get(node).unwrap_or(&0.0)
        }

        fn get_bandwidth(&self, node: &NodeId) -> u64 {
            *self.bandwidths.get(node).unwrap_or(&0)
        }

        fn get_reputation(&self, node: &NodeId) -> f64 {
            *self.reputations.get(node).unwrap_or(&0.0)
        }

        fn get_connection_count(&self, node: &NodeId) -> u32 {
            *self.connections.get(node).unwrap_or(&0)
        }
    }

    #[test]
    fn test_normalize_bandwidth() {
        assert!((normalize_bandwidth(50_000_000) - 0.5).abs() < 0.001);
        assert!((normalize_bandwidth(100_000_000) - 1.0).abs() < 0.001);
        assert!((normalize_bandwidth(200_000_000) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_normalize_connectivity() {
        let optimal = (MIN_LEAVES_PER_NEXUS + MAX_LEAVES_PER_NEXUS) / 2;
        assert!((normalize_connectivity(optimal) - 1.0).abs() < 0.1);
        assert!(normalize_connectivity(0) < normalize_connectivity(optimal));
        assert!(normalize_connectivity(100) < normalize_connectivity(optimal));
    }

    #[test]
    fn test_nexus_eligibility() {
        assert!(is_nexus_eligible(0.96, 15_000_000, 0.75));
        assert!(!is_nexus_eligible(0.90, 15_000_000, 0.75)); // Low uptime
        assert!(!is_nexus_eligible(0.96, 5_000_000, 0.75)); // Low bandwidth
        assert!(!is_nexus_eligible(0.96, 15_000_000, 0.60)); // Low reputation
    }

    #[test]
    fn test_election() {
        let node1 = NodeId::from_bytes([1u8; 32]);
        let node2 = NodeId::from_bytes([2u8; 32]);
        let node3 = NodeId::from_bytes([3u8; 32]);

        let mut metrics = MockMetrics {
            uptimes: HashMap::new(),
            bandwidths: HashMap::new(),
            reputations: HashMap::new(),
            connections: HashMap::new(),
        };

        // node1: high quality
        metrics.uptimes.insert(node1, 0.99);
        metrics.bandwidths.insert(node1, 50_000_000);
        metrics.reputations.insert(node1, 0.9);
        metrics.connections.insert(node1, 25);

        // node2: medium quality
        metrics.uptimes.insert(node2, 0.96);
        metrics.bandwidths.insert(node2, 20_000_000);
        metrics.reputations.insert(node2, 0.75);
        metrics.connections.insert(node2, 10);

        // node3: low quality
        metrics.uptimes.insert(node3, 0.85);
        metrics.bandwidths.insert(node3, 5_000_000);
        metrics.reputations.insert(node3, 0.5);
        metrics.connections.insert(node3, 5);

        let elector = NexusElector::new(metrics);
        let region = Region::with_nodes("test", vec![node1, node2, node3]);

        let winner = elector.elect(&region);
        assert_eq!(winner, Some(node1)); // node1 should win with highest score
    }
}

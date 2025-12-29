//! Nexus Topology Management
//!
//! Implements gossip routing from dol/nexus.dol lines 58-157

use super::types::*;
use crate::core::NodeId;

/// Estimate entropy for direct node-to-node path.
/// From dol/nexus.dol lines 77-87
pub fn estimate_direct_entropy(hops: u32, latency_ms: f64) -> f64 {
    hops as f64 * 0.5 + latency_ms * 0.01
}

/// Estimate entropy for path through hub.
/// From dol/nexus.dol lines 89-99
pub fn estimate_hub_entropy(
    from_to_hub_hops: u32,
    from_to_hub_latency: f64,
    hub_to_target_hops: u32,
    hub_to_target_latency: f64,
) -> f64 {
    let entropy_to_hub = estimate_direct_entropy(from_to_hub_hops, from_to_hub_latency);
    let entropy_from_hub = estimate_direct_entropy(hub_to_target_hops, hub_to_target_latency);
    let hub_processing = 0.1; // Fixed hub processing cost

    entropy_to_hub + hub_processing + entropy_from_hub
}

/// Determine optimal gossip path based on node roles and entropy budget.
/// From dol/nexus.dol lines 101-157
pub fn determine_gossip_path(
    from: NodeId,
    to: NodeId,
    from_role: &NexusRole,
    entropy_budget: f64,
    estimate_entropy: impl Fn(NodeId, NodeId) -> f64,
    get_nearest_poteau_mitan: impl Fn() -> NodeId,
) -> GossipPath {
    match from_role.role_type {
        NexusRoleType::Leaf => {
            // Leaves always route through their nexus
            let nexus = from_role.parent.expect("Leaf must have parent nexus");
            let entropy = estimate_entropy(from, nexus) + 0.1 + estimate_entropy(nexus, to);
            GossipPath {
                path_type: GossipPathType::ViaHub,
                hops: vec![nexus, to],
                estimated_entropy: entropy,
            }
        }
        NexusRoleType::Nexus => {
            let direct_entropy = estimate_entropy(from, to);

            if direct_entropy <= entropy_budget {
                // Direct routing within budget
                GossipPath::direct(to, direct_entropy)
            } else {
                // Route through super-nexus
                let super_nexus = from_role.parent.unwrap_or_else(get_nearest_poteau_mitan);
                let entropy =
                    estimate_entropy(from, super_nexus) + 0.1 + estimate_entropy(super_nexus, to);
                GossipPath {
                    path_type: GossipPathType::ViaSuperHub,
                    hops: vec![super_nexus, to],
                    estimated_entropy: entropy,
                }
            }
        }
        NexusRoleType::PoteauMitan => {
            // PoteauMitan always uses direct routing
            let direct_entropy = estimate_entropy(from, to);
            GossipPath::direct(to, direct_entropy)
        }
    }
}

/// Topology manager for tracking node roles and routing
#[derive(Debug, Default)]
pub struct TopologyManager {
    topologies: std::collections::HashMap<NodeId, NexusTopology>,
}

impl TopologyManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_role(&self, node: &NodeId) -> NexusRole {
        self.topologies
            .get(node)
            .map(|t| t.role.clone())
            .unwrap_or_default()
    }

    pub fn set_topology(&mut self, node: NodeId, topology: NexusTopology) {
        self.topologies.insert(node, topology);
    }

    pub fn get_topology(&self, node: &NodeId) -> Option<&NexusTopology> {
        self.topologies.get(node)
    }

    pub fn update_gradient(&mut self, node: &NodeId, gradient: ResourceGradient) {
        if let Some(topo) = self.topologies.get_mut(node) {
            topo.aggregated_gradient = gradient;
        }
    }

    pub fn get_nexuses(&self) -> Vec<NodeId> {
        self.topologies
            .iter()
            .filter(|(_, t)| t.role.is_nexus())
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn get_poteau_mitans(&self) -> Vec<NodeId> {
        self.topologies
            .iter()
            .filter(|(_, t)| t.role.is_poteau_mitan())
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn get_leaves_of(&self, nexus: &NodeId) -> Vec<NodeId> {
        self.topologies
            .iter()
            .filter(|(_, t)| t.role.parent.as_ref() == Some(nexus))
            .map(|(id, _)| *id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Timestamp;

    #[test]
    fn test_estimate_direct_entropy() {
        let entropy = estimate_direct_entropy(2, 10.0);
        // 2 * 0.5 + 10 * 0.01 = 1.0 + 0.1 = 1.1
        assert!((entropy - 1.1).abs() < 0.001);
    }

    #[test]
    fn test_estimate_hub_entropy() {
        let entropy = estimate_hub_entropy(1, 5.0, 1, 5.0);
        // to_hub: 1*0.5 + 5*0.01 = 0.55
        // from_hub: 0.55
        // processing: 0.1
        // total: 1.2
        assert!((entropy - 1.2).abs() < 0.001);
    }

    #[test]
    fn test_gossip_path_leaf() {
        let from = NodeId::from_bytes([1u8; 32]);
        let to = NodeId::from_bytes([2u8; 32]);
        let nexus = NodeId::from_bytes([3u8; 32]);

        let role = NexusRole::leaf(nexus);

        let path = determine_gossip_path(from, to, &role, 5.0, |_, _| 0.5, || nexus);

        assert_eq!(path.path_type, GossipPathType::ViaHub);
        assert_eq!(path.hops[0], nexus);
    }

    #[test]
    fn test_gossip_path_nexus_direct() {
        let from = NodeId::from_bytes([1u8; 32]);
        let to = NodeId::from_bytes([2u8; 32]);

        let role = NexusRole::nexus(None, vec![]);

        // Low entropy estimate, within budget
        let path = determine_gossip_path(from, to, &role, 5.0, |_, _| 1.0, || from);

        assert_eq!(path.path_type, GossipPathType::Direct);
        assert_eq!(path.hops[0], to);
    }

    #[test]
    fn test_gossip_path_nexus_via_superhub() {
        let from = NodeId::from_bytes([1u8; 32]);
        let to = NodeId::from_bytes([2u8; 32]);
        let poteau = NodeId::from_bytes([3u8; 32]);

        let role = NexusRole::nexus(None, vec![]);

        // High entropy estimate, exceeds budget
        let path = determine_gossip_path(from, to, &role, 5.0, |_, _| 10.0, || poteau);

        assert_eq!(path.path_type, GossipPathType::ViaSuperHub);
        assert_eq!(path.hops[0], poteau);
    }

    #[test]
    fn test_topology_manager() {
        let mut manager = TopologyManager::new();
        let node = NodeId::from_bytes([1u8; 32]);
        let nexus = NodeId::from_bytes([2u8; 32]);

        let topology = NexusTopology {
            node,
            role: NexusRole::leaf(nexus),
            aggregated_gradient: ResourceGradient::default(),
            leaf_count: 0,
            last_election: Timestamp::now(),
        };

        manager.set_topology(node, topology);

        let role = manager.get_role(&node);
        assert!(role.is_leaf());
        assert_eq!(role.parent, Some(nexus));
    }
}

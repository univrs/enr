//! Woronin Body (Isolation Mechanism)
//!
//! Implements Woronin body from dol/septal.dol lines 221-275

use crate::core::{NodeId, Timestamp};
use serde::{Deserialize, Serialize};

/// WoroninBody - from dol/septal.dol line 221
/// Isolation mechanism that plugs septal pores
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WoroninBody {
    pub node: NodeId,
    pub activated_at: Timestamp,
    pub reason: String,
    pub blocked_transactions: u64,
}

impl WoroninBody {
    pub fn new(node: NodeId, reason: impl Into<String>) -> Self {
        Self {
            node,
            activated_at: Timestamp::now(),
            reason: reason.into(),
            blocked_transactions: 0,
        }
    }

    /// Record a blocked transaction
    pub fn record_blocked(&mut self) {
        self.blocked_transactions += 1;
    }

    /// Duration since activation
    pub fn duration_active(&self) -> u64 {
        Timestamp::now().millis.saturating_sub(self.activated_at.millis)
    }
}

/// Woronin body manager
#[derive(Debug, Default)]
pub struct WoroninManager {
    active_bodies: std::collections::HashMap<NodeId, WoroninBody>,
}

impl WoroninManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Activate Woronin body to isolate a node
    /// From dol/septal.dol lines 235-256
    pub fn activate(&mut self, node: NodeId, reason: impl Into<String>) -> &WoroninBody {
        let body = WoroninBody::new(node, reason);
        self.active_bodies.insert(node, body);
        self.active_bodies.get(&node).unwrap()
    }

    /// Deactivate Woronin body, allowing traffic to resume
    /// From dol/septal.dol lines 258-275
    pub fn deactivate(&mut self, node: &NodeId) -> Option<WoroninBody> {
        self.active_bodies.remove(node)
    }

    /// Check if a node is isolated
    pub fn is_isolated(&self, node: &NodeId) -> bool {
        self.active_bodies.contains_key(node)
    }

    /// Get active Woronin body for a node
    pub fn get(&self, node: &NodeId) -> Option<&WoroninBody> {
        self.active_bodies.get(node)
    }

    /// Get mutable active Woronin body for a node
    pub fn get_mut(&mut self, node: &NodeId) -> Option<&mut WoroninBody> {
        self.active_bodies.get_mut(node)
    }

    /// Get all isolated nodes
    pub fn isolated_nodes(&self) -> Vec<NodeId> {
        self.active_bodies.keys().copied().collect()
    }

    /// Record a blocked transaction for a node
    pub fn record_blocked(&mut self, node: &NodeId) {
        if let Some(body) = self.active_bodies.get_mut(node) {
            body.record_blocked();
        }
    }

    /// Check if transaction should be blocked
    /// Returns true if either from or to node is isolated
    pub fn should_block(&self, from: &NodeId, to: &NodeId) -> bool {
        self.is_isolated(from) || self.is_isolated(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_woronin_body() {
        let node = NodeId::from_bytes([1u8; 32]);
        let mut body = WoroninBody::new(node, "test isolation");

        assert_eq!(body.blocked_transactions, 0);
        body.record_blocked();
        body.record_blocked();
        assert_eq!(body.blocked_transactions, 2);
    }

    #[test]
    fn test_woronin_manager() {
        let mut manager = WoroninManager::new();
        let node = NodeId::from_bytes([1u8; 32]);
        let other = NodeId::from_bytes([2u8; 32]);

        assert!(!manager.is_isolated(&node));

        manager.activate(node, "test");
        assert!(manager.is_isolated(&node));
        assert!(!manager.is_isolated(&other));

        // Check blocking
        assert!(manager.should_block(&node, &other));
        assert!(manager.should_block(&other, &node));
        assert!(!manager.should_block(&other, &NodeId::from_bytes([3u8; 32])));

        // Deactivate
        let body = manager.deactivate(&node);
        assert!(body.is_some());
        assert!(!manager.is_isolated(&node));
    }
}

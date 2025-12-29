//! Node Decomposition
//!
//! Implements decomposition from dol/revival.dol lines 119-209

use super::events::RevivalEvent;
use crate::core::{Credits, NodeId, Timestamp};
use serde::{Deserialize, Serialize};

/// DecompositionPhase - from dol/revival.dol line 57
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DecompositionPhase {
    /// Phase 1: Credits locked
    CreditsFrozen,
    /// Phase 2: Pending reservations returned
    ReservationsReleased,
    /// Phase 3: Orphaned state cleaned up
    StateReclaimed,
    /// Phase 4: Network topology adjusted
    TopologyUpdated,
    /// Phase 5: Decomposition finished
    Complete,
}

impl DecompositionPhase {
    pub fn next(&self) -> Option<DecompositionPhase> {
        match self {
            DecompositionPhase::CreditsFrozen => Some(DecompositionPhase::ReservationsReleased),
            DecompositionPhase::ReservationsReleased => Some(DecompositionPhase::StateReclaimed),
            DecompositionPhase::StateReclaimed => Some(DecompositionPhase::TopologyUpdated),
            DecompositionPhase::TopologyUpdated => Some(DecompositionPhase::Complete),
            DecompositionPhase::Complete => None,
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self, DecompositionPhase::Complete)
    }
}

/// DecompositionState - from dol/revival.dol line 67
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecompositionState {
    pub node: NodeId,
    pub phase: DecompositionPhase,
    pub frozen_credits: Credits,
    pub start_time: Timestamp,
    pub events_emitted: Vec<RevivalEvent>,
}

impl DecompositionState {
    pub fn new(node: NodeId, frozen_credits: Credits) -> Self {
        Self {
            node,
            phase: DecompositionPhase::CreditsFrozen,
            frozen_credits,
            start_time: Timestamp::now(),
            events_emitted: Vec::new(),
        }
    }

    pub fn advance(&mut self) -> bool {
        if let Some(next) = self.phase.next() {
            self.phase = next;
            true
        } else {
            false
        }
    }

    pub fn add_event(&mut self, event: RevivalEvent) {
        self.events_emitted.push(event);
    }
}

/// Decomposer for managing node decomposition
#[derive(Debug, Default)]
pub struct Decomposer {
    states: std::collections::HashMap<NodeId, DecompositionState>,
}

impl Decomposer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if node is being decomposed
    pub fn is_decomposing(&self, node: &NodeId) -> bool {
        self.states.contains_key(node)
    }

    /// Start decomposition for a failed node
    pub fn start_decomposition(&mut self, node: NodeId, frozen_credits: Credits) -> &DecompositionState {
        let state = DecompositionState::new(node, frozen_credits);
        self.states.insert(node, state);
        self.states.get(&node).unwrap()
    }

    /// Get decomposition state
    pub fn get_state(&self, node: &NodeId) -> Option<&DecompositionState> {
        self.states.get(node)
    }

    /// Get mutable decomposition state
    pub fn get_state_mut(&mut self, node: &NodeId) -> Option<&mut DecompositionState> {
        self.states.get_mut(node)
    }

    /// Complete decomposition and return events
    pub fn complete_decomposition(&mut self, node: &NodeId) -> Option<Vec<RevivalEvent>> {
        self.states.remove(node).map(|s| s.events_emitted)
    }

    /// Get all active decompositions
    pub fn active_decompositions(&self) -> Vec<NodeId> {
        self.states.keys().copied().collect()
    }
}

/// Decompose a failed node, returning revival events
/// From dol/revival.dol lines 119-209
pub fn decompose_failed_node<C: DecompositionContext>(
    context: &C,
    node: NodeId,
) -> Vec<RevivalEvent> {
    let mut events = Vec::new();

    // Step 1: Verify failure
    if !context.confirm_failure(&node) {
        return events; // False alarm
    }

    // Step 2: Freeze credits
    let frozen_credits = context.freeze_node_credits(&node);
    events.push(
        RevivalEvent::node_failure(node, frozen_credits).with_metadata("phase", "freeze"),
    );

    // Step 3: Release held reservations
    let reservations = context.get_held_reservations(&node);
    for (reservation_id, amount, _consumed) in reservations {
        events.push(RevivalEvent::reservation_expired(node, amount, reservation_id));
    }

    // Step 4: Reclaim stored state
    let stored_items = context.get_stored_items(&node);
    for (key, has_replicas) in stored_items {
        if !has_replicas {
            let storage_credits = context.estimate_storage_credits(&key);
            events.push(RevivalEvent::garbage_collected(node, storage_credits, key));
        }
    }

    // Step 5: Update topology (done by caller)

    // Step 6: Final event
    events.push(
        RevivalEvent::node_failure(node, frozen_credits).with_metadata("phase", "complete"),
    );

    events
}

/// Context required for decomposition operations
pub trait DecompositionContext {
    fn confirm_failure(&self, node: &NodeId) -> bool;
    fn freeze_node_credits(&self, node: &NodeId) -> Credits;
    fn get_held_reservations(&self, node: &NodeId) -> Vec<(u64, Credits, bool)>;
    fn get_stored_items(&self, node: &NodeId) -> Vec<(String, bool)>;
    fn estimate_storage_credits(&self, key: &str) -> Credits;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decomposition_phase_progression() {
        let mut phase = DecompositionPhase::CreditsFrozen;

        assert_eq!(phase.next(), Some(DecompositionPhase::ReservationsReleased));
        phase = phase.next().unwrap();

        assert_eq!(phase.next(), Some(DecompositionPhase::StateReclaimed));
        phase = phase.next().unwrap();

        assert_eq!(phase.next(), Some(DecompositionPhase::TopologyUpdated));
        phase = phase.next().unwrap();

        assert_eq!(phase.next(), Some(DecompositionPhase::Complete));
        phase = phase.next().unwrap();

        assert!(phase.is_complete());
        assert_eq!(phase.next(), None);
    }

    #[test]
    fn test_decomposition_state() {
        let node = NodeId::from_bytes([1u8; 32]);
        let mut state = DecompositionState::new(node, Credits::new(1000));

        assert_eq!(state.phase, DecompositionPhase::CreditsFrozen);
        assert!(state.advance());
        assert_eq!(state.phase, DecompositionPhase::ReservationsReleased);
    }

    #[test]
    fn test_decomposer() {
        let mut decomposer = Decomposer::new();
        let node = NodeId::from_bytes([1u8; 32]);

        assert!(!decomposer.is_decomposing(&node));

        decomposer.start_decomposition(node, Credits::new(500));
        assert!(decomposer.is_decomposing(&node));

        let events = decomposer.complete_decomposition(&node);
        assert!(events.is_some());
        assert!(!decomposer.is_decomposing(&node));
    }
}

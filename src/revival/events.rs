//! Revival Events
//!
//! Implements revival event types from dol/core.dol and dol/revival.dol

use crate::core::{Credits, NodeId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// RevivalEventType - from dol/core.dol line 360
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RevivalEventType {
    /// Node became unreachable
    NodeFailure,
    /// Reservation timed out
    ReservationExpired,
    /// Orphaned state cleaned up
    GarbageCollected,
    /// Tax deducted from transaction
    EntropyTax,
    /// Node isolated by circuit breaker
    SeptalIsolation,
    /// Node gracefully left network
    VoluntaryExit,
}

/// RevivalEvent - from dol/core.dol line 371
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevivalEvent {
    pub event_type: RevivalEventType,
    pub source: NodeId,
    pub credits: Credits,
    pub timestamp: Timestamp,
    pub metadata: HashMap<String, String>,
}

impl RevivalEvent {
    pub fn node_failure(node: NodeId, credits: Credits) -> Self {
        Self {
            event_type: RevivalEventType::NodeFailure,
            source: node,
            credits,
            timestamp: Timestamp::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn reservation_expired(node: NodeId, credits: Credits, reservation_id: u64) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("reservation_id".to_string(), reservation_id.to_string());
        Self {
            event_type: RevivalEventType::ReservationExpired,
            source: node,
            credits,
            timestamp: Timestamp::now(),
            metadata,
        }
    }

    pub fn garbage_collected(node: NodeId, credits: Credits, key: impl Into<String>) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), key.into());
        Self {
            event_type: RevivalEventType::GarbageCollected,
            source: node,
            credits,
            timestamp: Timestamp::now(),
            metadata,
        }
    }

    pub fn entropy_tax(node: NodeId, tax: Credits) -> Self {
        Self {
            event_type: RevivalEventType::EntropyTax,
            source: node,
            credits: tax,
            timestamp: Timestamp::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn septal_isolation(node: NodeId, credits: Credits, reason: impl Into<String>) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("reason".to_string(), reason.into());
        Self {
            event_type: RevivalEventType::SeptalIsolation,
            source: node,
            credits,
            timestamp: Timestamp::now(),
            metadata,
        }
    }

    pub fn voluntary_exit(node: NodeId, credits: Credits) -> Self {
        Self {
            event_type: RevivalEventType::VoluntaryExit,
            source: node,
            credits,
            timestamp: Timestamp::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_revival_event_construction() {
        let node = NodeId::from_bytes([1u8; 32]);
        let credits = Credits::new(100);

        let event = RevivalEvent::node_failure(node, credits);
        assert_eq!(event.event_type, RevivalEventType::NodeFailure);
        assert_eq!(event.credits.amount, 100);

        let event = RevivalEvent::entropy_tax(node, Credits::new(5));
        assert_eq!(event.event_type, RevivalEventType::EntropyTax);
        assert_eq!(event.credits.amount, 5);
    }

    #[test]
    fn test_revival_event_with_metadata() {
        let node = NodeId::from_bytes([1u8; 32]);
        let event = RevivalEvent::garbage_collected(node, Credits::new(50), "test_key")
            .with_metadata("extra", "value");

        assert_eq!(event.metadata.get("key"), Some(&"test_key".to_string()));
        assert_eq!(event.metadata.get("extra"), Some(&"value".to_string()));
    }
}

//! ENR Gossipsub Message Types
//!
//! Message types for the ENR gossipsub protocol.

use serde::{Deserialize, Serialize};

use crate::core::{NodeId, Timestamp};
use crate::nexus::ResourceGradient;
use crate::septal::SeptalGateState;

use super::error::BridgeError;
use super::topics::EnrTopics;

// ============================================================================
// Common Types
// ============================================================================

/// Cryptographic signature (placeholder - integrate with ed25519-dalek)
/// Uses Vec<u8> for serde compatibility with large arrays
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature(pub Vec<u8>);

impl Signature {
    /// Create an empty signature (for unsigned messages)
    pub fn empty() -> Self {
        Self(vec![0u8; 64])
    }

    /// Create a signature from bytes
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes.to_vec())
    }

    /// Get signature bytes (returns None if wrong size)
    pub fn as_bytes(&self) -> Option<[u8; 64]> {
        if self.0.len() == 64 {
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&self.0);
            Some(arr)
        } else {
            None
        }
    }

    /// Check if signature is empty
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }
}

impl Default for Signature {
    fn default() -> Self {
        Self::empty()
    }
}

/// Transfer identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransferId(pub [u8; 32]);

impl TransferId {
    /// Create a transfer ID from transfer details
    pub fn from_transfer(from: &NodeId, to: &NodeId, amount: u64, nonce: u64) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        from.hash(&mut hasher);
        to.hash(&mut hasher);
        amount.hash(&mut hasher);
        nonce.hash(&mut hasher);

        let hash = hasher.finish();
        let mut id = [0u8; 32];
        id[..8].copy_from_slice(&hash.to_le_bytes());
        // Add more entropy from nonce
        id[8..16].copy_from_slice(&nonce.to_le_bytes());
        Self(id)
    }
}

// ============================================================================
// Gradient Messages
// ============================================================================

/// Gradient payload optimized for network transmission
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GradientPayload {
    pub cpu_available: f64,
    pub memory_available: f64,
    pub gpu_available: f64,
    pub storage_available: f64,
    pub bandwidth_available: f64,
    pub credit_balance: f64,
}

impl From<&ResourceGradient> for GradientPayload {
    fn from(g: &ResourceGradient) -> Self {
        Self {
            cpu_available: g.cpu_available,
            memory_available: g.memory_available,
            gpu_available: g.gpu_available,
            storage_available: g.storage_available,
            bandwidth_available: g.bandwidth_available,
            credit_balance: g.credit_balance,
        }
    }
}

impl GradientPayload {
    /// Convert back to ResourceGradient
    pub fn to_resource_gradient(&self) -> ResourceGradient {
        ResourceGradient {
            cpu_available: self.cpu_available,
            memory_available: self.memory_available,
            gpu_available: self.gpu_available,
            storage_available: self.storage_available,
            bandwidth_available: self.bandwidth_available,
            credit_balance: self.credit_balance,
        }
    }
}

/// Gradient broadcast message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientMessage {
    pub node_id: NodeId,
    pub gradient: GradientPayload,
    pub timestamp: Timestamp,
    pub signature: Signature,
}

// ============================================================================
// Election Messages
// ============================================================================

/// Election announcement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElectionAnnouncement {
    pub election_id: [u8; 32],
    pub initiator: NodeId,
    pub timestamp: Timestamp,
    pub round: u32,
}

/// Election candidacy declaration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElectionCandidacy {
    pub election_id: [u8; 32],
    pub candidate: NodeId,
    pub uptime: f64,
    pub bandwidth: u64,
    pub reputation: f64,
    pub current_leaf_count: u32,
    pub timestamp: Timestamp,
    pub signature: Signature,
}

/// Election vote
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElectionVote {
    pub election_id: [u8; 32],
    pub voter: NodeId,
    pub candidate: NodeId,
    pub timestamp: Timestamp,
    pub signature: Signature,
}

/// Election result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElectionResult {
    pub election_id: [u8; 32],
    pub winner: NodeId,
    pub vote_count: u32,
    pub timestamp: Timestamp,
}

/// Election message variants
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ElectionMessage {
    Announcement(ElectionAnnouncement),
    Candidacy(ElectionCandidacy),
    Vote(ElectionVote),
    Result(ElectionResult),
}

// ============================================================================
// Credit Messages
// ============================================================================

/// Credit transfer message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreditTransfer {
    pub id: TransferId,
    pub from: NodeId,
    pub to: NodeId,
    pub amount: u64,
    pub nonce: u64,
    pub timestamp: Timestamp,
    pub memo: Option<String>,
    pub signature: Signature,
}

/// Transfer confirmation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferConfirmation {
    pub transfer_id: TransferId,
    pub confirmer: NodeId,
    pub timestamp: Timestamp,
    pub signature: Signature,
}

/// Credit state sync message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreditStateSync {
    pub node_id: NodeId,
    pub balance: u64,
    pub version: u64,
    pub timestamp: Timestamp,
    pub signature: Signature,
}

/// Credit message variants
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CreditMessage {
    Transfer(CreditTransfer),
    Confirmation(TransferConfirmation),
    StateSync(CreditStateSync),
    BalanceQuery {
        requester: NodeId,
        target: NodeId,
    },
    BalanceResponse {
        node_id: NodeId,
        balance: u64,
    },
}

// ============================================================================
// Septal Messages
// ============================================================================

/// Failure report message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailureReport {
    pub reporter: NodeId,
    pub failed_node: NodeId,
    pub failure_type: String,
    pub timestamp: Timestamp,
    pub signature: Signature,
}

/// Isolation notice
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IsolationNotice {
    pub isolated_node: NodeId,
    pub reason: String,
    pub timestamp: Timestamp,
}

/// Healing probe message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealingProbe {
    pub probe_id: [u8; 32],
    pub initiator: NodeId,
    pub target: NodeId,
    pub timestamp: Timestamp,
}

/// Healing response message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealingResponse {
    pub probe_id: [u8; 32],
    pub responder: NodeId,
    pub state: SeptalGateState,
    pub timestamp: Timestamp,
}

/// Recovery notice
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveryNotice {
    pub recovered_node: NodeId,
    pub timestamp: Timestamp,
}

/// Septal message variants
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SeptalMessage {
    FailureReport(FailureReport),
    Isolation(IsolationNotice),
    HealingProbe(HealingProbe),
    HealingResponse(HealingResponse),
    Recovery(RecoveryNotice),
}

// ============================================================================
// Unified Message Type
// ============================================================================

/// Unified ENR message for gossipsub
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EnrMessage {
    Gradient(GradientMessage),
    Election(ElectionMessage),
    Credit(CreditMessage),
    Septal(SeptalMessage),
}

impl EnrMessage {
    /// Get the topic for this message
    pub fn topic(&self) -> &'static str {
        match self {
            EnrMessage::Gradient(_) => EnrTopics::GRADIENT,
            EnrMessage::Election(_) => EnrTopics::ELECTION,
            EnrMessage::Credit(_) => EnrTopics::CREDIT,
            EnrMessage::Septal(_) => EnrTopics::SEPTAL,
        }
    }

    /// Serialize message to bytes using bincode
    pub fn to_bytes(&self) -> Result<Vec<u8>, BridgeError> {
        bincode::serialize(self).map_err(|e| BridgeError::Serialization(e.to_string()))
    }

    /// Deserialize message from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, BridgeError> {
        bincode::deserialize(data).map_err(|e| BridgeError::Deserialization(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_empty() {
        let sig = Signature::empty();
        assert!(sig.is_empty());

        let sig2 = Signature::from_bytes([1u8; 64]);
        assert!(!sig2.is_empty());
    }

    #[test]
    fn test_transfer_id() {
        let from = NodeId::from_bytes([1u8; 32]);
        let to = NodeId::from_bytes([2u8; 32]);

        let id1 = TransferId::from_transfer(&from, &to, 100, 12345);
        let id2 = TransferId::from_transfer(&from, &to, 100, 12345);
        let id3 = TransferId::from_transfer(&from, &to, 100, 12346);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_gradient_payload_conversion() {
        let gradient = ResourceGradient {
            cpu_available: 0.5,
            memory_available: 0.6,
            gpu_available: 0.7,
            storage_available: 0.8,
            bandwidth_available: 0.9,
            credit_balance: 100.0,
        };

        let payload = GradientPayload::from(&gradient);
        let back = payload.to_resource_gradient();

        assert!((back.cpu_available - 0.5).abs() < 0.001);
        assert!((back.memory_available - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_message_topic() {
        let node = NodeId::from_bytes([1u8; 32]);

        let gradient_msg = EnrMessage::Gradient(GradientMessage {
            node_id: node,
            gradient: GradientPayload::from(&ResourceGradient::default()),
            timestamp: Timestamp::now(),
            signature: Signature::empty(),
        });
        assert_eq!(gradient_msg.topic(), "/enr/gradient/1.0");

        let election_msg = EnrMessage::Election(ElectionMessage::Announcement(
            ElectionAnnouncement {
                election_id: [0u8; 32],
                initiator: node,
                timestamp: Timestamp::now(),
                round: 1,
            },
        ));
        assert_eq!(election_msg.topic(), "/enr/election/1.0");
    }

    #[test]
    fn test_message_serialization() {
        let node = NodeId::from_bytes([1u8; 32]);
        let msg = EnrMessage::Gradient(GradientMessage {
            node_id: node,
            gradient: GradientPayload::from(&ResourceGradient::default()),
            timestamp: Timestamp::now(),
            signature: Signature::empty(),
        });

        let bytes = msg.to_bytes().unwrap();
        let decoded = EnrMessage::from_bytes(&bytes).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_credit_transfer_serialization() {
        let from = NodeId::from_bytes([1u8; 32]);
        let to = NodeId::from_bytes([2u8; 32]);

        let transfer = CreditTransfer {
            id: TransferId::from_transfer(&from, &to, 100, 12345),
            from,
            to,
            amount: 100,
            nonce: 12345,
            timestamp: Timestamp::now(),
            memo: Some("test transfer".to_string()),
            signature: Signature::empty(),
        };

        let msg = EnrMessage::Credit(CreditMessage::Transfer(transfer.clone()));
        let bytes = msg.to_bytes().unwrap();
        let decoded = EnrMessage::from_bytes(&bytes).unwrap();

        if let EnrMessage::Credit(CreditMessage::Transfer(t)) = decoded {
            assert_eq!(t.amount, 100);
            assert_eq!(t.memo, Some("test transfer".to_string()));
        } else {
            panic!("Expected CreditMessage::Transfer");
        }
    }
}

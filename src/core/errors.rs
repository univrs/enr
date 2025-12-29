//! ENR Error Types

use crate::core::{Credits, NodeId, ReservationId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EnrError {
    #[error("Insufficient credits: required {required}, available {available}")]
    InsufficientCredits { required: Credits, available: Credits },

    #[error("Reservation not found: {0:?}")]
    ReservationNotFound(ReservationId),

    #[error("Reservation expired: {0:?}")]
    ReservationExpired(ReservationId),

    #[error("Reservation already consumed: {0:?}")]
    ReservationAlreadyConsumed(ReservationId),

    #[error("Node not found: {0}")]
    NodeNotFound(NodeId),

    #[error("Node isolated by septal gate: {0}")]
    NodeIsolated(NodeId),

    #[error("Invalid credit state transition")]
    InvalidStateTransition,

    #[error("Entropy calculation error: {0}")]
    EntropyError(String),

    #[error("Nexus election failed: {0}")]
    NexusElectionFailed(String),

    #[error("Revival pool exhausted")]
    RevivalPoolExhausted,

    #[error("Credit conservation violated: expected {expected}, actual {actual}")]
    ConservationViolation { expected: Credits, actual: Credits },

    #[error("Invalid entropy weights: must sum to 1.0")]
    InvalidEntropyWeights,

    #[error("Account not found")]
    AccountNotFound,

    #[error("Transfer failed: {0}")]
    TransferFailed(String),
}

/// Result type for ENR operations
pub type EnrResult<T> = Result<T, EnrError>;

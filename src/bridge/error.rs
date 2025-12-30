//! Bridge Error Types
//!
//! Error types for the ENR gossipsub bridge.

use thiserror::Error;

/// Bridge errors
#[derive(Debug, Error)]
pub enum BridgeError {
    /// Publisher not connected
    #[error("Publisher not connected - call connect_publisher() first")]
    NotConnected,

    /// Network error during publish
    #[error("Network error: {0}")]
    Network(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Message expired (too old)
    #[error("Message expired")]
    MessageExpired,

    /// Invalid message format
    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    /// Unknown topic
    #[error("Unknown topic: {0}")]
    UnknownTopic(String),

    /// Node is isolated
    #[error("Node is isolated")]
    NodeIsolated,
}

/// Credit transfer errors
#[derive(Debug, Error)]
pub enum TransferError {
    /// Zero amount transfer
    #[error("Cannot transfer zero credits")]
    ZeroAmount,

    /// Self transfer
    #[error("Cannot transfer credits to self")]
    SelfTransfer,

    /// Insufficient balance
    #[error("Insufficient balance for transfer")]
    InsufficientBalance,

    /// Transfer cancelled
    #[error("Transfer was cancelled")]
    Cancelled,

    /// Transfer timed out
    #[error("Transfer timed out")]
    Timeout,

    /// Invalid signature
    #[error("Invalid transfer signature")]
    InvalidSignature,

    /// Duplicate transfer
    #[error("Duplicate transfer ID")]
    Duplicate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = BridgeError::NotConnected;
        assert!(err.to_string().contains("not connected"));

        let err = TransferError::InsufficientBalance;
        assert!(err.to_string().contains("Insufficient"));
    }
}

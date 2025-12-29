//! ENR Core Types
//!
//! Rust implementation of genes from dol/core.dol:
//! - NodeId (line 24)
//! - AccountId (line 37)
//! - Credits (line 95)
//! - CreditTransfer (line 125)
//! - CreditReservation (line 144)

use serde::{Deserialize, Serialize};

/// NodeId - from dol/core.dol line 24
/// Ed25519 public key based identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub [u8; 32]);

impl NodeId {
    /// Constraint: valid_format requires 64 hex chars (32 bytes)
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// AccountType - from dol/core.dol line 46
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccountType {
    Node,
    RevivalPool,
    Treasury,
}

/// AccountId - from dol/core.dol line 37
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId {
    pub node: NodeId,
    pub account_type: AccountType,
}

impl AccountId {
    pub fn new(node: NodeId, account_type: AccountType) -> Self {
        Self { node, account_type }
    }

    pub fn node_account(node: NodeId) -> Self {
        Self {
            node,
            account_type: AccountType::Node,
        }
    }
}

/// Timestamp - from dol/core.dol line 54
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp {
    pub millis: u64,
}

impl Timestamp {
    pub fn new(millis: u64) -> Self {
        Self { millis }
    }

    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self { millis }
    }
}

/// Duration - from dol/core.dol line 67
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Duration {
    pub millis: u64,
}

impl Duration {
    pub fn new(millis: u64) -> Self {
        Self { millis }
    }

    pub fn seconds(s: u64) -> Self {
        Self { millis: s * 1000 }
    }

    pub fn minutes(m: u64) -> Self {
        Self { millis: m * 60 * 1000 }
    }

    pub fn hours(h: u64) -> Self {
        Self { millis: h * 60 * 60 * 1000 }
    }

    pub fn days(d: u64) -> Self {
        Self { millis: d * 24 * 60 * 60 * 1000 }
    }
}

/// Credits - from dol/core.dol line 95
/// Fundamental unit of value, conserved across transfers
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub struct Credits {
    pub amount: u64,
}

impl Credits {
    pub const ZERO: Self = Self { amount: 0 };

    /// Constraint: non_negative from dol/core.dol line 104
    pub fn new(amount: u64) -> Self {
        Self { amount }
    }

    /// provides zero() from dol/core.dol line 108
    pub fn zero() -> Self {
        Self::ZERO
    }

    /// provides add() from dol/core.dol line 112
    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.amount.checked_add(other.amount).map(|a| Self { amount: a })
    }

    /// provides sub() from dol/core.dol line 116
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.amount.checked_sub(other.amount).map(|a| Self { amount: a })
    }

    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            amount: self.amount.saturating_add(other.amount),
        }
    }

    pub fn saturating_sub(self, other: Self) -> Self {
        Self {
            amount: self.amount.saturating_sub(other.amount),
        }
    }

    pub fn is_zero(&self) -> bool {
        self.amount == 0
    }
}

impl std::ops::Add for Credits {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            amount: self.amount + other.amount,
        }
    }
}

impl std::ops::Sub for Credits {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self {
            amount: self.amount - other.amount,
        }
    }
}

impl std::ops::AddAssign for Credits {
    fn add_assign(&mut self, other: Self) {
        self.amount += other.amount;
    }
}

impl std::ops::SubAssign for Credits {
    fn sub_assign(&mut self, other: Self) {
        self.amount -= other.amount;
    }
}

impl std::fmt::Display for Credits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} credits", self.amount)
    }
}

/// ReservationId - from dol/core.dol CreditReservation.id
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReservationId(pub u64);

impl ReservationId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// CreditTransfer - from dol/core.dol line 125
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditTransfer {
    pub from: AccountId,
    pub to: AccountId,
    pub amount: Credits,
    pub entropy_cost: Credits,
    pub timestamp: Timestamp,
}

impl CreditTransfer {
    pub fn new(from: AccountId, to: AccountId, amount: Credits, entropy_cost: Credits) -> Self {
        Self {
            from,
            to,
            amount,
            entropy_cost,
            timestamp: Timestamp::now(),
        }
    }

    /// Total debited from sender
    pub fn total_cost(&self) -> Credits {
        self.amount.saturating_add(self.entropy_cost)
    }
}

/// CreditReservation - from dol/core.dol line 144
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditReservation {
    pub id: ReservationId,
    pub account: AccountId,
    pub amount: Credits,
    pub created_at: Timestamp,
    pub ttl: Duration,
    pub consumed: bool,
}

impl CreditReservation {
    pub fn new(id: ReservationId, account: AccountId, amount: Credits, ttl: Duration) -> Self {
        Self {
            id,
            account,
            amount,
            created_at: Timestamp::now(),
            ttl,
            consumed: false,
        }
    }

    /// Constraint: valid_ttl from dol/core.dol line 157
    pub fn is_valid(&self) -> bool {
        self.ttl.millis > 0
    }

    pub fn is_expired(&self, now: Timestamp) -> bool {
        now.millis > self.created_at.millis + self.ttl.millis
    }

    pub fn consume(&mut self) {
        self.consumed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credits_arithmetic() {
        let a = Credits::new(100);
        let b = Credits::new(50);

        assert_eq!((a + b).amount, 150);
        assert_eq!((a - b).amount, 50);
        assert_eq!(a.checked_add(b), Some(Credits::new(150)));
        assert_eq!(b.checked_sub(a), None);
    }

    #[test]
    fn test_node_id_hex() {
        let bytes = [1u8; 32];
        let id = NodeId::from_bytes(bytes);
        let hex = id.to_hex();
        assert_eq!(hex.len(), 64);

        let parsed = NodeId::from_hex(&hex).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_duration_conversions() {
        assert_eq!(Duration::seconds(1).millis, 1000);
        assert_eq!(Duration::minutes(1).millis, 60_000);
        assert_eq!(Duration::hours(1).millis, 3_600_000);
        assert_eq!(Duration::days(1).millis, 86_400_000);
    }

    #[test]
    fn test_reservation_expiry() {
        let node = NodeId::from_bytes([0u8; 32]);
        let account = AccountId::node_account(node);
        let mut reservation = CreditReservation::new(
            ReservationId::new(1),
            account,
            Credits::new(100),
            Duration::seconds(10),
        );

        assert!(reservation.is_valid());
        assert!(!reservation.consumed);

        // Check expiry with future timestamp
        let future = Timestamp::new(reservation.created_at.millis + 20_000);
        assert!(reservation.is_expired(future));

        reservation.consume();
        assert!(reservation.consumed);
    }
}

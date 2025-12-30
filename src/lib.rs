//! univrs-enr: Entropy-Nexus-Revival Economic Primitive
//!
//! The foundational economic layer for VUDO OS.
//!
//! ## Overview
//!
//! ENR is inspired by biological mycelial networks:
//! - **Entropy**: Thermodynamic cost of disorder in transactions
//! - **Nexus**: Hub-based aggregation and market-making
//! - **Revival**: Resource cycling and decomposition
//!
//! ## Modules
//!
//! - [`core`]: Credits, NodeId, state machine, invariants
//! - [`entropy`]: Four entropy types and price multiplier
//! - [`nexus`]: Topology, election, market making
//! - [`revival`]: Decomposition and redistribution
//! - [`septal`]: Circuit breaker and Woronin bodies
//! - [`pricing`]: Fixed, Dynamic, Auction models
//! - [`testing`]: Chaos testing framework (requires `chaos-testing` feature)
//!
//! ## DOL Specifications
//!
//! Authoritative specifications in the `dol/` directory:
//! - `core.dol`: Credits, NodeId, invariants
//! - `entropy.dol`: 4 entropy types, formulas
//! - `nexus.dol`: Topology, market making
//! - `pricing.dol`: Fixed, Dynamic, Auction models
//! - `revival.dol`: Decomposition, redistribution
//! - `septal.dol`: Circuit breaker, Woronin bodies
//!
//! ## Example
//!
//! ```rust
//! use univrs_enr::core::{Credits, NodeId};
//! use univrs_enr::entropy::{EntropyCalculator, NetworkEntropyInput};
//!
//! // Create a node and credits
//! let node = NodeId::from_bytes([1u8; 32]);
//! let credits = Credits::new(1000);
//!
//! // Calculate entropy for a network operation
//! let calculator = EntropyCalculator::new();
//! let network_input = NetworkEntropyInput {
//!     hops: 3,
//!     latency_variance_ms: 10.0,
//!     packet_loss_probability: 0.01,
//!     bandwidth_saturation: 0.5,
//! };
//!
//! let account = calculator.calculate(Some(&network_input), None, None, None);
//! let multiplier = calculator.price_multiplier(&account);
//!
//! // Price is adjusted by entropy
//! let adjusted_price = Credits::new((credits.amount as f64 * multiplier) as u64);
//! ```

pub mod core;
pub mod entropy;
pub mod nexus;
pub mod pricing;
pub mod revival;
pub mod septal;

#[cfg(feature = "chaos-testing")]
pub mod testing;

/// Bridge module for gossipsub integration
#[cfg(feature = "bridge")]
pub mod bridge;

// Re-export core types for convenience
pub use core::{
    AccountId, AccountType, Credits, CreditReservation, CreditTransfer, Duration, EnrError,
    NodeId, ReservationId, Timestamp,
};

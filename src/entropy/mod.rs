//! ENR Entropy Module
//!
//! Implements entropy calculation from dol/entropy.dol
//!
//! Four entropy types:
//! - Network (Sₙ): Path characteristics (hops, latency, loss)
//! - Compute (Sᶜ): Resource usage (CPU, memory, context switches)
//! - Storage (Sˢ): Data characteristics (size, fragmentation, replication)
//! - Temporal (Sᵗ): Time-related factors (staleness, drift, ordering)
//!
//! Total entropy: S_total = wₙ·Sₙ + wᶜ·Sᶜ + wˢ·Sˢ + wᵗ·Sᵗ

pub mod calculator;
pub mod types;

pub use calculator::*;
pub use types::*;

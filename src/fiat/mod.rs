//! Credit→Fiat bridge layer (the StripeBridge spine)
//!
//! The credit→fiat crossing, gated by three independent gates over the
//! [`ReservationLedger`](crate::core::ReservationLedger):
//!
//! - **Gate 1 — Ledger** ([`crate::core::ReservationLedger`]): escrow +
//!   conservation. Reserve before spend; commit on success, release on failure.
//! - **Gate 2 — Policy** ([`SpendPolicyGate`]): per-transaction / velocity caps
//!   and merchant/category allow-lists. Distinct from the
//!   [`SeptalGate`](crate::septal::SeptalGate) circuit breaker.
//! - **Gate 3 — Rail** ([`SpendRail`]): the payment rail (Stripe Issuing +
//!   real-time authorization webhook), abstracted as a trait so the spine is
//!   testable without a Stripe account.
//!
//! [`StripeBridge`] orchestrates all three with an append-only audit trail.

pub mod bridge;
pub mod policy;

pub use bridge::{AuditEntry, AuditEvent, RailOutcome, SpendRail, SpendReceipt, StripeBridge};
pub use policy::{Gate, SpendDenial, SpendPolicyConfig, SpendPolicyGate, SpendRequest};

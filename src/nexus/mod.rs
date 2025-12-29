//! ENR Nexus Module
//!
//! Implements nexus topology from dol/nexus.dol
//!
//! Hierarchy:
//! - Leaf: Regular node, routes through nexus
//! - Nexus: Hub node, aggregates 5-50 leaves
//! - PoteauMitan: Super-nexus with global view

pub mod aggregation;
pub mod election;
pub mod market_maker;
pub mod topology;
pub mod types;

pub use aggregation::*;
pub use election::*;
pub use market_maker::*;
pub use topology::*;
pub use types::*;

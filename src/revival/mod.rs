//! ENR Revival Module
//!
//! Implements revival pool from dol/revival.dol
//!
//! The revival pool collects credits from:
//! - Failed nodes
//! - Expired reservations
//! - Garbage collection
//! - Entropy tax on all transactions
//!
//! And redistributes to:
//! - Network maintenance (40%)
//! - New node subsidy (25%)
//! - Low-balance support (20%)
//! - Reserve buffer (15%)

pub mod decomposer;
pub mod events;
pub mod pool;

pub use decomposer::*;
pub use events::*;
pub use pool::*;

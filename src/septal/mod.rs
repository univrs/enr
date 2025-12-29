//! ENR Septal Module (Circuit Breaker)
//!
//! Implements septal gate from dol/septal.dol
//!
//! Named after the septal pores in fungal hyphae that can be plugged
//! by Woronin bodies to isolate damage.
//!
//! State machine:
//! - Open: Normal operation, traffic flows freely
//! - HalfOpen: Testing recovery, limited traffic
//! - Closed: Isolated, no traffic allowed (Woronin active)

pub mod gate;
pub mod healing;
pub mod woronin;

pub use gate::*;
pub use healing::*;
pub use woronin::*;

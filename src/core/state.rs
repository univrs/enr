//! Credit State Machine
//!
//! State transitions: Active → Reserved → Consumed | Released
//! From ENR-ARCHITECTURE.md Section 4

use serde::{Deserialize, Serialize};

/// Credit state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreditState {
    /// Credits available for use
    Active,
    /// Credits reserved for pending operation
    Reserved,
    /// Credits consumed by completed operation
    Consumed,
    /// Credits released back to active pool
    Released,
    /// Credits in revival pool awaiting redistribution
    InRevival,
}

impl CreditState {
    /// Valid state transitions
    pub fn can_transition_to(&self, next: CreditState) -> bool {
        use CreditState::*;
        matches!(
            (self, next),
            (Active, Reserved)
                | (Reserved, Consumed)
                | (Reserved, Released)
                | (Released, Active)
                | (Consumed, InRevival)
                | (InRevival, Active)
        )
    }

    /// Attempt state transition
    pub fn transition(self, next: CreditState) -> Result<CreditState, StateError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(StateError::InvalidTransition { from: self, to: next })
        }
    }

    /// Check if state allows spending
    pub fn is_spendable(&self) -> bool {
        matches!(self, CreditState::Active)
    }

    /// Check if state is terminal
    pub fn is_terminal(&self) -> bool {
        matches!(self, CreditState::Consumed)
    }
}

impl Default for CreditState {
    fn default() -> Self {
        CreditState::Active
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum StateError {
    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidTransition { from: CreditState, to: CreditState },
}

/// Tracked credit with state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedCredits {
    pub amount: super::Credits,
    pub state: CreditState,
}

impl TrackedCredits {
    pub fn new(amount: super::Credits) -> Self {
        Self {
            amount,
            state: CreditState::Active,
        }
    }

    pub fn transition(&mut self, next: CreditState) -> Result<(), StateError> {
        self.state = self.state.transition(next)?;
        Ok(())
    }

    pub fn reserve(&mut self) -> Result<(), StateError> {
        self.transition(CreditState::Reserved)
    }

    pub fn consume(&mut self) -> Result<(), StateError> {
        self.transition(CreditState::Consumed)
    }

    pub fn release(&mut self) -> Result<(), StateError> {
        self.transition(CreditState::Released)?;
        self.transition(CreditState::Active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Credits;

    #[test]
    fn test_valid_transitions() {
        assert!(CreditState::Active.can_transition_to(CreditState::Reserved));
        assert!(CreditState::Reserved.can_transition_to(CreditState::Consumed));
        assert!(CreditState::Reserved.can_transition_to(CreditState::Released));
        assert!(CreditState::Released.can_transition_to(CreditState::Active));
        assert!(CreditState::Consumed.can_transition_to(CreditState::InRevival));
        assert!(CreditState::InRevival.can_transition_to(CreditState::Active));
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(!CreditState::Active.can_transition_to(CreditState::Consumed));
        assert!(!CreditState::Consumed.can_transition_to(CreditState::Active));
        assert!(!CreditState::Active.can_transition_to(CreditState::Active));
        assert!(!CreditState::Reserved.can_transition_to(CreditState::Active));
    }

    #[test]
    fn test_transition_success() {
        let result = CreditState::Active.transition(CreditState::Reserved);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), CreditState::Reserved);
    }

    #[test]
    fn test_transition_failure() {
        let result = CreditState::Active.transition(CreditState::Consumed);
        assert!(result.is_err());
    }

    #[test]
    fn test_tracked_credits() {
        let mut tracked = TrackedCredits::new(Credits::new(100));
        assert_eq!(tracked.state, CreditState::Active);

        tracked.reserve().unwrap();
        assert_eq!(tracked.state, CreditState::Reserved);

        tracked.consume().unwrap();
        assert_eq!(tracked.state, CreditState::Consumed);
    }

    #[test]
    fn test_tracked_credits_release() {
        let mut tracked = TrackedCredits::new(Credits::new(100));
        tracked.reserve().unwrap();
        tracked.release().unwrap();
        assert_eq!(tracked.state, CreditState::Active);
    }
}

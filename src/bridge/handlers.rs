//! ENR Message Handlers
//!
//! Handler traits for processing incoming gossipsub messages.

use async_trait::async_trait;

use crate::core::NodeId;

use super::error::BridgeError;
use super::messages::*;

/// Handler for gradient messages
#[async_trait]
pub trait GradientHandler: Send + Sync {
    /// Handle incoming gradient broadcast
    async fn handle_gradient(&self, msg: GradientMessage) -> Result<(), BridgeError>;
}

/// Handler for election messages
#[async_trait]
pub trait ElectionHandler: Send + Sync {
    /// Handle election announcement
    async fn handle_announcement(&self, ann: ElectionAnnouncement) -> Result<(), BridgeError>;

    /// Handle candidacy declaration
    async fn handle_candidacy(&self, candidate: ElectionCandidacy) -> Result<(), BridgeError>;

    /// Handle election vote
    async fn handle_vote(&self, vote: ElectionVote) -> Result<(), BridgeError>;

    /// Handle election result
    async fn handle_result(&self, result: ElectionResult) -> Result<(), BridgeError>;
}

/// Handler for credit messages
#[async_trait]
pub trait CreditHandler: Send + Sync {
    /// Handle credit transfer
    async fn handle_transfer(&self, transfer: CreditTransfer) -> Result<(), BridgeError>;

    /// Handle transfer confirmation
    async fn handle_confirmation(&self, confirmation: TransferConfirmation)
        -> Result<(), BridgeError>;

    /// Handle credit state sync
    async fn handle_state_sync(&self, sync: CreditStateSync) -> Result<(), BridgeError>;

    /// Handle balance query
    async fn handle_balance_query(
        &self,
        requester: NodeId,
        target: NodeId,
    ) -> Result<(), BridgeError>;

    /// Handle balance response
    async fn handle_balance_response(
        &self,
        node_id: NodeId,
        balance: u64,
    ) -> Result<(), BridgeError>;
}

/// Handler for septal messages
#[async_trait]
pub trait SeptalHandler: Send + Sync {
    /// Handle failure report
    async fn handle_failure_report(&self, report: FailureReport) -> Result<(), BridgeError>;

    /// Handle isolation notice
    async fn handle_isolation(&self, notice: IsolationNotice) -> Result<(), BridgeError>;

    /// Handle healing probe
    async fn handle_healing_probe(&self, probe: HealingProbe) -> Result<(), BridgeError>;

    /// Handle healing response
    async fn handle_healing_response(&self, response: HealingResponse) -> Result<(), BridgeError>;

    /// Handle recovery notice
    async fn handle_recovery(&self, notice: RecoveryNotice) -> Result<(), BridgeError>;
}

/// Combined handler for all ENR message types
#[async_trait]
pub trait EnrMessageHandler:
    GradientHandler + ElectionHandler + CreditHandler + SeptalHandler
{
    /// Handle any ENR message
    async fn handle_message(&self, msg: EnrMessage) -> Result<(), BridgeError> {
        match msg {
            EnrMessage::Gradient(m) => self.handle_gradient(m).await,
            EnrMessage::Election(m) => match m {
                ElectionMessage::Announcement(a) => self.handle_announcement(a).await,
                ElectionMessage::Candidacy(c) => self.handle_candidacy(c).await,
                ElectionMessage::Vote(v) => self.handle_vote(v).await,
                ElectionMessage::Result(r) => self.handle_result(r).await,
            },
            EnrMessage::Credit(m) => match m {
                CreditMessage::Transfer(t) => self.handle_transfer(t).await,
                CreditMessage::Confirmation(c) => self.handle_confirmation(c).await,
                CreditMessage::StateSync(s) => self.handle_state_sync(s).await,
                CreditMessage::BalanceQuery { requester, target } => {
                    self.handle_balance_query(requester, target).await
                }
                CreditMessage::BalanceResponse { node_id, balance } => {
                    self.handle_balance_response(node_id, balance).await
                }
            },
            EnrMessage::Septal(m) => match m {
                SeptalMessage::FailureReport(r) => self.handle_failure_report(r).await,
                SeptalMessage::Isolation(n) => self.handle_isolation(n).await,
                SeptalMessage::HealingProbe(p) => self.handle_healing_probe(p).await,
                SeptalMessage::HealingResponse(r) => self.handle_healing_response(r).await,
                SeptalMessage::Recovery(n) => self.handle_recovery(n).await,
            },
        }
    }
}

// Blanket implementation for types that implement all sub-handlers
impl<T> EnrMessageHandler for T where
    T: GradientHandler + ElectionHandler + CreditHandler + SeptalHandler
{
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test that traits are object-safe
    #[allow(dead_code)]
    fn assert_object_safe() {
        fn _gradient(_: &dyn GradientHandler) {}
        fn _election(_: &dyn ElectionHandler) {}
        fn _credit(_: &dyn CreditHandler) {}
        fn _septal(_: &dyn SeptalHandler) {}
    }
}

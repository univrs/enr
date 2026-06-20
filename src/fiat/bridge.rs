//! StripeBridge — the credit→fiat crossing (the spine)
//!
//! Orchestrates the three gates over the [`ReservationLedger`]:
//!
//! 1. **`reserve`** runs Gate 2 (policy) then Gate 1 (ledger escrow), moving
//!    credits `Active → Reserved` and returning a [`ReservationId`]. No rail call
//!    yet — the cheap local gates must pass before any money moves.
//! 2. **`execute`** calls the [`SpendRail`] (Gate 3) with the reservation id as
//!    the idempotency key. On approval it commits (`Reserved → Consumed`); on
//!    decline or expiry it releases (`Reserved → Active`).
//!
//! The rail is a trait so the whole spine is testable without a Stripe account.
//! The real Stripe Issuing client (with the `issuing_authorization.request`
//! webhook) is a thin `SpendRail` impl added when keys arrive.

use std::collections::HashMap;

use crate::core::{Credits, Duration, EnrError, ReservationId, ReservationLedger, Timestamp};

use super::policy::{Gate, SpendDenial, SpendPolicyGate, SpendRequest};

/// Outcome of a payment-rail authorization (Gate 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RailOutcome {
    Approved { txn_id: String },
    Declined { reason: String },
}

/// The payment rail (Gate 3). For Stripe this is backed by Issuing + the
/// real-time `issuing_authorization.request` webhook.
pub trait SpendRail {
    /// Authorize a spend. `idempotency_key` is the reservation id — the rail
    /// must dedupe on it so a retry never double-charges.
    fn authorize(&mut self, req: &SpendRequest, idempotency_key: &str) -> RailOutcome;
}

/// A settled spend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpendReceipt {
    pub reservation_id: ReservationId,
    pub amount: Credits,
    pub txn_id: String,
}

/// One append-only audit record. Every fiat movement traces back here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    Reserved,
    Denied { gate: Gate, reason: String },
    Committed { txn_id: String },
    Released { reason: String },
    Refunded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub reservation_id: Option<ReservationId>,
    pub amount: Credits,
    pub event: AuditEvent,
    pub at: Timestamp,
}

/// The credit→fiat bridge over a single account's [`ReservationLedger`].
pub struct StripeBridge<R: SpendRail> {
    ledger: ReservationLedger,
    policy: SpendPolicyGate,
    rail: R,
    ttl: Duration,
    pending: HashMap<ReservationId, SpendRequest>,
    audit: Vec<AuditEntry>,
}

impl<R: SpendRail> StripeBridge<R> {
    pub fn new(ledger: ReservationLedger, policy: SpendPolicyGate, rail: R, ttl: Duration) -> Self {
        Self {
            ledger,
            policy,
            rail,
            ttl,
            pending: HashMap::new(),
            audit: Vec::new(),
        }
    }

    /// Spendable balance (Active pool).
    pub fn balance(&self) -> Credits {
        self.ledger.balance()
    }

    /// Credits currently escrowed in open reservations.
    pub fn reserved(&self) -> Credits {
        self.ledger.reserved_total()
    }

    /// The append-only audit trail.
    pub fn audit(&self) -> &[AuditEntry] {
        &self.audit
    }

    fn log(&mut self, reservation_id: Option<ReservationId>, amount: Credits, event: AuditEvent, at: Timestamp) {
        self.audit.push(AuditEntry {
            reservation_id,
            amount,
            event,
            at,
        });
    }

    /// Reserve credits for a spend: Gate 2 (policy) then Gate 1 (ledger escrow).
    ///
    /// Returns a [`ReservationId`] to pass to [`execute`](Self::execute). No rail
    /// call happens here. On any gate refusal returns a typed [`SpendDenial`] and
    /// takes no escrow.
    pub fn reserve(
        &mut self,
        req: SpendRequest,
        now: Timestamp,
    ) -> Result<ReservationId, SpendDenial> {
        // Gate 2 — policy (pure check, no state change).
        if let Err(denial) = self.policy.evaluate(&req, now) {
            self.log(None, req.amount, AuditEvent::Denied {
                gate: denial.gate,
                reason: denial.reason.clone(),
            }, now);
            return Err(denial);
        }

        // Gate 1 — ledger escrow (Active → Reserved).
        let id = self.ledger.reserve(req.amount, self.ttl, now).map_err(|e| {
            let denial = ledger_denial(e);
            self.log(None, req.amount, AuditEvent::Denied {
                gate: denial.gate,
                reason: denial.reason.clone(),
            }, now);
            denial
        })?;

        self.log(Some(id), req.amount, AuditEvent::Reserved, now);
        self.pending.insert(id, req);
        Ok(id)
    }

    /// Execute a held reservation against the rail (Gate 3).
    ///
    /// On approval: commit (`Reserved → Consumed`), record the spend against the
    /// policy velocity window, and return a [`SpendReceipt`]. On decline or
    /// expiry: release the credits and return a typed [`SpendDenial`].
    pub fn execute(
        &mut self,
        id: ReservationId,
        now: Timestamp,
    ) -> Result<SpendReceipt, SpendDenial> {
        let req = self
            .pending
            .get(&id)
            .cloned()
            .ok_or_else(|| SpendDenial::ledger(format!("unknown reservation: {:?}", id)))?;

        // Reject expired reservations before touching the rail — reclaim the credits.
        if let Some(reservation) = self.ledger.get(id) {
            if reservation.is_expired(now) {
                let _ = self.ledger.release(id);
                self.pending.remove(&id);
                let denial = SpendDenial::ledger("reservation expired");
                self.log(Some(id), req.amount, AuditEvent::Released {
                    reason: "expired".into(),
                }, now);
                return Err(denial);
            }
        }

        // Gate 3 — the rail. Reservation id is the idempotency key.
        let key = format!("resv-{}", id.0);
        match self.rail.authorize(&req, &key) {
            RailOutcome::Approved { txn_id } => {
                // Commit the escrow now that the charge is confirmed.
                let amount = self.ledger.commit(id, now).map_err(|e| {
                    // Should not happen (we checked expiry), but fail closed.
                    SpendDenial::ledger(format!("commit failed: {}", e))
                })?;
                self.policy.record_spend(amount, now);
                self.pending.remove(&id);
                self.log(Some(id), amount, AuditEvent::Committed {
                    txn_id: txn_id.clone(),
                }, now);
                Ok(SpendReceipt {
                    reservation_id: id,
                    amount,
                    txn_id,
                })
            }
            RailOutcome::Declined { reason } => {
                let _ = self.ledger.release(id);
                self.pending.remove(&id);
                self.log(Some(id), req.amount, AuditEvent::Released {
                    reason: reason.clone(),
                }, now);
                Err(SpendDenial::rail(reason))
            }
        }
    }

    /// Refund a settled spend: mint the credits back so conservation holds across
    /// the round trip (Consumed → Active, via re-funding the ledger).
    pub fn refund(&mut self, amount: Credits, now: Timestamp) {
        self.ledger.fund(amount);
        self.log(None, amount, AuditEvent::Refunded, now);
    }
}

/// Map a ledger error to a typed Gate-1 denial.
fn ledger_denial(e: EnrError) -> SpendDenial {
    match e {
        EnrError::InsufficientCredits { required, available } => SpendDenial::ledger(format!(
            "insufficient credits: required {}, available {}",
            required, available
        )),
        EnrError::ZeroAmount => SpendDenial::ledger("zero-amount spend"),
        other => SpendDenial::ledger(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{AccountId, NodeId};
    use crate::fiat::policy::SpendPolicyConfig;

    /// A scripted rail for tests: returns a fixed outcome and records the key.
    struct MockRail {
        outcome: RailOutcome,
        last_key: Option<String>,
    }

    impl MockRail {
        fn approve() -> Self {
            Self {
                outcome: RailOutcome::Approved { txn_id: "txn_test_1".into() },
                last_key: None,
            }
        }
        fn decline() -> Self {
            Self {
                outcome: RailOutcome::Declined { reason: "out of envelope".into() },
                last_key: None,
            }
        }
    }

    impl SpendRail for MockRail {
        fn authorize(&mut self, _req: &SpendRequest, idempotency_key: &str) -> RailOutcome {
            self.last_key = Some(idempotency_key.to_string());
            self.outcome.clone()
        }
    }

    fn account() -> AccountId {
        AccountId::node_account(NodeId::from_bytes([9u8; 32]))
    }

    fn bridge_with(balance: u64, config: SpendPolicyConfig, rail: MockRail) -> StripeBridge<MockRail> {
        let ledger = ReservationLedger::with_balance(account(), Credits::new(balance));
        let policy = SpendPolicyGate::new(config);
        StripeBridge::new(ledger, policy, rail, Duration::seconds(60))
    }

    fn req(amount: u64) -> SpendRequest {
        SpendRequest::new(Credits::new(amount), "buy api key", "api", "openweather")
    }

    fn now() -> Timestamp {
        Timestamp::new(1_000_000)
    }

    #[test]
    fn happy_path_reserve_then_commit() {
        let mut b = bridge_with(1000, SpendPolicyConfig::default(), MockRail::approve());

        let id = b.reserve(req(200), now()).unwrap();
        assert_eq!(b.balance(), Credits::new(800)); // escrowed
        assert_eq!(b.reserved(), Credits::new(200));

        let receipt = b.execute(id, now()).unwrap();
        assert_eq!(receipt.amount, Credits::new(200));
        assert_eq!(receipt.txn_id, "txn_test_1");
        assert_eq!(b.balance(), Credits::new(800)); // committed, not returned
        assert_eq!(b.reserved(), Credits::ZERO);

        // Audit: Reserved then Committed.
        let events: Vec<_> = b.audit().iter().map(|e| &e.event).collect();
        assert!(matches!(events[0], AuditEvent::Reserved));
        assert!(matches!(events[1], AuditEvent::Committed { .. }));
    }

    #[test]
    fn reservation_id_is_idempotency_key() {
        let mut b = bridge_with(1000, SpendPolicyConfig::default(), MockRail::approve());
        let id = b.reserve(req(100), now()).unwrap();
        b.execute(id, now()).unwrap();
        // Rail saw the reservation id as the key.
        assert_eq!(b.rail.last_key.as_deref(), Some(format!("resv-{}", id.0).as_str()));
    }

    #[test]
    fn rail_decline_releases_credits() {
        let mut b = bridge_with(1000, SpendPolicyConfig::default(), MockRail::decline());
        let id = b.reserve(req(300), now()).unwrap();
        assert_eq!(b.balance(), Credits::new(700));

        let d = b.execute(id, now()).unwrap_err();
        assert_eq!(d.gate, Gate::Rail);
        assert!(!d.retryable);
        assert_eq!(b.balance(), Credits::new(1000)); // released back
        assert_eq!(b.reserved(), Credits::ZERO);

        let last = &b.audit().last().unwrap().event;
        assert!(matches!(last, AuditEvent::Released { .. }));
    }

    #[test]
    fn policy_denial_takes_no_escrow() {
        let config = SpendPolicyConfig {
            per_transaction_cap: Credits::new(100),
            ..Default::default()
        };
        let mut b = bridge_with(1000, config, MockRail::approve());
        let d = b.reserve(req(500), now()).unwrap_err();
        assert_eq!(d.gate, Gate::Policy);
        assert_eq!(b.balance(), Credits::new(1000)); // untouched
        assert_eq!(b.reserved(), Credits::ZERO);
    }

    #[test]
    fn expired_reservation_releases_before_rail() {
        let mut b = bridge_with(1000, SpendPolicyConfig::default(), MockRail::approve());
        let id = b.reserve(req(200), now()).unwrap();

        // Execute well past the 60s ttl.
        let later = Timestamp::new(now().millis + 120_000);
        let d = b.execute(id, later).unwrap_err();
        assert_eq!(d.gate, Gate::Ledger);
        assert_eq!(b.balance(), Credits::new(1000)); // reclaimed
    }

    #[test]
    fn refund_recredits() {
        let mut b = bridge_with(1000, SpendPolicyConfig::default(), MockRail::approve());
        let id = b.reserve(req(200), now()).unwrap();
        b.execute(id, now()).unwrap();
        assert_eq!(b.balance(), Credits::new(800));

        b.refund(Credits::new(200), now());
        assert_eq!(b.balance(), Credits::new(1000)); // minted back
        assert!(matches!(b.audit().last().unwrap().event, AuditEvent::Refunded));
    }

    /// The demo's triple-denial beat, as a negative oracle: each gate refuses in
    /// turn, and the refusal is attributed to a *different* system.
    #[test]
    fn triple_denial_each_gate_fails_clean() {
        // Gate 1 (Ledger): reserve more than the balance.
        let mut b1 = bridge_with(50, SpendPolicyConfig::default(), MockRail::approve());
        let d1 = b1.reserve(req(100), now()).unwrap_err();
        assert_eq!(d1.gate, Gate::Ledger);
        assert_eq!(b1.balance(), Credits::new(50)); // nothing moved

        // Gate 2 (Policy): within balance, but over the per-transaction cap.
        let config = SpendPolicyConfig {
            per_transaction_cap: Credits::new(100),
            ..Default::default()
        };
        let mut b2 = bridge_with(1000, config, MockRail::approve());
        let d2 = b2.reserve(req(500), now()).unwrap_err();
        assert_eq!(d2.gate, Gate::Policy);
        assert_eq!(b2.balance(), Credits::new(1000)); // nothing moved

        // Gate 3 (Rail): passes 1 & 2, declined at the rail; credits released.
        let mut b3 = bridge_with(1000, SpendPolicyConfig::default(), MockRail::decline());
        let id = b3.reserve(req(200), now()).unwrap();
        let d3 = b3.execute(id, now()).unwrap_err();
        assert_eq!(d3.gate, Gate::Rail);
        assert_eq!(b3.balance(), Credits::new(1000)); // released, not spent

        // Three different systems, three clean refusals.
        assert_ne!(d1.gate, d2.gate);
        assert_ne!(d2.gate, d3.gate);
        assert!(!d1.retryable && !d2.retryable && !d3.retryable);
    }
}

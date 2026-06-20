//! Credit Reservation Ledger
//!
//! Balance-level escrow built on the `CreditState` lifecycle
//! (`Active → Reserved → Consumed | Released`, see `src/core/state.rs`).
//!
//! This is the foundation the ENR↔Stripe bridge sits on. An authorization is a
//! **reservation**: `reserve()` moves credits `Active → Reserved` and returns a
//! [`ReservationId`]. Only a held reservation can then be `commit()`ed
//! (`Reserved → Consumed`) or `release()`d (`Reserved → Active`).
//!
//! ## Why escrow, not check-then-spend
//!
//! A check-then-spend pattern (read balance, then debit) has a time-of-check /
//! time-of-use race: two concurrent conversions can both observe the same
//! balance and both proceed, double-spending. Here the credits leave the
//! spendable pool at *reserve* time, so a second reservation against the same
//! funds fails immediately with [`EnrError::InsufficientCredits`]. Conservation
//! is preserved by construction, not by a downstream check.
//!
//! ## Conservation invariant (holds after every operation)
//!
//! ```text
//! spendable + reserved + consumed == funded
//! ```
//!
//! - `funded`    — total credits ever deposited (earned) into this ledger
//! - `spendable` — the Active pool, available to reserve
//! - `reserved`  — sum of all currently-held reservations
//! - `consumed`  — credits terminally spent via `commit()`
//!
//! `release()` returns reserved credits to `spendable`; `commit()` moves them to
//! `consumed`. Either way the sum is unchanged.

use std::collections::HashMap;

use crate::core::state::CreditState;
use crate::core::{
    AccountId, CreditReservation, Credits, Duration, EnrError, EnrResult, ReservationId, Timestamp,
};

/// One tracked reservation plus its lifecycle state.
#[derive(Debug, Clone)]
struct LedgerEntry {
    reservation: CreditReservation,
    /// Lifecycle state; always `Reserved` while held (terminal entries are removed).
    state: CreditState,
}

/// Balance-level escrow ledger for a single account.
///
/// Wraps the `CreditState` state machine with pooled accounting so that an
/// authorization (`reserve`) and its settlement (`commit`/`release`) are
/// distinct, race-free steps.
#[derive(Debug, Clone)]
pub struct ReservationLedger {
    account: AccountId,
    /// Active pool — credits available to reserve.
    spendable: Credits,
    /// Terminally spent credits (committed).
    consumed: Credits,
    /// Total ever funded; the conservation anchor.
    funded: Credits,
    /// Monotonic reservation id counter.
    next_id: u64,
    /// Currently-held reservations, keyed by id.
    reservations: HashMap<ReservationId, LedgerEntry>,
}

impl ReservationLedger {
    /// Create an empty ledger for `account`.
    pub fn new(account: AccountId) -> Self {
        Self {
            account,
            spendable: Credits::ZERO,
            consumed: Credits::ZERO,
            funded: Credits::ZERO,
            next_id: 1,
            reservations: HashMap::new(),
        }
    }

    /// Create a ledger pre-funded with `amount` spendable credits.
    pub fn with_balance(account: AccountId, amount: Credits) -> Self {
        let mut ledger = Self::new(account);
        ledger.fund(amount);
        ledger
    }

    /// Deposit earned credits into the spendable pool.
    ///
    /// Models the EARN side: verified work mints credits that become available
    /// to reserve. Increments both `spendable` and `funded`, keeping the
    /// conservation invariant intact.
    pub fn fund(&mut self, amount: Credits) {
        self.spendable = self.spendable.saturating_add(amount);
        self.funded = self.funded.saturating_add(amount);
    }

    /// Spendable (Active) balance — what can still be reserved.
    pub fn balance(&self) -> Credits {
        self.spendable
    }

    /// Sum of all currently-held reservations.
    pub fn reserved_total(&self) -> Credits {
        self.reservations
            .values()
            .fold(Credits::ZERO, |acc, e| acc.saturating_add(e.reservation.amount))
    }

    /// Credits terminally committed.
    pub fn consumed_total(&self) -> Credits {
        self.consumed
    }

    /// Total ever funded.
    pub fn funded_total(&self) -> Credits {
        self.funded
    }

    /// Number of reservations currently held.
    pub fn active_count(&self) -> usize {
        self.reservations.len()
    }

    /// Inspect a held reservation.
    pub fn get(&self, id: ReservationId) -> Option<&CreditReservation> {
        self.reservations.get(&id).map(|e| &e.reservation)
    }

    /// Reserve `amount` for up to `ttl`, escrowing it out of the spendable pool.
    ///
    /// Moves credits `Active → Reserved` and returns a [`ReservationId`] that
    /// `commit`/`release` consume. No external side effects — this is the cheap
    /// local gate that must pass *before* any payment rail is touched.
    ///
    /// `now` stamps the reservation's `created_at`, so expiry is driven by the
    /// caller's clock (deterministic — no hidden wall-clock read).
    ///
    /// # Errors
    /// - [`EnrError::ZeroAmount`] if `amount` is zero.
    /// - [`EnrError::InsufficientCredits`] if `spendable < amount`.
    pub fn reserve(
        &mut self,
        amount: Credits,
        ttl: Duration,
        now: Timestamp,
    ) -> EnrResult<ReservationId> {
        if amount.is_zero() {
            return Err(EnrError::ZeroAmount);
        }
        if self.spendable < amount {
            return Err(EnrError::InsufficientCredits {
                required: amount,
                available: self.spendable,
            });
        }

        // Validate the lifecycle edge via the real state machine.
        let state = CreditState::Active
            .transition(CreditState::Reserved)
            .map_err(|_| EnrError::InvalidStateTransition)?;

        let id = ReservationId::new(self.next_id);
        self.next_id += 1;

        let mut reservation = CreditReservation::new(id, self.account.clone(), amount, ttl);
        reservation.created_at = now;
        self.spendable = self.spendable.saturating_sub(amount);
        self.reservations.insert(id, LedgerEntry { reservation, state });

        Ok(id)
    }

    /// Commit a held reservation: `Reserved → Consumed`.
    ///
    /// Call this only after the downstream spend (e.g. the Stripe charge) has
    /// confirmed. Returns the committed amount. An expired reservation cannot be
    /// committed — its credits are released back to spendable and
    /// [`EnrError::ReservationExpired`] is returned.
    ///
    /// # Errors
    /// - [`EnrError::ReservationNotFound`] if `id` is unknown (or already settled).
    /// - [`EnrError::ReservationExpired`] if the reservation's ttl has elapsed.
    pub fn commit(&mut self, id: ReservationId, now: Timestamp) -> EnrResult<Credits> {
        let entry = self
            .reservations
            .get(&id)
            .ok_or(EnrError::ReservationNotFound(id))?;

        if entry.reservation.is_expired(now) {
            // Reclaim rather than strand the credits.
            let amount = entry.reservation.amount;
            self.spendable = self.spendable.saturating_add(amount);
            self.reservations.remove(&id);
            return Err(EnrError::ReservationExpired(id));
        }

        let amount = entry.reservation.amount;
        // Reserved → Consumed, validated from the reservation's own state.
        entry
            .state
            .transition(CreditState::Consumed)
            .map_err(|_| EnrError::InvalidStateTransition)?;

        self.consumed = self.consumed.saturating_add(amount);
        self.reservations.remove(&id);
        Ok(amount)
    }

    /// Debit `amount` directly from spendable, bypassing the reserve step.
    ///
    /// For outflows that need no escrow (e.g. an immediate, already-authorized
    /// transfer). Moves credits straight `Active → Consumed`. Prefer
    /// `reserve`/`commit` whenever the settlement can fail or race.
    ///
    /// # Errors
    /// - [`EnrError::ZeroAmount`] if `amount` is zero.
    /// - [`EnrError::InsufficientCredits`] if `spendable < amount`.
    pub fn spend(&mut self, amount: Credits) -> EnrResult<Credits> {
        if amount.is_zero() {
            return Err(EnrError::ZeroAmount);
        }
        if self.spendable < amount {
            return Err(EnrError::InsufficientCredits {
                required: amount,
                available: self.spendable,
            });
        }
        self.spendable = self.spendable.saturating_sub(amount);
        self.consumed = self.consumed.saturating_add(amount);
        Ok(amount)
    }

    /// Release a held reservation: `Reserved → Active`.
    ///
    /// Returns the credits to the spendable pool (e.g. on a failed/timed-out
    /// spend). Returns the released amount.
    ///
    /// # Errors
    /// - [`EnrError::ReservationNotFound`] if `id` is unknown (or already settled).
    pub fn release(&mut self, id: ReservationId) -> EnrResult<Credits> {
        let entry = self
            .reservations
            .get(&id)
            .ok_or(EnrError::ReservationNotFound(id))?;

        let amount = entry.reservation.amount;
        // Reserved → Released → Active, validated from the reservation's own
        // state (mirrors TrackedCredits::release).
        entry
            .state
            .transition(CreditState::Released)
            .and_then(|s| s.transition(CreditState::Active))
            .map_err(|_| EnrError::InvalidStateTransition)?;

        self.spendable = self.spendable.saturating_add(amount);
        self.reservations.remove(&id);
        Ok(amount)
    }

    /// Release every reservation whose ttl has elapsed as of `now`.
    ///
    /// Returns the ids reclaimed. Lets a sweeper recover credits stranded by
    /// crashed or abandoned spends.
    pub fn release_expired(&mut self, now: Timestamp) -> Vec<ReservationId> {
        let expired: Vec<ReservationId> = self
            .reservations
            .iter()
            .filter(|(_, e)| e.reservation.is_expired(now))
            .map(|(id, _)| *id)
            .collect();

        for id in &expired {
            // Safe: ids came straight from the map.
            let _ = self.release(*id);
        }
        expired
    }

    /// True iff `spendable + reserved + consumed == funded`.
    pub fn conservation_holds(&self) -> bool {
        let actual = self
            .spendable
            .saturating_add(self.reserved_total())
            .saturating_add(self.consumed);
        actual == self.funded
    }

    /// Assert the conservation invariant, surfacing a typed violation.
    pub fn assert_conservation(&self) -> EnrResult<()> {
        if self.conservation_holds() {
            Ok(())
        } else {
            let actual = self
                .spendable
                .saturating_add(self.reserved_total())
                .saturating_add(self.consumed);
            Err(EnrError::ConservationViolation {
                expected: self.funded,
                actual,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{AccountId, NodeId};

    fn ledger(balance: u64) -> ReservationLedger {
        let account = AccountId::node_account(NodeId::from_bytes([7u8; 32]));
        ReservationLedger::with_balance(account, Credits::new(balance))
    }

    fn ttl() -> Duration {
        Duration::seconds(60)
    }

    fn t0() -> Timestamp {
        Timestamp::new(1_000_000)
    }

    #[test]
    fn reserve_decrements_spendable_and_conserves() {
        let mut l = ledger(100);
        assert!(l.conservation_holds());

        let id = l.reserve(Credits::new(40), ttl(), t0()).unwrap();
        assert_eq!(l.balance(), Credits::new(60));
        assert_eq!(l.reserved_total(), Credits::new(40));
        assert!(l.get(id).is_some());
        assert!(l.conservation_holds());
    }

    #[test]
    fn reserve_rejects_zero() {
        let mut l = ledger(100);
        assert!(matches!(
            l.reserve(Credits::ZERO, ttl(), t0()),
            Err(EnrError::ZeroAmount)
        ));
    }

    #[test]
    fn reserve_rejects_insufficient_balance() {
        let mut l = ledger(50);
        let r = l.reserve(Credits::new(80), ttl(), t0());
        assert!(matches!(
            r,
            Err(EnrError::InsufficientCredits { required, available })
                if required == Credits::new(80) && available == Credits::new(50)
        ));
        // Balance untouched on failure.
        assert_eq!(l.balance(), Credits::new(50));
        assert!(l.conservation_holds());
    }

    #[test]
    fn no_double_spend_toctou() {
        // The core guarantee: two reservations cannot both take from the same funds.
        let mut l = ledger(100);
        let _first = l.reserve(Credits::new(60), ttl(), t0()).unwrap();
        let second = l.reserve(Credits::new(60), ttl(), t0());
        assert!(matches!(second, Err(EnrError::InsufficientCredits { .. })));
        assert_eq!(l.reserved_total(), Credits::new(60));
        assert!(l.conservation_holds());
    }

    #[test]
    fn commit_moves_reserved_to_consumed() {
        let mut l = ledger(100);
        let id = l.reserve(Credits::new(40), ttl(), t0()).unwrap();
        let amount = l.commit(id, t0()).unwrap();

        assert_eq!(amount, Credits::new(40));
        assert_eq!(l.balance(), Credits::new(60));
        assert_eq!(l.reserved_total(), Credits::ZERO);
        assert_eq!(l.consumed_total(), Credits::new(40));
        assert!(l.get(id).is_none());
        assert!(l.conservation_holds());
    }

    #[test]
    fn release_restores_spendable_round_trip() {
        let mut l = ledger(100);
        let before = l.balance();
        let id = l.reserve(Credits::new(40), ttl(), t0()).unwrap();
        let amount = l.release(id).unwrap();

        assert_eq!(amount, Credits::new(40));
        assert_eq!(l.balance(), before); // exact round-trip
        assert_eq!(l.reserved_total(), Credits::ZERO);
        assert_eq!(l.consumed_total(), Credits::ZERO);
        assert!(l.conservation_holds());
    }

    #[test]
    fn double_commit_fails() {
        let mut l = ledger(100);
        let id = l.reserve(Credits::new(40), ttl(), t0()).unwrap();
        l.commit(id, t0()).unwrap();
        assert!(matches!(
            l.commit(id, t0()),
            Err(EnrError::ReservationNotFound(_))
        ));
    }

    #[test]
    fn double_release_fails() {
        let mut l = ledger(100);
        let id = l.reserve(Credits::new(40), ttl(), t0()).unwrap();
        l.release(id).unwrap();
        assert!(matches!(
            l.release(id),
            Err(EnrError::ReservationNotFound(_))
        ));
    }

    #[test]
    fn commit_after_release_fails() {
        let mut l = ledger(100);
        let id = l.reserve(Credits::new(40), ttl(), t0()).unwrap();
        l.release(id).unwrap();
        assert!(matches!(
            l.commit(id, t0()),
            Err(EnrError::ReservationNotFound(_))
        ));
    }

    #[test]
    fn expired_reservation_cannot_commit_and_is_reclaimed() {
        let mut l = ledger(100);
        let id = l.reserve(Credits::new(40), Duration::seconds(10), t0()).unwrap();

        // Commit well after the ttl.
        let later = Timestamp::new(t0().millis + 20_000);
        assert!(matches!(
            l.commit(id, later),
            Err(EnrError::ReservationExpired(_))
        ));

        // Credits reclaimed, not stranded.
        assert_eq!(l.balance(), Credits::new(100));
        assert_eq!(l.reserved_total(), Credits::ZERO);
        assert!(l.conservation_holds());
    }

    #[test]
    fn release_expired_reclaims_stranded_reservations() {
        let mut l = ledger(100);
        let a = l.reserve(Credits::new(30), Duration::seconds(10), t0()).unwrap();
        let _b = l.reserve(Credits::new(20), Duration::hours(1), t0()).unwrap();

        let now = Timestamp::new(t0().millis + 20_000); // a expired, b still valid

        let reclaimed = l.release_expired(now);
        assert_eq!(reclaimed, vec![a]);
        assert_eq!(l.balance(), Credits::new(80)); // 50 spendable + 30 reclaimed
        assert_eq!(l.reserved_total(), Credits::new(20)); // b still held
        assert!(l.conservation_holds());
    }

    #[test]
    fn spend_debits_directly_and_conserves() {
        let mut l = ledger(100);
        let amount = l.spend(Credits::new(30)).unwrap();
        assert_eq!(amount, Credits::new(30));
        assert_eq!(l.balance(), Credits::new(70));
        assert_eq!(l.consumed_total(), Credits::new(30));
        assert_eq!(l.reserved_total(), Credits::ZERO);
        assert!(l.conservation_holds());
    }

    #[test]
    fn spend_rejects_insufficient_and_zero() {
        let mut l = ledger(20);
        assert!(matches!(
            l.spend(Credits::new(50)),
            Err(EnrError::InsufficientCredits { .. })
        ));
        assert!(matches!(l.spend(Credits::ZERO), Err(EnrError::ZeroAmount)));
        assert_eq!(l.balance(), Credits::new(20)); // untouched
    }

    #[test]
    fn conservation_holds_across_mixed_sequence() {
        let mut l = ledger(1000);
        let a = l.reserve(Credits::new(100), ttl(), t0()).unwrap();
        let b = l.reserve(Credits::new(250), ttl(), t0()).unwrap();
        let c = l.reserve(Credits::new(50), ttl(), t0()).unwrap();

        l.commit(a, t0()).unwrap();
        l.release(b).unwrap();
        l.fund(Credits::new(500));
        let _d = l.reserve(Credits::new(75), ttl(), t0()).unwrap();
        l.commit(c, t0()).unwrap();

        // funded = 1500, consumed = 150, reserved = 75, spendable = 1275
        assert_eq!(l.funded_total(), Credits::new(1500));
        assert_eq!(l.consumed_total(), Credits::new(150));
        assert_eq!(l.reserved_total(), Credits::new(75));
        assert_eq!(l.balance(), Credits::new(1275));
        assert!(l.conservation_holds());
        assert!(l.assert_conservation().is_ok());
    }
}

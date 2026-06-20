//! Spend Policy Gate (Gate 2)
//!
//! The explicit, rate-limited policy that governs credit→fiat conversion. This
//! is **not** the [`SeptalGate`](crate::septal::SeptalGate) circuit breaker —
//! that is a node-health isolator (`isolated node ⇒ no credit flow`). This gate
//! governs the *envelope* of spend: per-transaction caps, a velocity cap over a
//! rolling window, and merchant/category allow-lists.
//!
//! All denials are typed ([`SpendDenial`]) and carry the gate that refused, so
//! the demo's triple-denial beat can show *which* system said no.

use serde::{Deserialize, Serialize};

use crate::core::{Credits, Duration, Timestamp};

/// Which gate refused a spend. Mirrors the three-gate model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gate {
    /// Gate 1 — ledger / conservation (insufficient or unverified credits).
    Ledger,
    /// Gate 2 — spend policy (cap / velocity / merchant / category).
    Policy,
    /// Gate 3 — payment rail (Stripe Issuing authorization).
    Rail,
}

/// A typed refusal. `retryable` is always `false` here — a denial is a clean
/// stop, never an auto-retry (see the skill's Safety Rules).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpendDenial {
    pub gate: Gate,
    pub reason: String,
    pub retryable: bool,
}

impl SpendDenial {
    pub fn new(gate: Gate, reason: impl Into<String>) -> Self {
        Self {
            gate,
            reason: reason.into(),
            retryable: false,
        }
    }

    pub fn ledger(reason: impl Into<String>) -> Self {
        Self::new(Gate::Ledger, reason)
    }

    pub fn policy(reason: impl Into<String>) -> Self {
        Self::new(Gate::Policy, reason)
    }

    pub fn rail(reason: impl Into<String>) -> Self {
        Self::new(Gate::Rail, reason)
    }
}

/// A request to convert credits to fiat spend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpendRequest {
    pub amount: Credits,
    pub purpose: String,
    pub category: String,
    pub merchant: String,
}

impl SpendRequest {
    pub fn new(
        amount: Credits,
        purpose: impl Into<String>,
        category: impl Into<String>,
        merchant: impl Into<String>,
    ) -> Self {
        Self {
            amount,
            purpose: purpose.into(),
            category: category.into(),
            merchant: merchant.into(),
        }
    }
}

/// Gate 2 policy configuration.
///
/// A zero cap means "unlimited"; an empty allow-list means "allow all". This
/// makes a default-constructed config permissive — callers tighten it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpendPolicyConfig {
    /// Max credits per single conversion (0 = unlimited).
    pub per_transaction_cap: Credits,
    /// Max credits converted within `window` (0 = unlimited).
    pub window_cap: Credits,
    /// Rolling velocity window (e.g. 24h).
    pub window: Duration,
    /// Allowed merchant categories (empty = allow all).
    pub allowed_categories: Vec<String>,
    /// Allowed merchants (empty = allow all).
    pub allowed_merchants: Vec<String>,
}

impl Default for SpendPolicyConfig {
    fn default() -> Self {
        Self {
            per_transaction_cap: Credits::ZERO,
            window_cap: Credits::ZERO,
            window: Duration::hours(24),
            allowed_categories: Vec::new(),
            allowed_merchants: Vec::new(),
        }
    }
}

/// The spend-policy gate (Gate 2), with velocity accounting.
#[derive(Debug, Clone)]
pub struct SpendPolicyGate {
    config: SpendPolicyConfig,
    /// Committed conversions within the window: (when, amount).
    history: Vec<(Timestamp, Credits)>,
}

impl SpendPolicyGate {
    pub fn new(config: SpendPolicyConfig) -> Self {
        Self {
            config,
            history: Vec::new(),
        }
    }

    /// Credits converted within the rolling window ending at `now`.
    pub fn spent_in_window(&self, now: Timestamp) -> Credits {
        let cutoff = now.millis.saturating_sub(self.config.window.millis);
        self.history
            .iter()
            .filter(|(t, _)| t.millis >= cutoff)
            .fold(Credits::ZERO, |acc, (_, a)| acc.saturating_add(*a))
    }

    /// Evaluate a request against the policy. `Ok(())` means *authorized so far*
    /// — the caller still escrows (Gate 1) and clears the rail (Gate 3).
    pub fn evaluate(&self, req: &SpendRequest, now: Timestamp) -> Result<(), SpendDenial> {
        if req.amount.is_zero() {
            return Err(SpendDenial::policy("zero-amount spend"));
        }

        // Per-transaction cap.
        if !self.config.per_transaction_cap.is_zero()
            && req.amount > self.config.per_transaction_cap
        {
            return Err(SpendDenial::policy(format!(
                "per-transaction cap exceeded: {} > {}",
                req.amount, self.config.per_transaction_cap
            )));
        }

        // Category allow-list.
        if !self.config.allowed_categories.is_empty()
            && !self.config.allowed_categories.contains(&req.category)
        {
            return Err(SpendDenial::policy(format!(
                "category not allowed: {}",
                req.category
            )));
        }

        // Merchant allow-list.
        if !self.config.allowed_merchants.is_empty()
            && !self.config.allowed_merchants.contains(&req.merchant)
        {
            return Err(SpendDenial::policy(format!(
                "merchant not allowed: {}",
                req.merchant
            )));
        }

        // Velocity cap over the rolling window.
        if !self.config.window_cap.is_zero() {
            let projected = self.spent_in_window(now).saturating_add(req.amount);
            if projected > self.config.window_cap {
                return Err(SpendDenial::policy(format!(
                    "window cap exceeded: {} over window > {}",
                    projected, self.config.window_cap
                )));
            }
        }

        Ok(())
    }

    /// Record a committed conversion against the velocity window. Call this only
    /// after the spend actually settles (rail approved + ledger committed).
    pub fn record_spend(&mut self, amount: Credits, now: Timestamp) {
        self.history.push((now, amount));
        self.prune(now);
    }

    /// Drop history entries older than the window.
    fn prune(&mut self, now: Timestamp) {
        let cutoff = now.millis.saturating_sub(self.config.window.millis);
        self.history.retain(|(t, _)| t.millis >= cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(amount: u64, category: &str, merchant: &str) -> SpendRequest {
        SpendRequest::new(Credits::new(amount), "test", category, merchant)
    }

    fn now() -> Timestamp {
        Timestamp::new(1_000_000)
    }

    #[test]
    fn permissive_by_default() {
        let gate = SpendPolicyGate::new(SpendPolicyConfig::default());
        assert!(gate.evaluate(&req(1_000_000, "anything", "anyone"), now()).is_ok());
    }

    #[test]
    fn per_transaction_cap_denies_over() {
        let gate = SpendPolicyGate::new(SpendPolicyConfig {
            per_transaction_cap: Credits::new(100),
            ..Default::default()
        });
        assert!(gate.evaluate(&req(100, "x", "y"), now()).is_ok());
        let d = gate.evaluate(&req(101, "x", "y"), now()).unwrap_err();
        assert_eq!(d.gate, Gate::Policy);
        assert!(!d.retryable);
    }

    #[test]
    fn category_allowlist() {
        let gate = SpendPolicyGate::new(SpendPolicyConfig {
            allowed_categories: vec!["compute".into(), "api".into()],
            ..Default::default()
        });
        assert!(gate.evaluate(&req(10, "compute", "y"), now()).is_ok());
        let d = gate.evaluate(&req(10, "gambling", "y"), now()).unwrap_err();
        assert_eq!(d.gate, Gate::Policy);
    }

    #[test]
    fn merchant_allowlist() {
        let gate = SpendPolicyGate::new(SpendPolicyConfig {
            allowed_merchants: vec!["openweather".into()],
            ..Default::default()
        });
        assert!(gate.evaluate(&req(10, "x", "openweather"), now()).is_ok());
        assert!(gate.evaluate(&req(10, "x", "shady-co"), now()).is_err());
    }

    #[test]
    fn velocity_window_cap() {
        let mut gate = SpendPolicyGate::new(SpendPolicyConfig {
            window_cap: Credits::new(100),
            window: Duration::hours(24),
            ..Default::default()
        });
        let t = now();
        assert!(gate.evaluate(&req(60, "x", "y"), t).is_ok());
        gate.record_spend(Credits::new(60), t);

        // 60 + 60 = 120 > 100 → denied.
        let d = gate.evaluate(&req(60, "x", "y"), t).unwrap_err();
        assert_eq!(d.gate, Gate::Policy);

        // 60 + 40 = 100 ≤ 100 → ok.
        assert!(gate.evaluate(&req(40, "x", "y"), t).is_ok());
    }

    #[test]
    fn velocity_window_resets_after_window() {
        let mut gate = SpendPolicyGate::new(SpendPolicyConfig {
            window_cap: Credits::new(100),
            window: Duration::hours(24),
            ..Default::default()
        });
        let t0 = Timestamp::new(0);
        gate.record_spend(Credits::new(100), t0);
        assert_eq!(gate.spent_in_window(t0), Credits::new(100));

        // 25h later: the old spend has aged out of the window.
        let later = Timestamp::new(Duration::hours(25).millis);
        assert_eq!(gate.spent_in_window(later), Credits::ZERO);
        assert!(gate.evaluate(&req(100, "x", "y"), later).is_ok());
    }
}

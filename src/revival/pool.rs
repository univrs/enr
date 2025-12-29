//! Revival Pool
//!
//! Implements revival pool from dol/revival.dol

use crate::core::{Credits, NodeId};
use serde::{Deserialize, Serialize};

/// Tax and timing constants - from dol/revival.dol lines 36-39
pub const ENTROPY_TAX_RATE: f64 = 0.02;

/// Allocation percentages - from dol/revival.dol lines 42-45
pub const NETWORK_MAINTENANCE_ALLOCATION: f64 = 0.40;
pub const NEW_NODE_SUBSIDY_ALLOCATION: f64 = 0.25;
pub const LOW_BALANCE_SUPPORT_ALLOCATION: f64 = 0.20;
pub const RESERVE_BUFFER_ALLOCATION: f64 = 0.15;

/// Eligibility thresholds - from dol/revival.dol lines 48-51
pub const SUBSIDY_THRESHOLD: u64 = 100;
pub const MIN_NEXUS_UPTIME_FOR_MAINTENANCE: f64 = 0.95;
pub const MIN_REPUTATION_FOR_SUPPORT: f64 = 0.5;

/// RevivalPool - from dol/core.dol line 335
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RevivalPool {
    /// Credits from failed nodes
    pub recycled_credits: Credits,
    /// Tax collected from transactions
    pub entropy_tax_collected: Credits,
    /// Fund for network maintenance
    pub maintenance_fund: Credits,
    /// Emergency reserve
    pub reserve_buffer: Credits,
}

impl RevivalPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Total balance in the pool
    pub fn total_balance(&self) -> Credits {
        Credits::new(
            self.recycled_credits.amount
                + self.entropy_tax_collected.amount
                + self.maintenance_fund.amount
                + self.reserve_buffer.amount,
        )
    }

    /// Available for redistribution
    pub fn available_for_redistribution(&self) -> Credits {
        Credits::new(self.recycled_credits.amount + self.entropy_tax_collected.amount)
    }

    /// Add recycled credits
    pub fn add_recycled(&mut self, amount: Credits) {
        self.recycled_credits = self.recycled_credits.saturating_add(amount);
    }

    /// Add entropy tax
    pub fn add_tax(&mut self, amount: Credits) {
        self.entropy_tax_collected = self.entropy_tax_collected.saturating_add(amount);
    }

    /// Add to maintenance fund
    pub fn add_maintenance(&mut self, amount: Credits) {
        self.maintenance_fund = self.maintenance_fund.saturating_add(amount);
    }

    /// Add to reserve
    pub fn add_reserve(&mut self, amount: Credits) {
        self.reserve_buffer = self.reserve_buffer.saturating_add(amount);
    }

    /// Clear redistribution pools after distribution
    pub fn clear_redistribution_pools(&mut self) {
        self.recycled_credits = Credits::ZERO;
        self.entropy_tax_collected = Credits::ZERO;
    }

    /// Constraint: non_negative from dol/core.dol line 352
    /// Note: Credits uses u64 internally, so values are always >= 0
    pub fn is_valid(&self) -> bool {
        // All Credits are u64 internally, so non-negative is guaranteed
        // This check is kept for documentation and potential future invariants
        true
    }
}

/// Calculate entropy tax for a transaction
/// From dol/revival.dol lines 377-388
pub fn calculate_entropy_tax(transaction_amount: Credits) -> Credits {
    let tax_amount = (transaction_amount.amount as f64 * ENTROPY_TAX_RATE).floor() as u64;
    Credits::new(tax_amount)
}

/// Redistribution plan - from dol/revival.dol line 215
#[derive(Debug, Clone, Default)]
pub struct RedistributionPlan {
    pub maintenance_recipients: Vec<(NodeId, Credits)>,
    pub subsidy_recipients: Vec<(NodeId, Credits)>,
    pub support_recipients: Vec<(NodeId, Credits)>,
    pub reserve_addition: Credits,
}

impl RedistributionPlan {
    pub fn total_distributed(&self) -> Credits {
        let maintenance: u64 = self.maintenance_recipients.iter().map(|(_, c)| c.amount).sum();
        let subsidy: u64 = self.subsidy_recipients.iter().map(|(_, c)| c.amount).sum();
        let support: u64 = self.support_recipients.iter().map(|(_, c)| c.amount).sum();

        Credits::new(maintenance + subsidy + support + self.reserve_addition.amount)
    }
}

/// Plan redistribution of revival pool credits
/// From dol/revival.dol lines 226-321
pub fn plan_redistribution<M: NodeMetricsProvider>(
    pool: &RevivalPool,
    metrics: &M,
) -> RedistributionPlan {
    let total_available = pool.available_for_redistribution().amount;

    if total_available == 0 {
        return RedistributionPlan::default();
    }

    // Calculate budgets
    let maintenance_budget = (total_available as f64 * NETWORK_MAINTENANCE_ALLOCATION) as u64;
    let subsidy_budget = (total_available as f64 * NEW_NODE_SUBSIDY_ALLOCATION) as u64;
    let support_budget = (total_available as f64 * LOW_BALANCE_SUPPORT_ALLOCATION) as u64;
    let reserve_budget = (total_available as f64 * RESERVE_BUFFER_ALLOCATION) as u64;

    // Find maintenance recipients (high-uptime nexus nodes)
    let nexus_nodes: Vec<NodeId> = metrics
        .get_nexus_nodes()
        .into_iter()
        .filter(|n| metrics.get_uptime(n) >= MIN_NEXUS_UPTIME_FOR_MAINTENANCE)
        .collect();

    let maintenance_per_node = if !nexus_nodes.is_empty() {
        maintenance_budget / nexus_nodes.len() as u64
    } else {
        0
    };

    let maintenance_recipients: Vec<(NodeId, Credits)> = nexus_nodes
        .into_iter()
        .map(|n| (n, Credits::new(maintenance_per_node)))
        .collect();

    // Find subsidy recipients (new nodes)
    let new_nodes: Vec<NodeId> = metrics
        .get_new_nodes()
        .into_iter()
        .filter(|n| metrics.is_healthy(n))
        .collect();

    let subsidy_per_node = if !new_nodes.is_empty() {
        subsidy_budget / new_nodes.len() as u64
    } else {
        0
    };

    let subsidy_recipients: Vec<(NodeId, Credits)> = new_nodes
        .into_iter()
        .map(|n| (n, Credits::new(subsidy_per_node)))
        .collect();

    // Find support recipients (low balance nodes)
    let struggling_nodes: Vec<NodeId> = metrics
        .get_all_nodes()
        .into_iter()
        .filter(|n| metrics.get_balance(n).amount < SUBSIDY_THRESHOLD)
        .filter(|n| metrics.get_reputation(n) >= MIN_REPUTATION_FOR_SUPPORT)
        .collect();

    let support_per_node = if !struggling_nodes.is_empty() {
        support_budget / struggling_nodes.len() as u64
    } else {
        0
    };

    let support_recipients: Vec<(NodeId, Credits)> = struggling_nodes
        .into_iter()
        .map(|n| (n, Credits::new(support_per_node)))
        .collect();

    RedistributionPlan {
        maintenance_recipients,
        subsidy_recipients,
        support_recipients,
        reserve_addition: Credits::new(reserve_budget),
    }
}

/// Node metrics provider trait for redistribution
pub trait NodeMetricsProvider {
    fn get_all_nodes(&self) -> Vec<NodeId>;
    fn get_nexus_nodes(&self) -> Vec<NodeId>;
    fn get_new_nodes(&self) -> Vec<NodeId>;
    fn get_uptime(&self, node: &NodeId) -> f64;
    fn get_reputation(&self, node: &NodeId) -> f64;
    fn get_balance(&self, node: &NodeId) -> Credits;
    fn is_healthy(&self, node: &NodeId) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_revival_pool() {
        let mut pool = RevivalPool::new();

        pool.add_recycled(Credits::new(1000));
        pool.add_tax(Credits::new(200));
        pool.add_reserve(Credits::new(100));

        assert_eq!(pool.available_for_redistribution().amount, 1200);
        assert_eq!(pool.total_balance().amount, 1300);
        assert!(pool.is_valid());
    }

    #[test]
    fn test_entropy_tax() {
        let amount = Credits::new(1000);
        let tax = calculate_entropy_tax(amount);
        // 2% of 1000 = 20
        assert_eq!(tax.amount, 20);
    }

    #[test]
    fn test_allocation_sum() {
        let sum = NETWORK_MAINTENANCE_ALLOCATION
            + NEW_NODE_SUBSIDY_ALLOCATION
            + LOW_BALANCE_SUPPORT_ALLOCATION
            + RESERVE_BUFFER_ALLOCATION;
        assert!((sum - 1.0).abs() < 0.001);
    }
}

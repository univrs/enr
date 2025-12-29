//! Gradient Aggregation
//!
//! Implements gradient aggregation from dol/nexus.dol lines 175-223

use super::types::*;

/// Aggregate gradients from leaf nodes using weighted average.
///
/// Formula: nexus.gradient[resource] =
///          Σ(leaf.gradient[resource] * leaf.weight) / Σ(leaf.weight)
///
/// From dol/nexus.dol lines 175-223
pub fn aggregate_gradients(reports: &[LeafGradientReport]) -> ResourceGradient {
    let total_weight: f64 = reports.iter().map(|r| r.weight).sum();

    if total_weight == 0.0 {
        return ResourceGradient::zero();
    }

    let cpu: f64 = reports
        .iter()
        .map(|r| r.gradient.cpu_available * r.weight)
        .sum::<f64>()
        / total_weight;

    let memory: f64 = reports
        .iter()
        .map(|r| r.gradient.memory_available * r.weight)
        .sum::<f64>()
        / total_weight;

    let gpu: f64 = reports
        .iter()
        .map(|r| r.gradient.gpu_available * r.weight)
        .sum::<f64>()
        / total_weight;

    let storage: f64 = reports
        .iter()
        .map(|r| r.gradient.storage_available * r.weight)
        .sum::<f64>()
        / total_weight;

    let bandwidth: f64 = reports
        .iter()
        .map(|r| r.gradient.bandwidth_available * r.weight)
        .sum::<f64>()
        / total_weight;

    let credits: f64 = reports
        .iter()
        .map(|r| r.gradient.credit_balance * r.weight)
        .sum::<f64>()
        / total_weight;

    ResourceGradient {
        cpu_available: cpu,
        memory_available: memory,
        gpu_available: gpu,
        storage_available: storage,
        bandwidth_available: bandwidth,
        credit_balance: credits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{NodeId, Timestamp};

    #[test]
    fn test_aggregate_empty() {
        let result = aggregate_gradients(&[]);
        assert_eq!(result, ResourceGradient::zero());
    }

    #[test]
    fn test_aggregate_single() {
        let node = NodeId::from_bytes([1u8; 32]);
        let report = LeafGradientReport {
            node,
            gradient: ResourceGradient {
                cpu_available: 0.5,
                memory_available: 0.6,
                gpu_available: 0.0,
                storage_available: 0.8,
                bandwidth_available: 0.9,
                credit_balance: 100.0,
            },
            weight: 1.0,
            timestamp: Timestamp::now(),
        };

        let result = aggregate_gradients(&[report.clone()]);
        assert!((result.cpu_available - 0.5).abs() < 0.001);
        assert!((result.memory_available - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_aggregate_weighted() {
        let node1 = NodeId::from_bytes([1u8; 32]);
        let node2 = NodeId::from_bytes([2u8; 32]);

        let report1 = LeafGradientReport {
            node: node1,
            gradient: ResourceGradient {
                cpu_available: 0.0,
                memory_available: 0.0,
                gpu_available: 0.0,
                storage_available: 0.0,
                bandwidth_available: 0.0,
                credit_balance: 0.0,
            },
            weight: 1.0,
            timestamp: Timestamp::now(),
        };

        let report2 = LeafGradientReport {
            node: node2,
            gradient: ResourceGradient {
                cpu_available: 1.0,
                memory_available: 1.0,
                gpu_available: 1.0,
                storage_available: 1.0,
                bandwidth_available: 1.0,
                credit_balance: 100.0,
            },
            weight: 3.0, // 3x weight
            timestamp: Timestamp::now(),
        };

        let result = aggregate_gradients(&[report1, report2]);

        // With weights 1 and 3, average should be (0*1 + 1*3) / 4 = 0.75
        assert!((result.cpu_available - 0.75).abs() < 0.001);
        assert!((result.memory_available - 0.75).abs() < 0.001);
        assert!((result.credit_balance - 75.0).abs() < 0.001);
    }
}

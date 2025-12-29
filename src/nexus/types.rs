//! ENR Nexus Types
//!
//! Rust implementation from dol/nexus.dol and dol/core.dol

use crate::core::{Credits, NodeId, Timestamp};
use serde::{Deserialize, Serialize};

/// NexusRoleType - from dol/core.dol line 257
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NexusRoleType {
    /// Regular node
    Leaf,
    /// Hub node
    Nexus,
    /// Super-nexus (central pillar)
    PoteauMitan,
}

impl Default for NexusRoleType {
    fn default() -> Self {
        NexusRoleType::Leaf
    }
}

/// NexusRole - from dol/core.dol line 242
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NexusRole {
    pub role_type: NexusRoleType,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

impl Default for NexusRole {
    fn default() -> Self {
        Self {
            role_type: NexusRoleType::Leaf,
            parent: None,
            children: Vec::new(),
        }
    }
}

impl NexusRole {
    pub fn leaf(parent: NodeId) -> Self {
        Self {
            role_type: NexusRoleType::Leaf,
            parent: Some(parent),
            children: Vec::new(),
        }
    }

    pub fn nexus(parent: Option<NodeId>, children: Vec<NodeId>) -> Self {
        Self {
            role_type: NexusRoleType::Nexus,
            parent,
            children,
        }
    }

    pub fn poteau_mitan(children: Vec<NodeId>) -> Self {
        Self {
            role_type: NexusRoleType::PoteauMitan,
            parent: None,
            children,
        }
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self.role_type, NexusRoleType::Leaf)
    }

    pub fn is_nexus(&self) -> bool {
        matches!(self.role_type, NexusRoleType::Nexus)
    }

    pub fn is_poteau_mitan(&self) -> bool {
        matches!(self.role_type, NexusRoleType::PoteauMitan)
    }
}

/// ResourceType - from dol/core.dol line 265
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    Cpu,
    Memory,
    Gpu,
    Storage,
    Bandwidth,
}

/// ResourceGradient - from dol/core.dol line 275
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ResourceGradient {
    pub cpu_available: f64,
    pub memory_available: f64,
    pub gpu_available: f64,
    pub storage_available: f64,
    pub bandwidth_available: f64,
    pub credit_balance: f64,
}

impl ResourceGradient {
    /// Constraint: normalized from dol/core.dol line 288
    pub fn is_valid(&self) -> bool {
        self.cpu_available >= 0.0
            && self.cpu_available <= 1.0
            && self.memory_available >= 0.0
            && self.memory_available <= 1.0
            && self.gpu_available >= 0.0
            && self.gpu_available <= 1.0
            && self.storage_available >= 0.0
            && self.storage_available <= 1.0
            && self.bandwidth_available >= 0.0
            && self.bandwidth_available <= 1.0
    }

    pub fn zero() -> Self {
        Self::default()
    }
}

/// NexusTopology - from dol/core.dol line 308
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NexusTopology {
    pub node: NodeId,
    pub role: NexusRole,
    pub aggregated_gradient: ResourceGradient,
    pub leaf_count: u32,
    pub last_election: Timestamp,
}

/// GossipPathType - from dol/nexus.dol line 58
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GossipPathType {
    /// Direct node-to-node
    Direct,
    /// Through nexus hub
    ViaHub,
    /// Through poteau-mitan
    ViaSuperHub,
}

/// GossipPath - from dol/nexus.dol line 66
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GossipPath {
    pub path_type: GossipPathType,
    pub hops: Vec<NodeId>,
    pub estimated_entropy: f64,
}

impl GossipPath {
    pub fn direct(target: NodeId, entropy: f64) -> Self {
        Self {
            path_type: GossipPathType::Direct,
            hops: vec![target],
            estimated_entropy: entropy,
        }
    }

    pub fn via_hub(hub: NodeId, target: NodeId, entropy: f64) -> Self {
        Self {
            path_type: GossipPathType::ViaHub,
            hops: vec![hub, target],
            estimated_entropy: entropy,
        }
    }
}

/// LeafGradientReport - from dol/nexus.dol line 164
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeafGradientReport {
    pub node: NodeId,
    pub gradient: ResourceGradient,
    pub weight: f64,
    pub timestamp: Timestamp,
}

/// Region - from dol/nexus.dol line 229
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub id: String,
    pub nodes: Vec<NodeId>,
    pub current_nexus: Option<NodeId>,
}

impl Region {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            nodes: Vec::new(),
            current_nexus: None,
        }
    }

    pub fn with_nodes(id: impl Into<String>, nodes: Vec<NodeId>) -> Self {
        Self {
            id: id.into(),
            nodes,
            current_nexus: None,
        }
    }
}

/// NexusCandidate - from dol/nexus.dol line 239
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NexusCandidate {
    pub node: NodeId,
    pub uptime: f64,
    pub bandwidth: u64,
    pub reputation: f64,
    pub current_leaf_count: u32,
    pub election_score: f64,
}

/// Order - from dol/nexus.dol line 344
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub price: Credits,
    pub quantity: u64,
    pub node: NodeId,
    pub timestamp: Timestamp,
}

/// OrderBook - from dol/nexus.dol line 355
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderBook {
    pub resource: ResourceType,
    /// Sorted by price descending
    pub bids: Vec<Order>,
    /// Sorted by price ascending
    pub asks: Vec<Order>,
}

impl OrderBook {
    pub fn new(resource: ResourceType) -> Self {
        Self {
            resource,
            bids: Vec::new(),
            asks: Vec::new(),
        }
    }

    pub fn best_bid(&self) -> Option<Credits> {
        self.bids.first().map(|o| o.price)
    }

    pub fn best_ask(&self) -> Option<Credits> {
        self.asks.first().map(|o| o.price)
    }

    pub fn total_inventory(&self) -> u64 {
        self.asks.iter().map(|o| o.quantity).sum()
    }

    pub fn spread(&self) -> Option<Credits> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) if ask.amount > bid.amount => {
                Some(Credits::new(ask.amount - bid.amount))
            }
            _ => None,
        }
    }
}

/// MarketMakerConfig - from dol/nexus.dol line 384
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketMakerConfig {
    pub minimum_spread: f64,
    pub volatility_factor: f64,
    pub inventory_factor: f64,
    pub entropy_spread_factor: f64,
    pub target_inventory: u64,
}

impl Default for MarketMakerConfig {
    fn default() -> Self {
        Self {
            minimum_spread: 0.01, // 1%
            volatility_factor: 0.5,
            inventory_factor: 0.3,
            entropy_spread_factor: 0.1,
            target_inventory: 1000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nexus_role() {
        let node = NodeId::from_bytes([1u8; 32]);
        let parent = NodeId::from_bytes([2u8; 32]);

        let leaf = NexusRole::leaf(parent);
        assert!(leaf.is_leaf());
        assert_eq!(leaf.parent, Some(parent));

        let nexus = NexusRole::nexus(None, vec![node]);
        assert!(nexus.is_nexus());
        assert_eq!(nexus.children.len(), 1);
    }

    #[test]
    fn test_resource_gradient_valid() {
        let valid = ResourceGradient {
            cpu_available: 0.5,
            memory_available: 0.3,
            gpu_available: 0.0,
            storage_available: 0.8,
            bandwidth_available: 1.0,
            credit_balance: 100.0,
        };
        assert!(valid.is_valid());

        let invalid = ResourceGradient {
            cpu_available: 1.5,
            ..Default::default()
        };
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_order_book() {
        let node = NodeId::from_bytes([1u8; 32]);
        let mut book = OrderBook::new(ResourceType::Cpu);

        book.bids.push(Order {
            price: Credits::new(100),
            quantity: 10,
            node,
            timestamp: Timestamp::now(),
        });

        book.asks.push(Order {
            price: Credits::new(110),
            quantity: 5,
            node,
            timestamp: Timestamp::now(),
        });

        assert_eq!(book.best_bid(), Some(Credits::new(100)));
        assert_eq!(book.best_ask(), Some(Credits::new(110)));
        assert_eq!(book.spread(), Some(Credits::new(10)));
        assert_eq!(book.total_inventory(), 5);
    }
}

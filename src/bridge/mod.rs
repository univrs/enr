//! ENR Bridge Module - Gossipsub Integration
//!
//! This module provides the bridge between the ENR economic layer
//! and the mycelial network gossipsub protocol.
//!
//! # Overview
//!
//! The EnrBridge coordinates:
//! - Gradient broadcasting for resource flow
//! - Distributed nexus election
//! - Credit transfer and synchronization
//! - Septal gate (circuit breaker) coordination
//!
//! # Usage
//!
//! ```ignore
//! use univrs_enr::bridge::{EnrBridge, EnrBridgeConfig};
//!
//! let config = EnrBridgeConfig::default();
//! let mut bridge = EnrBridge::new(local_id, config);
//!
//! // Connect publish_fn to swarm.gossipsub.publish()
//! bridge.connect_publisher(Arc::new(|topic, data| {
//!     swarm.behaviour_mut()
//!         .gossipsub
//!         .publish(gossipsub::IdentTopic::new(topic), data)
//!         .map_err(|e| BridgeError::Network(e.to_string()))
//! }));
//!
//! // Subscribe to ENR topics
//! for topic in bridge.topics_to_subscribe() {
//!     let topic = gossipsub::IdentTopic::new(topic);
//!     swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
//! }
//!
//! // Start gradient broadcasting
//! bridge.start_gradient_broadcast().await;
//! ```

pub mod error;
pub mod handlers;
pub mod messages;
pub mod topics;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::core::{Credits, NodeId, Timestamp};
use crate::nexus::{ResourceGradient, TopologyManager};
use crate::septal::{SeptalGate, SeptalGateConfig, SeptalGateState};

pub use error::{BridgeError, TransferError};
pub use handlers::*;
pub use messages::*;
pub use topics::EnrTopics;

/// Type alias for the publish function that connects to gossipsub
///
/// This function is called to publish messages to the gossipsub network.
/// It maps directly to `swarm.gossipsub.publish(topic, data)`.
///
/// # Parameters
/// - `topic`: The gossipsub topic string (e.g., "/enr/gradient/1.0")
/// - `data`: Serialized message bytes (bincode encoded)
///
/// # Returns
/// - `Ok(())` on successful publish
/// - `Err(BridgeError)` on failure
///
/// # Example Integration with libp2p Gossipsub
///
/// ```ignore
/// use libp2p::gossipsub;
///
/// let swarm = /* libp2p swarm with gossipsub */;
/// let publish_fn: PublishFn = Arc::new(move |topic, data| {
///     let topic = gossipsub::IdentTopic::new(topic);
///     swarm.behaviour_mut()
///         .gossipsub
///         .publish(topic, data)
///         .map_err(|e| BridgeError::Network(e.to_string()))
/// });
///
/// bridge.connect_publisher(publish_fn);
/// ```
pub type PublishFn = Arc<dyn Fn(&str, Vec<u8>) -> Result<(), BridgeError> + Send + Sync>;

/// Configuration for the ENR bridge
#[derive(Debug, Clone)]
pub struct EnrBridgeConfig {
    /// Interval for broadcasting local gradient (default: 10s)
    pub gradient_interval: Duration,
    /// Interval for nexus election checks (default: 1 hour)
    pub election_interval: Duration,
    /// Enable credit synchronization (default: true)
    pub credit_sync_enabled: bool,
    /// Interval for credit state sync (default: 30s)
    pub credit_sync_interval: Duration,
    /// Septal gate configuration
    pub septal_config: SeptalGateConfig,
    /// Maximum message age to accept (prevents replay attacks)
    pub max_message_age: Duration,
}

impl Default for EnrBridgeConfig {
    fn default() -> Self {
        Self {
            gradient_interval: Duration::from_secs(10),
            election_interval: Duration::from_secs(3600),
            credit_sync_enabled: true,
            credit_sync_interval: Duration::from_secs(30),
            septal_config: SeptalGateConfig::default(),
            max_message_age: Duration::from_secs(60),
        }
    }
}

/// The main ENR bridge coordinator
///
/// Connects the ENR economic layer to the gossipsub network.
/// Handles gradient broadcasting, nexus election, credit transfers,
/// and septal gate coordination.
pub struct EnrBridge {
    /// Configuration
    config: EnrBridgeConfig,
    /// Local node ID
    local_id: NodeId,
    /// Topology manager for nexus election
    topology: Arc<RwLock<TopologyManager>>,
    /// Aggregated gradients from network
    gradients: Arc<RwLock<HashMap<NodeId, ResourceGradient>>>,
    /// Local gradient state
    local_gradient: Arc<RwLock<ResourceGradient>>,
    /// Local credit balance
    local_balance: Arc<RwLock<Credits>>,
    /// Septal gates for nodes (circuit breaker state per node)
    septal_gates: Arc<RwLock<HashMap<NodeId, SeptalGate>>>,
    /// Publish function (connected to gossipsub)
    publish_fn: Option<PublishFn>,
    /// Active broadcast handles (abort handles for cleanup)
    broadcast_handles: Vec<tokio::task::AbortHandle>,
    /// Known node balances (for credit sync)
    known_balances: Arc<RwLock<HashMap<NodeId, Credits>>>,
    /// Pending credit transfers
    pending_transfers: Arc<RwLock<HashMap<TransferId, CreditTransfer>>>,
}

impl EnrBridge {
    /// Create a new ENR bridge
    pub fn new(local_id: NodeId, config: EnrBridgeConfig) -> Self {
        Self {
            config,
            local_id,
            topology: Arc::new(RwLock::new(TopologyManager::new())),
            gradients: Arc::new(RwLock::new(HashMap::new())),
            local_gradient: Arc::new(RwLock::new(ResourceGradient::default())),
            local_balance: Arc::new(RwLock::new(Credits::new(0))),
            septal_gates: Arc::new(RwLock::new(HashMap::new())),
            publish_fn: None,
            broadcast_handles: Vec::new(),
            known_balances: Arc::new(RwLock::new(HashMap::new())),
            pending_transfers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Connect the publish function from gossipsub
    ///
    /// This wires `publish_fn` to `swarm.gossipsub.publish()`.
    /// Must be called before starting any broadcast loops.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // In your libp2p swarm setup:
    /// let publish_fn: PublishFn = Arc::new(move |topic, data| {
    ///     let topic = gossipsub::IdentTopic::new(topic);
    ///     swarm.behaviour_mut()
    ///         .gossipsub
    ///         .publish(topic, data)
    ///         .map_err(|e| BridgeError::Network(e.to_string()))
    /// });
    ///
    /// bridge.connect_publisher(publish_fn);
    /// ```
    pub fn connect_publisher(&mut self, publish_fn: PublishFn) {
        self.publish_fn = Some(publish_fn);
    }

    /// Get the list of topics to subscribe to
    ///
    /// Call this to get all ENR topics that should be subscribed
    /// via gossipsub.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for topic in bridge.topics_to_subscribe() {
    ///     let topic = gossipsub::IdentTopic::new(topic);
    ///     swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
    /// }
    /// ```
    pub fn topics_to_subscribe(&self) -> Vec<&'static str> {
        EnrTopics::all()
    }

    /// Check if publisher is connected
    pub fn is_connected(&self) -> bool {
        self.publish_fn.is_some()
    }

    /// Get local node ID
    pub fn local_id(&self) -> NodeId {
        self.local_id
    }

    /// Handle incoming message from gossipsub
    ///
    /// Route messages received from gossipsub to appropriate handlers.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // In your gossipsub event handler:
    /// SwarmEvent::Behaviour(GossipsubEvent::Message { message, .. }) => {
    ///     bridge.handle_message(&message.topic, &message.data).await?;
    /// }
    /// ```
    pub async fn handle_message(&self, _topic: &str, data: &[u8]) -> Result<(), BridgeError> {
        let message = EnrMessage::from_bytes(data)?;

        // Validate message age
        let now = Timestamp::now();
        let max_age_ms = self.config.max_message_age.as_millis() as u64;

        match &message {
            EnrMessage::Gradient(msg) => {
                if now.millis.saturating_sub(msg.timestamp.millis) > max_age_ms {
                    return Err(BridgeError::MessageExpired);
                }
                self.handle_gradient_message(msg.clone()).await
            }
            EnrMessage::Election(msg) => self.handle_election_message(msg.clone()).await,
            EnrMessage::Credit(msg) => self.handle_credit_message(msg.clone()).await,
            EnrMessage::Septal(msg) => self.handle_septal_message(msg.clone()).await,
        }
    }

    /// Publish a message to the network
    pub fn publish(&self, message: EnrMessage) -> Result<(), BridgeError> {
        let publish_fn = self.publish_fn.as_ref().ok_or(BridgeError::NotConnected)?;

        let topic = message.topic();
        let data = message.to_bytes()?;

        publish_fn(topic, data)
    }

    // ========================================================================
    // Gradient Broadcasting
    // ========================================================================

    /// Update local gradient and optionally broadcast
    pub async fn update_gradient(&self, gradient: ResourceGradient) -> Result<(), BridgeError> {
        {
            let mut local = self.local_gradient.write().await;
            *local = gradient;
        }

        // Broadcast to network if connected
        if self.publish_fn.is_some() {
            let gradient_read = self.local_gradient.read().await;
            let message = EnrMessage::Gradient(GradientMessage {
                node_id: self.local_id,
                gradient: GradientPayload::from(&*gradient_read),
                timestamp: Timestamp::now(),
                signature: Signature::empty(),
            });

            self.publish(message)?;
        }

        Ok(())
    }

    /// Start the gradient broadcast loop
    ///
    /// Spawns a tokio task that periodically broadcasts local gradient.
    pub async fn start_gradient_broadcast(&mut self) -> Result<JoinHandle<()>, BridgeError> {
        if self.publish_fn.is_none() {
            return Err(BridgeError::NotConnected);
        }

        let local_gradient = self.local_gradient.clone();
        let local_id = self.local_id;
        let interval = self.config.gradient_interval;
        let publish_fn = self.publish_fn.clone().unwrap();

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                let gradient = local_gradient.read().await.clone();
                let message = EnrMessage::Gradient(GradientMessage {
                    node_id: local_id,
                    gradient: GradientPayload::from(&gradient),
                    timestamp: Timestamp::now(),
                    signature: Signature::empty(),
                });

                if let Ok(data) = message.to_bytes() {
                    let _ = publish_fn(EnrTopics::GRADIENT, data);
                }
            }
        });

        self.broadcast_handles.push(handle.abort_handle());
        Ok(handle)
    }

    /// Handle incoming gradient message
    async fn handle_gradient_message(&self, msg: GradientMessage) -> Result<(), BridgeError> {
        // Check if node is isolated
        {
            let gates = self.septal_gates.read().await;
            if let Some(gate) = gates.get(&msg.node_id) {
                if gate.state.is_closed() {
                    return Ok(()); // Ignore messages from isolated nodes
                }
            }
        }

        // Update gradient store
        {
            let mut gradients = self.gradients.write().await;
            gradients.insert(msg.node_id, msg.gradient.to_resource_gradient());
        }

        // Update topology gradient
        {
            let mut topology = self.topology.write().await;
            topology.update_gradient(&msg.node_id, msg.gradient.to_resource_gradient());
        }

        Ok(())
    }

    // ========================================================================
    // Election Handling
    // ========================================================================

    /// Trigger a new nexus election
    pub async fn trigger_election(&self) -> Result<ElectionAnnouncement, BridgeError> {
        let election_id = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            self.local_id.hash(&mut hasher);
            Timestamp::now().millis.hash(&mut hasher);

            let hash = hasher.finish();
            let mut id = [0u8; 32];
            id[..8].copy_from_slice(&hash.to_le_bytes());
            id
        };

        let announcement = ElectionAnnouncement {
            election_id,
            initiator: self.local_id,
            timestamp: Timestamp::now(),
            round: 1,
        };

        let message = EnrMessage::Election(ElectionMessage::Announcement(announcement.clone()));
        self.publish(message)?;

        Ok(announcement)
    }

    /// Handle incoming election message
    async fn handle_election_message(&self, msg: ElectionMessage) -> Result<(), BridgeError> {
        match msg {
            ElectionMessage::Announcement(_ann) => {
                // TODO: Participate in election by declaring candidacy
                Ok(())
            }
            ElectionMessage::Candidacy(_candidate) => {
                // TODO: Evaluate candidate and potentially vote
                Ok(())
            }
            ElectionMessage::Vote(_vote) => {
                // TODO: Collect votes if we're coordinating
                Ok(())
            }
            ElectionMessage::Result(result) => {
                // Update topology with new nexus
                let mut topology = self.topology.write().await;
                if let Some(topo) = topology.get_topology(&result.winner) {
                    let mut updated = topo.clone();
                    updated.role.role_type = crate::nexus::NexusRoleType::Nexus;
                    topology.set_topology(result.winner, updated);
                }
                Ok(())
            }
        }
    }

    // ========================================================================
    // Credit Transfer
    // ========================================================================

    /// Get local credit balance
    pub async fn balance(&self) -> Credits {
        *self.local_balance.read().await
    }

    /// Set local credit balance
    pub async fn set_balance(&self, balance: Credits) {
        let mut local = self.local_balance.write().await;
        *local = balance;
    }

    /// Transfer credits to another node
    pub async fn transfer(
        &self,
        to: NodeId,
        amount: Credits,
    ) -> Result<TransferId, TransferError> {
        // Validate transfer
        if amount.is_zero() {
            return Err(TransferError::ZeroAmount);
        }
        if to == self.local_id {
            return Err(TransferError::SelfTransfer);
        }

        // Check balance
        {
            let current = self.local_balance.read().await;
            if *current < amount {
                return Err(TransferError::InsufficientBalance);
            }
        } // Drop read lock before acquiring write lock

        // Generate transfer ID
        let nonce = Timestamp::now().millis;
        let transfer_id = TransferId::from_transfer(&self.local_id, &to, amount.amount, nonce);

        // Create transfer
        let transfer = CreditTransfer {
            id: transfer_id,
            from: self.local_id,
            to,
            amount: amount.amount,
            nonce,
            timestamp: Timestamp::now(),
            memo: None,
            signature: Signature::empty(),
        };

        // Reserve credits (deduct from local balance)
        {
            let mut balance = self.local_balance.write().await;
            *balance = balance.saturating_sub(amount);
        }

        // Store pending transfer
        {
            let mut pending = self.pending_transfers.write().await;
            pending.insert(transfer_id, transfer.clone());
        }

        // Broadcast transfer
        let message = EnrMessage::Credit(CreditMessage::Transfer(transfer));
        self.publish(message).map_err(|_| TransferError::Cancelled)?;

        Ok(transfer_id)
    }

    /// Handle incoming credit message
    async fn handle_credit_message(&self, msg: CreditMessage) -> Result<(), BridgeError> {
        match msg {
            CreditMessage::Transfer(transfer) => {
                // If we're the recipient, credit our balance
                if transfer.to == self.local_id {
                    let mut balance = self.local_balance.write().await;
                    *balance = *balance + Credits::new(transfer.amount);

                    // Send confirmation
                    let confirmation = TransferConfirmation {
                        transfer_id: transfer.id,
                        confirmer: self.local_id,
                        timestamp: Timestamp::now(),
                        signature: Signature::empty(),
                    };
                    let message = EnrMessage::Credit(CreditMessage::Confirmation(confirmation));
                    let _ = self.publish(message);
                }

                // Update known balances
                {
                    let mut balances = self.known_balances.write().await;
                    // Deduct from sender
                    if let Some(sender_balance) = balances.get_mut(&transfer.from) {
                        *sender_balance =
                            sender_balance.saturating_sub(Credits::new(transfer.amount));
                    }
                    // Add to recipient
                    let recipient_balance = balances.entry(transfer.to).or_insert(Credits::zero());
                    *recipient_balance = *recipient_balance + Credits::new(transfer.amount);
                }

                Ok(())
            }
            CreditMessage::Confirmation(confirmation) => {
                // Remove from pending
                let mut pending = self.pending_transfers.write().await;
                pending.remove(&confirmation.transfer_id);
                Ok(())
            }
            CreditMessage::StateSync(sync) => {
                // Update known balance
                let mut balances = self.known_balances.write().await;
                balances.insert(sync.node_id, Credits::new(sync.balance));
                Ok(())
            }
            CreditMessage::BalanceQuery { requester: _, target } => {
                if target == self.local_id {
                    let balance = self.local_balance.read().await;
                    let response = CreditMessage::BalanceResponse {
                        node_id: self.local_id,
                        balance: balance.amount,
                    };
                    let message = EnrMessage::Credit(response);
                    let _ = self.publish(message);
                }
                Ok(())
            }
            CreditMessage::BalanceResponse { node_id, balance } => {
                let mut balances = self.known_balances.write().await;
                balances.insert(node_id, Credits::new(balance));
                Ok(())
            }
        }
    }

    // ========================================================================
    // Septal Gate (Circuit Breaker)
    // ========================================================================

    /// Record a failure for a node
    pub async fn record_failure(&self, node_id: NodeId, reason: &str) {
        {
            let mut gates = self.septal_gates.write().await;
            let gate = gates.entry(node_id).or_insert_with(|| SeptalGate::new(node_id));
            gate.record_failure();

            // Check if should trip
            if gate.should_trip() {
                gate.trip();
            }
        }

        // Broadcast failure report
        let report = FailureReport {
            reporter: self.local_id,
            failed_node: node_id,
            failure_type: reason.to_string(),
            timestamp: Timestamp::now(),
            signature: Signature::empty(),
        };
        let message = EnrMessage::Septal(SeptalMessage::FailureReport(report));
        let _ = self.publish(message);
    }

    /// Check if a node is isolated
    pub async fn is_isolated(&self, node_id: &NodeId) -> bool {
        let gates = self.septal_gates.read().await;
        gates
            .get(node_id)
            .map(|g| g.state.is_closed())
            .unwrap_or(false)
    }

    /// Get gate state for a node
    pub async fn gate_state(&self, node_id: &NodeId) -> SeptalGateState {
        let gates = self.septal_gates.read().await;
        gates
            .get(node_id)
            .map(|g| g.state)
            .unwrap_or(SeptalGateState::Open)
    }

    /// Handle incoming septal message
    async fn handle_septal_message(&self, msg: SeptalMessage) -> Result<(), BridgeError> {
        match msg {
            SeptalMessage::FailureReport(report) => {
                let mut gates = self.septal_gates.write().await;
                let gate = gates
                    .entry(report.failed_node)
                    .or_insert_with(|| SeptalGate::new(report.failed_node));
                gate.record_failure();

                if gate.should_trip() {
                    gate.trip();
                }
                Ok(())
            }
            SeptalMessage::Isolation(notice) => {
                let mut gates = self.septal_gates.write().await;
                let gate = gates
                    .entry(notice.isolated_node)
                    .or_insert_with(|| SeptalGate::new(notice.isolated_node));
                gate.trip();
                Ok(())
            }
            SeptalMessage::HealingProbe(probe) => {
                if probe.target == self.local_id {
                    // Respond to probe
                    let response = HealingResponse {
                        probe_id: probe.probe_id,
                        responder: self.local_id,
                        state: SeptalGateState::Open,
                        timestamp: Timestamp::now(),
                    };
                    let message = EnrMessage::Septal(SeptalMessage::HealingResponse(response));
                    let _ = self.publish(message);
                }
                Ok(())
            }
            SeptalMessage::HealingResponse(response) => {
                // Mark successful probe - attempt half-open or recover
                let mut gates = self.septal_gates.write().await;
                if let Some(gate) = gates.get_mut(&response.responder) {
                    gate.record_success();
                    if gate.state.is_half_open() {
                        gate.recover();
                    }
                }
                Ok(())
            }
            SeptalMessage::Recovery(notice) => {
                let mut gates = self.septal_gates.write().await;
                if let Some(gate) = gates.get_mut(&notice.recovered_node) {
                    gate.recover();
                }
                Ok(())
            }
        }
    }

    // ========================================================================
    // Lifecycle
    // ========================================================================

    /// Stop all broadcast loops
    pub fn stop(&mut self) {
        for handle in self.broadcast_handles.drain(..) {
            handle.abort();
        }
    }
}

impl Drop for EnrBridge {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_node_id() -> NodeId {
        NodeId::from_bytes([1u8; 32])
    }

    #[test]
    fn test_bridge_config_default() {
        let config = EnrBridgeConfig::default();
        assert_eq!(config.gradient_interval, Duration::from_secs(10));
        assert_eq!(config.election_interval, Duration::from_secs(3600));
        assert!(config.credit_sync_enabled);
    }

    #[test]
    fn test_bridge_creation() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        assert_eq!(bridge.local_id(), local_id);
        assert!(!bridge.is_connected());
    }

    #[test]
    fn test_connect_publisher() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let mut bridge = EnrBridge::new(local_id, config);

        assert!(!bridge.is_connected());

        let publish_count = Arc::new(AtomicUsize::new(0));
        let count = publish_count.clone();
        let publish_fn: PublishFn = Arc::new(move |_topic, _data| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        bridge.connect_publisher(publish_fn);
        assert!(bridge.is_connected());
    }

    #[test]
    fn test_topics_to_subscribe() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let topics = bridge.topics_to_subscribe();
        assert_eq!(topics.len(), 4);
        assert!(topics.contains(&"/enr/gradient/1.0"));
        assert!(topics.contains(&"/enr/election/1.0"));
        assert!(topics.contains(&"/enr/credit/1.0"));
        assert!(topics.contains(&"/enr/septal/1.0"));
    }

    #[tokio::test]
    async fn test_update_gradient() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let gradient = ResourceGradient {
            cpu_available: 0.8,
            memory_available: 0.6,
            ..Default::default()
        };

        // Should succeed even without publisher (just updates local state)
        bridge.update_gradient(gradient).await.unwrap();

        let stored = bridge.local_gradient.read().await;
        assert!((stored.cpu_available - 0.8).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_credit_balance() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        assert_eq!(bridge.balance().await, Credits::zero());

        bridge.set_balance(Credits::new(1000)).await;
        assert_eq!(bridge.balance().await, Credits::new(1000));
    }

    #[tokio::test]
    async fn test_transfer_validation() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let mut bridge = EnrBridge::new(local_id, config);

        // Connect a mock publisher
        let publish_fn: PublishFn = Arc::new(|_topic, _data| Ok(()));
        bridge.connect_publisher(publish_fn);

        // Set initial balance
        bridge.set_balance(Credits::new(1000)).await;

        let recipient = NodeId::from_bytes([2u8; 32]);

        // Test zero amount
        let result = bridge.transfer(recipient, Credits::zero()).await;
        assert!(matches!(result, Err(TransferError::ZeroAmount)));

        // Test self transfer
        let result = bridge.transfer(local_id, Credits::new(100)).await;
        assert!(matches!(result, Err(TransferError::SelfTransfer)));

        // Test insufficient balance
        let result = bridge.transfer(recipient, Credits::new(2000)).await;
        assert!(matches!(result, Err(TransferError::InsufficientBalance)));

        // Test successful transfer
        let result = bridge.transfer(recipient, Credits::new(100)).await;
        assert!(result.is_ok());
        assert_eq!(bridge.balance().await, Credits::new(900));
    }

    #[tokio::test]
    async fn test_septal_gate_tracking() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let failing_node = NodeId::from_bytes([3u8; 32]);

        // Initially not isolated
        assert!(!bridge.is_isolated(&failing_node).await);
        assert_eq!(
            bridge.gate_state(&failing_node).await,
            SeptalGateState::Open
        );

        // Record failures (need 5 to trip)
        for _ in 0..5 {
            bridge.record_failure(failing_node, "test failure").await;
        }

        // Should now be isolated
        assert!(bridge.is_isolated(&failing_node).await);
        assert_eq!(
            bridge.gate_state(&failing_node).await,
            SeptalGateState::Closed
        );
    }

    // ========================================================================
    // Message Handler Tests
    // ========================================================================

    #[tokio::test]
    async fn test_handle_gradient_message() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let sender = NodeId::from_bytes([2u8; 32]);
        let gradient = GradientMessage {
            node_id: sender,
            gradient: GradientPayload {
                cpu_available: 0.75,
                memory_available: 0.50,
                gpu_available: 0.0,
                storage_available: 0.80,
                bandwidth_available: 0.90,
                credit_balance: 500.0,
            },
            timestamp: Timestamp::now(),
            signature: Signature::empty(),
        };

        // Handle the message
        bridge.handle_gradient_message(gradient).await.unwrap();

        // Verify gradient was stored
        let gradients = bridge.gradients.read().await;
        let stored = gradients.get(&sender).unwrap();
        assert!((stored.cpu_available - 0.75).abs() < 0.001);
        assert!((stored.memory_available - 0.50).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_handle_gradient_from_isolated_node() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let isolated_node = NodeId::from_bytes([2u8; 32]);

        // Isolate the node first
        for _ in 0..5 {
            bridge.record_failure(isolated_node, "test").await;
        }
        assert!(bridge.is_isolated(&isolated_node).await);

        // Try to handle gradient from isolated node
        let gradient = GradientMessage {
            node_id: isolated_node,
            gradient: GradientPayload::from(&ResourceGradient::default()),
            timestamp: Timestamp::now(),
            signature: Signature::empty(),
        };

        bridge.handle_gradient_message(gradient).await.unwrap();

        // Gradient should NOT be stored (node is isolated)
        let gradients = bridge.gradients.read().await;
        assert!(gradients.get(&isolated_node).is_none());
    }

    #[tokio::test]
    async fn test_handle_credit_transfer_as_recipient() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let mut bridge = EnrBridge::new(local_id, config);

        // Connect publisher for confirmation
        let publish_fn: PublishFn = Arc::new(|_topic, _data| Ok(()));
        bridge.connect_publisher(publish_fn);

        let sender = NodeId::from_bytes([2u8; 32]);
        let transfer = CreditTransfer {
            id: TransferId::from_transfer(&sender, &local_id, 100, 12345),
            from: sender,
            to: local_id, // We are the recipient
            amount: 100,
            nonce: 12345,
            timestamp: Timestamp::now(),
            memo: Some("test payment".to_string()),
            signature: Signature::empty(),
        };

        // Initial balance is 0
        assert_eq!(bridge.balance().await, Credits::zero());

        // Handle incoming transfer
        bridge
            .handle_credit_message(CreditMessage::Transfer(transfer))
            .await
            .unwrap();

        // Balance should be credited
        assert_eq!(bridge.balance().await, Credits::new(100));
    }

    #[tokio::test]
    async fn test_handle_credit_transfer_as_observer() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let sender = NodeId::from_bytes([2u8; 32]);
        let recipient = NodeId::from_bytes([3u8; 32]);

        // Pre-populate known balances
        {
            let mut balances = bridge.known_balances.write().await;
            balances.insert(sender, Credits::new(1000));
            balances.insert(recipient, Credits::new(500));
        }

        let transfer = CreditTransfer {
            id: TransferId::from_transfer(&sender, &recipient, 200, 12345),
            from: sender,
            to: recipient, // Someone else is the recipient
            amount: 200,
            nonce: 12345,
            timestamp: Timestamp::now(),
            memo: None,
            signature: Signature::empty(),
        };

        // Handle transfer as observer
        bridge
            .handle_credit_message(CreditMessage::Transfer(transfer))
            .await
            .unwrap();

        // Our balance unchanged
        assert_eq!(bridge.balance().await, Credits::zero());

        // Known balances should be updated
        let balances = bridge.known_balances.read().await;
        assert_eq!(balances.get(&sender), Some(&Credits::new(800)));
        assert_eq!(balances.get(&recipient), Some(&Credits::new(700)));
    }

    #[tokio::test]
    async fn test_handle_credit_confirmation() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let mut bridge = EnrBridge::new(local_id, config);

        let publish_fn: PublishFn = Arc::new(|_topic, _data| Ok(()));
        bridge.connect_publisher(publish_fn);
        bridge.set_balance(Credits::new(1000)).await;

        let recipient = NodeId::from_bytes([2u8; 32]);

        // Make a transfer (creates pending entry)
        let transfer_id = bridge.transfer(recipient, Credits::new(100)).await.unwrap();

        // Verify pending transfer exists
        {
            let pending = bridge.pending_transfers.read().await;
            assert!(pending.contains_key(&transfer_id));
        }

        // Handle confirmation
        let confirmation = TransferConfirmation {
            transfer_id,
            confirmer: recipient,
            timestamp: Timestamp::now(),
            signature: Signature::empty(),
        };

        bridge
            .handle_credit_message(CreditMessage::Confirmation(confirmation))
            .await
            .unwrap();

        // Pending transfer should be removed
        let pending = bridge.pending_transfers.read().await;
        assert!(!pending.contains_key(&transfer_id));
    }

    #[tokio::test]
    async fn test_handle_credit_state_sync() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let other_node = NodeId::from_bytes([2u8; 32]);

        let sync = CreditStateSync {
            node_id: other_node,
            balance: 5000,
            version: 1,
            timestamp: Timestamp::now(),
            signature: Signature::empty(),
        };

        bridge
            .handle_credit_message(CreditMessage::StateSync(sync))
            .await
            .unwrap();

        let balances = bridge.known_balances.read().await;
        assert_eq!(balances.get(&other_node), Some(&Credits::new(5000)));
    }

    #[tokio::test]
    async fn test_handle_balance_query() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let mut bridge = EnrBridge::new(local_id, config);

        let published = Arc::new(std::sync::Mutex::new(Vec::<(String, Vec<u8>)>::new()));
        let published_clone = published.clone();
        let publish_fn: PublishFn = Arc::new(move |topic, data| {
            let mut msgs = published_clone.lock().unwrap();
            msgs.push((topic.to_string(), data));
            Ok(())
        });
        bridge.connect_publisher(publish_fn);
        bridge.set_balance(Credits::new(1234)).await;

        let requester = NodeId::from_bytes([2u8; 32]);

        // Query for our balance
        bridge
            .handle_credit_message(CreditMessage::BalanceQuery {
                requester,
                target: local_id,
            })
            .await
            .unwrap();

        // Should have published a response
        let msgs = published.lock().unwrap();
        assert!(!msgs.is_empty());
        assert_eq!(msgs[0].0, EnrTopics::CREDIT);
    }

    #[tokio::test]
    async fn test_handle_balance_response() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let other_node = NodeId::from_bytes([2u8; 32]);

        bridge
            .handle_credit_message(CreditMessage::BalanceResponse {
                node_id: other_node,
                balance: 9999,
            })
            .await
            .unwrap();

        let balances = bridge.known_balances.read().await;
        assert_eq!(balances.get(&other_node), Some(&Credits::new(9999)));
    }

    // ========================================================================
    // Septal Message Handler Tests
    // ========================================================================

    #[tokio::test]
    async fn test_handle_failure_report() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let failing_node = NodeId::from_bytes([2u8; 32]);
        let reporter = NodeId::from_bytes([3u8; 32]);

        // Report 5 failures to trip the gate
        for _ in 0..5 {
            let report = FailureReport {
                reporter,
                failed_node: failing_node,
                failure_type: "timeout".to_string(),
                timestamp: Timestamp::now(),
                signature: Signature::empty(),
            };

            bridge
                .handle_septal_message(SeptalMessage::FailureReport(report))
                .await
                .unwrap();
        }

        // Node should be isolated
        assert!(bridge.is_isolated(&failing_node).await);
    }

    #[tokio::test]
    async fn test_handle_isolation_notice() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let bad_node = NodeId::from_bytes([2u8; 32]);

        let notice = IsolationNotice {
            isolated_node: bad_node,
            reason: "Byzantine behavior detected".to_string(),
            timestamp: Timestamp::now(),
        };

        bridge
            .handle_septal_message(SeptalMessage::Isolation(notice))
            .await
            .unwrap();

        // Node should be immediately isolated
        assert!(bridge.is_isolated(&bad_node).await);
        assert_eq!(bridge.gate_state(&bad_node).await, SeptalGateState::Closed);
    }

    #[tokio::test]
    async fn test_handle_healing_probe_as_target() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let mut bridge = EnrBridge::new(local_id, config);

        let published = Arc::new(std::sync::Mutex::new(Vec::<(String, Vec<u8>)>::new()));
        let published_clone = published.clone();
        let publish_fn: PublishFn = Arc::new(move |topic, data| {
            let mut msgs = published_clone.lock().unwrap();
            msgs.push((topic.to_string(), data));
            Ok(())
        });
        bridge.connect_publisher(publish_fn);

        let initiator = NodeId::from_bytes([2u8; 32]);
        let probe = HealingProbe {
            probe_id: [42u8; 32],
            initiator,
            target: local_id, // We are the target
            timestamp: Timestamp::now(),
        };

        bridge
            .handle_septal_message(SeptalMessage::HealingProbe(probe))
            .await
            .unwrap();

        // Should have published a response
        let msgs = published.lock().unwrap();
        assert!(!msgs.is_empty());
        assert_eq!(msgs[0].0, EnrTopics::SEPTAL);

        // Verify the response contains our node ID
        let response = EnrMessage::from_bytes(&msgs[0].1).unwrap();
        if let EnrMessage::Septal(SeptalMessage::HealingResponse(resp)) = response {
            assert_eq!(resp.responder, local_id);
            assert_eq!(resp.probe_id, [42u8; 32]);
        } else {
            panic!("Expected HealingResponse");
        }
    }

    #[tokio::test]
    async fn test_handle_healing_response_recovery() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let recovering_node = NodeId::from_bytes([2u8; 32]);

        // First, put the node in half-open state
        {
            let mut gates = bridge.septal_gates.write().await;
            let mut gate = SeptalGate::new(recovering_node);
            gate.state = SeptalGateState::HalfOpen;
            gates.insert(recovering_node, gate);
        }

        let response = HealingResponse {
            probe_id: [42u8; 32],
            responder: recovering_node,
            state: SeptalGateState::Open,
            timestamp: Timestamp::now(),
        };

        bridge
            .handle_septal_message(SeptalMessage::HealingResponse(response))
            .await
            .unwrap();

        // Node should be recovered (Open state)
        assert!(!bridge.is_isolated(&recovering_node).await);
        assert_eq!(
            bridge.gate_state(&recovering_node).await,
            SeptalGateState::Open
        );
    }

    #[tokio::test]
    async fn test_handle_recovery_notice() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let recovered_node = NodeId::from_bytes([2u8; 32]);

        // First isolate the node
        for _ in 0..5 {
            bridge.record_failure(recovered_node, "test").await;
        }
        assert!(bridge.is_isolated(&recovered_node).await);

        // Send recovery notice
        let notice = RecoveryNotice {
            recovered_node,
            timestamp: Timestamp::now(),
        };

        bridge
            .handle_septal_message(SeptalMessage::Recovery(notice))
            .await
            .unwrap();

        // Node should be recovered
        assert!(!bridge.is_isolated(&recovered_node).await);
    }

    // ========================================================================
    // Election Message Handler Tests
    // ========================================================================

    #[tokio::test]
    async fn test_handle_election_result() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let winner = NodeId::from_bytes([2u8; 32]);

        // Add winner to topology first
        {
            let mut topology = bridge.topology.write().await;
            topology.set_topology(
                winner,
                crate::nexus::NexusTopology {
                    node: winner,
                    role: crate::nexus::NexusRole::default(),
                    aggregated_gradient: ResourceGradient::default(),
                    leaf_count: 0,
                    last_election: Timestamp::now(),
                },
            );
        }

        let result = ElectionResult {
            election_id: [1u8; 32],
            winner,
            vote_count: 10,
            timestamp: Timestamp::now(),
        };

        bridge
            .handle_election_message(ElectionMessage::Result(result))
            .await
            .unwrap();

        // Winner should be promoted to Nexus role
        let topology = bridge.topology.read().await;
        let topo = topology.get_topology(&winner).unwrap();
        assert_eq!(topo.role.role_type, crate::nexus::NexusRoleType::Nexus);
    }

    #[tokio::test]
    async fn test_trigger_election() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let mut bridge = EnrBridge::new(local_id, config);

        let published = Arc::new(std::sync::Mutex::new(Vec::<(String, Vec<u8>)>::new()));
        let published_clone = published.clone();
        let publish_fn: PublishFn = Arc::new(move |topic, data| {
            let mut msgs = published_clone.lock().unwrap();
            msgs.push((topic.to_string(), data));
            Ok(())
        });
        bridge.connect_publisher(publish_fn);

        let announcement = bridge.trigger_election().await.unwrap();

        // Verify announcement details
        assert_eq!(announcement.initiator, local_id);
        assert_eq!(announcement.round, 1);

        // Verify published message
        let msgs = published.lock().unwrap();
        assert!(!msgs.is_empty());
        assert_eq!(msgs[0].0, EnrTopics::ELECTION);
    }

    // ========================================================================
    // Message Roundtrip Tests
    // ========================================================================

    #[tokio::test]
    async fn test_gradient_message_roundtrip() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let mut bridge = EnrBridge::new(local_id, config);

        let received = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
        let received_clone = received.clone();
        let publish_fn: PublishFn = Arc::new(move |_topic, data| {
            let mut msgs = received_clone.lock().unwrap();
            msgs.push(data);
            Ok(())
        });
        bridge.connect_publisher(publish_fn);

        // Update gradient (publishes message)
        let gradient = ResourceGradient {
            cpu_available: 0.42,
            memory_available: 0.84,
            ..Default::default()
        };
        bridge.update_gradient(gradient).await.unwrap();

        // Get published data
        let msgs = received.lock().unwrap();
        assert!(!msgs.is_empty());

        // Deserialize and handle as incoming message
        let msg = EnrMessage::from_bytes(&msgs[0]).unwrap();
        if let EnrMessage::Gradient(g) = msg {
            assert_eq!(g.node_id, local_id);
            assert!((g.gradient.cpu_available - 0.42).abs() < 0.001);
        } else {
            panic!("Expected GradientMessage");
        }
    }

    #[tokio::test]
    async fn test_credit_transfer_roundtrip() {
        let local_id = test_node_id();
        let recipient_id = NodeId::from_bytes([2u8; 32]);
        let config = EnrBridgeConfig::default();

        // Create sender bridge
        let mut sender = EnrBridge::new(local_id, config.clone());
        sender.set_balance(Credits::new(1000)).await;

        let received = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
        let received_clone = received.clone();
        let publish_fn: PublishFn = Arc::new(move |_topic, data| {
            let mut msgs = received_clone.lock().unwrap();
            msgs.push(data);
            Ok(())
        });
        sender.connect_publisher(publish_fn);

        // Create recipient bridge
        let mut recipient = EnrBridge::new(recipient_id, config);
        let publish_fn2: PublishFn = Arc::new(|_topic, _data| Ok(()));
        recipient.connect_publisher(publish_fn2);

        // Sender initiates transfer
        let _transfer_id = sender.transfer(recipient_id, Credits::new(250)).await.unwrap();

        // Verify sender balance deducted
        assert_eq!(sender.balance().await, Credits::new(750));

        // Simulate network delivery to recipient
        let msgs = received.lock().unwrap();
        assert!(!msgs.is_empty());

        let msg = EnrMessage::from_bytes(&msgs[0]).unwrap();
        if let EnrMessage::Credit(credit_msg) = msg {
            recipient.handle_credit_message(credit_msg).await.unwrap();
        } else {
            panic!("Expected CreditMessage");
        }

        // Verify recipient balance credited
        assert_eq!(recipient.balance().await, Credits::new(250));
    }

    // ========================================================================
    // Message Expiration Tests
    // ========================================================================

    #[tokio::test]
    async fn test_expired_gradient_message_rejected() {
        let local_id = test_node_id();
        let mut config = EnrBridgeConfig::default();
        config.max_message_age = Duration::from_secs(60);
        let bridge = EnrBridge::new(local_id, config);

        let sender = NodeId::from_bytes([2u8; 32]);

        // Create a message with old timestamp (2 minutes ago)
        let old_timestamp = Timestamp::new(Timestamp::now().millis - 120_000);

        let gradient = GradientMessage {
            node_id: sender,
            gradient: GradientPayload::from(&ResourceGradient::default()),
            timestamp: old_timestamp,
            signature: Signature::empty(),
        };

        let msg = EnrMessage::Gradient(gradient);
        let data = msg.to_bytes().unwrap();

        // Should reject expired message
        let result = bridge.handle_message(EnrTopics::GRADIENT, &data).await;
        assert!(matches!(result, Err(BridgeError::MessageExpired)));
    }

    #[tokio::test]
    async fn test_fresh_gradient_message_accepted() {
        let local_id = test_node_id();
        let mut config = EnrBridgeConfig::default();
        config.max_message_age = Duration::from_secs(60);
        let bridge = EnrBridge::new(local_id, config);

        let sender = NodeId::from_bytes([2u8; 32]);

        let gradient = GradientMessage {
            node_id: sender,
            gradient: GradientPayload {
                cpu_available: 0.5,
                ..Default::default()
            },
            timestamp: Timestamp::now(),
            signature: Signature::empty(),
        };

        let msg = EnrMessage::Gradient(gradient);
        let data = msg.to_bytes().unwrap();

        // Should accept fresh message
        let result = bridge.handle_message(EnrTopics::GRADIENT, &data).await;
        assert!(result.is_ok());

        // Verify gradient was stored
        let gradients = bridge.gradients.read().await;
        assert!(gradients.contains_key(&sender));
    }

    // ========================================================================
    // Edge Case Tests
    // ========================================================================

    #[tokio::test]
    async fn test_publish_without_connection() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        let msg = EnrMessage::Gradient(GradientMessage {
            node_id: local_id,
            gradient: GradientPayload::from(&ResourceGradient::default()),
            timestamp: Timestamp::now(),
            signature: Signature::empty(),
        });

        let result = bridge.publish(msg);
        assert!(matches!(result, Err(BridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_start_broadcast_without_connection() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let mut bridge = EnrBridge::new(local_id, config);

        let result = bridge.start_gradient_broadcast().await;
        assert!(matches!(result, Err(BridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_multiple_transfers_balance_consistency() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let mut bridge = EnrBridge::new(local_id, config);

        let publish_fn: PublishFn = Arc::new(|_topic, _data| Ok(()));
        bridge.connect_publisher(publish_fn);
        bridge.set_balance(Credits::new(1000)).await;

        let recipients: Vec<NodeId> = (2..=5)
            .map(|i| NodeId::from_bytes([i as u8; 32]))
            .collect();

        // Make multiple transfers
        for recipient in &recipients {
            bridge.transfer(*recipient, Credits::new(100)).await.unwrap();
        }

        // Balance should be consistent
        assert_eq!(bridge.balance().await, Credits::new(600)); // 1000 - 4*100

        // Pending transfers should exist
        let pending = bridge.pending_transfers.read().await;
        assert_eq!(pending.len(), 4);
    }

    #[tokio::test]
    async fn test_invalid_message_deserialization() {
        let local_id = test_node_id();
        let config = EnrBridgeConfig::default();
        let bridge = EnrBridge::new(local_id, config);

        // Random garbage data
        let invalid_data = vec![0xFF, 0xFE, 0x00, 0x01, 0x02];

        let result = bridge.handle_message(EnrTopics::GRADIENT, &invalid_data).await;
        assert!(matches!(result, Err(BridgeError::Deserialization(_))));
    }

    #[test]
    fn test_transfer_id_uniqueness() {
        let from = NodeId::from_bytes([1u8; 32]);
        let to = NodeId::from_bytes([2u8; 32]);

        // Same parameters, same nonce -> same ID
        let id1 = TransferId::from_transfer(&from, &to, 100, 12345);
        let id2 = TransferId::from_transfer(&from, &to, 100, 12345);
        assert_eq!(id1, id2);

        // Different nonce -> different ID
        let id3 = TransferId::from_transfer(&from, &to, 100, 12346);
        assert_ne!(id1, id3);

        // Different amount -> different ID
        let id4 = TransferId::from_transfer(&from, &to, 101, 12345);
        assert_ne!(id1, id4);

        // Different sender -> different ID
        let from2 = NodeId::from_bytes([3u8; 32]);
        let id5 = TransferId::from_transfer(&from2, &to, 100, 12345);
        assert_ne!(id1, id5);
    }
}

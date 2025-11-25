//! Enhanced pass-through node implementation.
//!
//! The `PassThroughNode` forwards events with optional transformation,
//! filtering, and monitoring capabilities.

use crate::event::{Event, EventPayload};
use crate::node::Node;
use crate::types::{NodeId, SimTime};

/// Filter mode for the pass-through node.
#[derive(Clone, Debug, Default)]
pub enum FilterMode {
    /// Pass all events through
    #[default]
    PassAll,
    /// Only pass events matching a key-value in the data
    MatchKey { key: String, value: serde_json::Value },
    /// Only pass every Nth event
    Sample { rate: usize },
    /// Drop events randomly with given probability (0.0-1.0)
    DropRandom { probability: f64 },
}

/// Transform mode for the pass-through node.
#[derive(Clone, Debug, Default)]
pub enum TransformMode {
    /// No transformation
    #[default]
    None,
    /// Add a timestamp field
    AddTimestamp,
    /// Add a sequence number
    AddSequence,
    /// Add both timestamp and sequence
    AddMetadata,
    /// Wrap data in a container with node info
    Wrap,
}

/// An enhanced pass-through node with filtering and transformation.
///
/// This node can:
/// - Forward events to a target node
/// - Filter events based on various criteria
/// - Transform event data before forwarding
/// - Collect statistics on event flow
///
/// # Example
///
/// ```rust
/// use akiku::nodes::passthrough::{EnhancedPassThroughNode, FilterMode, TransformMode};
/// use akiku::node::Node;
///
/// // Create a sampling pass-through (every 10th event)
/// let mut node = EnhancedPassThroughNode::new(1)
///     .with_forward(2, "in")
///     .with_filter(FilterMode::Sample { rate: 10 })
///     .with_transform(TransformMode::AddMetadata);
///
/// node.init();
/// ```
#[derive(Debug)]
pub struct EnhancedPassThroughNode {
    /// The node's unique identifier
    pub id: NodeId,
    /// Target node for forwarding
    forward_to: Option<NodeId>,
    /// Port on the target node
    forward_port: String,
    /// Filter mode
    filter: FilterMode,
    /// Transform mode
    transform: TransformMode,
    /// Random seed for probabilistic operations
    seed: u64,
    /// Event counter
    event_counter: u64,
    /// Sequence number for AddSequence transform
    sequence: u64,

    // Statistics
    /// Total events received
    pub events_received: u64,
    /// Events passed through
    pub events_passed: u64,
    /// Events filtered out
    pub events_filtered: u64,
}

impl EnhancedPassThroughNode {
    /// Creates a new enhanced pass-through node.
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            forward_to: None,
            forward_port: "in".to_string(),
            filter: FilterMode::default(),
            transform: TransformMode::default(),
            seed: 12345,
            event_counter: 0,
            sequence: 0,

            events_received: 0,
            events_passed: 0,
            events_filtered: 0,
        }
    }

    /// Sets the target node for forwarding.
    pub fn with_forward(mut self, target: NodeId, port: impl Into<String>) -> Self {
        self.forward_to = Some(target);
        self.forward_port = port.into();
        self
    }

    /// Sets the filter mode.
    pub fn with_filter(mut self, filter: FilterMode) -> Self {
        self.filter = filter;
        self
    }

    /// Sets the transform mode.
    pub fn with_transform(mut self, transform: TransformMode) -> Self {
        self.transform = transform;
        self
    }

    /// Sets the random seed.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Returns statistics about the node.
    pub fn stats(&self) -> serde_json::Value {
        let pass_rate = if self.events_received > 0 {
            self.events_passed as f64 / self.events_received as f64
        } else {
            0.0
        };

        serde_json::json!({
            "id": self.id,
            "events_received": self.events_received,
            "events_passed": self.events_passed,
            "events_filtered": self.events_filtered,
            "pass_rate": pass_rate,
        })
    }

    /// Checks if an event passes the filter.
    fn passes_filter(&mut self, event: &Event) -> bool {
        // Clone filter to avoid borrow issues
        let filter = self.filter.clone();
        match filter {
            FilterMode::PassAll => true,
            FilterMode::MatchKey { ref key, ref value } => {
                if let EventPayload::Message { data, .. } = &event.payload {
                    data.get(key).map_or(false, |v| v == value)
                } else {
                    false
                }
            }
            FilterMode::Sample { rate } => {
                self.event_counter % (rate as u64) == 0
            }
            FilterMode::DropRandom { probability } => {
                let random = self.simple_random();
                let threshold = (probability * u64::MAX as f64) as u64;
                random > threshold
            }
        }
    }

    /// Transforms the event data.
    fn transform_data(&mut self, time: SimTime, data: serde_json::Value) -> serde_json::Value {
        match &self.transform {
            TransformMode::None => data,
            TransformMode::AddTimestamp => {
                let mut obj = data.as_object().cloned().unwrap_or_default();
                obj.insert("_timestamp".to_string(), serde_json::json!(time));
                serde_json::Value::Object(obj)
            }
            TransformMode::AddSequence => {
                let mut obj = data.as_object().cloned().unwrap_or_default();
                obj.insert("_seq".to_string(), serde_json::json!(self.sequence));
                self.sequence += 1;
                serde_json::Value::Object(obj)
            }
            TransformMode::AddMetadata => {
                let mut obj = data.as_object().cloned().unwrap_or_default();
                obj.insert("_timestamp".to_string(), serde_json::json!(time));
                obj.insert("_seq".to_string(), serde_json::json!(self.sequence));
                obj.insert("_node".to_string(), serde_json::json!(self.id));
                self.sequence += 1;
                serde_json::Value::Object(obj)
            }
            TransformMode::Wrap => {
                serde_json::json!({
                    "source_node": self.id,
                    "timestamp": time,
                    "sequence": self.sequence,
                    "payload": data,
                })
            }
        }
    }

    fn simple_random(&mut self) -> u64 {
        let mut x = self.seed.wrapping_add(self.event_counter);
        if x == 0 {
            x = 0xDEADBEEF;
        }
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    }
}

impl Node for EnhancedPassThroughNode {
    fn init(&mut self) {
        self.event_counter = 0;
        self.sequence = 0;
        self.events_received = 0;
        self.events_passed = 0;
        self.events_filtered = 0;
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        Vec::new()
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        self.events_received += 1;
        self.event_counter += 1;

        if !self.passes_filter(&event) {
            self.events_filtered += 1;
            return Vec::new();
        }

        if let Some(target) = self.forward_to {
            self.events_passed += 1;

            let data = match &event.payload {
                EventPayload::Message { data, .. } => data.clone(),
                EventPayload::Custom(s) => serde_json::json!({ "custom": s }),
            };

            let transformed = self.transform_data(event.time, data);

            vec![Event::message(
                event.time,
                self.id,
                "out",
                target,
                &self.forward_port,
                transformed,
            )]
        } else {
            Vec::new()
        }
    }
}

/// A simple pass-through that just forwards events.
///
/// This is a minimal version for cases where no filtering or
/// transformation is needed.
#[derive(Debug, Default)]
pub struct SimplePassThrough {
    /// The node's unique identifier
    pub id: NodeId,
    /// Target node for forwarding
    pub forward_to: Option<NodeId>,
    /// Port on the target node  
    pub forward_port: String,
    /// Events forwarded
    pub count: u64,
}

impl SimplePassThrough {
    /// Creates a new simple pass-through.
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            forward_to: None,
            forward_port: "in".to_string(),
            count: 0,
        }
    }

    /// Sets the forwarding target.
    pub fn with_forward(mut self, target: NodeId, port: impl Into<String>) -> Self {
        self.forward_to = Some(target);
        self.forward_port = port.into();
        self
    }
}

impl Node for SimplePassThrough {
    fn init(&mut self) {
        self.count = 0;
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        Vec::new()
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        if let Some(target) = self.forward_to {
            self.count += 1;

            let data = match &event.payload {
                EventPayload::Message { data, .. } => data.clone(),
                EventPayload::Custom(s) => serde_json::json!({ "custom": s }),
            };

            vec![Event::message(
                event.time,
                self.id,
                "out",
                target,
                &self.forward_port,
                data,
            )]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enhanced_passthrough_creation() {
        let node = EnhancedPassThroughNode::new(1);
        assert_eq!(node.id, 1);
        assert!(node.forward_to.is_none());
    }

    #[test]
    fn test_enhanced_passthrough_pass_all() {
        let mut node = EnhancedPassThroughNode::new(1).with_forward(2, "in");
        node.init();

        for i in 0..5 {
            let event = Event::message(i, 0, "out", 1, "in", serde_json::json!({}));
            let output = node.on_event(event);
            assert_eq!(output.len(), 1);
        }

        assert_eq!(node.events_received, 5);
        assert_eq!(node.events_passed, 5);
        assert_eq!(node.events_filtered, 0);
    }

    #[test]
    fn test_enhanced_passthrough_sample_filter() {
        let mut node = EnhancedPassThroughNode::new(1)
            .with_forward(2, "in")
            .with_filter(FilterMode::Sample { rate: 3 });
        node.init();

        let mut passed = 0;
        for i in 0..9 {
            let event = Event::message(i, 0, "out", 1, "in", serde_json::json!({}));
            let output = node.on_event(event);
            passed += output.len();
        }

        assert_eq!(passed, 3); // 0, 3, 6
        assert_eq!(node.events_received, 9);
        assert_eq!(node.events_passed, 3);
        assert_eq!(node.events_filtered, 6);
    }

    #[test]
    fn test_enhanced_passthrough_match_filter() {
        let mut node = EnhancedPassThroughNode::new(1)
            .with_forward(2, "in")
            .with_filter(FilterMode::MatchKey {
                key: "type".to_string(),
                value: serde_json::json!("important"),
            });
        node.init();

        // Should pass
        let event = Event::message(0, 0, "out", 1, "in", serde_json::json!({"type": "important"}));
        assert_eq!(node.on_event(event).len(), 1);

        // Should filter
        let event = Event::message(1, 0, "out", 1, "in", serde_json::json!({"type": "normal"}));
        assert_eq!(node.on_event(event).len(), 0);
    }

    #[test]
    fn test_enhanced_passthrough_transform() {
        let mut node = EnhancedPassThroughNode::new(1)
            .with_forward(2, "in")
            .with_transform(TransformMode::AddMetadata);
        node.init();

        let event = Event::message(100, 0, "out", 1, "in", serde_json::json!({"data": 42}));
        let output = node.on_event(event);

        assert_eq!(output.len(), 1);
        if let EventPayload::Message { data, .. } = &output[0].payload {
            assert_eq!(data["data"], 42);
            assert_eq!(data["_timestamp"], 100);
            assert_eq!(data["_seq"], 0);
            assert_eq!(data["_node"], 1);
        }
    }

    #[test]
    fn test_simple_passthrough() {
        let mut node = SimplePassThrough::new(1).with_forward(2, "in");
        node.init();

        for i in 0..3 {
            let event = Event::message(i, 0, "out", 1, "in", serde_json::json!({}));
            let output = node.on_event(event);
            assert_eq!(output.len(), 1);
        }

        assert_eq!(node.count, 3);
    }

    #[test]
    fn test_stats() {
        let mut node = EnhancedPassThroughNode::new(1)
            .with_forward(2, "in")
            .with_filter(FilterMode::Sample { rate: 2 });
        node.init();

        for i in 0..10 {
            let event = Event::message(i, 0, "out", 1, "in", serde_json::json!({}));
            node.on_event(event);
        }

        let stats = node.stats();
        assert_eq!(stats["events_received"], 10);
        assert_eq!(stats["events_passed"], 5);
        assert_eq!(stats["events_filtered"], 5);
        assert_eq!(stats["pass_rate"], 0.5);
    }
}

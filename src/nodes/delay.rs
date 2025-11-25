//! Fixed delay node implementation.
//!
//! The `DelayNode` adds a configurable fixed delay to incoming events
//! before forwarding them to the next stage.

use crate::event::{Event, EventPayload};
use crate::node::Node;
use crate::types::{NodeId, SimTime};

/// A node that adds a fixed delay to events.
///
/// When an event arrives, the `DelayNode` creates a new event with the same
/// payload but scheduled for `current_time + delay`. This is useful for
/// modeling fixed-latency components like memory access times or network hops.
///
/// # Example
///
/// ```rust
/// use akiku::nodes::delay::DelayNode;
/// use akiku::node::Node;
/// use akiku::Event;
///
/// // Create a delay node with 10ns delay, forwarding to node 2
/// let mut node = DelayNode::new(1, 10).with_forward(2, "in");
///
/// // Initialize the node
/// node.init();
///
/// // Process an incoming event
/// let event = Event::message(100, 0, "out", 1, "in", serde_json::json!({"data": 42}));
/// let output = node.on_event(event);
///
/// // The output event is scheduled at time 110 (100 + 10)
/// assert_eq!(output.len(), 1);
/// assert_eq!(output[0].time, 110);
/// ```
#[derive(Debug)]
pub struct DelayNode {
    /// The node's unique identifier
    pub id: NodeId,
    /// The fixed delay to add to events
    pub delay: SimTime,
    /// Target node ID to forward events to
    pub forward_to: Option<NodeId>,
    /// Port name on the target node
    pub forward_port: String,
    /// Number of events processed
    pub events_processed: u64,
    /// Number of events forwarded
    pub events_forwarded: u64,
}

impl DelayNode {
    /// Creates a new delay node with the specified delay.
    ///
    /// # Arguments
    /// * `id` - The node's unique identifier
    /// * `delay` - The fixed delay in simulation time units
    pub fn new(id: NodeId, delay: SimTime) -> Self {
        Self {
            id,
            delay,
            forward_to: None,
            forward_port: "in".to_string(),
            events_processed: 0,
            events_forwarded: 0,
        }
    }

    /// Sets the target node for forwarding events.
    ///
    /// # Arguments
    /// * `target` - The target node ID
    /// * `port` - The port name on the target node
    pub fn with_forward(mut self, target: NodeId, port: impl Into<String>) -> Self {
        self.forward_to = Some(target);
        self.forward_port = port.into();
        self
    }

    /// Returns statistics about the node's operation.
    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "delay": self.delay,
            "events_processed": self.events_processed,
            "events_forwarded": self.events_forwarded,
        })
    }
}

impl Node for DelayNode {
    fn init(&mut self) {
        self.events_processed = 0;
        self.events_forwarded = 0;
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        // DelayNode is primarily event-driven, no tick behavior
        Vec::new()
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        self.events_processed += 1;

        if let Some(target) = self.forward_to {
            self.events_forwarded += 1;

            // Extract data from the incoming event
            let data = match &event.payload {
                EventPayload::Message { data, .. } => data.clone(),
                EventPayload::Custom(s) => serde_json::json!({ "custom": s }),
            };

            // Create delayed event
            vec![Event::message(
                event.time + self.delay,
                self.id,
                "out",
                target,
                &self.forward_port,
                data,
            )]
        } else {
            // No forwarding target, event is consumed
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_node_creation() {
        let node = DelayNode::new(1, 10);
        assert_eq!(node.id, 1);
        assert_eq!(node.delay, 10);
        assert!(node.forward_to.is_none());
    }

    #[test]
    fn test_delay_node_with_forward() {
        let node = DelayNode::new(1, 10).with_forward(2, "input");
        assert_eq!(node.forward_to, Some(2));
        assert_eq!(node.forward_port, "input");
    }

    #[test]
    fn test_delay_node_event_processing() {
        let mut node = DelayNode::new(1, 15).with_forward(2, "in");
        node.init();

        let event = Event::message(100, 0, "out", 1, "in", serde_json::json!({"value": 42}));
        let output = node.on_event(event);

        assert_eq!(output.len(), 1);
        assert_eq!(output[0].time, 115); // 100 + 15
        assert_eq!(node.events_processed, 1);
        assert_eq!(node.events_forwarded, 1);
    }

    #[test]
    fn test_delay_node_no_forward() {
        let mut node = DelayNode::new(1, 10);
        node.init();

        let event = Event::message(100, 0, "out", 1, "in", serde_json::json!({}));
        let output = node.on_event(event);

        assert!(output.is_empty());
        assert_eq!(node.events_processed, 1);
        assert_eq!(node.events_forwarded, 0);
    }

    #[test]
    fn test_delay_node_stats() {
        let mut node = DelayNode::new(1, 20).with_forward(2, "in");
        node.init();

        // Process some events
        for i in 0..5 {
            let event = Event::message(i * 10, 0, "out", 1, "in", serde_json::json!({}));
            node.on_event(event);
        }

        let stats = node.stats();
        assert_eq!(stats["events_processed"], 5);
        assert_eq!(stats["events_forwarded"], 5);
    }
}

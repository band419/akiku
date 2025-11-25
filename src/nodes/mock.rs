//! Mock node implementations for testing.
//!
//! These nodes provide simple, predictable behaviors useful for
//! testing the simulation framework.

use crate::event::{Event, EventPayload};
use crate::node::Node;
use crate::types::{NodeId, SimTime};

/// A simple counter node that counts tick invocations.
///
/// Useful for verifying that tick-driven execution is working correctly.
#[derive(Debug, Default)]
pub struct CounterNode {
    /// The node's unique identifier
    pub id: NodeId,
    /// Number of ticks processed
    pub tick_count: u64,
    /// Number of events processed
    pub event_count: u64,
}

impl CounterNode {
    /// Creates a new counter node with the given ID.
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            tick_count: 0,
            event_count: 0,
        }
    }
}

impl Node for CounterNode {
    fn init(&mut self) {
        self.tick_count = 0;
        self.event_count = 0;
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        self.tick_count += 1;
        Vec::new()
    }

    fn on_event(&mut self, _event: Event) -> Vec<Event> {
        self.event_count += 1;
        Vec::new()
    }
}

/// A source node that generates events at each tick.
///
/// Useful for testing event generation and propagation.
#[derive(Debug)]
pub struct SourceNode {
    /// The node's unique identifier
    pub id: NodeId,
    /// Target node ID to send events to
    pub target_id: NodeId,
    /// Port name on the target node
    pub target_port: String,
    /// Counter for generated events
    pub generated_count: u64,
}

impl SourceNode {
    /// Creates a new source node.
    pub fn new(id: NodeId, target_id: NodeId, target_port: impl Into<String>) -> Self {
        Self {
            id,
            target_id,
            target_port: target_port.into(),
            generated_count: 0,
        }
    }
}

impl Node for SourceNode {
    fn init(&mut self) {
        self.generated_count = 0;
    }

    fn on_tick(&mut self, time: SimTime) -> Vec<Event> {
        self.generated_count += 1;
        vec![Event::message(
            time,
            self.id,
            "out",
            self.target_id,
            &self.target_port,
            serde_json::json!({
                "seq": self.generated_count,
                "time": time,
            }),
        )]
    }
}

/// An echo node that returns received events with a delay.
///
/// Useful for testing event-driven execution and latency modeling.
#[derive(Debug)]
pub struct EchoNode {
    /// The node's unique identifier
    pub id: NodeId,
    /// Delay to add to echoed events
    pub delay: SimTime,
    /// Number of events echoed
    pub echo_count: u64,
}

impl EchoNode {
    /// Creates a new echo node with the given delay.
    pub fn new(id: NodeId, delay: SimTime) -> Self {
        Self {
            id,
            delay,
            echo_count: 0,
        }
    }
}

impl Node for EchoNode {
    fn init(&mut self) {
        self.echo_count = 0;
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        self.echo_count += 1;

        // Echo the event back with added delay
        if let EventPayload::Message { src, data, .. } = event.payload {
            vec![Event::message(
                event.time + self.delay,
                self.id,
                "echo_out",
                src.0,
                &src.1,
                data,
            )]
        } else {
            Vec::new()
        }
    }
}

/// A pass-through node that forwards events unchanged.
///
/// Useful for testing event routing and pipeline construction.
#[derive(Debug)]
pub struct PassThroughNode {
    /// The node's unique identifier
    pub id: NodeId,
    /// Target node ID to forward events to
    pub target_id: NodeId,
    /// Port name on the target node
    pub target_port: String,
    /// Number of events forwarded
    pub forward_count: u64,
}

impl PassThroughNode {
    /// Creates a new pass-through node.
    pub fn new(id: NodeId, target_id: NodeId, target_port: impl Into<String>) -> Self {
        Self {
            id,
            target_id,
            target_port: target_port.into(),
            forward_count: 0,
        }
    }
}

impl Node for PassThroughNode {
    fn init(&mut self) {
        self.forward_count = 0;
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        self.forward_count += 1;

        if let EventPayload::Message { data, .. } = event.payload {
            vec![Event::message(
                event.time,
                self.id,
                "out",
                self.target_id,
                &self.target_port,
                data,
            )]
        } else {
            Vec::new()
        }
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        // Pass-through does nothing on tick
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_node_tick() {
        let mut node = CounterNode::new(1);
        node.init();

        assert_eq!(node.tick_count, 0);

        node.on_tick(10);
        node.on_tick(20);
        node.on_tick(30);

        assert_eq!(node.tick_count, 3);
    }

    #[test]
    fn test_counter_node_event() {
        let mut node = CounterNode::new(1);
        node.init();

        assert_eq!(node.event_count, 0);

        node.on_event(Event::custom(10, "test1"));
        node.on_event(Event::custom(20, "test2"));

        assert_eq!(node.event_count, 2);
    }

    #[test]
    fn test_source_node() {
        let mut node = SourceNode::new(1, 2, "input");
        node.init();

        let events = node.on_tick(100);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].time, 100);
        assert_eq!(node.generated_count, 1);

        if let EventPayload::Message { src, dst, data } = &events[0].payload {
            assert_eq!(src.0, 1);
            assert_eq!(dst.0, 2);
            assert_eq!(dst.1, "input");
            assert_eq!(data["seq"], 1);
        } else {
            panic!("Expected Message payload");
        }
    }

    #[test]
    fn test_echo_node() {
        let mut node = EchoNode::new(2, 50);
        node.init();

        let incoming = Event::message(100, 1, "out", 2, "in", serde_json::json!({"data": "hello"}));

        let events = node.on_event(incoming);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].time, 150); // 100 + 50 delay
        assert_eq!(node.echo_count, 1);

        if let EventPayload::Message { src, dst, data } = &events[0].payload {
            assert_eq!(src.0, 2); // Echo from node 2
            assert_eq!(dst.0, 1); // Back to original sender
            assert_eq!(data["data"], "hello");
        } else {
            panic!("Expected Message payload");
        }
    }

    #[test]
    fn test_pass_through_node() {
        let mut node = PassThroughNode::new(2, 3, "next_input");
        node.init();

        let incoming = Event::message(100, 1, "out", 2, "in", serde_json::json!({"value": 42}));

        let events = node.on_event(incoming);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].time, 100); // Same time, no delay
        assert_eq!(node.forward_count, 1);

        if let EventPayload::Message { src, dst, data } = &events[0].payload {
            assert_eq!(src.0, 2);
            assert_eq!(dst.0, 3);
            assert_eq!(dst.1, "next_input");
            assert_eq!(data["value"], 42);
        } else {
            panic!("Expected Message payload");
        }
    }
}


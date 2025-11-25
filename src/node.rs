//! Node definitions and the `Node` trait.
//!
//! Nodes are the fundamental units of computation in the simulation graph.
//! Each node is a local state machine that responds to input changes/events
//! and produces outputs/new events.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::event::Event;
use crate::types::{NodeId, SimTime};

/// The kind/type of a node, indicating what model it represents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// RTL block (register-transfer level)
    RtlBlock,
    /// Behavioral model
    Behavioral,
    /// Truth table based model
    TruthTable,
    /// Probabilistic/statistical model
    Probabilistic,
    /// Custom node type with a string identifier
    Custom(String),
}

/// Describes an input or output port of a node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodePort {
    /// Name of the port (e.g., "clk", "data_in", "ready")
    pub name: String,
    /// Type descriptor for the port (e.g., "u32", "bool", "Packet")
    pub ty: String,
}

impl NodePort {
    /// Creates a new `NodePort` with the given name and type.
    pub fn new(name: impl Into<String>, ty: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ty: ty.into(),
        }
    }
}

/// Static description of a node in the simulation graph.
///
/// This describes the node's identity, type, attributes, and interface (ports).
/// The actual behavior is provided by implementing the `Node` trait.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeDesc {
    /// Unique identifier for this node
    pub id: NodeId,
    /// The kind of model this node represents
    pub kind: NodeKind,
    /// Additional attributes as key-value pairs
    pub attrs: HashMap<String, String>,
    /// Input ports
    pub inputs: Vec<NodePort>,
    /// Output ports
    pub outputs: Vec<NodePort>,
}

impl NodeDesc {
    /// Creates a new `NodeDesc` with the given id and kind.
    pub fn new(id: NodeId, kind: NodeKind) -> Self {
        Self {
            id,
            kind,
            attrs: HashMap::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    /// Adds an attribute to this node description.
    pub fn with_attr(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attrs.insert(key.into(), value.into());
        self
    }

    /// Adds an input port to this node description.
    pub fn with_input(mut self, port: NodePort) -> Self {
        self.inputs.push(port);
        self
    }

    /// Adds an output port to this node description.
    pub fn with_output(mut self, port: NodePort) -> Self {
        self.outputs.push(port);
        self
    }
}

/// The core trait that all simulation nodes must implement.
///
/// A node is a local state machine that can operate in two modes:
/// - **Tick-Driven**: Called at each tick boundary via `on_tick`
/// - **Event-Driven**: Called when events arrive via `on_event`
///
/// Nodes may implement one or both methods depending on the subgraph's time mode.
pub trait Node: Send {
    /// Initialize the node state.
    ///
    /// Called once before simulation starts.
    fn init(&mut self) {}

    /// Called at each tick boundary in Tick-Driven mode.
    ///
    /// # Arguments
    /// * `time` - The current simulation time at this tick boundary
    ///
    /// # Returns
    /// A vector of events to be sent (possibly to other subgraphs)
    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        Vec::new()
    }

    /// Called when an event arrives in Event-Driven mode.
    ///
    /// # Arguments
    /// * `event` - The incoming event to process
    ///
    /// # Returns
    /// A vector of new events generated in response
    fn on_event(&mut self, _event: Event) -> Vec<Event> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_kind() {
        let kind = NodeKind::RtlBlock;
        assert_eq!(kind, NodeKind::RtlBlock);

        let custom = NodeKind::Custom("MyModel".to_string());
        if let NodeKind::Custom(name) = custom {
            assert_eq!(name, "MyModel");
        }
    }

    #[test]
    fn test_node_port() {
        let port = NodePort::new("data_in", "u64");
        assert_eq!(port.name, "data_in");
        assert_eq!(port.ty, "u64");
    }

    #[test]
    fn test_node_desc() {
        let desc = NodeDesc::new(1, NodeKind::Behavioral)
            .with_attr("latency", "10")
            .with_input(NodePort::new("req", "Request"))
            .with_output(NodePort::new("resp", "Response"));

        assert_eq!(desc.id, 1);
        assert_eq!(desc.kind, NodeKind::Behavioral);
        assert_eq!(desc.attrs.get("latency"), Some(&"10".to_string()));
        assert_eq!(desc.inputs.len(), 1);
        assert_eq!(desc.outputs.len(), 1);
    }

    struct MockNode {
        counter: u64,
    }

    impl Node for MockNode {
        fn init(&mut self) {
            self.counter = 0;
        }

        fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
            self.counter += 1;
            Vec::new()
        }
    }

    #[test]
    fn test_node_trait() {
        let mut node = MockNode { counter: 0 };
        node.init();
        assert_eq!(node.counter, 0);

        node.on_tick(100);
        assert_eq!(node.counter, 1);
    }
}


//! Core type definitions for the simulation framework.
//!
//! This module defines the fundamental types used throughout the simulation engine.

/// Simulation time unit (e.g., nanoseconds or global cycles).
///
/// All events and tick boundaries use the same `SimTime` representation,
/// enabling a unified timeline across different subgraphs.
pub type SimTime = u64;

/// Unique identifier for a node in the simulation graph.
///
/// Each node represents a logical unit such as an RTL module, pipeline stage,
/// cache, DRAM controller, or statistical model.
pub type NodeId = u64;

/// Unique identifier for a subgraph.
///
/// Subgraphs partition the global simulation graph and can run in parallel
/// with communication through message channels.
pub type SubgraphId = u32;

/// Port identifier type.
///
/// Used to identify specific input/output ports on a node.
pub type PortId = String;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_aliases() {
        let time: SimTime = 1000;
        let node_id: NodeId = 42;
        let subgraph_id: SubgraphId = 1;
        let port_id: PortId = "input_0".to_string();

        assert_eq!(time, 1000);
        assert_eq!(node_id, 42);
        assert_eq!(subgraph_id, 1);
        assert_eq!(port_id, "input_0");
    }
}


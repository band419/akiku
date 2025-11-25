//! Subgraph definitions for the simulation framework.
//!
//! Subgraphs partition the global simulation graph into smaller units
//! that can potentially run in parallel, communicating via message channels.

use serde::{Deserialize, Serialize};

use crate::types::{NodeId, SimTime, SubgraphId};

/// The time advancement mode for a subgraph.
///
/// Each subgraph operates in one of two modes:
/// - **Tick**: Advances time in fixed increments (cycle-accurate)
/// - **Event**: Advances time based on events (discrete-event simulation)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeMode {
    /// Tick-driven mode with a fixed period.
    ///
    /// Suitable for cycle-accurate simulation of pipelines and microarchitecture.
    Tick {
        /// The tick period in SimTime units
        period: SimTime,
    },

    /// Event-driven mode.
    ///
    /// Suitable for:
    /// - Transaction-level models
    /// - Memory/network models
    /// - Statistical/probabilistic models
    /// - Trace-driven performance models
    Event,
}

impl TimeMode {
    /// Creates a new Tick mode with the given period.
    pub fn tick(period: SimTime) -> Self {
        TimeMode::Tick { period }
    }

    /// Creates a new Event mode.
    pub fn event() -> Self {
        TimeMode::Event
    }

    /// Returns the tick period if this is Tick mode, otherwise 0.
    pub fn tick_period(&self) -> SimTime {
        match self {
            TimeMode::Tick { period } => *period,
            TimeMode::Event => 0,
        }
    }

    /// Returns true if this is Tick mode.
    pub fn is_tick(&self) -> bool {
        matches!(self, TimeMode::Tick { .. })
    }

    /// Returns true if this is Event mode.
    pub fn is_event(&self) -> bool {
        matches!(self, TimeMode::Event)
    }
}

/// Describes a subgraph in the simulation.
///
/// A subgraph is a partition of the global simulation graph containing
/// a subset of nodes. Each subgraph has its own time advancement mode
/// and can run independently (potentially in parallel with other subgraphs).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubgraphDesc {
    /// Unique identifier for this subgraph
    pub id: SubgraphId,
    /// The time advancement mode (Tick or Event)
    pub time_mode: TimeMode,
    /// IDs of nodes contained in this subgraph
    pub nodes: Vec<NodeId>,
}

impl SubgraphDesc {
    /// Creates a new subgraph description.
    pub fn new(id: SubgraphId, time_mode: TimeMode) -> Self {
        Self {
            id,
            time_mode,
            nodes: Vec::new(),
        }
    }

    /// Creates a new Tick-driven subgraph.
    pub fn tick(id: SubgraphId, period: SimTime) -> Self {
        Self::new(id, TimeMode::tick(period))
    }

    /// Creates a new Event-driven subgraph.
    pub fn event(id: SubgraphId) -> Self {
        Self::new(id, TimeMode::event())
    }

    /// Adds a node to this subgraph.
    pub fn with_node(mut self, node_id: NodeId) -> Self {
        self.nodes.push(node_id);
        self
    }

    /// Adds multiple nodes to this subgraph.
    pub fn with_nodes(mut self, node_ids: impl IntoIterator<Item = NodeId>) -> Self {
        self.nodes.extend(node_ids);
        self
    }

    /// Returns the tick period if this is a Tick subgraph, otherwise 0.
    pub fn tick_period(&self) -> SimTime {
        self.time_mode.tick_period()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_mode_tick() {
        let mode = TimeMode::tick(10);
        assert!(mode.is_tick());
        assert!(!mode.is_event());
        assert_eq!(mode.tick_period(), 10);
    }

    #[test]
    fn test_time_mode_event() {
        let mode = TimeMode::event();
        assert!(!mode.is_tick());
        assert!(mode.is_event());
        assert_eq!(mode.tick_period(), 0);
    }

    #[test]
    fn test_subgraph_desc_tick() {
        let subgraph = SubgraphDesc::tick(1, 10)
            .with_node(100)
            .with_node(101)
            .with_nodes([102, 103, 104]);

        assert_eq!(subgraph.id, 1);
        assert!(subgraph.time_mode.is_tick());
        assert_eq!(subgraph.tick_period(), 10);
        assert_eq!(subgraph.nodes, vec![100, 101, 102, 103, 104]);
    }

    #[test]
    fn test_subgraph_desc_event() {
        let subgraph = SubgraphDesc::event(2).with_nodes([200, 201]);

        assert_eq!(subgraph.id, 2);
        assert!(subgraph.time_mode.is_event());
        assert_eq!(subgraph.tick_period(), 0);
        assert_eq!(subgraph.nodes, vec![200, 201]);
    }

    #[test]
    fn test_subgraph_serialization() {
        let subgraph = SubgraphDesc::tick(1, 10).with_nodes([1, 2, 3]);
        let json = serde_json::to_string(&subgraph).unwrap();
        let deserialized: SubgraphDesc = serde_json::from_str(&json).unwrap();

        assert_eq!(subgraph.id, deserialized.id);
        assert_eq!(subgraph.time_mode, deserialized.time_mode);
        assert_eq!(subgraph.nodes, deserialized.nodes);
    }
}


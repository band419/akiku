//! Tick-driven subgraph executor implementation.
//!
//! The `TickSubgraphExecutor` advances time in fixed increments (ticks),
//! calling each node's `on_tick()` method in topological order at each tick boundary.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::event::Event;
use crate::executor::SubgraphExecutor;
use crate::node::Node;
use crate::types::{NodeId, SimTime, SubgraphId};

/// A dependency edge between two nodes.
#[derive(Clone, Debug)]
pub struct NodeDependency {
    /// The node that depends on another
    pub from: NodeId,
    /// The node being depended upon
    pub to: NodeId,
}

impl NodeDependency {
    /// Creates a new dependency: `from` depends on `to`.
    pub fn new(from: NodeId, to: NodeId) -> Self {
        Self { from, to }
    }
}

/// Statistics collected by the tick executor.
#[derive(Clone, Debug, Default)]
pub struct TickExecutorStats {
    /// Total number of ticks executed
    pub ticks_executed: u64,
    /// Total number of node invocations
    pub node_invocations: u64,
    /// Total number of events generated
    pub events_generated: u64,
    /// Total number of incoming events processed
    pub incoming_events: u64,
}

/// A tick-driven subgraph executor.
///
/// This executor:
/// - Maintains nodes in topologically sorted order based on dependencies
/// - Advances time in fixed `tick_period` increments
/// - Calls each node's `on_tick()` at every tick boundary
/// - Collects events from nodes for delivery to other subgraphs
pub struct TickSubgraphExecutor {
    /// Unique identifier for this subgraph
    id: SubgraphId,
    /// The tick period (time between tick boundaries)
    tick_period: SimTime,
    /// Current simulation time
    current_time: SimTime,
    /// Nodes in this subgraph, stored by their ID
    nodes: HashMap<NodeId, Box<dyn Node>>,
    /// Node IDs in topological execution order
    execution_order: Vec<NodeId>,
    /// Pending incoming events (buffered until the appropriate tick)
    incoming_events: Vec<Event>,
    /// Statistics
    stats: TickExecutorStats,
}

impl TickSubgraphExecutor {
    /// Creates a new tick-driven executor.
    ///
    /// # Arguments
    /// * `id` - The subgraph identifier
    /// * `tick_period` - The time between tick boundaries
    pub fn new(id: SubgraphId, tick_period: SimTime) -> Self {
        Self {
            id,
            tick_period,
            current_time: 0,
            nodes: HashMap::new(),
            execution_order: Vec::new(),
            incoming_events: Vec::new(),
            stats: TickExecutorStats::default(),
        }
    }

    /// Adds a node to this executor.
    ///
    /// Nodes should be added before calling `set_execution_order()` or
    /// `compute_execution_order()`.
    pub fn add_node(&mut self, id: NodeId, node: Box<dyn Node>) {
        self.nodes.insert(id, node);
    }

    /// Sets the execution order directly (for simple cases without dependencies).
    pub fn set_execution_order(&mut self, order: Vec<NodeId>) {
        self.execution_order = order;
    }

    /// Computes the topological execution order from dependencies.
    ///
    /// Uses Kahn's algorithm for topological sorting.
    ///
    /// # Arguments
    /// * `dependencies` - List of dependencies where each entry means
    ///                    "from depends on to" (to must execute before from)
    ///
    /// # Returns
    /// `Ok(())` if successful, `Err` with a message if there's a cycle
    pub fn compute_execution_order(
        &mut self,
        dependencies: &[NodeDependency],
    ) -> Result<(), String> {
        let node_ids: HashSet<NodeId> = self.nodes.keys().copied().collect();

        // Build adjacency list and in-degree count
        // Edge: to -> from (to must execute before from)
        let mut adj: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();

        // Initialize
        for &id in &node_ids {
            adj.insert(id, Vec::new());
            in_degree.insert(id, 0);
        }

        // Add edges
        for dep in dependencies {
            if !node_ids.contains(&dep.from) || !node_ids.contains(&dep.to) {
                continue; // Skip dependencies involving nodes not in this subgraph
            }
            adj.get_mut(&dep.to).unwrap().push(dep.from);
            *in_degree.get_mut(&dep.from).unwrap() += 1;
        }

        // Kahn's algorithm
        let mut queue: VecDeque<NodeId> = VecDeque::new();
        for (&id, &deg) in &in_degree {
            if deg == 0 {
                queue.push_back(id);
            }
        }

        let mut order = Vec::new();
        while let Some(node) = queue.pop_front() {
            order.push(node);
            for &next in &adj[&node] {
                let deg = in_degree.get_mut(&next).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(next);
                }
            }
        }

        if order.len() != node_ids.len() {
            return Err("Cycle detected in node dependencies".to_string());
        }

        self.execution_order = order;
        Ok(())
    }

    /// Executes a single tick.
    ///
    /// Calls `on_tick()` on each node in execution order and collects
    /// any generated events.
    fn step_tick(&mut self) -> Vec<Event> {
        let mut outgoing_events = Vec::new();

        for &node_id in &self.execution_order {
            if let Some(node) = self.nodes.get_mut(&node_id) {
                let events = node.on_tick(self.current_time);
                self.stats.node_invocations += 1;
                self.stats.events_generated += events.len() as u64;
                outgoing_events.extend(events);
            }
        }

        self.stats.ticks_executed += 1;
        outgoing_events
    }

    /// Returns the tick period.
    pub fn tick_period(&self) -> SimTime {
        self.tick_period
    }

    /// Returns a reference to a node by ID.
    pub fn get_node(&self, id: NodeId) -> Option<&Box<dyn Node>> {
        self.nodes.get(&id)
    }

    /// Returns a mutable reference to a node by ID.
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut Box<dyn Node>> {
        self.nodes.get_mut(&id)
    }

    /// Returns the current execution order.
    pub fn execution_order(&self) -> &[NodeId] {
        &self.execution_order
    }
}

impl SubgraphExecutor for TickSubgraphExecutor {
    fn id(&self) -> SubgraphId {
        self.id
    }

    fn init(&mut self) {
        self.current_time = 0;
        self.incoming_events.clear();
        self.stats = TickExecutorStats::default();

        // Initialize all nodes
        for node in self.nodes.values_mut() {
            node.init();
        }
    }

    fn handle_incoming(&mut self, events: Vec<Event>) {
        self.stats.incoming_events += events.len() as u64;
        self.incoming_events.extend(events);
    }

    fn run_until(&mut self, target: SimTime) -> Vec<Event> {
        let mut all_events = Vec::new();

        // Process incoming events that should be handled before we start
        // (In a more complete implementation, we'd route these to specific nodes)
        // For now, we just track them in stats

        // Advance by ticks until we reach or exceed target
        while self.current_time + self.tick_period <= target {
            self.current_time += self.tick_period;
            let events = self.step_tick();
            all_events.extend(events);
        }

        all_events
    }

    fn export_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "subgraph_id": self.id,
            "tick_period": self.tick_period,
            "current_time": self.current_time,
            "ticks_executed": self.stats.ticks_executed,
            "node_invocations": self.stats.node_invocations,
            "events_generated": self.stats.events_generated,
            "incoming_events": self.stats.incoming_events,
        })
    }

    fn current_time(&self) -> SimTime {
        self.current_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::mock::CounterNode;

    #[test]
    fn test_tick_executor_creation() {
        let exec = TickSubgraphExecutor::new(1, 10);
        assert_eq!(exec.id(), 1);
        assert_eq!(exec.tick_period(), 10);
        assert_eq!(exec.current_time(), 0);
    }

    #[test]
    fn test_single_node_tick() {
        let mut exec = TickSubgraphExecutor::new(1, 10);

        let node = CounterNode::new(100);
        exec.add_node(100, Box::new(node));
        exec.set_execution_order(vec![100]);
        exec.init();

        // Run until time 50 (should execute 5 ticks: at 10, 20, 30, 40, 50)
        let events = exec.run_until(50);
        assert!(events.is_empty()); // CounterNode doesn't generate events
        assert_eq!(exec.current_time(), 50);

        let stats = exec.export_stats();
        assert_eq!(stats["ticks_executed"], 5);
        assert_eq!(stats["node_invocations"], 5);
    }

    #[test]
    fn test_topological_sort_linear() {
        let mut exec = TickSubgraphExecutor::new(1, 10);

        // Create a chain: 1 -> 2 -> 3 (3 depends on 2, 2 depends on 1)
        exec.add_node(1, Box::new(CounterNode::new(1)));
        exec.add_node(2, Box::new(CounterNode::new(2)));
        exec.add_node(3, Box::new(CounterNode::new(3)));

        let deps = vec![
            NodeDependency::new(2, 1), // 2 depends on 1
            NodeDependency::new(3, 2), // 3 depends on 2
        ];

        exec.compute_execution_order(&deps).unwrap();

        // Execution order should be [1, 2, 3]
        assert_eq!(exec.execution_order, vec![1, 2, 3]);
    }

    #[test]
    fn test_topological_sort_diamond() {
        let mut exec = TickSubgraphExecutor::new(1, 10);

        // Diamond: 1 -> 2,3 -> 4
        //   1
        //  / \
        // 2   3
        //  \ /
        //   4
        exec.add_node(1, Box::new(CounterNode::new(1)));
        exec.add_node(2, Box::new(CounterNode::new(2)));
        exec.add_node(3, Box::new(CounterNode::new(3)));
        exec.add_node(4, Box::new(CounterNode::new(4)));

        let deps = vec![
            NodeDependency::new(2, 1), // 2 depends on 1
            NodeDependency::new(3, 1), // 3 depends on 1
            NodeDependency::new(4, 2), // 4 depends on 2
            NodeDependency::new(4, 3), // 4 depends on 3
        ];

        exec.compute_execution_order(&deps).unwrap();

        // Node 1 must come first, node 4 must come last
        assert_eq!(exec.execution_order[0], 1);
        assert_eq!(exec.execution_order[3], 4);

        // 2 and 3 can be in either order, but both before 4
        let pos_2 = exec.execution_order.iter().position(|&x| x == 2).unwrap();
        let pos_3 = exec.execution_order.iter().position(|&x| x == 3).unwrap();
        let pos_4 = exec.execution_order.iter().position(|&x| x == 4).unwrap();
        assert!(pos_2 < pos_4);
        assert!(pos_3 < pos_4);
    }

    #[test]
    fn test_topological_sort_cycle_detection() {
        let mut exec = TickSubgraphExecutor::new(1, 10);

        exec.add_node(1, Box::new(CounterNode::new(1)));
        exec.add_node(2, Box::new(CounterNode::new(2)));

        // Cycle: 1 -> 2 -> 1
        let deps = vec![
            NodeDependency::new(1, 2),
            NodeDependency::new(2, 1),
        ];

        let result = exec.compute_execution_order(&deps);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cycle"));
    }

    #[test]
    fn test_multiple_nodes_execution() {
        let mut exec = TickSubgraphExecutor::new(1, 10);

        exec.add_node(1, Box::new(CounterNode::new(1)));
        exec.add_node(2, Box::new(CounterNode::new(2)));
        exec.add_node(3, Box::new(CounterNode::new(3)));

        exec.set_execution_order(vec![1, 2, 3]);
        exec.init();

        // Run until time 30 (3 ticks)
        exec.run_until(30);

        let stats = exec.export_stats();
        assert_eq!(stats["ticks_executed"], 3);
        // 3 nodes * 3 ticks = 9 invocations
        assert_eq!(stats["node_invocations"], 9);
    }

    #[test]
    fn test_handle_incoming() {
        let mut exec = TickSubgraphExecutor::new(1, 10);
        exec.init();

        let events = vec![
            Event::custom(15, "event1"),
            Event::custom(25, "event2"),
        ];
        exec.handle_incoming(events);

        let stats = exec.export_stats();
        assert_eq!(stats["incoming_events"], 2);
    }
}


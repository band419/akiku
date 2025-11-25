//! Integration tests for the Tick-driven subgraph executor.

use akiku::executor::tick::{NodeDependency, TickSubgraphExecutor};
use akiku::executor::SubgraphExecutor;
use akiku::node::Node;
use akiku::types::{NodeId, SimTime};
use akiku::{Event, EventPayload};

/// A node that generates events on each tick.
struct EventGeneratorNode {
    id: NodeId,
    target_id: NodeId,
    sequence: u64,
}

impl EventGeneratorNode {
    fn new(id: NodeId, target_id: NodeId) -> Self {
        Self {
            id,
            target_id,
            sequence: 0,
        }
    }
}

impl Node for EventGeneratorNode {
    fn init(&mut self) {
        self.sequence = 0;
    }

    fn on_tick(&mut self, time: SimTime) -> Vec<Event> {
        self.sequence += 1;
        vec![Event::message(
            time,
            self.id,
            "out",
            self.target_id,
            "in",
            serde_json::json!({ "seq": self.sequence }),
        )]
    }
}

/// A node that tracks received values (simulating pipeline stage).
struct AccumulatorNode {
    tick_count: u64,
}

impl AccumulatorNode {
    fn new(_id: NodeId) -> Self {
        Self { tick_count: 0 }
    }
}

impl Node for AccumulatorNode {
    fn init(&mut self) {
        self.tick_count = 0;
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        self.tick_count += 1;
        Vec::new()
    }
}

#[test]
fn test_single_node_tick_advancement() {
    let mut exec = TickSubgraphExecutor::new(1, 10);

    let node = AccumulatorNode::new(100);
    exec.add_node(100, Box::new(node));
    exec.set_execution_order(vec![100]);
    exec.init();

    // Run until time 100 (10 ticks)
    let events = exec.run_until(100);
    assert!(events.is_empty());
    assert_eq!(exec.current_time(), 100);

    let stats = exec.export_stats();
    assert_eq!(stats["ticks_executed"], 10);
}

#[test]
fn test_single_node_event_generation() {
    let mut exec = TickSubgraphExecutor::new(1, 10);

    let node = EventGeneratorNode::new(1, 99);
    exec.add_node(1, Box::new(node));
    exec.set_execution_order(vec![1]);
    exec.init();

    // Run until time 30 (3 ticks)
    let events = exec.run_until(30);

    // Should have 3 events
    assert_eq!(events.len(), 3);

    // Verify event contents
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event.time, ((i + 1) * 10) as SimTime);
        if let EventPayload::Message { src, dst, data } = &event.payload {
            assert_eq!(src.0, 1);
            assert_eq!(dst.0, 99);
            assert_eq!(data["seq"], (i + 1) as i64);
        } else {
            panic!("Expected Message payload");
        }
    }
}

#[test]
fn test_multi_node_dependency_chain() {
    let mut exec = TickSubgraphExecutor::new(1, 10);

    // Create a pipeline: A -> B -> C
    exec.add_node(1, Box::new(AccumulatorNode::new(1)));
    exec.add_node(2, Box::new(AccumulatorNode::new(2)));
    exec.add_node(3, Box::new(AccumulatorNode::new(3)));

    let deps = vec![
        NodeDependency::new(2, 1), // B depends on A
        NodeDependency::new(3, 2), // C depends on B
    ];

    exec.compute_execution_order(&deps).unwrap();
    exec.init();

    // Verify execution order
    assert_eq!(exec.execution_order(), &[1, 2, 3]);

    // Run until time 50 (5 ticks)
    exec.run_until(50);

    let stats = exec.export_stats();
    assert_eq!(stats["ticks_executed"], 5);
    // 3 nodes * 5 ticks = 15 invocations
    assert_eq!(stats["node_invocations"], 15);
}

#[test]
fn test_complex_dependency_graph() {
    let mut exec = TickSubgraphExecutor::new(1, 10);

    // Create a more complex graph:
    //     1
    //    /|\
    //   2 3 4
    //    \|/
    //     5
    exec.add_node(1, Box::new(AccumulatorNode::new(1)));
    exec.add_node(2, Box::new(AccumulatorNode::new(2)));
    exec.add_node(3, Box::new(AccumulatorNode::new(3)));
    exec.add_node(4, Box::new(AccumulatorNode::new(4)));
    exec.add_node(5, Box::new(AccumulatorNode::new(5)));

    let deps = vec![
        NodeDependency::new(2, 1),
        NodeDependency::new(3, 1),
        NodeDependency::new(4, 1),
        NodeDependency::new(5, 2),
        NodeDependency::new(5, 3),
        NodeDependency::new(5, 4),
    ];

    exec.compute_execution_order(&deps).unwrap();

    // Verify: 1 must be first, 5 must be last
    let order = exec.execution_order();
    assert_eq!(order[0], 1);
    assert_eq!(order[4], 5);

    // 2, 3, 4 must all come before 5
    let pos_5 = order.iter().position(|&x| x == 5).unwrap();
    for &node in &[2, 3, 4] {
        let pos = order.iter().position(|&x| x == node).unwrap();
        assert!(pos < pos_5, "Node {} should execute before node 5", node);
    }
}

#[test]
fn test_partial_tick_advancement() {
    let mut exec = TickSubgraphExecutor::new(1, 10);

    exec.add_node(1, Box::new(AccumulatorNode::new(1)));
    exec.set_execution_order(vec![1]);
    exec.init();

    // Run until time 25 - should only execute 2 ticks (at 10 and 20)
    // because 20 + 10 = 30 > 25
    exec.run_until(25);

    assert_eq!(exec.current_time(), 20);
    assert_eq!(exec.export_stats()["ticks_executed"], 2);

    // Now run until 35 - should execute 1 more tick (at 30)
    exec.run_until(35);

    assert_eq!(exec.current_time(), 30);
    assert_eq!(exec.export_stats()["ticks_executed"], 3);
}

#[test]
fn test_incoming_events_handling() {
    let mut exec = TickSubgraphExecutor::new(1, 10);

    exec.add_node(1, Box::new(AccumulatorNode::new(1)));
    exec.set_execution_order(vec![1]);
    exec.init();

    // Simulate receiving events from another subgraph
    let incoming = vec![
        Event::custom(5, "early_event"),
        Event::custom(15, "mid_event"),
        Event::custom(25, "late_event"),
    ];
    exec.handle_incoming(incoming);

    let stats = exec.export_stats();
    assert_eq!(stats["incoming_events"], 3);
}

#[test]
fn test_executor_reset_on_init() {
    let mut exec = TickSubgraphExecutor::new(1, 10);

    exec.add_node(1, Box::new(AccumulatorNode::new(1)));
    exec.set_execution_order(vec![1]);
    exec.init();

    // Run some ticks
    exec.run_until(50);
    assert_eq!(exec.current_time(), 50);
    assert_eq!(exec.export_stats()["ticks_executed"], 5);

    // Re-initialize
    exec.init();

    // Should be reset
    assert_eq!(exec.current_time(), 0);
    assert_eq!(exec.export_stats()["ticks_executed"], 0);
}

#[test]
fn test_empty_subgraph() {
    let mut exec = TickSubgraphExecutor::new(1, 10);
    exec.init();

    // Run with no nodes
    let events = exec.run_until(100);

    assert!(events.is_empty());
    assert_eq!(exec.current_time(), 100);
    assert_eq!(exec.export_stats()["ticks_executed"], 10);
    assert_eq!(exec.export_stats()["node_invocations"], 0);
}


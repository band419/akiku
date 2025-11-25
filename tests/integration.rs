//! Integration tests for the SimulationEngine.
//!
//! These tests verify end-to-end simulation scenarios including:
//! - Pure Tick subgraph simulation
//! - Pure Event subgraph simulation
//! - Mixed Tick + Event subgraph simulation

use akiku::executor::event::EventSubgraphExecutor;
use akiku::executor::tick::{NodeDependency, TickSubgraphExecutor};
use akiku::node::Node;
use akiku::types::{NodeId, SimTime};
use akiku::{ChannelDesc, Event, EventPayload, SimulationEngine, SubgraphConfig, TimeAlignment};

// ============================================================================
// Test Nodes
// ============================================================================

/// A tick-driven node that counts ticks and optionally generates events.
struct TickCounterNode {
    id: NodeId,
    tick_count: u64,
    generate_events: bool,
    target_node: Option<NodeId>,
}

impl TickCounterNode {
    fn new(id: NodeId) -> Self {
        Self {
            id,
            tick_count: 0,
            generate_events: false,
            target_node: None,
        }
    }

    fn with_event_generation(mut self, target: NodeId) -> Self {
        self.generate_events = true;
        self.target_node = Some(target);
        self
    }
}

impl Node for TickCounterNode {
    fn init(&mut self) {
        self.tick_count = 0;
    }

    fn on_tick(&mut self, time: SimTime) -> Vec<Event> {
        self.tick_count += 1;

        if self.generate_events {
            if let Some(target) = self.target_node {
                return vec![Event::message(
                    time,
                    self.id,
                    "out",
                    target,
                    "in",
                    serde_json::json!({
                        "tick": self.tick_count,
                        "time": time,
                    }),
                )];
            }
        }

        Vec::new()
    }
}

/// An event-driven node that processes events and optionally forwards them.
struct EventProcessorNode {
    id: NodeId,
    events_received: u64,
    delay: SimTime,
    forward_to: Option<NodeId>,
}

impl EventProcessorNode {
    fn new(id: NodeId) -> Self {
        Self {
            id,
            events_received: 0,
            delay: 0,
            forward_to: None,
        }
    }

    fn with_delay(mut self, delay: SimTime) -> Self {
        self.delay = delay;
        self
    }

    fn with_forward(mut self, target: NodeId) -> Self {
        self.forward_to = Some(target);
        self
    }
}

impl Node for EventProcessorNode {
    fn init(&mut self) {
        self.events_received = 0;
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        self.events_received += 1;

        if let Some(target) = self.forward_to {
            if let EventPayload::Message { data, .. } = event.payload {
                return vec![Event::message(
                    event.time + self.delay,
                    self.id,
                    "out",
                    target,
                    "in",
                    data,
                )];
            }
        }

        Vec::new()
    }
}

/// A sink node that just counts received events.
struct SinkNode {
    events_received: u64,
    last_event_time: SimTime,
}

impl SinkNode {
    fn new() -> Self {
        Self {
            events_received: 0,
            last_event_time: 0,
        }
    }
}

impl Node for SinkNode {
    fn init(&mut self) {
        self.events_received = 0;
        self.last_event_time = 0;
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        self.events_received += 1;
        self.last_event_time = event.time;
        Vec::new()
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        Vec::new()
    }
}

// ============================================================================
// Pure Tick Subgraph Tests (Task 6.6)
// ============================================================================

#[test]
fn test_pure_tick_single_subgraph() {
    let mut engine = SimulationEngine::new(10);

    // Create a tick subgraph with a single counter node
    let mut exec = TickSubgraphExecutor::new(1, 10);
    exec.add_node(1001, Box::new(TickCounterNode::new(1001)));
    exec.set_execution_order(vec![1001]);

    engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 10));
    engine.init();

    // Run for 100ns (10 ticks)
    engine.run(100);

    assert_eq!(engine.current_time(), 100);

    let stats = engine.export_stats();
    assert_eq!(stats["engine"]["steps_executed"], 10);
    assert_eq!(stats["subgraphs"]["1"]["ticks_executed"], 10);
}

#[test]
fn test_pure_tick_multiple_nodes() {
    let mut engine = SimulationEngine::new(10);

    // Create a pipeline: A -> B -> C
    let mut exec = TickSubgraphExecutor::new(1, 10);
    exec.add_node(1001, Box::new(TickCounterNode::new(1001)));
    exec.add_node(1002, Box::new(TickCounterNode::new(1002)));
    exec.add_node(1003, Box::new(TickCounterNode::new(1003)));

    let deps = vec![
        NodeDependency::new(1002, 1001),
        NodeDependency::new(1003, 1002),
    ];
    exec.compute_execution_order(&deps).unwrap();

    engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 10));
    engine.init();

    engine.run(50);

    let stats = engine.export_stats();
    assert_eq!(stats["subgraphs"]["1"]["ticks_executed"], 5);
    // 3 nodes * 5 ticks = 15 invocations
    assert_eq!(stats["subgraphs"]["1"]["node_invocations"], 15);
}

#[test]
fn test_pure_tick_multiple_subgraphs() {
    let mut engine = SimulationEngine::new(10);

    // Subgraph 1 with period 10
    let mut exec1 = TickSubgraphExecutor::new(1, 10);
    exec1.add_node(1001, Box::new(TickCounterNode::new(1001)));
    exec1.set_execution_order(vec![1001]);

    // Subgraph 2 with period 20
    let mut exec2 = TickSubgraphExecutor::new(2, 20);
    exec2.add_node(2001, Box::new(TickCounterNode::new(2001)));
    exec2.set_execution_order(vec![2001]);

    engine.add_subgraph(Box::new(exec1), SubgraphConfig::tick(1, 10));
    engine.add_subgraph(Box::new(exec2), SubgraphConfig::tick(2, 20));
    engine.init();

    engine.run(100);

    let stats = engine.export_stats();
    // Subgraph 1: 10 ticks (every 10ns)
    assert_eq!(stats["subgraphs"]["1"]["ticks_executed"], 10);
    // Subgraph 2: 5 ticks (every 20ns)
    assert_eq!(stats["subgraphs"]["2"]["ticks_executed"], 5);
}

// ============================================================================
// Pure Event Subgraph Tests (Task 6.7)
// ============================================================================

#[test]
fn test_pure_event_single_subgraph() {
    let mut engine = SimulationEngine::new(10);

    let mut exec = EventSubgraphExecutor::new(1);
    exec.add_node(1001, Box::new(SinkNode::new()));

    engine.add_subgraph(Box::new(exec), SubgraphConfig::event(1));
    engine.init();

    // Inject events
    engine.inject_event(Event::message(
        5,
        0,
        "external",
        1001,
        "in",
        serde_json::json!({"seq": 1}),
    ));
    engine.inject_event(Event::message(
        15,
        0,
        "external",
        1001,
        "in",
        serde_json::json!({"seq": 2}),
    ));

    engine.run(50);

    let stats = engine.export_stats();
    assert_eq!(stats["subgraphs"]["1"]["events_processed"], 2);
}

#[test]
fn test_pure_event_chain() {
    let mut engine = SimulationEngine::new(10);

    // Create a chain: A(delay=5) -> B(delay=10) -> Sink
    let mut exec = EventSubgraphExecutor::new(1);

    let node_a = EventProcessorNode::new(1001).with_delay(5).with_forward(1002);
    let node_b = EventProcessorNode::new(1002).with_delay(10).with_forward(1003);
    let sink = SinkNode::new();

    exec.add_node(1001, Box::new(node_a));
    exec.add_node(1002, Box::new(node_b));
    exec.add_node(1003, Box::new(sink));

    engine.add_subgraph(Box::new(exec), SubgraphConfig::event(1));
    engine.init();

    // Inject initial event at t=0
    engine.inject_event(Event::message(
        0,
        0,
        "external",
        1001,
        "in",
        serde_json::json!({"data": "test"}),
    ));

    // t=0: event at A
    // t=5: A forwards to B
    // t=15: B forwards to Sink
    engine.run(30);

    let stats = engine.export_stats();
    // 3 events processed: initial + 2 forwards
    assert_eq!(stats["subgraphs"]["1"]["events_processed"], 3);
}

#[test]
fn test_pure_event_multiple_subgraphs() {
    let mut engine = SimulationEngine::new(10);

    // Two independent event subgraphs
    let mut exec1 = EventSubgraphExecutor::new(1);
    exec1.add_node(1001, Box::new(SinkNode::new()));

    let mut exec2 = EventSubgraphExecutor::new(2);
    exec2.add_node(2001, Box::new(SinkNode::new()));

    engine.add_subgraph(Box::new(exec1), SubgraphConfig::event(1));
    engine.add_subgraph(Box::new(exec2), SubgraphConfig::event(2));
    engine.init();

    // Inject events to both subgraphs
    engine.inject_event(Event::message(5, 0, "ext", 1001, "in", serde_json::json!({})));
    engine.inject_event(Event::message(5, 0, "ext", 2001, "in", serde_json::json!({})));

    engine.run(20);

    let stats = engine.export_stats();
    assert_eq!(stats["subgraphs"]["1"]["events_processed"], 1);
    assert_eq!(stats["subgraphs"]["2"]["events_processed"], 1);
}

// ============================================================================
// Mixed Tick + Event Subgraph Tests (Task 6.8)
// ============================================================================

#[test]
fn test_mixed_tick_event_basic() {
    let mut engine = SimulationEngine::new(10);

    // Tick subgraph (subgraph 1)
    let mut tick_exec = TickSubgraphExecutor::new(1, 10);
    tick_exec.add_node(1001, Box::new(TickCounterNode::new(1001)));
    tick_exec.set_execution_order(vec![1001]);

    // Event subgraph (subgraph 2)
    let mut event_exec = EventSubgraphExecutor::new(2);
    event_exec.add_node(2001, Box::new(SinkNode::new()));

    engine.add_subgraph(Box::new(tick_exec), SubgraphConfig::tick(1, 10));
    engine.add_subgraph(Box::new(event_exec), SubgraphConfig::event(2));
    engine.init();

    engine.run(100);

    let stats = engine.export_stats();
    assert_eq!(stats["subgraphs"]["1"]["ticks_executed"], 10);
    assert_eq!(engine.current_time(), 100);
}

#[test]
fn test_mixed_tick_generates_to_event() {
    let mut engine = SimulationEngine::new(10);

    // Tick subgraph generates events
    let mut tick_exec = TickSubgraphExecutor::new(1, 10);
    let tick_node = TickCounterNode::new(1001).with_event_generation(2001);
    tick_exec.add_node(1001, Box::new(tick_node));
    tick_exec.set_execution_order(vec![1001]);

    // Event subgraph receives events
    let mut event_exec = EventSubgraphExecutor::new(2);
    event_exec.add_node(2001, Box::new(SinkNode::new()));

    engine.add_subgraph(Box::new(tick_exec), SubgraphConfig::tick(1, 10));
    engine.add_subgraph(Box::new(event_exec), SubgraphConfig::event(2));

    // Add channel from tick to event subgraph
    engine.add_channel(ChannelDesc::new(1, 2).with_latency(0));

    engine.init();
    engine.run(50);

    let stats = engine.export_stats();
    // Tick subgraph generated events
    assert_eq!(stats["subgraphs"]["1"]["events_generated"], 5);
    // Events should have been processed by the engine
    assert!(stats["engine"]["events_processed"].as_u64().unwrap() >= 5);
}

#[test]
fn test_mixed_with_channel_latency() {
    let mut engine = SimulationEngine::new(10);

    // Tick subgraph
    let mut tick_exec = TickSubgraphExecutor::new(1, 10);
    let tick_node = TickCounterNode::new(1001).with_event_generation(2001);
    tick_exec.add_node(1001, Box::new(tick_node));
    tick_exec.set_execution_order(vec![1001]);

    // Event subgraph
    let mut event_exec = EventSubgraphExecutor::new(2);
    event_exec.add_node(2001, Box::new(SinkNode::new()));

    engine.add_subgraph(Box::new(tick_exec), SubgraphConfig::tick(1, 10));
    engine.add_subgraph(Box::new(event_exec), SubgraphConfig::event(2));

    // Channel with 5ns latency
    engine.add_channel(ChannelDesc::new(1, 2).with_latency(5));

    engine.init();
    engine.run(30);

    // Verify simulation completed
    assert_eq!(engine.current_time(), 30);
}

#[test]
fn test_mixed_bidirectional_communication() {
    let mut engine = SimulationEngine::new(10);

    // Tick subgraph
    let mut tick_exec = TickSubgraphExecutor::new(1, 10);
    tick_exec.add_node(1001, Box::new(TickCounterNode::new(1001)));
    tick_exec.set_execution_order(vec![1001]);

    // Event subgraph
    let mut event_exec = EventSubgraphExecutor::new(2);
    event_exec.add_node(2001, Box::new(SinkNode::new()));

    engine.add_subgraph(Box::new(tick_exec), SubgraphConfig::tick(1, 10));
    engine.add_subgraph(Box::new(event_exec), SubgraphConfig::event(2));

    // Bidirectional channels
    engine.add_channel(ChannelDesc::new(1, 2).with_latency(2));
    engine.add_channel(ChannelDesc::new(2, 1).with_latency(3));

    engine.init();
    engine.run(50);

    assert_eq!(engine.current_time(), 50);
}

#[test]
fn test_mixed_time_alignment_ceil_to_tick() {
    let mut engine = SimulationEngine::new(10);

    // Event subgraph generates at arbitrary times
    let mut event_exec = EventSubgraphExecutor::new(1);
    let processor = EventProcessorNode::new(1001).with_delay(3).with_forward(2001);
    event_exec.add_node(1001, Box::new(processor));

    // Tick subgraph receives (period=10)
    let mut tick_exec = TickSubgraphExecutor::new(2, 10);
    tick_exec.add_node(2001, Box::new(SinkNode::new()));
    tick_exec.set_execution_order(vec![2001]);

    engine.add_subgraph(Box::new(event_exec), SubgraphConfig::event(1));
    engine.add_subgraph(Box::new(tick_exec), SubgraphConfig::tick(2, 10));

    // Channel with CeilToTick alignment
    engine.add_channel(
        ChannelDesc::new(1, 2)
            .with_latency(0)
            .with_alignment(TimeAlignment::CeilToTick),
    );

    engine.init();

    // Inject event at t=0, processor adds 3ns delay
    // Result: event at t=3 should be aligned to t=10 for tick subgraph
    engine.inject_event(Event::message(
        0,
        0,
        "external",
        1001,
        "in",
        serde_json::json!({}),
    ));

    engine.run(30);

    assert_eq!(engine.current_time(), 30);
}

// ============================================================================
// Advanced Integration Tests
// ============================================================================

#[test]
fn test_complex_multi_subgraph_simulation() {
    let mut engine = SimulationEngine::new(10);

    // Create 3 subgraphs: 2 tick + 1 event
    // Subgraph 1 (Tick, period=10)
    let mut exec1 = TickSubgraphExecutor::new(1, 10);
    exec1.add_node(1001, Box::new(TickCounterNode::new(1001)));
    exec1.set_execution_order(vec![1001]);

    // Subgraph 2 (Tick, period=5)
    let mut exec2 = TickSubgraphExecutor::new(2, 5);
    exec2.add_node(2001, Box::new(TickCounterNode::new(2001)));
    exec2.set_execution_order(vec![2001]);

    // Subgraph 3 (Event)
    let mut exec3 = EventSubgraphExecutor::new(3);
    exec3.add_node(3001, Box::new(SinkNode::new()));

    engine.add_subgraph(Box::new(exec1), SubgraphConfig::tick(1, 10));
    engine.add_subgraph(Box::new(exec2), SubgraphConfig::tick(2, 5));
    engine.add_subgraph(Box::new(exec3), SubgraphConfig::event(3));

    engine.init();
    engine.run(100);

    let stats = engine.export_stats();
    assert_eq!(stats["engine"]["subgraph_count"], 3);
    assert_eq!(stats["subgraphs"]["1"]["ticks_executed"], 10);
    assert_eq!(stats["subgraphs"]["2"]["ticks_executed"], 20); // period 5, so 20 ticks in 100ns
}

#[test]
fn test_simulation_reset() {
    let mut engine = SimulationEngine::new(10);

    let mut exec = TickSubgraphExecutor::new(1, 10);
    exec.add_node(1001, Box::new(TickCounterNode::new(1001)));
    exec.set_execution_order(vec![1001]);

    engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 10));

    // First run
    engine.init();
    engine.run(50);
    assert_eq!(engine.current_time(), 50);

    // Reset and run again
    engine.init();
    assert_eq!(engine.current_time(), 0);

    engine.run(30);
    assert_eq!(engine.current_time(), 30);
}

#[test]
fn test_empty_simulation() {
    let mut engine = SimulationEngine::new(10);
    engine.init();
    engine.run(100);

    assert_eq!(engine.current_time(), 100);
    assert_eq!(engine.stats().steps_executed, 10);
}

#[test]
fn test_step_by_step_execution() {
    let mut engine = SimulationEngine::new(10);

    let mut exec = TickSubgraphExecutor::new(1, 10);
    exec.add_node(1001, Box::new(TickCounterNode::new(1001)));
    exec.set_execution_order(vec![1001]);

    engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 10));
    engine.init();

    // Execute step by step
    for i in 1..=5 {
        engine.step();
        assert_eq!(engine.current_time(), i * 10);
        assert_eq!(engine.stats().steps_executed, i);
    }
}

#[test]
fn test_event_injection_timing() {
    let mut engine = SimulationEngine::new(10);

    let mut exec = EventSubgraphExecutor::new(1);
    exec.add_node(1001, Box::new(SinkNode::new()));

    engine.add_subgraph(Box::new(exec), SubgraphConfig::event(1));
    engine.init();

    // Run first step
    engine.step();
    assert_eq!(engine.current_time(), 10);

    // Inject event
    engine.inject_event(Event::message(
        15,
        0,
        "external",
        1001,
        "in",
        serde_json::json!({}),
    ));

    // Run more steps
    engine.run(30);

    let stats = engine.export_stats();
    assert_eq!(stats["subgraphs"]["1"]["events_processed"], 1);
}

#[test]
fn test_stats_aggregation() {
    let mut engine = SimulationEngine::new(10);

    // Add multiple subgraphs
    for i in 1..=3 {
        let mut exec = TickSubgraphExecutor::new(i, 10);
        exec.add_node(i as NodeId * 1000 + 1, Box::new(TickCounterNode::new(i as NodeId * 1000 + 1)));
        exec.set_execution_order(vec![i as NodeId * 1000 + 1]);
        engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(i, 10));
    }

    engine.init();
    engine.run(100);

    let stats = engine.export_stats();

    // Verify engine stats
    assert_eq!(stats["engine"]["current_time"], 100);
    assert_eq!(stats["engine"]["steps_executed"], 10);
    assert_eq!(stats["engine"]["subgraph_count"], 3);

    // Verify each subgraph has stats
    for i in 1..=3 {
        assert!(stats["subgraphs"][&i.to_string()].is_object());
        assert_eq!(stats["subgraphs"][&i.to_string()]["ticks_executed"], 10);
    }
}


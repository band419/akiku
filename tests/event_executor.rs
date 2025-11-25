//! Integration tests for the Event-driven subgraph executor.

use akiku::executor::event::EventSubgraphExecutor;
use akiku::executor::SubgraphExecutor;
use akiku::node::Node;
use akiku::types::{NodeId, SimTime};
use akiku::{Event, EventPayload};

/// A node that forwards events with a configurable delay.
struct DelayNode {
    id: NodeId,
    target_id: NodeId,
    delay: SimTime,
    processed_count: u64,
}

impl DelayNode {
    fn new(id: NodeId, target_id: NodeId, delay: SimTime) -> Self {
        Self {
            id,
            target_id,
            delay,
            processed_count: 0,
        }
    }
}

impl Node for DelayNode {
    fn init(&mut self) {
        self.processed_count = 0;
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        self.processed_count += 1;

        if let EventPayload::Message { data, .. } = event.payload {
            vec![Event::message(
                event.time + self.delay,
                self.id,
                "out",
                self.target_id,
                "in",
                data,
            )]
        } else {
            Vec::new()
        }
    }
}

/// A sink node that consumes events and counts them.
struct SinkNode {
    received_count: u64,
    last_time: SimTime,
}

impl SinkNode {
    fn new() -> Self {
        Self {
            received_count: 0,
            last_time: 0,
        }
    }
}

impl Node for SinkNode {
    fn init(&mut self) {
        self.received_count = 0;
        self.last_time = 0;
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        self.received_count += 1;
        self.last_time = event.time;
        Vec::new() // Sink doesn't produce events
    }
}

/// A splitter node that forwards events to multiple targets.
struct SplitterNode {
    id: NodeId,
    targets: Vec<NodeId>,
}

impl SplitterNode {
    fn new(id: NodeId, targets: Vec<NodeId>) -> Self {
        Self { id, targets }
    }
}

impl Node for SplitterNode {
    fn on_event(&mut self, event: Event) -> Vec<Event> {
        if let EventPayload::Message { data, .. } = event.payload {
            self.targets
                .iter()
                .map(|&target_id| {
                    Event::message(event.time, self.id, "out", target_id, "in", data.clone())
                })
                .collect()
        } else {
            Vec::new()
        }
    }
}

#[test]
fn test_event_production_and_consumption() {
    let mut exec = EventSubgraphExecutor::new(1);

    // Simple pipeline: Source -> Delay(10) -> Sink
    let delay = DelayNode::new(100, 200, 10);
    let sink = SinkNode::new();

    exec.add_node(100, Box::new(delay));
    exec.add_node(200, Box::new(sink));
    exec.init();

    // Inject initial event
    exec.schedule_event(Event::message(
        0,
        999,
        "external",
        100,
        "in",
        serde_json::json!({"seq": 1}),
    ));

    // Run simulation
    exec.run_until(50);

    let stats = exec.export_stats();
    // 2 events: initial to delay, then delay forwards to sink
    assert_eq!(stats["events_processed"], 2);
}

#[test]
fn test_event_chain_reaction() {
    let mut exec = EventSubgraphExecutor::new(1);

    // Chain: A(delay=5) -> B(delay=10) -> C(delay=15) -> Sink
    let node_a = DelayNode::new(1, 2, 5);
    let node_b = DelayNode::new(2, 3, 10);
    let node_c = DelayNode::new(3, 4, 15);
    let sink = SinkNode::new();

    exec.add_node(1, Box::new(node_a));
    exec.add_node(2, Box::new(node_b));
    exec.add_node(3, Box::new(node_c));
    exec.add_node(4, Box::new(sink));
    exec.init();

    // Inject event at t=0
    exec.schedule_event(Event::message(
        0,
        999,
        "external",
        1,
        "in",
        serde_json::json!({"data": "test"}),
    ));

    // t=0: event at A
    // t=5: A forwards to B
    // t=15: B forwards to C
    // t=30: C forwards to Sink
    exec.run_until(35);

    let stats = exec.export_stats();
    assert_eq!(stats["events_processed"], 4);
    assert_eq!(exec.current_time(), 35);
}

#[test]
fn test_multiple_events_ordering() {
    let mut exec = EventSubgraphExecutor::new(1);

    let sink = SinkNode::new();
    exec.add_node(100, Box::new(sink));
    exec.init();

    // Schedule events in random time order
    exec.schedule_event(Event::message(
        30,
        1,
        "out",
        100,
        "in",
        serde_json::json!({"seq": 3}),
    ));
    exec.schedule_event(Event::message(
        10,
        1,
        "out",
        100,
        "in",
        serde_json::json!({"seq": 1}),
    ));
    exec.schedule_event(Event::message(
        20,
        1,
        "out",
        100,
        "in",
        serde_json::json!({"seq": 2}),
    ));

    // Run simulation - events should be processed in time order
    exec.run_until(35);

    let stats = exec.export_stats();
    assert_eq!(stats["events_processed"], 3);
}

#[test]
fn test_event_fanout() {
    let mut exec = EventSubgraphExecutor::new(1);

    // Splitter fans out to 3 sinks
    let splitter = SplitterNode::new(1, vec![10, 20, 30]);
    let sink1 = SinkNode::new();
    let sink2 = SinkNode::new();
    let sink3 = SinkNode::new();

    exec.add_node(1, Box::new(splitter));
    exec.add_node(10, Box::new(sink1));
    exec.add_node(20, Box::new(sink2));
    exec.add_node(30, Box::new(sink3));
    exec.init();

    // Send one event to splitter
    exec.schedule_event(Event::message(
        10,
        999,
        "external",
        1,
        "in",
        serde_json::json!({"broadcast": true}),
    ));

    exec.run_until(20);

    let stats = exec.export_stats();
    // 1 event to splitter + 3 events to sinks = 4 total
    assert_eq!(stats["events_processed"], 4);
    assert_eq!(stats["events_generated"], 3);
}

#[test]
fn test_partial_event_processing() {
    let mut exec = EventSubgraphExecutor::new(1);

    let sink = SinkNode::new();
    exec.add_node(100, Box::new(sink));
    exec.init();

    // Schedule events at different times
    exec.schedule_event(Event::message(
        10,
        1,
        "out",
        100,
        "in",
        serde_json::json!({}),
    ));
    exec.schedule_event(Event::message(
        20,
        1,
        "out",
        100,
        "in",
        serde_json::json!({}),
    ));
    exec.schedule_event(Event::message(
        30,
        1,
        "out",
        100,
        "in",
        serde_json::json!({}),
    ));

    // Only process up to t=15
    exec.run_until(15);

    assert_eq!(exec.export_stats()["events_processed"], 1);
    assert_eq!(exec.pending_event_count(), 2);

    // Process remaining
    exec.run_until(35);

    assert_eq!(exec.export_stats()["events_processed"], 3);
    assert_eq!(exec.pending_event_count(), 0);
}

#[test]
fn test_outgoing_events_to_external() {
    let mut exec = EventSubgraphExecutor::new(1);

    // Node that forwards to a non-existent node (simulating external subgraph)
    let delay = DelayNode::new(100, 999, 5);
    exec.add_node(100, Box::new(delay));
    exec.init();

    exec.schedule_event(Event::message(
        0,
        1,
        "external",
        100,
        "in",
        serde_json::json!({"outbound": true}),
    ));

    let outgoing = exec.run_until(10);

    // The delayed event should be returned as outgoing (node 999 not in subgraph)
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].time, 5);

    let stats = exec.export_stats();
    assert_eq!(stats["outgoing_events"], 1);
}

#[test]
fn test_concurrent_events_same_time() {
    let mut exec = EventSubgraphExecutor::new(1);

    let sink = SinkNode::new();
    exec.add_node(100, Box::new(sink));
    exec.init();

    // Multiple events at the same time
    for i in 0..5 {
        exec.schedule_event(Event::message(
            10,
            1,
            "out",
            100,
            "in",
            serde_json::json!({"seq": i}),
        ));
    }

    exec.run_until(15);

    let stats = exec.export_stats();
    assert_eq!(stats["events_processed"], 5);
}

#[test]
fn test_executor_reset() {
    let mut exec = EventSubgraphExecutor::new(1);

    let sink = SinkNode::new();
    exec.add_node(100, Box::new(sink));
    exec.init();

    // Process some events
    exec.schedule_event(Event::message(
        10,
        1,
        "out",
        100,
        "in",
        serde_json::json!({}),
    ));
    exec.run_until(20);

    assert_eq!(exec.export_stats()["events_processed"], 1);
    assert_eq!(exec.current_time(), 20);

    // Reset
    exec.init();

    assert_eq!(exec.export_stats()["events_processed"], 0);
    assert_eq!(exec.current_time(), 0);
    assert_eq!(exec.pending_event_count(), 0);
}

#[test]
fn test_empty_simulation() {
    let mut exec = EventSubgraphExecutor::new(1);
    exec.init();

    // Run with no nodes and no events
    let outgoing = exec.run_until(100);

    assert!(outgoing.is_empty());
    assert_eq!(exec.current_time(), 100);
}

#[test]
fn test_peak_queue_size_tracking() {
    let mut exec = EventSubgraphExecutor::new(1);

    let sink = SinkNode::new();
    exec.add_node(100, Box::new(sink));
    exec.init();

    // Schedule many events
    for i in 0..10 {
        exec.schedule_event(Event::message(
            (i * 10) as SimTime,
            1,
            "out",
            100,
            "in",
            serde_json::json!({}),
        ));
    }

    // Peak should be 10
    let stats = exec.export_stats();
    assert_eq!(stats["peak_queue_size"], 10);

    // After processing, pending should be 0 but peak remains
    exec.run_until(100);

    let stats = exec.export_stats();
    assert_eq!(stats["peak_queue_size"], 10);
    assert_eq!(stats["pending_events"], 0);
}


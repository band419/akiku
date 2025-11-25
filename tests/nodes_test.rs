//! Tests for Phase 9 node implementations.
//!
//! Tests DelayNode, ProbabilisticDelayNode, PipelineStageNode, and PassThroughNode.

use akiku::executor::event::EventSubgraphExecutor;
use akiku::executor::tick::TickSubgraphExecutor;
use akiku::executor::SubgraphExecutor;
use akiku::node::Node;
use akiku::nodes::delay::DelayNode;
use akiku::nodes::passthrough::{EnhancedPassThroughNode, FilterMode, SimplePassThrough, TransformMode};
use akiku::nodes::pipeline::{Pipeline, PipelineStageNode, StageState};
use akiku::nodes::probabilistic::{DelayDistribution, ProbabilisticDelayNode};
use akiku::types::SimTime;
use akiku::{Event, SimulationEngine, SubgraphConfig};

// ============================================================================
// DelayNode Tests
// ============================================================================

#[test]
fn test_delay_node_basic() {
    let mut node = DelayNode::new(1, 10).with_forward(2, "in");
    node.init();

    let event = Event::message(100, 0, "out", 1, "in", serde_json::json!({"value": 42}));
    let output = node.on_event(event);

    assert_eq!(output.len(), 1);
    assert_eq!(output[0].time, 110); // 100 + 10
}

#[test]
fn test_delay_node_chain() {
    // Create a chain: node1 (delay=5) -> node2 (delay=10) -> node3
    let mut node1 = DelayNode::new(1, 5).with_forward(2, "in");
    let mut node2 = DelayNode::new(2, 10).with_forward(3, "in");

    node1.init();
    node2.init();

    // Start at time 0
    let event = Event::message(0, 0, "out", 1, "in", serde_json::json!({}));
    let out1 = node1.on_event(event);
    assert_eq!(out1[0].time, 5);

    let out2 = node2.on_event(out1[0].clone());
    assert_eq!(out2[0].time, 15); // 5 + 10
}

#[test]
fn test_delay_node_in_event_executor() {
    let mut exec = EventSubgraphExecutor::new(1);

    exec.add_node(1, Box::new(DelayNode::new(1, 20).with_forward(2, "in")));
    exec.add_node(2, Box::new(DelayNode::new(2, 30)));

    exec.init();

    // Schedule an initial event
    exec.schedule_event(Event::message(0, 0, "ext", 1, "in", serde_json::json!({})));

    // Run until time 100
    exec.run_until(100);

    // Events should have propagated through the chain
    let stats = exec.export_stats();
    assert!(stats["events_processed"].as_u64().unwrap() >= 2);
}

// ============================================================================
// ProbabilisticDelayNode Tests
// ============================================================================

#[test]
fn test_probabilistic_delay_fixed() {
    let mut node = ProbabilisticDelayNode::new(1, DelayDistribution::Fixed { delay: 25 })
        .with_forward(2, "in");
    node.init();

    let event = Event::message(100, 0, "out", 1, "in", serde_json::json!({}));
    let output = node.on_event(event);

    assert_eq!(output.len(), 1);
    assert_eq!(output[0].time, 125); // Always 100 + 25
}

#[test]
fn test_probabilistic_delay_uniform() {
    let mut node = ProbabilisticDelayNode::new(
        1,
        DelayDistribution::Uniform { min: 10, max: 20 },
    )
    .with_forward(2, "in")
    .with_seed(42);

    node.init();

    let mut min_delay = SimTime::MAX;
    let mut max_delay = 0;

    for i in 0..100 {
        let event = Event::message(100, 0, "out", 1, "in", serde_json::json!({}));
        let output = node.on_event(event);
        let delay = output[0].time - 100;
        min_delay = min_delay.min(delay);
        max_delay = max_delay.max(delay);
    }

    assert!(min_delay >= 10, "min_delay = {}", min_delay);
    assert!(max_delay <= 20, "max_delay = {}", max_delay);
}

#[test]
fn test_probabilistic_delay_reproducibility() {
    let run_with_seed = |seed: u64| -> Vec<SimTime> {
        let mut node = ProbabilisticDelayNode::new(
            1,
            DelayDistribution::Uniform { min: 0, max: 100 },
        )
        .with_forward(2, "in")
        .with_seed(seed);

        node.init();

        (0..10)
            .map(|_| {
                let event = Event::message(0, 0, "out", 1, "in", serde_json::json!({}));
                node.on_event(event)[0].time
            })
            .collect()
    };

    let run1 = run_with_seed(12345);
    let run2 = run_with_seed(12345);
    let run3 = run_with_seed(54321);

    assert_eq!(run1, run2, "Same seed should produce same results");
    assert_ne!(run1, run3, "Different seeds should produce different results");
}

// ============================================================================
// PipelineStageNode Tests
// ============================================================================

#[test]
fn test_pipeline_stage_basic() {
    let mut stage = PipelineStageNode::new(1, 3).with_next_stage(2, "in");
    stage.init();

    assert_eq!(stage.state, StageState::Idle);
    assert!(stage.can_accept());
}

#[test]
fn test_pipeline_stage_processing() {
    let mut stage = PipelineStageNode::new(1, 2).with_next_stage(2, "in");
    stage.init();

    // Add a work item
    let event = Event::message(0, 0, "out", 1, "in", serde_json::json!({"data": 42}));
    stage.on_event(event);

    // Should start processing on first tick
    stage.on_tick(0);
    assert_eq!(stage.state, StageState::Busy);

    // Continue processing
    stage.on_tick(1);
    assert_eq!(stage.state, StageState::Busy);

    // Should complete and output
    let output = stage.on_tick(2);
    assert_eq!(stage.state, StageState::Idle);
    assert_eq!(output.len(), 1);
    assert_eq!(stage.items_processed, 1);
}

#[test]
fn test_pipeline_stage_backpressure() {
    let mut stage = PipelineStageNode::new(1, 1).with_queue_depth(2);
    stage.init();

    // Fill the queue
    for i in 0..3 {
        let event = Event::message(i, 0, "out", 1, "in", serde_json::json!({}));
        stage.on_event(event);
    }

    assert_eq!(stage.items_received, 3);
    assert_eq!(stage.items_dropped, 1); // Third item dropped
    assert_eq!(stage.queue_occupancy(), 2);
}

#[test]
fn test_pipeline_creation() {
    let pipeline = Pipeline::new(1, 100, vec![2, 3, 2]);

    assert_eq!(pipeline.num_stages, 3);
    assert_eq!(pipeline.stage_ids, vec![100, 101, 102]);

    let stages = pipeline.create_stages(Some(4));
    assert_eq!(stages.len(), 3);
}

#[test]
fn test_pipeline_in_tick_executor() {
    let mut exec = TickSubgraphExecutor::new(1, 1);

    // Create a 3-stage pipeline
    let pipeline = Pipeline::new(1, 100, vec![2, 2, 2]);
    for (id, node) in pipeline.create_stages(Some(4)) {
        exec.add_node(id, node);
    }
    exec.set_execution_order(pipeline.stage_ids.clone());

    exec.init();

    // Inject a work item
    exec.handle_incoming(vec![Event::message(
        0,
        0,
        "ext",
        100,
        "in",
        serde_json::json!({"item": 1}),
    )]);

    // Run for enough time to process through pipeline
    exec.run_until(20);

    let stats = exec.export_stats();
    assert!(stats["ticks_executed"].as_u64().unwrap() >= 10);
}

// ============================================================================
// PassThroughNode Tests
// ============================================================================

#[test]
fn test_simple_passthrough() {
    let mut node = SimplePassThrough::new(1).with_forward(2, "in");
    node.init();

    let event = Event::message(10, 0, "out", 1, "in", serde_json::json!({"value": 42}));
    let output = node.on_event(event);

    assert_eq!(output.len(), 1);
    assert_eq!(output[0].time, 10);
    assert_eq!(node.count, 1);
}

#[test]
fn test_enhanced_passthrough_pass_all() {
    let mut node = EnhancedPassThroughNode::new(1).with_forward(2, "in");
    node.init();

    for i in 0..5 {
        let event = Event::message(i, 0, "out", 1, "in", serde_json::json!({}));
        node.on_event(event);
    }

    assert_eq!(node.events_received, 5);
    assert_eq!(node.events_passed, 5);
    assert_eq!(node.events_filtered, 0);
}

#[test]
fn test_enhanced_passthrough_sample_filter() {
    let mut node = EnhancedPassThroughNode::new(1)
        .with_forward(2, "in")
        .with_filter(FilterMode::Sample { rate: 5 });
    node.init();

    let mut passed = 0;
    for i in 0..20 {
        let event = Event::message(i, 0, "out", 1, "in", serde_json::json!({}));
        passed += node.on_event(event).len();
    }

    assert_eq!(passed, 4); // 0, 5, 10, 15
    assert_eq!(node.events_filtered, 16);
}

#[test]
fn test_enhanced_passthrough_match_filter() {
    let mut node = EnhancedPassThroughNode::new(1)
        .with_forward(2, "in")
        .with_filter(FilterMode::MatchKey {
            key: "priority".to_string(),
            value: serde_json::json!("high"),
        });
    node.init();

    // Should pass
    let event = Event::message(0, 0, "out", 1, "in", serde_json::json!({"priority": "high"}));
    assert_eq!(node.on_event(event).len(), 1);

    // Should filter
    let event = Event::message(1, 0, "out", 1, "in", serde_json::json!({"priority": "low"}));
    assert_eq!(node.on_event(event).len(), 0);

    // Should filter (no key)
    let event = Event::message(2, 0, "out", 1, "in", serde_json::json!({"other": "data"}));
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
    if let akiku::EventPayload::Message { data, .. } = &output[0].payload {
        assert_eq!(data["data"], 42);
        assert_eq!(data["_timestamp"], 100);
        assert_eq!(data["_seq"], 0);
        assert_eq!(data["_node"], 1);
    } else {
        panic!("Expected Message payload");
    }
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_delay_chain_in_engine() {
    let mut engine = SimulationEngine::new(10);

    // Create event-driven subgraph with delay chain
    let mut exec = EventSubgraphExecutor::new(1);
    exec.add_node(1, Box::new(DelayNode::new(1, 15).with_forward(2, "in")));
    exec.add_node(2, Box::new(DelayNode::new(2, 25).with_forward(3, "in")));
    exec.add_node(3, Box::new(DelayNode::new(3, 10)));

    engine.add_subgraph(Box::new(exec), SubgraphConfig::event(1));

    engine.init();

    // Inject initial event
    engine.inject_event(Event::message(0, 0, "ext", 1, "in", serde_json::json!({})));

    // Run simulation
    engine.run(100);

    let stats = engine.export_stats();
    assert!(stats["subgraphs"]["1"]["events_processed"].as_u64().unwrap() >= 3);
}

#[test]
fn test_mixed_pipeline_and_probabilistic() {
    let mut engine = SimulationEngine::new(1);

    // Tick-driven pipeline subgraph
    let mut tick_exec = TickSubgraphExecutor::new(1, 1);
    let pipeline = Pipeline::new(1, 100, vec![2, 2]);
    for (id, node) in pipeline.create_stages(Some(4)) {
        tick_exec.add_node(id, node);
    }
    tick_exec.set_execution_order(pipeline.stage_ids.clone());
    engine.add_subgraph(Box::new(tick_exec), SubgraphConfig::tick(1, 1));

    // Event-driven probabilistic delay subgraph
    let mut event_exec = EventSubgraphExecutor::new(2);
    event_exec.add_node(
        200,
        Box::new(
            ProbabilisticDelayNode::new(200, DelayDistribution::Uniform { min: 5, max: 15 })
                .with_forward(201, "in"),
        ),
    );
    event_exec.add_node(201, Box::new(DelayNode::new(201, 10)));
    engine.add_subgraph(Box::new(event_exec), SubgraphConfig::event(2));

    engine.init();
    engine.run(50);

    assert_eq!(engine.current_time(), 50);
}

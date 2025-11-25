//! Pipeline stage node implementation.
//!
//! The `PipelineStageNode` models a single stage in a pipeline processor,
//! handling backpressure, bubbles, and stage-to-stage communication.

use std::collections::VecDeque;

use crate::event::{Event, EventPayload};
use crate::node::Node;
use crate::types::{NodeId, SimTime};

/// The state of a pipeline stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageState {
    /// Stage is empty and ready to accept input
    Idle,
    /// Stage is processing an item
    Busy,
    /// Stage is stalled waiting for downstream to accept
    Stalled,
}

/// A work item in the pipeline.
#[derive(Clone, Debug)]
pub struct PipelineItem {
    /// Unique identifier for this item
    pub id: u64,
    /// When the item entered this stage
    pub enter_time: SimTime,
    /// Arbitrary data payload
    pub data: serde_json::Value,
    /// Remaining processing cycles
    pub remaining_cycles: u64,
}

/// A node that models a pipeline stage.
///
/// Features:
/// - Configurable processing latency (in ticks)
/// - Input queue with optional depth limit
/// - Backpressure signaling when full
/// - Statistics collection (throughput, utilization, stalls)
///
/// # Example
///
/// ```rust
/// use akiku::nodes::pipeline::PipelineStageNode;
/// use akiku::node::Node;
///
/// // Create a 3-cycle pipeline stage with queue depth 4
/// let mut stage = PipelineStageNode::new(1, 3)
///     .with_queue_depth(4)
///     .with_next_stage(2, "in");
///
/// stage.init();
/// ```
#[derive(Debug)]
pub struct PipelineStageNode {
    /// The node's unique identifier
    pub id: NodeId,
    /// Number of cycles to process an item
    pub latency_cycles: u64,
    /// Current state of the stage
    pub state: StageState,
    /// Item currently being processed (if any)
    current_item: Option<PipelineItem>,
    /// Input queue for pending items
    input_queue: VecDeque<PipelineItem>,
    /// Maximum queue depth (None = unlimited)
    max_queue_depth: Option<usize>,
    /// Next stage in the pipeline
    next_stage: Option<NodeId>,
    /// Port on the next stage
    next_port: String,
    /// Counter for generating item IDs
    item_counter: u64,
    /// Whether downstream is ready to accept
    downstream_ready: bool,

    // Statistics
    /// Total items processed
    pub items_processed: u64,
    /// Total cycles spent idle
    pub idle_cycles: u64,
    /// Total cycles spent busy
    pub busy_cycles: u64,
    /// Total cycles spent stalled
    pub stalled_cycles: u64,
    /// Total items received
    pub items_received: u64,
    /// Items dropped due to queue full
    pub items_dropped: u64,
}

impl PipelineStageNode {
    /// Creates a new pipeline stage node.
    ///
    /// # Arguments
    /// * `id` - The node's unique identifier
    /// * `latency_cycles` - Number of cycles to process each item
    pub fn new(id: NodeId, latency_cycles: u64) -> Self {
        Self {
            id,
            latency_cycles,
            state: StageState::Idle,
            current_item: None,
            input_queue: VecDeque::new(),
            max_queue_depth: None,
            next_stage: None,
            next_port: "in".to_string(),
            item_counter: 0,
            downstream_ready: true,

            items_processed: 0,
            idle_cycles: 0,
            busy_cycles: 0,
            stalled_cycles: 0,
            items_received: 0,
            items_dropped: 0,
        }
    }

    /// Sets the maximum queue depth for backpressure.
    pub fn with_queue_depth(mut self, depth: usize) -> Self {
        self.max_queue_depth = Some(depth);
        self
    }

    /// Sets the next stage in the pipeline.
    pub fn with_next_stage(mut self, stage: NodeId, port: impl Into<String>) -> Self {
        self.next_stage = Some(stage);
        self.next_port = port.into();
        self
    }

    /// Returns whether the stage can accept new input.
    pub fn can_accept(&self) -> bool {
        match self.max_queue_depth {
            Some(max) => self.input_queue.len() < max,
            None => true,
        }
    }

    /// Returns the current queue occupancy.
    pub fn queue_occupancy(&self) -> usize {
        self.input_queue.len()
    }

    /// Returns statistics about the stage.
    pub fn stats(&self) -> serde_json::Value {
        let total_cycles = self.idle_cycles + self.busy_cycles + self.stalled_cycles;
        let utilization = if total_cycles > 0 {
            self.busy_cycles as f64 / total_cycles as f64
        } else {
            0.0
        };

        serde_json::json!({
            "id": self.id,
            "latency_cycles": self.latency_cycles,
            "items_processed": self.items_processed,
            "items_received": self.items_received,
            "items_dropped": self.items_dropped,
            "idle_cycles": self.idle_cycles,
            "busy_cycles": self.busy_cycles,
            "stalled_cycles": self.stalled_cycles,
            "utilization": utilization,
            "current_queue_depth": self.input_queue.len(),
        })
    }

    /// Tries to start processing the next item from the queue.
    fn try_start_processing(&mut self) {
        if self.current_item.is_none() && !self.input_queue.is_empty() {
            if let Some(mut item) = self.input_queue.pop_front() {
                item.remaining_cycles = self.latency_cycles;
                self.current_item = Some(item);
                self.state = StageState::Busy;
            }
        }
    }

    /// Creates an output event for the completed item.
    fn create_output_event(&self, time: SimTime, item: &PipelineItem) -> Option<Event> {
        self.next_stage.map(|target| {
            Event::message(
                time,
                self.id,
                "out",
                target,
                &self.next_port,
                serde_json::json!({
                    "item_id": item.id,
                    "data": item.data,
                    "enter_time": item.enter_time,
                    "exit_time": time,
                }),
            )
        })
    }
}

impl Node for PipelineStageNode {
    fn init(&mut self) {
        self.state = StageState::Idle;
        self.current_item = None;
        self.input_queue.clear();
        self.item_counter = 0;
        self.downstream_ready = true;

        self.items_processed = 0;
        self.idle_cycles = 0;
        self.busy_cycles = 0;
        self.stalled_cycles = 0;
        self.items_received = 0;
        self.items_dropped = 0;
    }

    fn on_tick(&mut self, time: SimTime) -> Vec<Event> {
        let mut events = Vec::new();

        match self.state {
            StageState::Idle => {
                self.idle_cycles += 1;
                self.try_start_processing();
            }
            StageState::Busy => {
                self.busy_cycles += 1;

                let mut should_complete = false;
                let mut item_clone = None;

                if let Some(ref mut item) = self.current_item {
                    if item.remaining_cycles > 0 {
                        item.remaining_cycles -= 1;
                    }

                    if item.remaining_cycles == 0 {
                        should_complete = true;
                        item_clone = Some(item.clone());
                    }
                }

                if should_complete {
                    if self.downstream_ready || self.next_stage.is_none() {
                        // Can forward the item
                        if let Some(ref item) = item_clone {
                            if let Some(event) = self.create_output_event(time, item) {
                                events.push(event);
                            }
                        }
                        self.items_processed += 1;
                        self.current_item = None;
                        self.state = StageState::Idle;
                        self.try_start_processing();
                    } else {
                        // Must wait for downstream
                        self.state = StageState::Stalled;
                    }
                }
            }
            StageState::Stalled => {
                self.stalled_cycles += 1;

                // Check if downstream became ready
                if self.downstream_ready {
                    let item_clone = self.current_item.clone();
                    if let Some(ref item) = item_clone {
                        if let Some(event) = self.create_output_event(time, item) {
                            events.push(event);
                        }
                        self.items_processed += 1;
                    }
                    self.current_item = None;
                    self.state = StageState::Idle;
                    self.try_start_processing();
                }
            }
        }

        events
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        // Handle incoming work items
        if let EventPayload::Message { data, .. } = &event.payload {
            // Check for backpressure signal
            if let Some(ready) = data.get("downstream_ready").and_then(|v| v.as_bool()) {
                self.downstream_ready = ready;
                return Vec::new();
            }

            // Regular work item
            self.items_received += 1;

            if self.can_accept() {
                let item = PipelineItem {
                    id: self.item_counter,
                    enter_time: event.time,
                    data: data.clone(),
                    remaining_cycles: self.latency_cycles,
                };
                self.item_counter += 1;
                self.input_queue.push_back(item);
            } else {
                self.items_dropped += 1;
            }
        }

        Vec::new()
    }
}

/// A complete pipeline composed of multiple stages.
///
/// This is a convenience wrapper that manages a chain of `PipelineStageNode`s.
#[derive(Debug)]
pub struct Pipeline {
    /// Pipeline identifier
    pub id: u64,
    /// Number of stages
    pub num_stages: usize,
    /// Node IDs for each stage
    pub stage_ids: Vec<NodeId>,
    /// Latency per stage
    pub stage_latencies: Vec<u64>,
}

impl Pipeline {
    /// Creates a new pipeline configuration.
    ///
    /// # Arguments
    /// * `id` - Pipeline identifier
    /// * `base_node_id` - Starting node ID (stages will use consecutive IDs)
    /// * `latencies` - Latency for each stage in cycles
    pub fn new(id: u64, base_node_id: NodeId, latencies: Vec<u64>) -> Self {
        let num_stages = latencies.len();
        let stage_ids: Vec<NodeId> = (0..num_stages as u64)
            .map(|i| base_node_id + i)
            .collect();

        Self {
            id,
            num_stages,
            stage_ids,
            stage_latencies: latencies,
        }
    }

    /// Creates the pipeline stage nodes.
    ///
    /// Returns a vector of boxed nodes ready to be added to an executor.
    pub fn create_stages(&self, queue_depth: Option<usize>) -> Vec<(NodeId, Box<dyn Node>)> {
        self.stage_ids
            .iter()
            .enumerate()
            .map(|(i, &id)| {
                let latency = self.stage_latencies[i];
                let mut stage = PipelineStageNode::new(id, latency);

                if let Some(depth) = queue_depth {
                    stage = stage.with_queue_depth(depth);
                }

                // Connect to next stage if not the last one
                if i + 1 < self.num_stages {
                    stage = stage.with_next_stage(self.stage_ids[i + 1], "in");
                }

                (id, Box::new(stage) as Box<dyn Node>)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_stage_creation() {
        let stage = PipelineStageNode::new(1, 3);
        assert_eq!(stage.id, 1);
        assert_eq!(stage.latency_cycles, 3);
        assert_eq!(stage.state, StageState::Idle);
    }

    #[test]
    fn test_pipeline_stage_with_options() {
        let stage = PipelineStageNode::new(1, 3)
            .with_queue_depth(4)
            .with_next_stage(2, "input");

        assert_eq!(stage.max_queue_depth, Some(4));
        assert_eq!(stage.next_stage, Some(2));
        assert_eq!(stage.next_port, "input");
    }

    #[test]
    fn test_pipeline_stage_accepts_items() {
        let mut stage = PipelineStageNode::new(1, 2).with_queue_depth(2);
        stage.init();

        assert!(stage.can_accept());

        // Add items via events
        for i in 0..2 {
            let event = Event::message(i, 0, "out", 1, "in", serde_json::json!({"item": i}));
            stage.on_event(event);
        }

        assert_eq!(stage.queue_occupancy(), 2);
        assert!(!stage.can_accept());
    }

    #[test]
    fn test_pipeline_stage_processes_items() {
        let mut stage = PipelineStageNode::new(1, 2).with_next_stage(2, "in");
        stage.init();

        // Add an item
        let event = Event::message(0, 0, "out", 1, "in", serde_json::json!({"value": 42}));
        stage.on_event(event);

        // Process through ticks
        stage.on_tick(0); // Start processing, remaining = 2
        assert_eq!(stage.state, StageState::Busy);

        stage.on_tick(1); // remaining = 1
        assert_eq!(stage.state, StageState::Busy);

        let events = stage.on_tick(2); // remaining = 0, output
        assert_eq!(stage.state, StageState::Idle);
        assert_eq!(events.len(), 1);
        assert_eq!(stage.items_processed, 1);
    }

    #[test]
    fn test_pipeline_stage_stats() {
        let mut stage = PipelineStageNode::new(1, 1);
        stage.init();

        // Idle ticks
        stage.on_tick(0);
        stage.on_tick(1);

        // Add and process item
        let event = Event::message(2, 0, "out", 1, "in", serde_json::json!({}));
        stage.on_event(event);
        stage.on_tick(2);
        stage.on_tick(3);

        let stats = stage.stats();
        assert!(stats["idle_cycles"].as_u64().unwrap() >= 2);
        assert!(stats["busy_cycles"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn test_pipeline_creation() {
        let pipeline = Pipeline::new(1, 100, vec![2, 3, 2]);

        assert_eq!(pipeline.num_stages, 3);
        assert_eq!(pipeline.stage_ids, vec![100, 101, 102]);
        assert_eq!(pipeline.stage_latencies, vec![2, 3, 2]);
    }

    #[test]
    fn test_pipeline_create_stages() {
        let pipeline = Pipeline::new(1, 100, vec![2, 3]);
        let stages = pipeline.create_stages(Some(4));

        assert_eq!(stages.len(), 2);
        assert_eq!(stages[0].0, 100);
        assert_eq!(stages[1].0, 101);
    }

    #[test]
    fn test_backpressure() {
        let mut stage = PipelineStageNode::new(1, 1)
            .with_queue_depth(1)
            .with_next_stage(2, "in");
        stage.init();

        // Fill the queue
        let event = Event::message(0, 0, "out", 1, "in", serde_json::json!({}));
        stage.on_event(event);

        // This should be dropped
        let event = Event::message(1, 0, "out", 1, "in", serde_json::json!({}));
        stage.on_event(event);

        assert_eq!(stage.items_received, 2);
        assert_eq!(stage.items_dropped, 1);
        assert_eq!(stage.queue_occupancy(), 1);
    }
}

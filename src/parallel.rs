//! Parallel simulation engine using multi-threaded execution.
//!
//! This module provides `ParallelSimulationEngine`, a variant of the
//! simulation engine that executes subgraphs in parallel using rayon.
//!
//! # Feature Flag
//!
//! This module requires the `parallel` feature:
//! ```toml
//! [dependencies]
//! akiku = { version = "0.1", features = ["parallel"] }
//! ```

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::channel::ChannelDesc;
use crate::event::Event;
use crate::executor::SubgraphExecutor;
use crate::subgraph::TimeMode;
use crate::types::{NodeId, SimTime, SubgraphId};

/// Statistics for the parallel engine.
#[derive(Clone, Debug, Default)]
pub struct ParallelEngineStats {
    /// Total simulation steps executed
    pub steps_executed: u64,
    /// Total events dispatched
    pub events_dispatched: u64,
    /// Total events processed
    pub events_processed: u64,
    /// Events dropped due to routing failures
    pub events_dropped: u64,
}

/// Thread-safe event queue for cross-subgraph communication.
#[derive(Debug, Default)]
pub struct ConcurrentEventQueue {
    events: RwLock<Vec<Event>>,
}

impl ConcurrentEventQueue {
    /// Creates a new empty queue.
    pub fn new() -> Self {
        Self {
            events: RwLock::new(Vec::new()),
        }
    }

    /// Pushes events into the queue.
    pub fn push(&self, events: Vec<Event>) {
        let mut guard = self.events.write();
        guard.extend(events);
    }

    /// Takes all events from the queue, leaving it empty.
    pub fn take_all(&self) -> Vec<Event> {
        let mut guard = self.events.write();
        std::mem::take(&mut *guard)
    }

    /// Returns the number of pending events.
    pub fn len(&self) -> usize {
        self.events.read().len()
    }

    /// Returns true if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.events.read().is_empty()
    }
}

/// A wrapper that allows mutable access to a subgraph executor in parallel contexts.
struct SubgraphWrapper {
    executor: RwLock<Box<dyn SubgraphExecutor>>,
    config: SubgraphConfig,
}

/// Configuration for a subgraph.
#[derive(Clone, Debug)]
pub struct SubgraphConfig {
    /// The subgraph ID
    pub id: SubgraphId,
    /// The time mode
    pub time_mode: TimeMode,
}

impl SubgraphConfig {
    /// Creates a new subgraph configuration.
    pub fn new(id: SubgraphId, time_mode: TimeMode) -> Self {
        Self { id, time_mode }
    }

    /// Creates a Tick-mode subgraph configuration.
    pub fn tick(id: SubgraphId, period: SimTime) -> Self {
        Self::new(id, TimeMode::Tick { period })
    }

    /// Creates an Event-mode subgraph configuration.
    pub fn event(id: SubgraphId) -> Self {
        Self::new(id, TimeMode::Event)
    }
}

/// Parallel simulation engine.
///
/// This engine executes subgraphs in parallel when the `parallel` feature
/// is enabled, falling back to sequential execution otherwise.
///
/// # Example
///
/// ```ignore
/// use akiku::parallel::{ParallelSimulationEngine, SubgraphConfig};
///
/// let mut engine = ParallelSimulationEngine::new(10);
/// engine.add_subgraph(executor, SubgraphConfig::tick(1, 10));
/// engine.init();
/// engine.run(1000);
/// ```
pub struct ParallelSimulationEngine {
    /// Subgraph executors wrapped for thread-safe access
    subgraphs: HashMap<SubgraphId, Arc<SubgraphWrapper>>,
    /// Communication channels
    channels: Vec<ChannelDesc>,
    /// Channel lookup map
    channel_map: HashMap<(SubgraphId, SubgraphId), usize>,
    /// Current global time
    current_time: SimTime,
    /// Time step per round
    time_step: SimTime,
    /// Concurrent event queue
    pending_events: ConcurrentEventQueue,
    /// Outgoing events from the current step
    outgoing_events: ConcurrentEventQueue,
    /// Statistics
    stats: RwLock<ParallelEngineStats>,
    /// Number of worker threads (0 = auto)
    num_threads: usize,
}

impl ParallelSimulationEngine {
    /// Creates a new parallel simulation engine.
    pub fn new(time_step: SimTime) -> Self {
        Self {
            subgraphs: HashMap::new(),
            channels: Vec::new(),
            channel_map: HashMap::new(),
            current_time: 0,
            time_step,
            pending_events: ConcurrentEventQueue::new(),
            outgoing_events: ConcurrentEventQueue::new(),
            stats: RwLock::new(ParallelEngineStats::default()),
            num_threads: 0, // Auto
        }
    }

    /// Sets the number of worker threads.
    ///
    /// Pass 0 for automatic detection (uses number of CPUs).
    pub fn with_threads(mut self, threads: usize) -> Self {
        self.num_threads = threads;
        self
    }

    /// Adds a subgraph to the engine.
    pub fn add_subgraph(
        &mut self,
        executor: Box<dyn SubgraphExecutor>,
        config: SubgraphConfig,
    ) {
        let id = config.id;
        let wrapper = Arc::new(SubgraphWrapper {
            executor: RwLock::new(executor),
            config,
        });
        self.subgraphs.insert(id, wrapper);
    }

    /// Adds a communication channel.
    pub fn add_channel(&mut self, channel: ChannelDesc) {
        let key = (channel.src_subgraph, channel.dst_subgraph);
        let index = self.channels.len();
        self.channels.push(channel);
        self.channel_map.insert(key, index);
    }

    /// Returns the current simulation time.
    pub fn current_time(&self) -> SimTime {
        self.current_time
    }

    /// Returns the time step.
    pub fn time_step(&self) -> SimTime {
        self.time_step
    }

    /// Returns the number of subgraphs.
    pub fn subgraph_count(&self) -> usize {
        self.subgraphs.len()
    }

    /// Initializes the engine and all subgraphs.
    pub fn init(&mut self) {
        self.current_time = 0;
        *self.stats.write() = ParallelEngineStats::default();

        for wrapper in self.subgraphs.values() {
            wrapper.executor.write().init();
        }
    }

    /// Injects an event for the next step.
    pub fn inject_event(&self, event: Event) {
        self.pending_events.push(vec![event]);
    }

    /// Finds which subgraph contains a node.
    fn find_subgraph_for_node(&self, node_id: NodeId) -> Option<SubgraphId> {
        // Simple heuristic based on node ID
        let hint = (node_id / 1000) as SubgraphId;
        if self.subgraphs.contains_key(&hint) {
            return Some(hint);
        }

        if self.subgraphs.contains_key(&1) {
            return Some(1);
        }

        self.subgraphs.keys().next().copied()
    }

    /// Dispatches events to their destination subgraphs.
    fn dispatch_events(&self, events: Vec<Event>) {
        let mut by_subgraph: HashMap<SubgraphId, Vec<Event>> = HashMap::new();

        for event in events {
            if let Some(dst_node) = event.payload.dst_node() {
                if let Some(dst_subgraph) = self.find_subgraph_for_node(dst_node) {
                    by_subgraph.entry(dst_subgraph).or_default().push(event);
                    self.stats.write().events_dispatched += 1;
                } else {
                    self.stats.write().events_dropped += 1;
                }
            }
        }

        for (subgraph_id, events) in by_subgraph {
            if let Some(wrapper) = self.subgraphs.get(&subgraph_id) {
                wrapper.executor.write().handle_incoming(events);
            }
        }
    }

    /// Processes outgoing events with channel latency.
    fn process_outgoing(&self, events: Vec<Event>) -> Vec<Event> {
        events
            .into_iter()
            .map(|mut event| {
                self.stats.write().events_processed += 1;

                if let (Some(src), Some(dst)) = (event.payload.src_node(), event.payload.dst_node())
                {
                    let src_sg = self.find_subgraph_for_node(src);
                    let dst_sg = self.find_subgraph_for_node(dst);

                    if let (Some(src_sg), Some(dst_sg)) = (src_sg, dst_sg) {
                        if let Some(&idx) = self.channel_map.get(&(src_sg, dst_sg)) {
                            let channel = &self.channels[idx];
                            let dst_period = self
                                .subgraphs
                                .get(&dst_sg)
                                .map(|w| w.config.time_mode.tick_period())
                                .unwrap_or(0);

                            if let Some(arrival) = channel.compute_arrival_time(event.time, dst_period)
                            {
                                event.time = arrival;
                            }
                        }
                    }
                }

                event
            })
            .collect()
    }

    /// Runs simulation until max_time using sequential execution.
    #[cfg(not(feature = "parallel"))]
    pub fn run(&mut self, max_time: SimTime) {
        while self.current_time < max_time {
            self.step_sequential();
        }
    }

    /// Runs simulation until max_time using parallel execution.
    #[cfg(feature = "parallel")]
    pub fn run(&mut self, max_time: SimTime) {
        // Configure rayon thread pool if needed
        if self.num_threads > 0 {
            rayon::ThreadPoolBuilder::new()
                .num_threads(self.num_threads)
                .build_global()
                .ok(); // Ignore if already configured
        }

        while self.current_time < max_time {
            self.step_parallel();
        }
    }

    /// Executes a single step sequentially.
    fn step_sequential(&mut self) {
        let next_time = self.current_time + self.time_step;

        // Dispatch pending events
        let arrivals = self.pending_events.take_all();
        self.dispatch_events(arrivals);

        // Run all subgraphs
        let mut outgoing = Vec::new();
        for wrapper in self.subgraphs.values() {
            let events = wrapper.executor.write().run_until(next_time);
            outgoing.extend(events);
        }

        // Process outgoing events
        let processed = self.process_outgoing(outgoing);
        self.pending_events.push(processed);

        self.current_time = next_time;
        self.stats.write().steps_executed += 1;
    }

    /// Executes a single step with parallel subgraph execution.
    #[cfg(feature = "parallel")]
    fn step_parallel(&mut self) {
        let next_time = self.current_time + self.time_step;

        // Dispatch pending events (sequential, as it modifies subgraph state)
        let arrivals = self.pending_events.take_all();
        self.dispatch_events(arrivals);

        // Run all subgraphs in parallel
        let subgraph_list: Vec<_> = self.subgraphs.values().collect();
        let all_outgoing: Vec<Vec<Event>> = subgraph_list
            .par_iter()
            .map(|wrapper| {
                wrapper.executor.write().run_until(next_time)
            })
            .collect();

        // Flatten outgoing events
        let outgoing: Vec<Event> = all_outgoing.into_iter().flatten().collect();

        // Process outgoing events
        let processed = self.process_outgoing(outgoing);
        self.pending_events.push(processed);

        self.current_time = next_time;
        self.stats.write().steps_executed += 1;
    }

    /// Runs a single simulation step.
    pub fn step(&mut self) {
        #[cfg(feature = "parallel")]
        self.step_parallel();
        
        #[cfg(not(feature = "parallel"))]
        self.step_sequential();
    }

    /// Returns the engine statistics.
    pub fn stats(&self) -> ParallelEngineStats {
        self.stats.read().clone()
    }

    /// Exports statistics as JSON.
    pub fn export_stats(&self) -> serde_json::Value {
        let stats = self.stats.read();
        let mut subgraph_stats = serde_json::Map::new();

        for (id, wrapper) in &self.subgraphs {
            subgraph_stats.insert(id.to_string(), wrapper.executor.read().export_stats());
        }

        serde_json::json!({
            "engine": {
                "current_time": self.current_time,
                "time_step": self.time_step,
                "steps_executed": stats.steps_executed,
                "events_dispatched": stats.events_dispatched,
                "events_processed": stats.events_processed,
                "events_dropped": stats.events_dropped,
                "subgraph_count": self.subgraphs.len(),
                "channel_count": self.channels.len(),
                "parallel": cfg!(feature = "parallel"),
            },
            "subgraphs": subgraph_stats,
        })
    }
}

// Allow SubgraphWrapper to be sent between threads
unsafe impl Send for SubgraphWrapper {}
unsafe impl Sync for SubgraphWrapper {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::tick::TickSubgraphExecutor;
    use crate::nodes::mock::CounterNode;

    #[test]
    fn test_parallel_engine_creation() {
        let engine = ParallelSimulationEngine::new(10);
        assert_eq!(engine.current_time(), 0);
        assert_eq!(engine.time_step(), 10);
    }

    #[test]
    fn test_parallel_engine_add_subgraph() {
        let mut engine = ParallelSimulationEngine::new(10);

        let exec = TickSubgraphExecutor::new(1, 10);
        engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 10));

        assert_eq!(engine.subgraph_count(), 1);
    }

    #[test]
    fn test_concurrent_event_queue() {
        let queue = ConcurrentEventQueue::new();

        assert!(queue.is_empty());

        queue.push(vec![Event::message(10, 1, "out", 2, "in", serde_json::json!({}))]);
        assert_eq!(queue.len(), 1);

        let events = queue.take_all();
        assert_eq!(events.len(), 1);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_parallel_engine_run() {
        let mut engine = ParallelSimulationEngine::new(10);

        let mut exec = TickSubgraphExecutor::new(1, 10);
        exec.add_node(1, Box::new(CounterNode::new(1)));
        exec.set_execution_order(vec![1]);
        engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 10));

        engine.init();
        engine.run(100);

        assert_eq!(engine.current_time(), 100);
        assert_eq!(engine.stats().steps_executed, 10);
    }

    #[test]
    fn test_parallel_engine_multiple_subgraphs() {
        let mut engine = ParallelSimulationEngine::new(10);

        // Add multiple subgraphs
        for i in 1..=4 {
            let mut exec = TickSubgraphExecutor::new(i, 10);
            exec.add_node(i as u64, Box::new(CounterNode::new(i as u64)));
            exec.set_execution_order(vec![i as u64]);
            engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(i, 10));
        }

        engine.init();
        engine.run(50);

        assert_eq!(engine.current_time(), 50);
        assert_eq!(engine.subgraph_count(), 4);
    }
}

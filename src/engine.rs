//! Global simulation engine (coordinator).
//!
//! The `SimulationEngine` is the top-level coordinator that manages all subgraphs,
//! handles inter-subgraph event routing, and advances the global simulation time.

use std::collections::HashMap;

use crate::channel::ChannelDesc;
use crate::event::Event;
use crate::executor::SubgraphExecutor;
use crate::subgraph::TimeMode;
use crate::types::{SimTime, SubgraphId};

/// Statistics collected by the simulation engine.
#[derive(Clone, Debug, Default)]
pub struct EngineStats {
    /// Total number of time steps executed
    pub steps_executed: u64,
    /// Total number of cross-subgraph events dispatched
    pub events_dispatched: u64,
    /// Total number of cross-subgraph events processed
    pub events_processed: u64,
    /// Number of events dropped due to routing failures
    pub events_dropped: u64,
}

/// Configuration for a subgraph in the engine.
#[derive(Clone, Debug)]
pub struct SubgraphConfig {
    /// The subgraph ID
    pub id: SubgraphId,
    /// The time mode (Tick or Event)
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

/// The global simulation engine.
///
/// The engine coordinates multiple subgraphs, routes events between them,
/// and advances the global simulation time in fixed steps.
///
/// # Example
///
/// ```ignore
/// let mut engine = SimulationEngine::new(10); // 10ns time step
/// engine.add_subgraph(executor, SubgraphConfig::tick(1, 10));
/// engine.add_channel(ChannelDesc::new(1, 2).with_latency(5));
/// engine.init();
/// engine.run(1000); // Run until 1000ns
/// ```
pub struct SimulationEngine {
    /// Subgraph executors indexed by their ID
    subgraphs: HashMap<SubgraphId, Box<dyn SubgraphExecutor>>,
    /// Subgraph configurations indexed by their ID
    configs: HashMap<SubgraphId, SubgraphConfig>,
    /// Communication channels between subgraphs
    channels: Vec<ChannelDesc>,
    /// Current global simulation time
    current_time: SimTime,
    /// Time step for each simulation round (ΔT)
    time_step: SimTime,
    /// Pending cross-subgraph events to be dispatched in the next round
    pending_events: Vec<Event>,
    /// Mapping from (src_subgraph, dst_subgraph) to channel index for fast lookup
    channel_map: HashMap<(SubgraphId, SubgraphId), usize>,
    /// Statistics
    stats: EngineStats,
}

impl SimulationEngine {
    /// Creates a new simulation engine with the given time step.
    ///
    /// # Arguments
    /// * `time_step` - The global time step (ΔT) for each simulation round
    pub fn new(time_step: SimTime) -> Self {
        Self {
            subgraphs: HashMap::new(),
            configs: HashMap::new(),
            channels: Vec::new(),
            current_time: 0,
            time_step,
            pending_events: Vec::new(),
            channel_map: HashMap::new(),
            stats: EngineStats::default(),
        }
    }

    /// Adds a subgraph executor to the engine.
    ///
    /// # Arguments
    /// * `executor` - The subgraph executor
    /// * `config` - Configuration for the subgraph
    pub fn add_subgraph(
        &mut self,
        executor: Box<dyn SubgraphExecutor>,
        config: SubgraphConfig,
    ) {
        let id = config.id;
        self.subgraphs.insert(id, executor);
        self.configs.insert(id, config);
    }

    /// Adds a communication channel between subgraphs.
    ///
    /// # Arguments
    /// * `channel` - The channel description
    pub fn add_channel(&mut self, channel: ChannelDesc) {
        let key = (channel.src_subgraph, channel.dst_subgraph);
        let index = self.channels.len();
        self.channels.push(channel);
        self.channel_map.insert(key, index);
    }

    /// Returns the current global simulation time.
    pub fn current_time(&self) -> SimTime {
        self.current_time
    }

    /// Returns the time step.
    pub fn time_step(&self) -> SimTime {
        self.time_step
    }

    /// Returns the number of registered subgraphs.
    pub fn subgraph_count(&self) -> usize {
        self.subgraphs.len()
    }

    /// Returns a reference to a subgraph executor by ID.
    pub fn get_subgraph(&self, id: SubgraphId) -> Option<&Box<dyn SubgraphExecutor>> {
        self.subgraphs.get(&id)
    }

    /// Returns a mutable reference to a subgraph executor by ID.
    pub fn get_subgraph_mut(&mut self, id: SubgraphId) -> Option<&mut Box<dyn SubgraphExecutor>> {
        self.subgraphs.get_mut(&id)
    }

    /// Injects an external event into the simulation.
    ///
    /// The event will be dispatched to the appropriate subgraph
    /// in the next simulation step.
    pub fn inject_event(&mut self, event: Event) {
        self.pending_events.push(event);
    }

    /// Injects multiple external events into the simulation.
    pub fn inject_events(&mut self, events: impl IntoIterator<Item = Event>) {
        self.pending_events.extend(events);
    }

    /// Initializes the engine and all subgraphs.
    ///
    /// This must be called before `run()`.
    pub fn init(&mut self) {
        self.current_time = 0;
        self.pending_events.clear();
        self.stats = EngineStats::default();

        for executor in self.subgraphs.values_mut() {
            executor.init();
        }
    }

    /// Dispatches pending events to their destination subgraphs.
    ///
    /// Events are grouped by destination subgraph and delivered via
    /// each subgraph's `handle_incoming()` method.
    fn dispatch_to_subgraphs(&mut self, events: Vec<Event>) {
        // Group events by destination subgraph
        let mut events_by_subgraph: HashMap<SubgraphId, Vec<Event>> = HashMap::new();

        for event in events {
            // Determine destination subgraph from event payload
            if let Some(dst_node) = event.payload.dst_node() {
                // Find which subgraph contains this node
                // For now, we use a simple heuristic: the node ID's high bits indicate subgraph
                // In a real implementation, you'd have a node->subgraph mapping
                if let Some(dst_subgraph) = self.find_subgraph_for_node(dst_node) {
                    events_by_subgraph
                        .entry(dst_subgraph)
                        .or_default()
                        .push(event);
                    self.stats.events_dispatched += 1;
                } else {
                    // Node not found in any subgraph
                    self.stats.events_dropped += 1;
                }
            }
        }

        // Deliver events to each subgraph
        for (subgraph_id, events) in events_by_subgraph {
            if let Some(executor) = self.subgraphs.get_mut(&subgraph_id) {
                executor.handle_incoming(events);
            }
        }
    }

    /// Finds which subgraph contains a given node.
    ///
    /// This is a simple implementation that checks each subgraph.
    /// In a production system, you'd maintain a node->subgraph index.
    fn find_subgraph_for_node(&self, node_id: crate::types::NodeId) -> Option<SubgraphId> {
        // Simple heuristic: assume node IDs are prefixed with subgraph ID
        // e.g., node 1_100 belongs to subgraph 1
        // This can be customized based on your node ID scheme
        
        // For now, check if any subgraph has current_time set (meaning it's active)
        // and use node_id / 1000 as subgraph hint
        let hint = (node_id / 1000) as SubgraphId;
        if self.subgraphs.contains_key(&hint) {
            return Some(hint);
        }

        // Fallback: try subgraph 1 if it exists (common case for simple setups)
        if self.subgraphs.contains_key(&1) {
            return Some(1);
        }

        // Return the first available subgraph
        self.subgraphs.keys().next().copied()
    }

    /// Processes outgoing events from subgraphs.
    ///
    /// Applies channel latency and time alignment rules, then
    /// returns events ready for the next dispatch round.
    fn process_outgoing(&mut self, events: Vec<Event>) -> Vec<Event> {
        let mut processed = Vec::new();

        for mut event in events {
            self.stats.events_processed += 1;

            // Find the source subgraph from the event
            let src_node = event.payload.src_node();
            let dst_node = event.payload.dst_node();

            if let (Some(src), Some(dst)) = (src_node, dst_node) {
                let src_subgraph = self.find_subgraph_for_node(src);
                let dst_subgraph = self.find_subgraph_for_node(dst);

                if let (Some(src_sg), Some(dst_sg)) = (src_subgraph, dst_subgraph) {
                    // Check if there's a channel between these subgraphs
                    if let Some(&channel_idx) = self.channel_map.get(&(src_sg, dst_sg)) {
                        let channel = &self.channels[channel_idx];
                        let dst_config = self.configs.get(&dst_sg);
                        let dst_tick_period = dst_config
                            .map(|c| c.time_mode.tick_period())
                            .unwrap_or(0);

                        // Apply channel latency and alignment
                        if let Some(arrival_time) =
                            channel.compute_arrival_time(event.time, dst_tick_period)
                        {
                            event.time = arrival_time;
                        }
                    }
                }
            }

            processed.push(event);
        }

        processed
    }

    /// Runs the simulation until the specified time.
    ///
    /// This is the main simulation loop that:
    /// 1. Dispatches pending events to subgraphs
    /// 2. Advances all subgraphs to the next time step
    /// 3. Collects and processes outgoing events
    ///
    /// # Arguments
    /// * `max_time` - The simulation will run until current_time >= max_time
    pub fn run(&mut self, max_time: SimTime) {
        while self.current_time < max_time {
            self.step();
        }
    }

    /// Executes a single simulation step.
    ///
    /// Advances the simulation by one time_step.
    pub fn step(&mut self) {
        let next_time = self.current_time + self.time_step;

        // 1) Dispatch pending cross-subgraph events
        let arrivals = std::mem::take(&mut self.pending_events);
        self.dispatch_to_subgraphs(arrivals);

        // 2) Advance all subgraphs to next_time and collect outgoing events
        let mut outgoing = Vec::new();
        for executor in self.subgraphs.values_mut() {
            let events = executor.run_until(next_time);
            outgoing.extend(events);
        }

        // 3) Process outgoing events (apply latency/alignment) for next round
        self.pending_events = self.process_outgoing(outgoing);

        self.current_time = next_time;
        self.stats.steps_executed += 1;
    }

    /// Runs the simulation for a specific number of steps.
    pub fn run_steps(&mut self, steps: u64) {
        for _ in 0..steps {
            self.step();
        }
    }

    /// Exports statistics from the engine and all subgraphs.
    pub fn export_stats(&self) -> serde_json::Value {
        let mut subgraph_stats = serde_json::Map::new();
        for (id, executor) in &self.subgraphs {
            subgraph_stats.insert(id.to_string(), executor.export_stats());
        }

        serde_json::json!({
            "engine": {
                "current_time": self.current_time,
                "time_step": self.time_step,
                "steps_executed": self.stats.steps_executed,
                "events_dispatched": self.stats.events_dispatched,
                "events_processed": self.stats.events_processed,
                "events_dropped": self.stats.events_dropped,
                "subgraph_count": self.subgraphs.len(),
                "channel_count": self.channels.len(),
            },
            "subgraphs": subgraph_stats,
        })
    }

    /// Returns the engine statistics.
    pub fn stats(&self) -> &EngineStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::event::EventSubgraphExecutor;
    use crate::executor::tick::TickSubgraphExecutor;
    use crate::nodes::mock::CounterNode;

    #[test]
    fn test_engine_creation() {
        let engine = SimulationEngine::new(10);
        assert_eq!(engine.current_time(), 0);
        assert_eq!(engine.time_step(), 10);
        assert_eq!(engine.subgraph_count(), 0);
    }

    #[test]
    fn test_add_subgraph() {
        let mut engine = SimulationEngine::new(10);

        let exec = TickSubgraphExecutor::new(1, 10);
        engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 10));

        assert_eq!(engine.subgraph_count(), 1);
        assert!(engine.get_subgraph(1).is_some());
        assert!(engine.get_subgraph(2).is_none());
    }

    #[test]
    fn test_add_channel() {
        let mut engine = SimulationEngine::new(10);

        let channel = ChannelDesc::new(1, 2).with_latency(5);
        engine.add_channel(channel);

        assert_eq!(engine.channels.len(), 1);
    }

    #[test]
    fn test_engine_init() {
        let mut engine = SimulationEngine::new(10);

        let mut exec = TickSubgraphExecutor::new(1, 10);
        exec.add_node(1, Box::new(CounterNode::new(1)));
        exec.set_execution_order(vec![1]);
        engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 10));

        engine.init();

        assert_eq!(engine.current_time(), 0);
    }

    #[test]
    fn test_single_step() {
        let mut engine = SimulationEngine::new(10);

        let mut exec = TickSubgraphExecutor::new(1, 10);
        exec.add_node(1, Box::new(CounterNode::new(1)));
        exec.set_execution_order(vec![1]);
        engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 10));

        engine.init();
        engine.step();

        assert_eq!(engine.current_time(), 10);
        assert_eq!(engine.stats().steps_executed, 1);
    }

    #[test]
    fn test_run_until() {
        let mut engine = SimulationEngine::new(10);

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
    fn test_run_steps() {
        let mut engine = SimulationEngine::new(10);

        let exec = EventSubgraphExecutor::new(1);
        engine.add_subgraph(Box::new(exec), SubgraphConfig::event(1));

        engine.init();
        engine.run_steps(5);

        assert_eq!(engine.current_time(), 50);
        assert_eq!(engine.stats().steps_executed, 5);
    }

    #[test]
    fn test_inject_event() {
        let mut engine = SimulationEngine::new(10);

        let mut exec = EventSubgraphExecutor::new(1);
        exec.add_node(1000, Box::new(CounterNode::new(1000)));
        engine.add_subgraph(Box::new(exec), SubgraphConfig::event(1));

        engine.init();
        engine.inject_event(Event::message(5, 0, "ext", 1000, "in", serde_json::json!({})));

        engine.step();

        // Event should have been dispatched
        assert!(engine.stats().events_dispatched >= 1);
    }

    #[test]
    fn test_export_stats() {
        let mut engine = SimulationEngine::new(10);

        let exec = TickSubgraphExecutor::new(1, 10);
        engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 10));

        engine.init();
        engine.run(50);

        let stats = engine.export_stats();
        assert_eq!(stats["engine"]["current_time"], 50);
        assert_eq!(stats["engine"]["steps_executed"], 5);
        assert!(stats["subgraphs"]["1"].is_object());
    }
}


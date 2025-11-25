//! Subgraph executor trait and implementations.
//!
//! A `SubgraphExecutor` is responsible for executing all nodes within a subgraph,
//! managing local time advancement, and handling inter-subgraph communication.

pub mod event;
pub mod tick;

use crate::event::Event;
use crate::types::{SimTime, SubgraphId};

pub use event::EventSubgraphExecutor;
pub use tick::TickSubgraphExecutor;

/// The core trait for subgraph execution units.
///
/// Each subgraph has an executor that manages:
/// - Time advancement (tick-based or event-based)
/// - Node execution in proper order
/// - Incoming event handling from other subgraphs
/// - Outgoing event collection for other subgraphs
///
/// # Implementation Notes
///
/// ## Tick-Driven Executor
/// - Maintains `current_time: SimTime`
/// - Stores topologically sorted node list
/// - `run_until(target)`: Loop `while current_time + period <= target { step_tick(); }`
/// - `step_tick()`: Call each node's `on_tick()` in topological order
///
/// ## Event-Driven Executor
/// - Maintains `current_time: SimTime` and a priority queue (e.g., `BTreeMap<SimTime, Vec<Event>>`)
/// - `run_until(target)`: Pop events with `time <= target`, call corresponding node's `on_event()`
/// - New events are inserted into the queue or sent outward
pub trait SubgraphExecutor: Send {
    /// Returns the unique identifier of this subgraph.
    fn id(&self) -> SubgraphId;

    /// Initialize the executor and all contained nodes.
    ///
    /// Called once before simulation starts.
    fn init(&mut self);

    /// Handle incoming events from other subgraphs.
    ///
    /// These events have already been time-aligned according to the channel's
    /// alignment rules. The executor should incorporate them into its local
    /// event processing.
    ///
    /// # Arguments
    /// * `events` - Events from other subgraphs, with timestamps already adjusted
    fn handle_incoming(&mut self, events: Vec<Event>);

    /// Advance local time to at least `target` and return outgoing events.
    ///
    /// This method advances the subgraph's simulation until its local time
    /// reaches or exceeds `target`. Any events generated that are destined
    /// for other subgraphs should be returned.
    ///
    /// # Arguments
    /// * `target` - The target simulation time to advance to
    ///
    /// # Returns
    /// A vector of events to be sent to other subgraphs
    fn run_until(&mut self, target: SimTime) -> Vec<Event>;

    /// Export statistics collected during simulation.
    ///
    /// Returns a JSON value containing metrics such as:
    /// - Node activation counts
    /// - Event processing totals
    /// - Average/P95/P99 latencies
    /// - Queue depth distributions
    /// - Resource utilization
    fn export_stats(&self) -> serde_json::Value;

    /// Returns the current local time of this subgraph.
    fn current_time(&self) -> SimTime;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal mock executor for testing the trait definition.
    struct MockExecutor {
        subgraph_id: SubgraphId,
        time: SimTime,
        incoming_count: usize,
        run_count: usize,
    }

    impl MockExecutor {
        fn new(id: SubgraphId) -> Self {
            Self {
                subgraph_id: id,
                time: 0,
                incoming_count: 0,
                run_count: 0,
            }
        }
    }

    impl SubgraphExecutor for MockExecutor {
        fn id(&self) -> SubgraphId {
            self.subgraph_id
        }

        fn init(&mut self) {
            self.time = 0;
            self.incoming_count = 0;
            self.run_count = 0;
        }

        fn handle_incoming(&mut self, events: Vec<Event>) {
            self.incoming_count += events.len();
        }

        fn run_until(&mut self, target: SimTime) -> Vec<Event> {
            self.run_count += 1;
            self.time = target;
            Vec::new()
        }

        fn export_stats(&self) -> serde_json::Value {
            serde_json::json!({
                "subgraph_id": self.subgraph_id,
                "incoming_count": self.incoming_count,
                "run_count": self.run_count,
                "current_time": self.time,
            })
        }

        fn current_time(&self) -> SimTime {
            self.time
        }
    }

    #[test]
    fn test_mock_executor_basic() {
        let mut exec = MockExecutor::new(1);
        exec.init();

        assert_eq!(exec.id(), 1);
        assert_eq!(exec.current_time(), 0);

        let events = exec.run_until(100);
        assert!(events.is_empty());
        assert_eq!(exec.current_time(), 100);
        assert_eq!(exec.run_count, 1);
    }

    #[test]
    fn test_mock_executor_incoming() {
        let mut exec = MockExecutor::new(2);
        exec.init();

        let events = vec![Event::custom(10, "event1"), Event::custom(20, "event2")];
        exec.handle_incoming(events);

        assert_eq!(exec.incoming_count, 2);
    }

    #[test]
    fn test_mock_executor_stats() {
        let mut exec = MockExecutor::new(3);
        exec.init();
        exec.run_until(50);
        exec.run_until(100);

        let stats = exec.export_stats();
        assert_eq!(stats["subgraph_id"], 3);
        assert_eq!(stats["run_count"], 2);
        assert_eq!(stats["current_time"], 100);
    }
}



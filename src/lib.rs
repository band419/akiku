//! # Akiku Simulation Framework
//!
//! A generic, high-performance graph-driven simulation engine for behavioral
//! and performance simulation of systems described by dependency topology graphs.
//!
//! ## Design Principles
//!
//! - **Graph-Driven**: Simulation objects are described through directed dependency
//!   topology graphs, which serve as the "source of truth" for the engine.
//! - **Subgraph + Parallelism**: The global graph is divided into subgraphs that
//!   can be mapped to multi-threaded/multi-process parallel execution.
//! - **Dual Time Mechanism**:
//!   - **Tick-Driven**: Cycle-driven, suitable for fine-grained pipelines.
//!   - **Event-Driven**: Event-driven, for functional/transactional/statistical models.
//! - **Unified Timeline**: All subgraphs share a unified timeline (`SimTime`).
//!
//! ## Features
//!
//! - `parallel` - Enable parallel subgraph execution using rayon
//!
//! ## Quick Start
//!
//! ```rust
//! use akiku::{SimulationEngine, SubgraphConfig, Event};
//! use akiku::executor::tick::TickSubgraphExecutor;
//! use akiku::nodes::mock::CounterNode;
//!
//! // Create engine with 10ns time step
//! let mut engine = SimulationEngine::new(10);
//!
//! // Create and add a tick subgraph
//! let mut exec = TickSubgraphExecutor::new(1, 10);
//! exec.add_node(1, Box::new(CounterNode::new(1)));
//! exec.set_execution_order(vec![1]);
//! engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 10));
//!
//! // Run simulation
//! engine.init();
//! engine.run(100);
//!
//! // Get statistics
//! let stats = engine.export_stats();
//! println!("Final time: {}", stats["engine"]["current_time"]);
//! ```
//!
//! ## Parallel Execution
//!
//! Enable the `parallel` feature for multi-threaded subgraph execution:
//!
//! ```rust,ignore
//! use akiku::parallel::{ParallelSimulationEngine, SubgraphConfig};
//!
//! let mut engine = ParallelSimulationEngine::new(10)
//!     .with_threads(4);  // Use 4 worker threads
//! // ... add subgraphs
//! engine.run(1000);
//! ```
//!
//! ## Configuration-Driven Setup
//!
//! ```rust,ignore
//! use akiku::config::SimConfig;
//!
//! let config = SimConfig::from_yaml_file("simulation.yaml")?;
//! // ... build engine from config
//! ```

pub mod types;
pub mod event;
pub mod node;
pub mod channel;
pub mod subgraph;
pub mod executor;
pub mod engine;
pub mod config;
pub mod registry;
pub mod stats;
pub mod nodes;
pub mod parallel;

// Re-export commonly used types
pub use types::{SimTime, NodeId, SubgraphId};
pub use event::{Event, EventPayload};
pub use node::{Node, NodeKind, NodePort, NodeDesc};
pub use channel::{ChannelDesc, ChannelRegistry, ChannelResult, TimeAlignment};
pub use subgraph::{TimeMode, SubgraphDesc};
pub use executor::{EventSubgraphExecutor, SubgraphExecutor, TickSubgraphExecutor};
pub use engine::{SimulationEngine, SubgraphConfig, EngineStats};
pub use config::{SimConfig, SimConfigBuilder, ConfigError};
pub use registry::{NodeRegistry, create_default_registry};
pub use stats::{SimulationStats, StatsCollector, Timer};

// Re-export parallel module types
pub use parallel::{ParallelSimulationEngine, ConcurrentEventQueue, ParallelEngineStats};

/// Initialize the tracing subscriber for logging.
///
/// Call this at the start of your program to enable logging.
///
/// # Example
///
/// ```rust,ignore
/// akiku::init_logging("info");
/// ```
pub fn init_logging(level: &str) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}

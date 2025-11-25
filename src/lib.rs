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

pub mod types;
pub mod event;
pub mod node;
pub mod channel;
pub mod subgraph;
pub mod executor;
pub mod nodes;

// Re-export commonly used types
pub use types::{SimTime, NodeId, SubgraphId};
pub use event::{Event, EventPayload};
pub use node::{Node, NodeKind, NodePort, NodeDesc};
pub use channel::{ChannelDesc, TimeAlignment};
pub use subgraph::{TimeMode, SubgraphDesc};
pub use executor::{EventSubgraphExecutor, SubgraphExecutor, TickSubgraphExecutor};


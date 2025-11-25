//! Built-in node implementations.
//!
//! This module contains various pre-built node types that can be used
//! directly or as references for implementing custom nodes.
//!
//! # Available Nodes
//!
//! ## Basic Nodes (mock)
//! - [`CounterNode`] - Counts tick/event invocations
//! - [`EchoNode`] - Returns events with configurable delay
//! - [`SourceNode`] - Generates events at each tick
//!
//! ## Delay Nodes
//! - [`DelayNode`] - Adds fixed delay to events
//! - [`ProbabilisticDelayNode`] - Adds probabilistic delay (uniform, normal, exponential)
//!
//! ## Pipeline Nodes
//! - [`PipelineStageNode`] - Models a pipeline stage with backpressure
//! - [`Pipeline`] - Convenience wrapper for multi-stage pipelines
//!
//! ## Pass-Through Nodes
//! - [`EnhancedPassThroughNode`] - Filtering and transformation
//! - [`SimplePassThrough`] - Minimal forwarding

pub mod mock;
pub mod delay;
pub mod probabilistic;
pub mod pipeline;
pub mod passthrough;

// Re-export mock nodes
pub use mock::{CounterNode, EchoNode, PassThroughNode, SourceNode};

// Re-export delay nodes
pub use delay::DelayNode;
pub use probabilistic::{DelayDistribution, ProbabilisticDelayNode};

// Re-export pipeline nodes
pub use pipeline::{Pipeline, PipelineItem, PipelineStageNode, StageState};

// Re-export passthrough nodes
pub use passthrough::{EnhancedPassThroughNode, FilterMode, SimplePassThrough, TransformMode};


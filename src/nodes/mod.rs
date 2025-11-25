//! Built-in node implementations.
//!
//! This module contains various pre-built node types that can be used
//! directly or as references for implementing custom nodes.

pub mod mock;

pub use mock::{CounterNode, EchoNode, PassThroughNode, SourceNode};


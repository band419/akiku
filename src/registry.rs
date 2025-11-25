//! Node factory registry for dynamic node creation.
//!
//! The registry allows node implementations to be registered by name,
//! enabling configuration-driven simulation setup.
//!
//! # Example
//!
//! ```
//! use akiku::registry::NodeRegistry;
//! use akiku::node::Node;
//! use akiku::types::NodeId;
//! use std::collections::HashMap;
//!
//! // Define a custom node
//! struct MyNode { id: NodeId }
//! impl Node for MyNode {}
//!
//! // Create and populate registry
//! let mut registry = NodeRegistry::new();
//! registry.register("MyNode", |id, _attrs| Box::new(MyNode { id }));
//!
//! // Create node from registry
//! let node = registry.create("MyNode", 1, &HashMap::new()).unwrap();
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use crate::node::Node;
use crate::types::NodeId;

/// Type alias for node factory functions.
pub type NodeFactory = Arc<dyn Fn(NodeId, &HashMap<String, String>) -> Box<dyn Node> + Send + Sync>;

/// A registry for node factories.
///
/// Allows registering node types by name and creating instances dynamically.
#[derive(Default)]
pub struct NodeRegistry {
    factories: HashMap<String, NodeFactory>,
}

impl NodeRegistry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a node factory with the given name.
    ///
    /// # Arguments
    /// * `name` - The type name to register
    /// * `factory` - A function that creates node instances
    ///
    /// # Example
    ///
    /// ```
    /// use akiku::registry::NodeRegistry;
    /// use akiku::nodes::mock::CounterNode;
    ///
    /// let mut registry = NodeRegistry::new();
    /// registry.register("Counter", |id, _| Box::new(CounterNode::new(id)));
    /// ```
    pub fn register<F>(&mut self, name: impl Into<String>, factory: F)
    where
        F: Fn(NodeId, &HashMap<String, String>) -> Box<dyn Node> + Send + Sync + 'static,
    {
        self.factories.insert(name.into(), Arc::new(factory));
    }

    /// Creates a node instance by type name.
    ///
    /// # Arguments
    /// * `type_name` - The registered type name
    /// * `id` - The node ID to assign
    /// * `attrs` - Attributes to pass to the factory
    ///
    /// # Returns
    /// `Some(node)` if the type is registered, `None` otherwise
    pub fn create(
        &self,
        type_name: &str,
        id: NodeId,
        attrs: &HashMap<String, String>,
    ) -> Option<Box<dyn Node>> {
        self.factories.get(type_name).map(|f| f(id, attrs))
    }

    /// Returns true if a type is registered.
    pub fn contains(&self, type_name: &str) -> bool {
        self.factories.contains_key(type_name)
    }

    /// Returns the number of registered types.
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Returns true if no types are registered.
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    /// Returns an iterator over registered type names.
    pub fn type_names(&self) -> impl Iterator<Item = &String> {
        self.factories.keys()
    }

    /// Unregisters a node type.
    pub fn unregister(&mut self, type_name: &str) -> bool {
        self.factories.remove(type_name).is_some()
    }

    /// Clears all registered types.
    pub fn clear(&mut self) {
        self.factories.clear();
    }
}

impl std::fmt::Debug for NodeRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeRegistry")
            .field("registered_types", &self.factories.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Creates a default registry with built-in node types.
///
/// Includes:
/// - `Counter` - CounterNode
/// - `Source` - SourceNode
/// - `Echo` - EchoNode
/// - `PassThrough` - PassThroughNode
pub fn create_default_registry() -> NodeRegistry {
    use crate::nodes::mock::{CounterNode, EchoNode, PassThroughNode, SourceNode};

    let mut registry = NodeRegistry::new();

    registry.register("Counter", |id, _| Box::new(CounterNode::new(id)));

    registry.register("Source", |id, attrs| {
        let target = attrs
            .get("target")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let port = attrs.get("port").cloned().unwrap_or_else(|| "in".to_string());
        Box::new(SourceNode::new(id, target, port))
    });

    registry.register("Echo", |id, attrs| {
        let delay = attrs
            .get("delay")
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        Box::new(EchoNode::new(id, delay))
    });

    registry.register("PassThrough", |id, attrs| {
        let target = attrs
            .get("target")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let port = attrs.get("port").cloned().unwrap_or_else(|| "in".to_string());
        Box::new(PassThroughNode::new(id, target, port))
    });

    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::mock::CounterNode;

    #[test]
    fn test_registry_basic() {
        let mut registry = NodeRegistry::new();
        assert!(registry.is_empty());

        registry.register("Test", |id, _| Box::new(CounterNode::new(id)));
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("Test"));
    }

    #[test]
    fn test_registry_create() {
        let mut registry = NodeRegistry::new();
        registry.register("Counter", |id, _| Box::new(CounterNode::new(id)));

        let attrs = HashMap::new();
        let node = registry.create("Counter", 42, &attrs);
        assert!(node.is_some());

        let missing = registry.create("NonExistent", 1, &attrs);
        assert!(missing.is_none());
    }

    #[test]
    fn test_registry_with_attrs() {
        let mut registry = NodeRegistry::new();

        // Register a factory that uses attributes
        registry.register("Configurable", |id, attrs| {
            let _value = attrs.get("value").cloned().unwrap_or_default();
            Box::new(CounterNode::new(id))
        });

        let mut attrs = HashMap::new();
        attrs.insert("value".to_string(), "42".to_string());

        let node = registry.create("Configurable", 1, &attrs);
        assert!(node.is_some());
    }

    #[test]
    fn test_default_registry() {
        let registry = create_default_registry();

        assert!(registry.contains("Counter"));
        assert!(registry.contains("Source"));
        assert!(registry.contains("Echo"));
        assert!(registry.contains("PassThrough"));
        assert_eq!(registry.len(), 4);
    }

    #[test]
    fn test_registry_unregister() {
        let mut registry = NodeRegistry::new();
        registry.register("Test", |id, _| Box::new(CounterNode::new(id)));

        assert!(registry.contains("Test"));
        assert!(registry.unregister("Test"));
        assert!(!registry.contains("Test"));
        assert!(!registry.unregister("Test")); // Already removed
    }

    #[test]
    fn test_registry_type_names() {
        let mut registry = NodeRegistry::new();
        registry.register("A", |id, _| Box::new(CounterNode::new(id)));
        registry.register("B", |id, _| Box::new(CounterNode::new(id)));
        registry.register("C", |id, _| Box::new(CounterNode::new(id)));

        let names: Vec<_> = registry.type_names().collect();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&&"A".to_string()));
        assert!(names.contains(&&"B".to_string()));
        assert!(names.contains(&&"C".to_string()));
    }
}


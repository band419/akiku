//! Configuration system for the simulation framework.
//!
//! This module provides YAML/JSON configuration file support for defining
//! simulations declaratively.
//!
//! # Configuration File Structure
//!
//! ```yaml
//! simulation:
//!   max_time: 1000
//!   time_step: 10
//!   
//! subgraphs:
//!   - id: 1
//!     mode: tick
//!     tick_period: 10
//!     nodes: [1, 2, 3]
//!   - id: 2
//!     mode: event
//!     nodes: [4, 5]
//!     
//! channels:
//!   - src: 1
//!     dst: 2
//!     latency: 5
//!     alignment: CeilToTick
//!
//! nodes:
//!   - id: 1
//!     type: Counter
//!     attrs:
//!       initial_value: "0"
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

use crate::channel::TimeAlignment;
use crate::node::NodeKind;
use crate::types::{NodeId, SimTime, SubgraphId};

/// Errors that can occur during configuration loading.
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Unknown file format: {0}")]
    UnknownFormat(String),
}

/// Result type for configuration operations.
pub type ConfigResult<T> = Result<T, ConfigError>;

/// Global simulation parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimulationParams {
    /// Maximum simulation time
    #[serde(default = "default_max_time")]
    pub max_time: SimTime,

    /// Global time step (ΔT)
    #[serde(default = "default_time_step")]
    pub time_step: SimTime,

    /// Logging level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Whether to collect detailed statistics
    #[serde(default)]
    pub collect_stats: bool,

    /// Output directory for results
    #[serde(default)]
    pub output_dir: Option<String>,
}

fn default_max_time() -> SimTime {
    1000
}

fn default_time_step() -> SimTime {
    10
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for SimulationParams {
    fn default() -> Self {
        Self {
            max_time: default_max_time(),
            time_step: default_time_step(),
            log_level: default_log_level(),
            collect_stats: false,
            output_dir: None,
        }
    }
}

/// Subgraph time mode configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubgraphMode {
    Tick,
    Event,
}

/// Configuration for a single subgraph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubgraphConfig {
    /// Unique subgraph identifier
    pub id: SubgraphId,

    /// Time mode (tick or event)
    pub mode: SubgraphMode,

    /// Tick period (required for tick mode)
    #[serde(default)]
    pub tick_period: Option<SimTime>,

    /// Node IDs contained in this subgraph
    #[serde(default)]
    pub nodes: Vec<NodeId>,

    /// Optional thread affinity hint
    #[serde(default)]
    pub thread_affinity: Option<usize>,
}

impl SubgraphConfig {
    /// Validates the subgraph configuration.
    pub fn validate(&self) -> ConfigResult<()> {
        match self.mode {
            SubgraphMode::Tick => {
                if self.tick_period.is_none() || self.tick_period == Some(0) {
                    return Err(ConfigError::Validation(format!(
                        "Subgraph {} is tick mode but has no valid tick_period",
                        self.id
                    )));
                }
            }
            SubgraphMode::Event => {
                if self.tick_period.is_some() {
                    tracing::warn!(
                        "Subgraph {} is event mode but has tick_period set (ignored)",
                        self.id
                    );
                }
            }
        }
        Ok(())
    }
}

/// Configuration for a channel between subgraphs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// Source subgraph ID
    pub src: SubgraphId,

    /// Destination subgraph ID
    pub dst: SubgraphId,

    /// Base latency in SimTime units
    #[serde(default)]
    pub latency: SimTime,

    /// Time alignment rule
    #[serde(default)]
    pub alignment: TimeAlignment,

    /// Optional capacity limit
    #[serde(default)]
    pub capacity: Option<usize>,
}

/// Configuration for a node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Unique node identifier
    pub id: NodeId,

    /// Node type/implementation name
    #[serde(rename = "type")]
    pub node_type: String,

    /// Node kind (optional, can be inferred from type)
    #[serde(default)]
    pub kind: Option<NodeKind>,

    /// Custom attributes as key-value pairs
    #[serde(default)]
    pub attrs: HashMap<String, String>,

    /// Input port names
    #[serde(default)]
    pub inputs: Vec<String>,

    /// Output port names
    #[serde(default)]
    pub outputs: Vec<String>,
}

/// Complete simulation configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimConfig {
    /// Global simulation parameters
    #[serde(default)]
    pub simulation: SimulationParams,

    /// Subgraph definitions
    #[serde(default)]
    pub subgraphs: Vec<SubgraphConfig>,

    /// Channel definitions
    #[serde(default)]
    pub channels: Vec<ChannelConfig>,

    /// Node definitions
    #[serde(default)]
    pub nodes: Vec<NodeConfig>,
}

impl SimConfig {
    /// Creates a new empty configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads configuration from a YAML file.
    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> ConfigResult<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    /// Loads configuration from a YAML string.
    pub fn from_yaml(yaml: &str) -> ConfigResult<Self> {
        let config: SimConfig = serde_yaml::from_str(yaml)?;
        config.validate()?;
        Ok(config)
    }

    /// Loads configuration from a JSON file.
    pub fn from_json_file<P: AsRef<Path>>(path: P) -> ConfigResult<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    /// Loads configuration from a JSON string.
    pub fn from_json(json: &str) -> ConfigResult<Self> {
        let config: SimConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    /// Loads configuration from a file, auto-detecting format.
    pub fn from_file<P: AsRef<Path>>(path: P) -> ConfigResult<Self> {
        let path = path.as_ref();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        match ext.to_lowercase().as_str() {
            "yaml" | "yml" => Self::from_yaml_file(path),
            "json" => Self::from_json_file(path),
            _ => Err(ConfigError::UnknownFormat(ext.to_string())),
        }
    }

    /// Validates the entire configuration.
    pub fn validate(&self) -> ConfigResult<()> {
        // Validate subgraphs
        let mut subgraph_ids = std::collections::HashSet::new();
        for sg in &self.subgraphs {
            sg.validate()?;
            if !subgraph_ids.insert(sg.id) {
                return Err(ConfigError::Validation(format!(
                    "Duplicate subgraph ID: {}",
                    sg.id
                )));
            }
        }

        // Validate channels reference existing subgraphs
        for ch in &self.channels {
            if !subgraph_ids.contains(&ch.src) {
                return Err(ConfigError::Validation(format!(
                    "Channel references non-existent source subgraph: {}",
                    ch.src
                )));
            }
            if !subgraph_ids.contains(&ch.dst) {
                return Err(ConfigError::Validation(format!(
                    "Channel references non-existent destination subgraph: {}",
                    ch.dst
                )));
            }
        }

        // Validate nodes have unique IDs
        let mut node_ids = std::collections::HashSet::new();
        for node in &self.nodes {
            if !node_ids.insert(node.id) {
                return Err(ConfigError::Validation(format!(
                    "Duplicate node ID: {}",
                    node.id
                )));
            }
        }

        Ok(())
    }

    /// Saves configuration to a YAML file.
    pub fn to_yaml_file<P: AsRef<Path>>(&self, path: P) -> ConfigResult<()> {
        let yaml = serde_yaml::to_string(self)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }

    /// Saves configuration to a JSON file.
    pub fn to_json_file<P: AsRef<Path>>(&self, path: P) -> ConfigResult<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Converts to YAML string.
    pub fn to_yaml(&self) -> ConfigResult<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    /// Converts to JSON string.
    pub fn to_json(&self) -> ConfigResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Returns the number of subgraphs.
    pub fn subgraph_count(&self) -> usize {
        self.subgraphs.len()
    }

    /// Returns the number of channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Returns the number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Finds a subgraph configuration by ID.
    pub fn find_subgraph(&self, id: SubgraphId) -> Option<&SubgraphConfig> {
        self.subgraphs.iter().find(|sg| sg.id == id)
    }

    /// Finds a node configuration by ID.
    pub fn find_node(&self, id: NodeId) -> Option<&NodeConfig> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            simulation: SimulationParams::default(),
            subgraphs: Vec::new(),
            channels: Vec::new(),
            nodes: Vec::new(),
        }
    }
}

/// Builder for creating SimConfig programmatically.
#[derive(Default)]
pub struct SimConfigBuilder {
    config: SimConfig,
}

impl SimConfigBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum simulation time.
    pub fn max_time(mut self, time: SimTime) -> Self {
        self.config.simulation.max_time = time;
        self
    }

    /// Sets the global time step.
    pub fn time_step(mut self, step: SimTime) -> Self {
        self.config.simulation.time_step = step;
        self
    }

    /// Sets the log level.
    pub fn log_level(mut self, level: impl Into<String>) -> Self {
        self.config.simulation.log_level = level.into();
        self
    }

    /// Enables statistics collection.
    pub fn collect_stats(mut self, enable: bool) -> Self {
        self.config.simulation.collect_stats = enable;
        self
    }

    /// Adds a tick subgraph.
    pub fn add_tick_subgraph(mut self, id: SubgraphId, period: SimTime, nodes: Vec<NodeId>) -> Self {
        self.config.subgraphs.push(SubgraphConfig {
            id,
            mode: SubgraphMode::Tick,
            tick_period: Some(period),
            nodes,
            thread_affinity: None,
        });
        self
    }

    /// Adds an event subgraph.
    pub fn add_event_subgraph(mut self, id: SubgraphId, nodes: Vec<NodeId>) -> Self {
        self.config.subgraphs.push(SubgraphConfig {
            id,
            mode: SubgraphMode::Event,
            tick_period: None,
            nodes,
            thread_affinity: None,
        });
        self
    }

    /// Adds a channel.
    pub fn add_channel(
        mut self,
        src: SubgraphId,
        dst: SubgraphId,
        latency: SimTime,
        alignment: TimeAlignment,
    ) -> Self {
        self.config.channels.push(ChannelConfig {
            src,
            dst,
            latency,
            alignment,
            capacity: None,
        });
        self
    }

    /// Adds a node configuration.
    pub fn add_node(mut self, id: NodeId, node_type: impl Into<String>) -> Self {
        self.config.nodes.push(NodeConfig {
            id,
            node_type: node_type.into(),
            kind: None,
            attrs: HashMap::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
        });
        self
    }

    /// Builds and validates the configuration.
    pub fn build(self) -> ConfigResult<SimConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SimConfig::new();
        assert_eq!(config.simulation.max_time, 1000);
        assert_eq!(config.simulation.time_step, 10);
        assert!(config.subgraphs.is_empty());
    }

    #[test]
    fn test_yaml_parsing() {
        let yaml = r#"
simulation:
  max_time: 5000
  time_step: 20
  log_level: debug

subgraphs:
  - id: 1
    mode: tick
    tick_period: 10
    nodes: [100, 101, 102]
  - id: 2
    mode: event
    nodes: [200, 201]

channels:
  - src: 1
    dst: 2
    latency: 5
    alignment: CeilToTick

nodes:
  - id: 100
    type: Counter
    attrs:
      initial: "0"
"#;

        let config = SimConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.simulation.max_time, 5000);
        assert_eq!(config.simulation.time_step, 20);
        assert_eq!(config.subgraphs.len(), 2);
        assert_eq!(config.channels.len(), 1);
        assert_eq!(config.nodes.len(), 1);
    }

    #[test]
    fn test_json_parsing() {
        let json = r#"{
            "simulation": {
                "max_time": 1000,
                "time_step": 10
            },
            "subgraphs": [
                {"id": 1, "mode": "tick", "tick_period": 10, "nodes": [1, 2]}
            ],
            "channels": [],
            "nodes": []
        }"#;

        let config = SimConfig::from_json(json).unwrap();
        assert_eq!(config.simulation.max_time, 1000);
        assert_eq!(config.subgraphs.len(), 1);
    }

    #[test]
    fn test_builder() {
        let config = SimConfigBuilder::new()
            .max_time(2000)
            .time_step(5)
            .add_tick_subgraph(1, 10, vec![100, 101])
            .add_event_subgraph(2, vec![200])
            .add_channel(1, 2, 5, TimeAlignment::CeilToTick)
            .build()
            .unwrap();

        assert_eq!(config.simulation.max_time, 2000);
        assert_eq!(config.subgraph_count(), 2);
        assert_eq!(config.channel_count(), 1);
    }

    #[test]
    fn test_validation_duplicate_subgraph() {
        let yaml = r#"
subgraphs:
  - id: 1
    mode: event
  - id: 1
    mode: tick
    tick_period: 10
"#;
        let result = SimConfig::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_missing_tick_period() {
        let yaml = r#"
subgraphs:
  - id: 1
    mode: tick
    nodes: []
"#;
        let result = SimConfig::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_invalid_channel() {
        let yaml = r#"
subgraphs:
  - id: 1
    mode: event
channels:
  - src: 1
    dst: 99
    latency: 5
"#;
        let result = SimConfig::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let config = SimConfigBuilder::new()
            .max_time(1000)
            .add_tick_subgraph(1, 10, vec![1, 2, 3])
            .build()
            .unwrap();

        let yaml = config.to_yaml().unwrap();
        let restored = SimConfig::from_yaml(&yaml).unwrap();

        assert_eq!(config.simulation.max_time, restored.simulation.max_time);
        assert_eq!(config.subgraph_count(), restored.subgraph_count());
    }
}


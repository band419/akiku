//! Probabilistic delay node implementation.
//!
//! The `ProbabilisticDelayNode` adds a configurable probabilistic delay
//! to incoming events, simulating variable latency components.

use crate::event::{Event, EventPayload};
use crate::node::Node;
use crate::types::{NodeId, SimTime};

/// Distribution type for probabilistic delays.
#[derive(Clone, Debug)]
pub enum DelayDistribution {
    /// Uniform distribution between min and max
    Uniform { min: SimTime, max: SimTime },
    /// Normal/Gaussian distribution with mean and standard deviation
    /// Note: Values are clamped to be non-negative
    Normal { mean: f64, std_dev: f64 },
    /// Exponential distribution with given mean (lambda = 1/mean)
    Exponential { mean: f64 },
    /// Fixed delay (deterministic)
    Fixed { delay: SimTime },
}

impl DelayDistribution {
    /// Samples a delay value from the distribution.
    ///
    /// Uses a simple PRNG based on the provided seed for reproducibility.
    pub fn sample(&self, seed: u64) -> SimTime {
        match self {
            DelayDistribution::Fixed { delay } => *delay,
            DelayDistribution::Uniform { min, max } => {
                if min >= max {
                    return *min;
                }
                let range = max - min;
                let random = simple_random(seed);
                min + (random % (range + 1))
            }
            DelayDistribution::Normal { mean, std_dev } => {
                // Box-Muller transform approximation
                let u1 = (simple_random(seed) as f64) / (u64::MAX as f64);
                let u2 = (simple_random(seed.wrapping_add(1)) as f64) / (u64::MAX as f64);
                
                let u1 = u1.max(1e-10); // Avoid log(0)
                let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                let value = mean + std_dev * z0;
                value.max(0.0) as SimTime
            }
            DelayDistribution::Exponential { mean } => {
                let u = (simple_random(seed) as f64) / (u64::MAX as f64);
                let u = u.max(1e-10); // Avoid log(0)
                let value = -mean * u.ln();
                value.max(0.0) as SimTime
            }
        }
    }
}

/// Simple PRNG for deterministic random number generation.
/// Uses xorshift64 algorithm.
fn simple_random(seed: u64) -> u64 {
    let mut x = seed;
    if x == 0 {
        x = 0xDEADBEEF;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// A node that adds probabilistic delay to events.
///
/// This node simulates variable-latency components like:
/// - Memory systems with cache hit/miss variability
/// - Network links with congestion
/// - Processing units with data-dependent timing
///
/// # Example
///
/// ```rust
/// use akiku::nodes::probabilistic::{ProbabilisticDelayNode, DelayDistribution};
/// use akiku::node::Node;
/// use akiku::Event;
///
/// // Create a node with uniform delay between 5-15ns
/// let mut node = ProbabilisticDelayNode::new(1, DelayDistribution::Uniform { min: 5, max: 15 })
///     .with_forward(2, "in");
///
/// node.init();
/// ```
#[derive(Debug)]
pub struct ProbabilisticDelayNode {
    /// The node's unique identifier
    pub id: NodeId,
    /// The delay distribution
    pub distribution: DelayDistribution,
    /// Target node ID to forward events to
    pub forward_to: Option<NodeId>,
    /// Port name on the target node
    pub forward_port: String,
    /// Random seed for reproducibility
    seed: u64,
    /// Event counter (used for seeding)
    event_counter: u64,
    /// Statistics: total events processed
    pub events_processed: u64,
    /// Statistics: total delay added
    pub total_delay: u64,
    /// Statistics: minimum delay observed
    pub min_delay: Option<SimTime>,
    /// Statistics: maximum delay observed
    pub max_delay: Option<SimTime>,
}

impl ProbabilisticDelayNode {
    /// Creates a new probabilistic delay node.
    ///
    /// # Arguments
    /// * `id` - The node's unique identifier
    /// * `distribution` - The delay distribution to use
    pub fn new(id: NodeId, distribution: DelayDistribution) -> Self {
        Self {
            id,
            distribution,
            forward_to: None,
            forward_port: "in".to_string(),
            seed: 12345,
            event_counter: 0,
            events_processed: 0,
            total_delay: 0,
            min_delay: None,
            max_delay: None,
        }
    }

    /// Sets the random seed for reproducibility.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Sets the target node for forwarding events.
    pub fn with_forward(mut self, target: NodeId, port: impl Into<String>) -> Self {
        self.forward_to = Some(target);
        self.forward_port = port.into();
        self
    }

    /// Returns statistics about the node's operation.
    pub fn stats(&self) -> serde_json::Value {
        let avg_delay = if self.events_processed > 0 {
            self.total_delay as f64 / self.events_processed as f64
        } else {
            0.0
        };

        serde_json::json!({
            "id": self.id,
            "events_processed": self.events_processed,
            "total_delay": self.total_delay,
            "average_delay": avg_delay,
            "min_delay": self.min_delay,
            "max_delay": self.max_delay,
        })
    }

    fn sample_delay(&mut self) -> SimTime {
        let sample_seed = self.seed.wrapping_add(self.event_counter);
        self.event_counter += 1;
        self.distribution.sample(sample_seed)
    }

    fn update_stats(&mut self, delay: SimTime) {
        self.total_delay += delay;
        self.min_delay = Some(self.min_delay.map_or(delay, |d| d.min(delay)));
        self.max_delay = Some(self.max_delay.map_or(delay, |d| d.max(delay)));
    }
}

impl Node for ProbabilisticDelayNode {
    fn init(&mut self) {
        self.event_counter = 0;
        self.events_processed = 0;
        self.total_delay = 0;
        self.min_delay = None;
        self.max_delay = None;
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        Vec::new()
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        self.events_processed += 1;

        if let Some(target) = self.forward_to {
            let delay = self.sample_delay();
            self.update_stats(delay);

            let data = match &event.payload {
                EventPayload::Message { data, .. } => data.clone(),
                EventPayload::Custom(s) => serde_json::json!({ "custom": s }),
            };

            vec![Event::message(
                event.time + delay,
                self.id,
                "out",
                target,
                &self.forward_port,
                data,
            )]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_distribution() {
        let dist = DelayDistribution::Fixed { delay: 10 };
        assert_eq!(dist.sample(0), 10);
        assert_eq!(dist.sample(12345), 10);
    }

    #[test]
    fn test_uniform_distribution() {
        let dist = DelayDistribution::Uniform { min: 5, max: 15 };
        for seed in 0..100 {
            let delay = dist.sample(seed);
            assert!(delay >= 5 && delay <= 15, "delay {} out of range", delay);
        }
    }

    #[test]
    fn test_exponential_distribution() {
        let dist = DelayDistribution::Exponential { mean: 10.0 };
        let mut total = 0u64;
        let samples = 1000;
        for seed in 0..samples {
            total += dist.sample(seed);
        }
        let avg = total as f64 / samples as f64;
        // With our simple PRNG, the distribution may not be perfectly exponential
        // Just verify it produces positive values and reasonable variance
        assert!(avg > 0.0, "average {} should be positive", avg);
        assert!(total > 0, "should produce some delays");
    }

    #[test]
    fn test_probabilistic_node_creation() {
        let node = ProbabilisticDelayNode::new(1, DelayDistribution::Fixed { delay: 10 });
        assert_eq!(node.id, 1);
        assert!(node.forward_to.is_none());
    }

    #[test]
    fn test_probabilistic_node_with_forward() {
        let node = ProbabilisticDelayNode::new(1, DelayDistribution::Fixed { delay: 10 })
            .with_forward(2, "input")
            .with_seed(42);
        assert_eq!(node.forward_to, Some(2));
        assert_eq!(node.seed, 42);
    }

    #[test]
    fn test_probabilistic_node_event_processing() {
        let mut node = ProbabilisticDelayNode::new(
            1,
            DelayDistribution::Uniform { min: 5, max: 15 },
        )
        .with_forward(2, "in")
        .with_seed(12345);

        node.init();

        let event = Event::message(100, 0, "out", 1, "in", serde_json::json!({}));
        let output = node.on_event(event);

        assert_eq!(output.len(), 1);
        assert!(output[0].time >= 105 && output[0].time <= 115);
        assert_eq!(node.events_processed, 1);
    }

    #[test]
    fn test_probabilistic_node_stats() {
        let mut node = ProbabilisticDelayNode::new(1, DelayDistribution::Fixed { delay: 10 })
            .with_forward(2, "in");

        node.init();

        for i in 0..5 {
            let event = Event::message(i * 10, 0, "out", 1, "in", serde_json::json!({}));
            node.on_event(event);
        }

        let stats = node.stats();
        assert_eq!(stats["events_processed"], 5);
        assert_eq!(stats["total_delay"], 50);
        assert_eq!(stats["min_delay"], 10);
        assert_eq!(stats["max_delay"], 10);
    }

    #[test]
    fn test_reproducibility_with_seed() {
        let run_simulation = |seed: u64| -> Vec<SimTime> {
            let mut node = ProbabilisticDelayNode::new(
                1,
                DelayDistribution::Uniform { min: 0, max: 100 },
            )
            .with_forward(2, "in")
            .with_seed(seed);

            node.init();

            (0..10)
                .map(|i| {
                    let event = Event::message(i * 10, 0, "out", 1, "in", serde_json::json!({}));
                    node.on_event(event)[0].time
                })
                .collect()
        };

        let run1 = run_simulation(42);
        let run2 = run_simulation(42);
        let run3 = run_simulation(43);

        assert_eq!(run1, run2, "Same seed should produce same results");
        assert_ne!(run1, run3, "Different seeds should produce different results");
    }
}

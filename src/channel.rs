//! Channel definitions for inter-subgraph communication.
//!
//! Channels connect subgraphs and define how messages are transmitted
//! between them, including latency and time alignment rules.
//!
//! # Time Alignment
//!
//! When events flow from Event-Driven subgraphs to Tick-Driven subgraphs,
//! the event timestamps must be aligned to tick boundaries. This module
//! provides three alignment strategies:
//!
//! - [`TimeAlignment::CeilToTick`]: Round up to the next tick boundary (default, causally safe)
//! - [`TimeAlignment::FloorToTick`]: Round down to the previous tick boundary
//! - [`TimeAlignment::StrictTickBoundary`]: Require exact tick boundary alignment
//!
//! # Example
//!
//! ```
//! use akiku::channel::{TimeAlignment, ChannelDesc};
//!
//! // Create a channel with 5ns latency and ceil-to-tick alignment
//! let channel = ChannelDesc::new(1, 2)
//!     .with_latency(5)
//!     .with_alignment(TimeAlignment::CeilToTick);
//!
//! // Event sent at t=100ns, with 5ns latency and 10ns tick period
//! // 100 + 5 = 105 → ceil to 110
//! assert_eq!(channel.compute_arrival_time(100, 10), Some(110));
//! ```

use serde::{Deserialize, Serialize};

use crate::types::{SimTime, SubgraphId};

/// Time alignment rules for event→tick conversion.
///
/// When an event from an Event-Driven subgraph needs to be delivered
/// to a Tick-Driven subgraph, the event timestamp must be aligned
/// to a tick boundary using one of these rules.
///
/// # Alignment Formula
///
/// Given:
/// - `t_e`: Event time
/// - `Δt`: Tick period
///
/// The aligned time `t_vis` is calculated as:
///
/// | Rule | Formula |
/// |------|---------|
/// | CeilToTick | `t_vis = ⌈t_e / Δt⌉ × Δt` |
/// | FloorToTick | `t_vis = ⌊t_e / Δt⌋ × Δt` |
/// | StrictTickBoundary | `t_vis = t_e` if `t_e % Δt == 0`, else error |
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TimeAlignment {
    /// Ceil to tick: `t_vis = ⌈t_e / Δt⌉ × Δt`
    ///
    /// This is the default and recommended alignment rule.
    /// It ensures that Tick subgraphs never see "future events"
    /// (causally correct).
    ///
    /// # Example
    /// ```
    /// use akiku::channel::TimeAlignment;
    ///
    /// let align = TimeAlignment::CeilToTick;
    /// // 103ns with 10ns tick → 110ns
    /// assert_eq!(align.align(103, 10), Some(110));
    /// // Exactly on boundary stays the same
    /// assert_eq!(align.align(100, 10), Some(100));
    /// ```
    #[default]
    CeilToTick,

    /// Floor to tick: `t_vis = ⌊t_e / Δt⌋ × Δt`
    ///
    /// Use with caution - may cause the Tick subgraph to see
    /// results "earlier" than internal Event subgraph logic,
    /// potentially introducing causality issues.
    ///
    /// # Example
    /// ```
    /// use akiku::channel::TimeAlignment;
    ///
    /// let align = TimeAlignment::FloorToTick;
    /// // 103ns with 10ns tick → 100ns
    /// assert_eq!(align.align(103, 10), Some(100));
    /// ```
    FloorToTick,

    /// Strict tick boundary: requires `t_e` to exactly fall on a tick boundary.
    ///
    /// Returns `None` if the event timestamp doesn't fall on a tick boundary.
    /// This is useful for debugging or when the upstream guarantees
    /// events only occur at tick boundaries.
    ///
    /// # Example
    /// ```
    /// use akiku::channel::TimeAlignment;
    ///
    /// let align = TimeAlignment::StrictTickBoundary;
    /// // Exactly on boundary
    /// assert_eq!(align.align(100, 10), Some(100));
    /// // Not on boundary - returns None
    /// assert_eq!(align.align(103, 10), None);
    /// ```
    StrictTickBoundary,
}

impl TimeAlignment {
    /// Aligns an event time to a tick boundary based on this alignment rule.
    ///
    /// # Arguments
    /// * `event_time` - The original event time
    /// * `tick_period` - The tick period of the destination subgraph.
    ///                   If 0 (event-driven destination), returns `event_time` unchanged.
    ///
    /// # Returns
    /// - `Some(aligned_time)` - The aligned time
    /// - `None` - For `StrictTickBoundary` when not on a tick boundary
    ///
    /// # Examples
    ///
    /// ```
    /// use akiku::channel::TimeAlignment;
    ///
    /// // CeilToTick alignment
    /// assert_eq!(TimeAlignment::CeilToTick.align(103, 10), Some(110));
    /// assert_eq!(TimeAlignment::CeilToTick.align(100, 10), Some(100));
    ///
    /// // FloorToTick alignment
    /// assert_eq!(TimeAlignment::FloorToTick.align(103, 10), Some(100));
    ///
    /// // StrictTickBoundary
    /// assert_eq!(TimeAlignment::StrictTickBoundary.align(100, 10), Some(100));
    /// assert_eq!(TimeAlignment::StrictTickBoundary.align(103, 10), None);
    ///
    /// // Event-driven destination (tick_period = 0)
    /// assert_eq!(TimeAlignment::CeilToTick.align(103, 0), Some(103));
    /// ```
    pub fn align(&self, event_time: SimTime, tick_period: SimTime) -> Option<SimTime> {
        // Event-driven destination: no alignment needed
        if tick_period == 0 {
            return Some(event_time);
        }

        match self {
            TimeAlignment::CeilToTick => {
                Some(Self::ceil_to_tick(event_time, tick_period))
            }
            TimeAlignment::FloorToTick => {
                Some(Self::floor_to_tick(event_time, tick_period))
            }
            TimeAlignment::StrictTickBoundary => {
                if event_time % tick_period == 0 {
                    Some(event_time)
                } else {
                    None
                }
            }
        }
    }

    /// Aligns time using ceiling division.
    ///
    /// Formula: `⌈t / Δt⌉ × Δt`
    #[inline]
    pub fn ceil_to_tick(event_time: SimTime, tick_period: SimTime) -> SimTime {
        if tick_period == 0 {
            return event_time;
        }
        if event_time == 0 {
            return 0;
        }
        // Ceiling division: (a + b - 1) / b * b
        ((event_time + tick_period - 1) / tick_period) * tick_period
    }

    /// Aligns time using floor division.
    ///
    /// Formula: `⌊t / Δt⌋ × Δt`
    #[inline]
    pub fn floor_to_tick(event_time: SimTime, tick_period: SimTime) -> SimTime {
        if tick_period == 0 {
            return event_time;
        }
        (event_time / tick_period) * tick_period
    }

    /// Rounds to the nearest tick boundary.
    ///
    /// Rounds up if the remainder is >= half the tick period, down otherwise.
    #[inline]
    pub fn round_to_tick(event_time: SimTime, tick_period: SimTime) -> SimTime {
        if tick_period == 0 {
            return event_time;
        }
        let remainder = event_time % tick_period;
        if remainder >= tick_period / 2 {
            // Round up
            event_time + (tick_period - remainder)
        } else {
            // Round down
            event_time - remainder
        }
    }

    /// Checks if a time is exactly on a tick boundary.
    #[inline]
    pub fn is_on_boundary(event_time: SimTime, tick_period: SimTime) -> bool {
        if tick_period == 0 {
            return true;
        }
        event_time % tick_period == 0
    }

    /// Returns the next tick boundary after the given time.
    ///
    /// If the time is already on a boundary, returns the *next* boundary.
    #[inline]
    pub fn next_tick_boundary(event_time: SimTime, tick_period: SimTime) -> SimTime {
        if tick_period == 0 {
            return event_time;
        }
        ((event_time / tick_period) + 1) * tick_period
    }

    /// Returns the previous tick boundary before the given time.
    ///
    /// If the time is already on a boundary, returns the *previous* boundary.
    /// Returns 0 if the time is less than or equal to tick_period.
    #[inline]
    pub fn prev_tick_boundary(event_time: SimTime, tick_period: SimTime) -> SimTime {
        if tick_period == 0 || event_time <= tick_period {
            return 0;
        }
        if event_time % tick_period == 0 {
            event_time - tick_period
        } else {
            (event_time / tick_period) * tick_period
        }
    }

    /// Returns the tick number for a given time.
    ///
    /// This is the number of complete ticks that have occurred.
    #[inline]
    pub fn tick_number(event_time: SimTime, tick_period: SimTime) -> u64 {
        if tick_period == 0 {
            return 0;
        }
        event_time / tick_period
    }
}

/// Result of applying channel transformation to an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelResult {
    /// Event successfully transformed with new arrival time
    Delivered { arrival_time: SimTime },
    /// Event rejected due to strict boundary alignment failure
    AlignmentFailed { original_time: SimTime },
    /// Event dropped due to capacity limits (future use)
    Dropped { reason: String },
}

impl ChannelResult {
    /// Returns the arrival time if delivered, None otherwise.
    pub fn arrival_time(&self) -> Option<SimTime> {
        match self {
            ChannelResult::Delivered { arrival_time } => Some(*arrival_time),
            _ => None,
        }
    }

    /// Returns true if the event was successfully delivered.
    pub fn is_delivered(&self) -> bool {
        matches!(self, ChannelResult::Delivered { .. })
    }
}

/// Describes a communication channel between two subgraphs.
///
/// Channels define how messages flow between subgraphs, including
/// transmission latency and time alignment rules.
///
/// # Example
///
/// ```
/// use akiku::channel::{ChannelDesc, TimeAlignment};
///
/// // Create a channel from subgraph 1 to subgraph 2
/// let channel = ChannelDesc::new(1, 2)
///     .with_latency(5)           // 5ns base latency
///     .with_alignment(TimeAlignment::CeilToTick)
///     .with_capacity(100);       // 100 messages max
///
/// // Compute arrival time
/// let arrival = channel.compute_arrival_time(100, 10);
/// assert_eq!(arrival, Some(110)); // 100 + 5 = 105, ceil to 110
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelDesc {
    /// Source subgraph ID
    pub src_subgraph: SubgraphId,
    /// Destination subgraph ID
    pub dst_subgraph: SubgraphId,
    /// Base transmission latency (in SimTime units)
    pub base_latency: SimTime,
    /// Time alignment rule for event→tick conversion
    pub time_alignment: TimeAlignment,
    /// Optional capacity limit (for future backpressure support)
    #[serde(default)]
    pub capacity: Option<usize>,
    /// Optional minimum latency (for jitter modeling)
    #[serde(default)]
    pub min_latency: Option<SimTime>,
    /// Optional maximum latency (for jitter modeling)
    #[serde(default)]
    pub max_latency: Option<SimTime>,
}

impl ChannelDesc {
    /// Creates a new channel description.
    pub fn new(src: SubgraphId, dst: SubgraphId) -> Self {
        Self {
            src_subgraph: src,
            dst_subgraph: dst,
            base_latency: 0,
            time_alignment: TimeAlignment::default(),
            capacity: None,
            min_latency: None,
            max_latency: None,
        }
    }

    /// Sets the base latency for this channel.
    pub fn with_latency(mut self, latency: SimTime) -> Self {
        self.base_latency = latency;
        self
    }

    /// Sets the time alignment rule for this channel.
    pub fn with_alignment(mut self, alignment: TimeAlignment) -> Self {
        self.time_alignment = alignment;
        self
    }

    /// Sets the capacity limit for this channel.
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = Some(capacity);
        self
    }

    /// Sets the latency range for jitter modeling.
    ///
    /// # Arguments
    /// * `min` - Minimum latency
    /// * `max` - Maximum latency
    pub fn with_latency_range(mut self, min: SimTime, max: SimTime) -> Self {
        self.min_latency = Some(min);
        self.max_latency = Some(max);
        self
    }

    /// Calculates the arrival time for a message sent at `send_time`.
    ///
    /// # Arguments
    /// * `send_time` - The time at which the message is sent
    /// * `dst_tick_period` - The tick period of the destination subgraph
    ///                       (0 if destination is event-driven)
    ///
    /// # Returns
    /// The arrival time after applying latency and alignment, or `None` if alignment fails
    ///
    /// # Example
    ///
    /// ```
    /// use akiku::channel::{ChannelDesc, TimeAlignment};
    ///
    /// let channel = ChannelDesc::new(1, 2)
    ///     .with_latency(5)
    ///     .with_alignment(TimeAlignment::CeilToTick);
    ///
    /// // Event-driven destination (tick_period = 0)
    /// assert_eq!(channel.compute_arrival_time(100, 0), Some(105));
    ///
    /// // Tick-driven destination (tick_period = 10)
    /// // 100 + 5 = 105 → ceil to 110
    /// assert_eq!(channel.compute_arrival_time(100, 10), Some(110));
    /// ```
    pub fn compute_arrival_time(
        &self,
        send_time: SimTime,
        dst_tick_period: SimTime,
    ) -> Option<SimTime> {
        let after_latency = send_time + self.base_latency;
        self.time_alignment.align(after_latency, dst_tick_period)
    }

    /// Computes the arrival time and returns a detailed result.
    ///
    /// This method provides more information about the transformation,
    /// including failure reasons.
    pub fn transform_event_time(
        &self,
        send_time: SimTime,
        dst_tick_period: SimTime,
    ) -> ChannelResult {
        let after_latency = send_time + self.base_latency;

        match self.time_alignment.align(after_latency, dst_tick_period) {
            Some(arrival_time) => ChannelResult::Delivered { arrival_time },
            None => ChannelResult::AlignmentFailed {
                original_time: after_latency,
            },
        }
    }

    /// Returns the effective latency (base latency).
    ///
    /// For future extensions, this could incorporate dynamic latency models.
    #[inline]
    pub fn effective_latency(&self) -> SimTime {
        self.base_latency
    }

    /// Checks if this channel connects the given subgraph pair.
    #[inline]
    pub fn connects(&self, src: SubgraphId, dst: SubgraphId) -> bool {
        self.src_subgraph == src && self.dst_subgraph == dst
    }

    /// Returns true if this is a self-loop (same source and destination).
    #[inline]
    pub fn is_self_loop(&self) -> bool {
        self.src_subgraph == self.dst_subgraph
    }
}

/// A collection of channels with fast lookup.
#[derive(Clone, Debug, Default)]
pub struct ChannelRegistry {
    channels: Vec<ChannelDesc>,
    /// Map from (src, dst) to channel indices
    lookup: std::collections::HashMap<(SubgraphId, SubgraphId), Vec<usize>>,
}

impl ChannelRegistry {
    /// Creates a new empty channel registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a channel to the registry.
    pub fn add(&mut self, channel: ChannelDesc) {
        let key = (channel.src_subgraph, channel.dst_subgraph);
        let index = self.channels.len();
        self.channels.push(channel);
        self.lookup.entry(key).or_default().push(index);
    }

    /// Finds all channels from src to dst.
    pub fn find(&self, src: SubgraphId, dst: SubgraphId) -> Vec<&ChannelDesc> {
        self.lookup
            .get(&(src, dst))
            .map(|indices| indices.iter().map(|&i| &self.channels[i]).collect())
            .unwrap_or_default()
    }

    /// Finds the first channel from src to dst.
    pub fn find_one(&self, src: SubgraphId, dst: SubgraphId) -> Option<&ChannelDesc> {
        self.lookup
            .get(&(src, dst))
            .and_then(|indices| indices.first())
            .map(|&i| &self.channels[i])
    }

    /// Returns all channels.
    pub fn all(&self) -> &[ChannelDesc] {
        &self.channels
    }

    /// Returns the number of channels.
    pub fn len(&self) -> usize {
        self.channels.len()
    }

    /// Returns true if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }

    /// Returns all outgoing channels from a subgraph.
    pub fn outgoing(&self, src: SubgraphId) -> Vec<&ChannelDesc> {
        self.channels
            .iter()
            .filter(|c| c.src_subgraph == src)
            .collect()
    }

    /// Returns all incoming channels to a subgraph.
    pub fn incoming(&self, dst: SubgraphId) -> Vec<&ChannelDesc> {
        self.channels
            .iter()
            .filter(|c| c.dst_subgraph == dst)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== TimeAlignment Tests ==========

    #[test]
    fn test_ceil_to_tick_alignment() {
        let align = TimeAlignment::CeilToTick;

        // 103ns with 10ns tick → 110ns
        assert_eq!(align.align(103, 10), Some(110));

        // Exactly on boundary
        assert_eq!(align.align(100, 10), Some(100));

        // Zero time
        assert_eq!(align.align(0, 10), Some(0));

        // Just after boundary
        assert_eq!(align.align(101, 10), Some(110));

        // Large values
        assert_eq!(align.align(999, 100), Some(1000));
        assert_eq!(align.align(1000, 100), Some(1000));
        assert_eq!(align.align(1001, 100), Some(1100));
    }

    #[test]
    fn test_floor_to_tick_alignment() {
        let align = TimeAlignment::FloorToTick;

        // 103ns with 10ns tick → 100ns
        assert_eq!(align.align(103, 10), Some(100));

        // Exactly on boundary
        assert_eq!(align.align(100, 10), Some(100));

        // Zero time
        assert_eq!(align.align(0, 10), Some(0));

        // Just before boundary
        assert_eq!(align.align(99, 10), Some(90));

        // Large values
        assert_eq!(align.align(999, 100), Some(900));
        assert_eq!(align.align(1000, 100), Some(1000));
    }

    #[test]
    fn test_strict_tick_boundary() {
        let align = TimeAlignment::StrictTickBoundary;

        // Exactly on boundary
        assert_eq!(align.align(100, 10), Some(100));

        // Not on boundary
        assert_eq!(align.align(103, 10), None);
        assert_eq!(align.align(99, 10), None);
        assert_eq!(align.align(1, 10), None);

        // Zero time
        assert_eq!(align.align(0, 10), Some(0));
    }

    #[test]
    fn test_alignment_zero_tick_period() {
        // All alignments should pass through unchanged when tick_period is 0
        assert_eq!(TimeAlignment::CeilToTick.align(103, 0), Some(103));
        assert_eq!(TimeAlignment::FloorToTick.align(103, 0), Some(103));
        assert_eq!(TimeAlignment::StrictTickBoundary.align(103, 0), Some(103));
    }

    #[test]
    fn test_static_alignment_functions() {
        // ceil_to_tick
        assert_eq!(TimeAlignment::ceil_to_tick(103, 10), 110);
        assert_eq!(TimeAlignment::ceil_to_tick(100, 10), 100);
        assert_eq!(TimeAlignment::ceil_to_tick(0, 10), 0);

        // floor_to_tick
        assert_eq!(TimeAlignment::floor_to_tick(103, 10), 100);
        assert_eq!(TimeAlignment::floor_to_tick(100, 10), 100);

        // round_to_tick
        assert_eq!(TimeAlignment::round_to_tick(103, 10), 100); // 3 < 5, round down
        assert_eq!(TimeAlignment::round_to_tick(105, 10), 110); // 5 >= 5, round up
        assert_eq!(TimeAlignment::round_to_tick(108, 10), 110); // 8 >= 5, round up
    }

    #[test]
    fn test_boundary_helpers() {
        // is_on_boundary
        assert!(TimeAlignment::is_on_boundary(100, 10));
        assert!(!TimeAlignment::is_on_boundary(103, 10));
        assert!(TimeAlignment::is_on_boundary(0, 10));
        assert!(TimeAlignment::is_on_boundary(103, 0)); // tick_period 0 always true

        // next_tick_boundary
        assert_eq!(TimeAlignment::next_tick_boundary(100, 10), 110);
        assert_eq!(TimeAlignment::next_tick_boundary(103, 10), 110);
        assert_eq!(TimeAlignment::next_tick_boundary(0, 10), 10);

        // prev_tick_boundary
        assert_eq!(TimeAlignment::prev_tick_boundary(100, 10), 90);
        assert_eq!(TimeAlignment::prev_tick_boundary(103, 10), 100);
        assert_eq!(TimeAlignment::prev_tick_boundary(10, 10), 0);
        assert_eq!(TimeAlignment::prev_tick_boundary(5, 10), 0);

        // tick_number
        assert_eq!(TimeAlignment::tick_number(0, 10), 0);
        assert_eq!(TimeAlignment::tick_number(10, 10), 1);
        assert_eq!(TimeAlignment::tick_number(15, 10), 1);
        assert_eq!(TimeAlignment::tick_number(100, 10), 10);
    }

    // ========== ChannelDesc Tests ==========

    #[test]
    fn test_channel_desc() {
        let channel = ChannelDesc::new(1, 2)
            .with_latency(5)
            .with_alignment(TimeAlignment::CeilToTick);

        assert_eq!(channel.src_subgraph, 1);
        assert_eq!(channel.dst_subgraph, 2);
        assert_eq!(channel.base_latency, 5);
        assert_eq!(channel.time_alignment, TimeAlignment::CeilToTick);
    }

    #[test]
    fn test_channel_capacity_and_latency_range() {
        let channel = ChannelDesc::new(1, 2)
            .with_latency(10)
            .with_capacity(100)
            .with_latency_range(5, 20);

        assert_eq!(channel.capacity, Some(100));
        assert_eq!(channel.min_latency, Some(5));
        assert_eq!(channel.max_latency, Some(20));
    }

    #[test]
    fn test_compute_arrival_time() {
        let channel = ChannelDesc::new(1, 2)
            .with_latency(5)
            .with_alignment(TimeAlignment::CeilToTick);

        // send at 100, latency 5, tick period 10
        // 100 + 5 = 105 → ceil to 110
        assert_eq!(channel.compute_arrival_time(100, 10), Some(110));

        // Event-driven destination (tick_period = 0)
        assert_eq!(channel.compute_arrival_time(100, 0), Some(105));
    }

    #[test]
    fn test_compute_arrival_time_floor() {
        let channel = ChannelDesc::new(1, 2)
            .with_latency(5)
            .with_alignment(TimeAlignment::FloorToTick);

        // 100 + 5 = 105 → floor to 100
        assert_eq!(channel.compute_arrival_time(100, 10), Some(100));
    }

    #[test]
    fn test_compute_arrival_time_strict() {
        let channel = ChannelDesc::new(1, 2)
            .with_latency(5)
            .with_alignment(TimeAlignment::StrictTickBoundary);

        // 100 + 5 = 105, not on boundary → None
        assert_eq!(channel.compute_arrival_time(100, 10), None);

        // 95 + 5 = 100, on boundary → Some(100)
        assert_eq!(channel.compute_arrival_time(95, 10), Some(100));
    }

    #[test]
    fn test_transform_event_time() {
        let channel = ChannelDesc::new(1, 2)
            .with_latency(5)
            .with_alignment(TimeAlignment::CeilToTick);

        let result = channel.transform_event_time(100, 10);
        assert!(result.is_delivered());
        assert_eq!(result.arrival_time(), Some(110));

        // Test strict alignment failure
        let strict_channel = ChannelDesc::new(1, 2)
            .with_latency(5)
            .with_alignment(TimeAlignment::StrictTickBoundary);

        let result = strict_channel.transform_event_time(100, 10);
        assert!(!result.is_delivered());
        assert!(matches!(result, ChannelResult::AlignmentFailed { .. }));
    }

    #[test]
    fn test_channel_helpers() {
        let channel = ChannelDesc::new(1, 2).with_latency(10);

        assert!(channel.connects(1, 2));
        assert!(!channel.connects(2, 1));
        assert!(!channel.connects(1, 3));

        assert!(!channel.is_self_loop());

        let self_loop = ChannelDesc::new(1, 1);
        assert!(self_loop.is_self_loop());

        assert_eq!(channel.effective_latency(), 10);
    }

    #[test]
    fn test_channel_serialization() {
        let channel = ChannelDesc::new(1, 2)
            .with_latency(10)
            .with_capacity(50);

        let json = serde_json::to_string(&channel).unwrap();
        let deserialized: ChannelDesc = serde_json::from_str(&json).unwrap();

        assert_eq!(channel.src_subgraph, deserialized.src_subgraph);
        assert_eq!(channel.base_latency, deserialized.base_latency);
        assert_eq!(channel.capacity, deserialized.capacity);
    }

    // ========== ChannelRegistry Tests ==========

    #[test]
    fn test_channel_registry_basic() {
        let mut registry = ChannelRegistry::new();
        assert!(registry.is_empty());

        registry.add(ChannelDesc::new(1, 2).with_latency(5));
        registry.add(ChannelDesc::new(1, 3).with_latency(10));
        registry.add(ChannelDesc::new(2, 3).with_latency(15));

        assert_eq!(registry.len(), 3);
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_channel_registry_lookup() {
        let mut registry = ChannelRegistry::new();
        registry.add(ChannelDesc::new(1, 2).with_latency(5));
        registry.add(ChannelDesc::new(1, 2).with_latency(10)); // Multiple channels same pair
        registry.add(ChannelDesc::new(2, 3).with_latency(15));

        // Find multiple
        let channels = registry.find(1, 2);
        assert_eq!(channels.len(), 2);

        // Find one
        let channel = registry.find_one(1, 2);
        assert!(channel.is_some());
        assert_eq!(channel.unwrap().base_latency, 5);

        // Find none
        assert!(registry.find(3, 1).is_empty());
        assert!(registry.find_one(3, 1).is_none());
    }

    #[test]
    fn test_channel_registry_incoming_outgoing() {
        let mut registry = ChannelRegistry::new();
        registry.add(ChannelDesc::new(1, 2));
        registry.add(ChannelDesc::new(1, 3));
        registry.add(ChannelDesc::new(2, 3));

        // Outgoing from 1
        let outgoing = registry.outgoing(1);
        assert_eq!(outgoing.len(), 2);

        // Incoming to 3
        let incoming = registry.incoming(3);
        assert_eq!(incoming.len(), 2);

        // No outgoing from 3
        assert!(registry.outgoing(3).is_empty());
    }
}


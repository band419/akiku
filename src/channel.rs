//! Channel definitions for inter-subgraph communication.
//!
//! Channels connect subgraphs and define how messages are transmitted
//! between them, including latency and time alignment rules.

use serde::{Deserialize, Serialize};

use crate::types::{SimTime, SubgraphId};

/// Time alignment rules for event→tick conversion.
///
/// When an event from an Event-Driven subgraph needs to be delivered
/// to a Tick-Driven subgraph, the event timestamp must be aligned
/// to a tick boundary using one of these rules.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TimeAlignment {
    /// Ceil to tick: t_vis = ceil(t_e / Δt_tick) * Δt_tick
    ///
    /// This is the default and recommended alignment rule.
    /// It ensures that Tick subgraphs never see "future events"
    /// (causally correct).
    ///
    /// Example: t_e = 103ns, Δt_tick = 10ns → t_vis = 110ns
    #[default]
    CeilToTick,

    /// Floor to tick: t_vis = floor(t_e / Δt_tick) * Δt_tick
    ///
    /// Use with caution - may cause the Tick subgraph to see
    /// results "earlier" than internal Event subgraph logic,
    /// potentially introducing causality issues.
    FloorToTick,

    /// Strict tick boundary: requires t_e to exactly fall on a tick boundary.
    ///
    /// If the event timestamp doesn't fall on a tick boundary,
    /// behavior depends on implementation (error or snap to nearest).
    StrictTickBoundary,
}

impl TimeAlignment {
    /// Aligns an event time to a tick boundary based on this alignment rule.
    ///
    /// # Arguments
    /// * `event_time` - The original event time
    /// * `tick_period` - The tick period of the destination subgraph
    ///
    /// # Returns
    /// The aligned time, or `None` for `StrictTickBoundary` if not aligned
    pub fn align(&self, event_time: SimTime, tick_period: SimTime) -> Option<SimTime> {
        if tick_period == 0 {
            return Some(event_time);
        }

        match self {
            TimeAlignment::CeilToTick => {
                // Ceiling division: (a + b - 1) / b
                let aligned = ((event_time + tick_period - 1) / tick_period) * tick_period;
                // Handle the case where event_time is already aligned
                if event_time > 0 && event_time % tick_period == 0 {
                    Some(event_time)
                } else {
                    Some(aligned)
                }
            }
            TimeAlignment::FloorToTick => {
                let aligned = (event_time / tick_period) * tick_period;
                Some(aligned)
            }
            TimeAlignment::StrictTickBoundary => {
                if event_time % tick_period == 0 {
                    Some(event_time)
                } else {
                    None // Not on tick boundary
                }
            }
        }
    }
}

/// Describes a communication channel between two subgraphs.
///
/// Channels define how messages flow between subgraphs, including
/// transmission latency and time alignment rules.
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
}

impl ChannelDesc {
    /// Creates a new channel description.
    pub fn new(src: SubgraphId, dst: SubgraphId) -> Self {
        Self {
            src_subgraph: src,
            dst_subgraph: dst,
            base_latency: 0,
            time_alignment: TimeAlignment::default(),
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

    /// Calculates the arrival time for a message sent at `send_time`.
    ///
    /// # Arguments
    /// * `send_time` - The time at which the message is sent
    /// * `dst_tick_period` - The tick period of the destination subgraph
    ///                       (0 if destination is event-driven)
    ///
    /// # Returns
    /// The arrival time after applying latency and alignment, or `None` if alignment fails
    pub fn compute_arrival_time(
        &self,
        send_time: SimTime,
        dst_tick_period: SimTime,
    ) -> Option<SimTime> {
        let after_latency = send_time + self.base_latency;
        self.time_alignment.align(after_latency, dst_tick_period)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn test_strict_tick_boundary() {
        let align = TimeAlignment::StrictTickBoundary;

        // Exactly on boundary
        assert_eq!(align.align(100, 10), Some(100));

        // Not on boundary
        assert_eq!(align.align(103, 10), None);

        // Zero time
        assert_eq!(align.align(0, 10), Some(0));
    }

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
    fn test_channel_serialization() {
        let channel = ChannelDesc::new(1, 2).with_latency(10);
        let json = serde_json::to_string(&channel).unwrap();
        let deserialized: ChannelDesc = serde_json::from_str(&json).unwrap();

        assert_eq!(channel.src_subgraph, deserialized.src_subgraph);
        assert_eq!(channel.base_latency, deserialized.base_latency);
    }
}


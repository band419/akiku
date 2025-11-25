//! Tests for time alignment and channel logic.
//!
//! These tests verify the correctness of:
//! - CeilToTick alignment algorithm
//! - FloorToTick alignment algorithm
//! - StrictTickBoundary alignment algorithm
//! - Channel latency computation
//! - Edge cases and boundary conditions

use akiku::channel::{ChannelDesc, ChannelRegistry, ChannelResult, TimeAlignment};
use akiku::types::SimTime;

// ============================================================================
// CeilToTick Alignment Tests
// ============================================================================

#[test]
fn test_ceil_to_tick_basic() {
    let align = TimeAlignment::CeilToTick;

    // Standard cases from spec: t_e = 103ns, Δt = 10ns → t_vis = 110ns
    assert_eq!(align.align(103, 10), Some(110));
    assert_eq!(align.align(101, 10), Some(110));
    assert_eq!(align.align(109, 10), Some(110));
}

#[test]
fn test_ceil_to_tick_exact_boundary() {
    let align = TimeAlignment::CeilToTick;

    // Exactly on boundary should stay unchanged
    assert_eq!(align.align(100, 10), Some(100));
    assert_eq!(align.align(200, 10), Some(200));
    assert_eq!(align.align(0, 10), Some(0));
}

#[test]
fn test_ceil_to_tick_various_periods() {
    let align = TimeAlignment::CeilToTick;

    // Different tick periods
    assert_eq!(align.align(15, 5), Some(15));  // On boundary
    assert_eq!(align.align(16, 5), Some(20));  // Just after
    assert_eq!(align.align(14, 5), Some(15));  // Just before

    assert_eq!(align.align(999, 100), Some(1000));
    assert_eq!(align.align(1, 100), Some(100));
}

#[test]
fn test_ceil_to_tick_large_values() {
    let align = TimeAlignment::CeilToTick;

    // Large simulation times
    let large_time: SimTime = 1_000_000_000; // 1 billion
    assert_eq!(align.align(large_time, 1000), Some(large_time));
    assert_eq!(align.align(large_time + 1, 1000), Some(large_time + 1000));
}

// ============================================================================
// FloorToTick Alignment Tests
// ============================================================================

#[test]
fn test_floor_to_tick_basic() {
    let align = TimeAlignment::FloorToTick;

    // Standard cases: t_e = 103ns, Δt = 10ns → t_vis = 100ns
    assert_eq!(align.align(103, 10), Some(100));
    assert_eq!(align.align(101, 10), Some(100));
    assert_eq!(align.align(109, 10), Some(100));
}

#[test]
fn test_floor_to_tick_exact_boundary() {
    let align = TimeAlignment::FloorToTick;

    // Exactly on boundary should stay unchanged
    assert_eq!(align.align(100, 10), Some(100));
    assert_eq!(align.align(0, 10), Some(0));
}

#[test]
fn test_floor_to_tick_causality_concern() {
    // Demonstrates the causality concern with FloorToTick:
    // Event at t=105 gets aligned to t=100, which is in the past
    let align = TimeAlignment::FloorToTick;

    assert_eq!(align.align(105, 10), Some(100));
    // This means a tick subgraph at t=100 would "see" an event
    // that didn't happen until t=105 - causality violation!
}

// ============================================================================
// StrictTickBoundary Alignment Tests
// ============================================================================

#[test]
fn test_strict_boundary_exact() {
    let align = TimeAlignment::StrictTickBoundary;

    // Exactly on boundary
    assert_eq!(align.align(100, 10), Some(100));
    assert_eq!(align.align(0, 10), Some(0));
    assert_eq!(align.align(1000, 100), Some(1000));
}

#[test]
fn test_strict_boundary_off() {
    let align = TimeAlignment::StrictTickBoundary;

    // Not on boundary - must return None
    assert_eq!(align.align(101, 10), None);
    assert_eq!(align.align(99, 10), None);
    assert_eq!(align.align(1, 10), None);
    assert_eq!(align.align(1001, 100), None);
}

#[test]
fn test_strict_boundary_zero() {
    let align = TimeAlignment::StrictTickBoundary;

    // Zero is always on boundary
    assert_eq!(align.align(0, 10), Some(0));
    assert_eq!(align.align(0, 1), Some(0));
    assert_eq!(align.align(0, 1000), Some(0));
}

// ============================================================================
// Event-Driven Destination (tick_period = 0) Tests
// ============================================================================

#[test]
fn test_event_driven_destination() {
    // When destination is event-driven, alignment should pass through unchanged
    assert_eq!(TimeAlignment::CeilToTick.align(103, 0), Some(103));
    assert_eq!(TimeAlignment::FloorToTick.align(103, 0), Some(103));
    assert_eq!(TimeAlignment::StrictTickBoundary.align(103, 0), Some(103));
}

// ============================================================================
// Static Helper Function Tests
// ============================================================================

#[test]
fn test_round_to_tick() {
    // Round to nearest tick boundary
    assert_eq!(TimeAlignment::round_to_tick(100, 10), 100); // On boundary
    assert_eq!(TimeAlignment::round_to_tick(102, 10), 100); // 2 < 5, round down
    assert_eq!(TimeAlignment::round_to_tick(105, 10), 110); // 5 >= 5, round up
    assert_eq!(TimeAlignment::round_to_tick(108, 10), 110); // 8 >= 5, round up
    assert_eq!(TimeAlignment::round_to_tick(104, 10), 100); // 4 < 5, round down
}

#[test]
fn test_is_on_boundary() {
    assert!(TimeAlignment::is_on_boundary(100, 10));
    assert!(TimeAlignment::is_on_boundary(0, 10));
    assert!(!TimeAlignment::is_on_boundary(101, 10));
    assert!(!TimeAlignment::is_on_boundary(99, 10));

    // tick_period 0 always returns true
    assert!(TimeAlignment::is_on_boundary(123, 0));
}

#[test]
fn test_next_tick_boundary() {
    assert_eq!(TimeAlignment::next_tick_boundary(0, 10), 10);
    assert_eq!(TimeAlignment::next_tick_boundary(1, 10), 10);
    assert_eq!(TimeAlignment::next_tick_boundary(9, 10), 10);
    assert_eq!(TimeAlignment::next_tick_boundary(10, 10), 20);  // Already on, goes to next
    assert_eq!(TimeAlignment::next_tick_boundary(11, 10), 20);
}

#[test]
fn test_prev_tick_boundary() {
    assert_eq!(TimeAlignment::prev_tick_boundary(100, 10), 90);   // On boundary, goes to prev
    assert_eq!(TimeAlignment::prev_tick_boundary(105, 10), 100);  // Off boundary
    assert_eq!(TimeAlignment::prev_tick_boundary(10, 10), 0);     // Would go to 0
    assert_eq!(TimeAlignment::prev_tick_boundary(5, 10), 0);      // Below first tick
}

#[test]
fn test_tick_number() {
    assert_eq!(TimeAlignment::tick_number(0, 10), 0);
    assert_eq!(TimeAlignment::tick_number(9, 10), 0);
    assert_eq!(TimeAlignment::tick_number(10, 10), 1);
    assert_eq!(TimeAlignment::tick_number(19, 10), 1);
    assert_eq!(TimeAlignment::tick_number(100, 10), 10);
}

// ============================================================================
// Channel Latency Computation Tests
// ============================================================================

#[test]
fn test_channel_latency_to_event_driven() {
    // Channel to event-driven subgraph (no alignment needed)
    let channel = ChannelDesc::new(1, 2).with_latency(5);

    // tick_period = 0 means event-driven destination
    assert_eq!(channel.compute_arrival_time(100, 0), Some(105));
    assert_eq!(channel.compute_arrival_time(0, 0), Some(5));
}

#[test]
fn test_channel_latency_to_tick_driven_ceil() {
    // Channel to tick-driven subgraph with CeilToTick (default)
    let channel = ChannelDesc::new(1, 2)
        .with_latency(5)
        .with_alignment(TimeAlignment::CeilToTick);

    // 100 + 5 = 105 → ceil to 110
    assert_eq!(channel.compute_arrival_time(100, 10), Some(110));

    // 95 + 5 = 100 → exactly on boundary, stays 100
    assert_eq!(channel.compute_arrival_time(95, 10), Some(100));

    // 0 + 5 = 5 → ceil to 10
    assert_eq!(channel.compute_arrival_time(0, 10), Some(10));
}

#[test]
fn test_channel_latency_to_tick_driven_floor() {
    let channel = ChannelDesc::new(1, 2)
        .with_latency(5)
        .with_alignment(TimeAlignment::FloorToTick);

    // 100 + 5 = 105 → floor to 100
    assert_eq!(channel.compute_arrival_time(100, 10), Some(100));

    // 95 + 5 = 100 → exactly on boundary, stays 100
    assert_eq!(channel.compute_arrival_time(95, 10), Some(100));
}

#[test]
fn test_channel_latency_to_tick_driven_strict() {
    let channel = ChannelDesc::new(1, 2)
        .with_latency(5)
        .with_alignment(TimeAlignment::StrictTickBoundary);

    // 100 + 5 = 105 → not on boundary, fails
    assert_eq!(channel.compute_arrival_time(100, 10), None);

    // 95 + 5 = 100 → exactly on boundary, succeeds
    assert_eq!(channel.compute_arrival_time(95, 10), Some(100));
}

#[test]
fn test_channel_zero_latency() {
    let channel = ChannelDesc::new(1, 2)
        .with_latency(0)
        .with_alignment(TimeAlignment::CeilToTick);

    // No latency, but still needs alignment
    assert_eq!(channel.compute_arrival_time(103, 10), Some(110));
    assert_eq!(channel.compute_arrival_time(100, 10), Some(100));
}

#[test]
fn test_channel_large_latency() {
    let channel = ChannelDesc::new(1, 2)
        .with_latency(1000)
        .with_alignment(TimeAlignment::CeilToTick);

    // 100 + 1000 = 1100, already on boundary (with period 100)
    assert_eq!(channel.compute_arrival_time(100, 100), Some(1100));

    // 50 + 1000 = 1050 → ceil to 1100
    assert_eq!(channel.compute_arrival_time(50, 100), Some(1100));
}

// ============================================================================
// ChannelResult Tests
// ============================================================================

#[test]
fn test_channel_result_delivered() {
    let channel = ChannelDesc::new(1, 2)
        .with_latency(5)
        .with_alignment(TimeAlignment::CeilToTick);

    let result = channel.transform_event_time(100, 10);
    assert!(result.is_delivered());
    assert_eq!(result.arrival_time(), Some(110));

    match result {
        ChannelResult::Delivered { arrival_time } => {
            assert_eq!(arrival_time, 110);
        }
        _ => panic!("Expected Delivered"),
    }
}

#[test]
fn test_channel_result_alignment_failed() {
    let channel = ChannelDesc::new(1, 2)
        .with_latency(5)
        .with_alignment(TimeAlignment::StrictTickBoundary);

    let result = channel.transform_event_time(100, 10); // 105 not on boundary
    assert!(!result.is_delivered());
    assert_eq!(result.arrival_time(), None);

    match result {
        ChannelResult::AlignmentFailed { original_time } => {
            assert_eq!(original_time, 105); // 100 + 5 latency
        }
        _ => panic!("Expected AlignmentFailed"),
    }
}

// ============================================================================
// ChannelRegistry Tests
// ============================================================================

#[test]
fn test_registry_basic_operations() {
    let mut registry = ChannelRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    registry.add(ChannelDesc::new(1, 2).with_latency(5));
    registry.add(ChannelDesc::new(2, 3).with_latency(10));

    assert!(!registry.is_empty());
    assert_eq!(registry.len(), 2);
}

#[test]
fn test_registry_find() {
    let mut registry = ChannelRegistry::new();
    registry.add(ChannelDesc::new(1, 2).with_latency(5));
    registry.add(ChannelDesc::new(1, 2).with_latency(10)); // Same pair, different latency
    registry.add(ChannelDesc::new(2, 3).with_latency(15));

    // Find all channels between 1 and 2
    let channels = registry.find(1, 2);
    assert_eq!(channels.len(), 2);
    assert_eq!(channels[0].base_latency, 5);
    assert_eq!(channels[1].base_latency, 10);

    // Find one (returns first added)
    let channel = registry.find_one(1, 2).unwrap();
    assert_eq!(channel.base_latency, 5);

    // Non-existent pair
    assert!(registry.find(3, 1).is_empty());
    assert!(registry.find_one(3, 1).is_none());
}

#[test]
fn test_registry_incoming_outgoing() {
    let mut registry = ChannelRegistry::new();
    registry.add(ChannelDesc::new(1, 2));
    registry.add(ChannelDesc::new(1, 3));
    registry.add(ChannelDesc::new(2, 3));
    registry.add(ChannelDesc::new(3, 1));

    // Outgoing from node 1
    let outgoing = registry.outgoing(1);
    assert_eq!(outgoing.len(), 2);

    // Incoming to node 3
    let incoming = registry.incoming(3);
    assert_eq!(incoming.len(), 2);

    // No outgoing from node that only receives
    // (In this case, all nodes have some outgoing)
}

// ============================================================================
// Edge Cases and Boundary Conditions
// ============================================================================

#[test]
fn test_alignment_at_time_zero() {
    let alignments = [
        TimeAlignment::CeilToTick,
        TimeAlignment::FloorToTick,
        TimeAlignment::StrictTickBoundary,
    ];

    for align in &alignments {
        // Time 0 should always align to 0
        assert_eq!(align.align(0, 10), Some(0), "Failed for {:?}", align);
    }
}

#[test]
fn test_alignment_tick_period_one() {
    let align = TimeAlignment::CeilToTick;

    // With tick period 1, every time is on a boundary
    assert_eq!(align.align(0, 1), Some(0));
    assert_eq!(align.align(1, 1), Some(1));
    assert_eq!(align.align(100, 1), Some(100));
}

#[test]
fn test_channel_self_loop() {
    let channel = ChannelDesc::new(1, 1).with_latency(10);

    assert!(channel.is_self_loop());
    assert!(channel.connects(1, 1));
    assert!(!channel.connects(1, 2));
}

#[test]
fn test_channel_effective_latency() {
    let channel = ChannelDesc::new(1, 2)
        .with_latency(10)
        .with_latency_range(5, 20);

    // Currently returns base latency
    assert_eq!(channel.effective_latency(), 10);
}

// ============================================================================
// Alignment Correctness Verification
// ============================================================================

#[test]
fn test_ceil_to_tick_never_goes_backward() {
    // CeilToTick should never produce a result less than input
    let align = TimeAlignment::CeilToTick;

    for event_time in [0, 1, 9, 10, 11, 99, 100, 101, 999, 1000, 1001] {
        let aligned = align.align(event_time, 10).unwrap();
        assert!(
            aligned >= event_time,
            "CeilToTick went backward: {} -> {}",
            event_time,
            aligned
        );
    }
}

#[test]
fn test_floor_to_tick_never_goes_forward() {
    // FloorToTick should never produce a result greater than input
    let align = TimeAlignment::FloorToTick;

    for event_time in [0, 1, 9, 10, 11, 99, 100, 101, 999, 1000, 1001] {
        let aligned = align.align(event_time, 10).unwrap();
        assert!(
            aligned <= event_time,
            "FloorToTick went forward: {} -> {}",
            event_time,
            aligned
        );
    }
}

#[test]
fn test_aligned_times_are_on_boundaries() {
    let tick_period = 10;

    // CeilToTick result should be on boundary
    for event_time in [1, 5, 9, 11, 15, 19, 100, 105] {
        let aligned = TimeAlignment::CeilToTick.align(event_time, tick_period).unwrap();
        assert!(
            TimeAlignment::is_on_boundary(aligned, tick_period),
            "CeilToTick result {} is not on boundary",
            aligned
        );
    }

    // FloorToTick result should be on boundary
    for event_time in [1, 5, 9, 11, 15, 19, 100, 105] {
        let aligned = TimeAlignment::FloorToTick.align(event_time, tick_period).unwrap();
        assert!(
            TimeAlignment::is_on_boundary(aligned, tick_period),
            "FloorToTick result {} is not on boundary",
            aligned
        );
    }
}


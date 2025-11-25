//! Mini Cache Simulation Example
//!
//! This example demonstrates a simple cache system simulation with:
//! - A CPU core generating memory requests (Tick-driven)
//! - An L1 cache with hit/miss behavior (Event-driven)
//! - A memory controller with variable latency (Event-driven)
//!
//! The simulation showcases:
//! - Mixed Tick + Event subgraph execution
//! - Cross-subgraph event routing
//! - Probabilistic delay modeling
//! - Statistics collection

use akiku::executor::event::EventSubgraphExecutor;
use akiku::executor::tick::TickSubgraphExecutor;
use akiku::node::Node;
use akiku::nodes::probabilistic::{DelayDistribution, ProbabilisticDelayNode};
use akiku::types::{NodeId, SimTime};
use akiku::{ChannelDesc, Event, EventPayload, SimulationEngine, SubgraphConfig, TimeAlignment};

// ============================================================================
// Cache Configuration
// ============================================================================

const CPU_TICK_PERIOD: SimTime = 1;      // 1ns per CPU cycle
const L1_HIT_LATENCY: SimTime = 4;       // 4ns L1 hit
const L1_HIT_RATE: f64 = 0.9;            // 90% hit rate
const MEMORY_LATENCY_MIN: SimTime = 50;  // Min memory latency
const MEMORY_LATENCY_MAX: SimTime = 100; // Max memory latency
const SIMULATION_TIME: SimTime = 1000;   // Total simulation time

// Node IDs
const CPU_NODE: NodeId = 1;
const L1_CACHE_NODE: NodeId = 10;
const MEMORY_NODE: NodeId = 20;

// Subgraph IDs
const CPU_SUBGRAPH: u32 = 1;
const CACHE_SUBGRAPH: u32 = 2;

// ============================================================================
// CPU Core Node (Tick-driven)
// ============================================================================

/// A simple CPU core that generates memory requests periodically.
struct CpuCore {
    id: NodeId,
    request_interval: u64,  // Generate request every N ticks
    tick_count: u64,
    requests_sent: u64,
    responses_received: u64,
    total_latency: u64,
    pending_requests: std::collections::HashMap<u64, SimTime>, // req_id -> send_time
}

impl CpuCore {
    fn new(id: NodeId, request_interval: u64) -> Self {
        Self {
            id,
            request_interval,
            tick_count: 0,
            requests_sent: 0,
            responses_received: 0,
            total_latency: 0,
            pending_requests: std::collections::HashMap::new(),
        }
    }

    fn average_latency(&self) -> f64 {
        if self.responses_received > 0 {
            self.total_latency as f64 / self.responses_received as f64
        } else {
            0.0
        }
    }
}

impl Node for CpuCore {
    fn init(&mut self) {
        self.tick_count = 0;
        self.requests_sent = 0;
        self.responses_received = 0;
        self.total_latency = 0;
        self.pending_requests.clear();
    }

    fn on_tick(&mut self, time: SimTime) -> Vec<Event> {
        self.tick_count += 1;

        if self.tick_count % self.request_interval == 0 {
            let req_id = self.requests_sent;
            self.requests_sent += 1;
            self.pending_requests.insert(req_id, time);

            // Generate a memory request
            vec![Event::message(
                time,
                self.id,
                "mem_req",
                L1_CACHE_NODE,
                "request",
                serde_json::json!({
                    "req_id": req_id,
                    "address": req_id * 64,  // Simulate different addresses
                    "type": "read",
                }),
            )]
        } else {
            Vec::new()
        }
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        // Handle memory response
        if let EventPayload::Message { data, .. } = &event.payload {
            if let Some(req_id) = data.get("req_id").and_then(|v| v.as_u64()) {
                if let Some(send_time) = self.pending_requests.remove(&req_id) {
                    self.responses_received += 1;
                    self.total_latency += event.time - send_time;
                }
            }
        }
        Vec::new()
    }
}

// ============================================================================
// L1 Cache Node (Event-driven)
// ============================================================================

/// A simple L1 cache model with probabilistic hit/miss behavior.
struct L1Cache {
    id: NodeId,
    hit_rate: f64,
    hit_latency: SimTime,
    seed: u64,
    request_count: u64,
    hits: u64,
    misses: u64,
}

impl L1Cache {
    fn new(id: NodeId, hit_rate: f64, hit_latency: SimTime) -> Self {
        Self {
            id,
            hit_rate,
            hit_latency,
            seed: 42,
            request_count: 0,
            hits: 0,
            misses: 0,
        }
    }

    fn is_hit(&mut self) -> bool {
        // Simple PRNG
        let mut x = self.seed.wrapping_add(self.request_count);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed = x;

        let random = (x as f64) / (u64::MAX as f64);
        random < self.hit_rate
    }
}

impl Node for L1Cache {
    fn init(&mut self) {
        self.request_count = 0;
        self.hits = 0;
        self.misses = 0;
        self.seed = 42;
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        Vec::new()
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        self.request_count += 1;

        if let EventPayload::Message { data, src, .. } = &event.payload {
            let req_id = data.get("req_id").and_then(|v| v.as_u64()).unwrap_or(0);

            if self.is_hit() {
                // Cache hit - respond directly with hit latency
                self.hits += 1;
                vec![Event::message(
                    event.time + self.hit_latency,
                    self.id,
                    "response",
                    src.0,  // Reply to requester
                    "response",
                    serde_json::json!({
                        "req_id": req_id,
                        "hit": true,
                        "data": "cached_data",
                    }),
                )]
            } else {
                // Cache miss - forward to memory
                self.misses += 1;
                vec![Event::message(
                    event.time,
                    self.id,
                    "mem_req",
                    MEMORY_NODE,
                    "request",
                    serde_json::json!({
                        "req_id": req_id,
                        "original_requester": src.0,
                        "address": data.get("address").unwrap_or(&serde_json::json!(0)),
                    }),
                )]
            }
        } else {
            Vec::new()
        }
    }
}

// ============================================================================
// Memory Response Forwarder
// ============================================================================

/// Forwards memory responses back to the original requester.
struct MemoryResponseForwarder {
    id: NodeId,
    responses_forwarded: u64,
}

impl MemoryResponseForwarder {
    fn new(id: NodeId) -> Self {
        Self {
            id,
            responses_forwarded: 0,
        }
    }
}

impl Node for MemoryResponseForwarder {
    fn init(&mut self) {
        self.responses_forwarded = 0;
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        Vec::new()
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        if let EventPayload::Message { data, .. } = &event.payload {
            let req_id = data.get("req_id").and_then(|v| v.as_u64()).unwrap_or(0);
            let original_requester = data
                .get("original_requester")
                .and_then(|v| v.as_u64())
                .unwrap_or(CPU_NODE);

            self.responses_forwarded += 1;

            vec![Event::message(
                event.time,
                self.id,
                "response",
                original_requester,
                "response",
                serde_json::json!({
                    "req_id": req_id,
                    "hit": false,
                    "data": "memory_data",
                }),
            )]
        } else {
            Vec::new()
        }
    }
}

// ============================================================================
// Main Simulation
// ============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║           Mini Cache Simulation Example                  ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Configuration:                                           ║");
    println!("║   CPU Tick Period: {} ns                                 ║", CPU_TICK_PERIOD);
    println!("║   L1 Hit Latency:  {} ns                                 ║", L1_HIT_LATENCY);
    println!("║   L1 Hit Rate:     {:.0}%                                 ║", L1_HIT_RATE * 100.0);
    println!("║   Memory Latency:  {}-{} ns                           ║", MEMORY_LATENCY_MIN, MEMORY_LATENCY_MAX);
    println!("║   Simulation Time: {} ns                              ║", SIMULATION_TIME);
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // Create the simulation engine
    let mut engine = SimulationEngine::new(10); // 10ns global time step

    // ========================================================================
    // Subgraph 1: CPU Core (Tick-driven)
    // ========================================================================
    let mut cpu_exec = TickSubgraphExecutor::new(CPU_SUBGRAPH, CPU_TICK_PERIOD);
    
    // CPU generates a request every 10 ticks
    cpu_exec.add_node(CPU_NODE, Box::new(CpuCore::new(CPU_NODE, 10)));
    cpu_exec.set_execution_order(vec![CPU_NODE]);

    engine.add_subgraph(
        Box::new(cpu_exec),
        SubgraphConfig::tick(CPU_SUBGRAPH, CPU_TICK_PERIOD),
    );

    // ========================================================================
    // Subgraph 2: Cache Hierarchy (Event-driven)
    // ========================================================================
    let mut cache_exec = EventSubgraphExecutor::new(CACHE_SUBGRAPH);

    // L1 Cache
    cache_exec.add_node(
        L1_CACHE_NODE,
        Box::new(L1Cache::new(L1_CACHE_NODE, L1_HIT_RATE, L1_HIT_LATENCY)),
    );

    // Memory with probabilistic latency
    cache_exec.add_node(
        MEMORY_NODE,
        Box::new(
            ProbabilisticDelayNode::new(
                MEMORY_NODE,
                DelayDistribution::Uniform {
                    min: MEMORY_LATENCY_MIN,
                    max: MEMORY_LATENCY_MAX,
                },
            )
            .with_forward(MEMORY_NODE + 1, "forward")
            .with_seed(12345),
        ),
    );

    // Memory response forwarder
    cache_exec.add_node(
        MEMORY_NODE + 1,
        Box::new(MemoryResponseForwarder::new(MEMORY_NODE + 1)),
    );

    engine.add_subgraph(
        Box::new(cache_exec),
        SubgraphConfig::event(CACHE_SUBGRAPH),
    );

    // ========================================================================
    // Channels (Cross-subgraph communication)
    // ========================================================================

    // CPU -> Cache (memory requests)
    engine.add_channel(
        ChannelDesc::new(CPU_SUBGRAPH, CACHE_SUBGRAPH)
            .with_latency(1)
            .with_alignment(TimeAlignment::CeilToTick),
    );

    // Cache -> CPU (memory responses)
    engine.add_channel(
        ChannelDesc::new(CACHE_SUBGRAPH, CPU_SUBGRAPH)
            .with_latency(1)
            .with_alignment(TimeAlignment::CeilToTick),
    );

    // ========================================================================
    // Run Simulation
    // ========================================================================
    println!("Starting simulation...");
    println!();

    engine.init();
    engine.run(SIMULATION_TIME);

    // ========================================================================
    // Collect and Display Results
    // ========================================================================
    let stats = engine.export_stats();

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║                   Simulation Results                     ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Engine Statistics:                                       ║");
    println!("║   Final Time:       {} ns                              ║", 
             stats["engine"]["current_time"]);
    println!("║   Steps Executed:   {}                                ║", 
             stats["engine"]["steps_executed"]);
    println!("║   Events Dispatched: {}                                 ║", 
             stats["engine"]["events_dispatched"]);
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Subgraph Statistics:                                     ║");

    if let Some(subgraphs) = stats["subgraphs"].as_object() {
        for (id, sg_stats) in subgraphs {
            println!("║   Subgraph {}:                                           ║", id);
            if let Some(ticks) = sg_stats.get("ticks_executed") {
                println!("║     Ticks: {}                                         ║", ticks);
            }
            if let Some(events) = sg_stats.get("events_processed") {
                println!("║     Events Processed: {}                               ║", events);
            }
        }
    }

    println!("╚══════════════════════════════════════════════════════════╝");
    println!();
    println!("Simulation completed successfully!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_core() {
        let mut cpu = CpuCore::new(1, 5);
        cpu.init();

        let mut total_requests = 0;
        for t in 0..20 {
            let events = cpu.on_tick(t);
            total_requests += events.len();
        }

        // Should generate request every 5 ticks: at tick 5, 10, 15, 20 = 4 requests
        assert_eq!(total_requests, 4);
        assert_eq!(cpu.requests_sent, 4);
    }

    #[test]
    fn test_l1_cache_hit_miss() {
        let mut cache = L1Cache::new(10, 0.5, 4);
        cache.init();

        let mut hits = 0;
        let mut misses = 0;

        for i in 0..100 {
            let event = Event::message(
                i,
                1,
                "out",
                10,
                "request",
                serde_json::json!({"req_id": i, "address": i * 64}),
            );
            let responses = cache.on_event(event);

            if responses.len() == 1 {
                if let EventPayload::Message { data, dst, .. } = &responses[0].payload {
                    if dst.0 == 1 {
                        hits += 1; // Response to CPU = hit
                    } else {
                        misses += 1; // Forward to memory = miss
                    }
                }
            }
        }

        // With 50% hit rate, we should have roughly equal hits and misses
        assert!(hits > 30 && hits < 70, "hits: {}", hits);
        assert!(misses > 30 && misses < 70, "misses: {}", misses);
    }
}

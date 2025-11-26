//! Simple logic gate simulation example.
//!
//! A tick-driven source generates two random bits every cycle and sends them to
//! an event-driven AND gate node. The gate computes the logical AND and prints
//! the input/output tuple, demonstrating how to connect tick and event
//! subgraphs with a channel.

use akiku::executor::event::EventSubgraphExecutor;
use akiku::executor::tick::TickSubgraphExecutor;
use akiku::node::Node;
use akiku::types::{NodeId, SimTime};
use akiku::{ChannelDesc, Event, EventPayload, SimulationEngine, SubgraphConfig, TimeAlignment};

const SOURCE_SUBGRAPH: u32 = 1;
const GATE_SUBGRAPH: u32 = 2;
const SOURCE_NODE: NodeId = 10;
const AND_NODE: NodeId = 200;
const TICK_PERIOD: SimTime = 5;
const SIM_TIME: SimTime = 200;

// -----------------------------------------------------------------------------
// Tick-driven random bit source
// -----------------------------------------------------------------------------

struct RandomBitSource {
    id: NodeId,
    seed: u64,
    emitted: u64,
}

impl RandomBitSource {
    fn new(id: NodeId, seed: u64) -> Self {
        Self { id, seed, emitted: 0 }
    }

    fn next_bits(&mut self) -> (bool, bool) {
        // Xorshift64* PRNG to avoid external dependencies
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed = x;

        // Use the lowest two bits as boolean values
        let bit_a = (x & 0b01) != 0;
        let bit_b = (x & 0b10) != 0;
        (bit_a, bit_b)
    }
}

impl Node for RandomBitSource {
    fn init(&mut self) {
        self.emitted = 0;
    }

    fn on_tick(&mut self, time: SimTime) -> Vec<Event> {
        let (bit_a, bit_b) = self.next_bits();
        self.emitted += 1;

        vec![Event::message(
            time,
            self.id,
            "bits",
            AND_NODE,
            "input",
            serde_json::json!({
                "bit_a": bit_a,
                "bit_b": bit_b,
                "sample": self.emitted,
            }),
        )]
    }

    fn on_event(&mut self, _event: Event) -> Vec<Event> {
        Vec::new()
    }
}

// -----------------------------------------------------------------------------
// Event-driven AND gate / probe
// -----------------------------------------------------------------------------

struct AndGateProbe {
    id: NodeId,
    observed: u64,
    ones: u64,
}

impl AndGateProbe {
    fn new(id: NodeId) -> Self {
        Self {
            id,
            observed: 0,
            ones: 0,
        }
    }
}

impl Node for AndGateProbe {
    fn init(&mut self) {
        self.observed = 0;
        self.ones = 0;
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        Vec::new()
    }

    fn on_event(&mut self, event: Event) -> Vec<Event> {
        if let EventPayload::Message { data, .. } = &event.payload {
            let bit_a = data.get("bit_a").and_then(|v| v.as_bool()).unwrap_or(false);
            let bit_b = data.get("bit_b").and_then(|v| v.as_bool()).unwrap_or(false);
            let sample = data.get("sample").and_then(|v| v.as_u64()).unwrap_or(0);
            let output = bit_a && bit_b;

            self.observed += 1;
            if output {
                self.ones += 1;
            }

            println!(
                "time {:>3}  sample {:>3}: {:>5} & {:>5} => {:>5}",
                event.time,
                sample,
                bit_a as u8,
                bit_b as u8,
                output as u8
            );
        }
        Vec::new()
    }
}

// -----------------------------------------------------------------------------
// Main simulation
// -----------------------------------------------------------------------------

fn main() {
    println!("==== Logic gate example ====");
    println!("Tick source generates two bits; AND gate consumes them.\n");

    let mut engine = SimulationEngine::new(TICK_PERIOD);

    // Subgraph A: tick-driven source
    let mut source_exec = TickSubgraphExecutor::new(SOURCE_SUBGRAPH, TICK_PERIOD);
    source_exec.add_node(SOURCE_NODE, Box::new(RandomBitSource::new(SOURCE_NODE, 0x5eed))); // seeded for determinism
    source_exec.set_execution_order(vec![SOURCE_NODE]);
    engine.add_subgraph(
        Box::new(source_exec),
        SubgraphConfig::tick(SOURCE_SUBGRAPH, TICK_PERIOD),
    );

    // Subgraph B: event-driven AND gate
    let mut gate_exec = EventSubgraphExecutor::new(GATE_SUBGRAPH);
    gate_exec.add_node(AND_NODE, Box::new(AndGateProbe::new(AND_NODE)));
    engine.add_subgraph(Box::new(gate_exec), SubgraphConfig::event(GATE_SUBGRAPH));

    // Channel from source to gate
    engine.add_channel(
        ChannelDesc::new(SOURCE_SUBGRAPH, GATE_SUBGRAPH)
            .with_latency(1)
            .with_alignment(TimeAlignment::CeilToTick),
    );

    engine.init();
    engine.run(SIM_TIME);

    let stats = engine.export_stats();
    println!("\nEngine advanced to time {} ns", stats["engine"]["current_time"]);
    println!("Samples observed: {}", stats["subgraphs"][GATE_SUBGRAPH.to_string()]["events_processed"].as_u64().unwrap_or(0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_bit_source_emits_each_tick() {
        let mut src = RandomBitSource::new(1, 123);
        src.init();

        for t in 0..10 {
            let events = src.on_tick(t);
            assert_eq!(events.len(), 1);
        }
    }
}

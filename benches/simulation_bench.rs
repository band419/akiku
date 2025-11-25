//! Performance benchmarks for the Akiku simulation framework.
//!
//! Run with: `cargo bench`
//! Or for specific bench: `cargo bench --bench simulation_bench`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use akiku::executor::event::EventSubgraphExecutor;
use akiku::executor::tick::TickSubgraphExecutor;
use akiku::node::Node;
use akiku::parallel::{ParallelSimulationEngine, SubgraphConfig as ParallelSubgraphConfig};
use akiku::types::{NodeId, SimTime};
use akiku::{Event, EventPayload, SimulationEngine, SubgraphConfig};

// ============================================================================
// Benchmark Nodes
// ============================================================================

/// A simple node that counts invocations (no I/O)
struct BenchCounterNode {
    id: NodeId,
    count: u64,
}

impl BenchCounterNode {
    fn new(id: NodeId) -> Self {
        Self { id, count: 0 }
    }
}

impl Node for BenchCounterNode {
    fn init(&mut self) {
        self.count = 0;
    }

    fn on_tick(&mut self, _time: SimTime) -> Vec<Event> {
        self.count += 1;
        Vec::new()
    }

    fn on_event(&mut self, _event: Event) -> Vec<Event> {
        self.count += 1;
        Vec::new()
    }
}

/// A node that generates events to another node
struct BenchEventGenerator {
    id: NodeId,
    target_id: NodeId,
    count: u64,
}

impl BenchEventGenerator {
    fn new(id: NodeId, target_id: NodeId) -> Self {
        Self {
            id,
            target_id,
            count: 0,
        }
    }
}

impl Node for BenchEventGenerator {
    fn init(&mut self) {
        self.count = 0;
    }

    fn on_tick(&mut self, time: SimTime) -> Vec<Event> {
        self.count += 1;
        vec![Event::message(
            time,
            self.id,
            "out",
            self.target_id,
            "in",
            serde_json::json!({"seq": self.count}),
        )]
    }

    fn on_event(&mut self, _event: Event) -> Vec<Event> {
        Vec::new()
    }
}

// ============================================================================
// Tick Executor Benchmarks
// ============================================================================

fn bench_tick_executor(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick_executor");

    for num_nodes in [1, 10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*num_nodes as u64));
        group.bench_with_input(
            BenchmarkId::new("nodes", num_nodes),
            num_nodes,
            |b, &num_nodes| {
                b.iter(|| {
                    let mut exec = TickSubgraphExecutor::new(1, 1);

                    // Add nodes
                    for i in 0..num_nodes {
                        exec.add_node(i as NodeId, Box::new(BenchCounterNode::new(i as NodeId)));
                    }
                    exec.set_execution_order((0..num_nodes as NodeId).collect());

                    exec.init();

                    // Run for 100 ticks
                    black_box(exec.run_until(100));
                });
            },
        );
    }

    group.finish();
}

fn bench_tick_executor_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick_throughput");

    for ticks in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*ticks as u64));
        group.bench_with_input(BenchmarkId::new("ticks", ticks), ticks, |b, &ticks| {
            let mut exec = TickSubgraphExecutor::new(1, 1);

            // Add 10 nodes
            for i in 0..10 {
                exec.add_node(i, Box::new(BenchCounterNode::new(i)));
            }
            exec.set_execution_order((0..10).collect());

            b.iter(|| {
                exec.init();
                black_box(exec.run_until(ticks));
            });
        });
    }

    group.finish();
}

// ============================================================================
// Event Executor Benchmarks
// ============================================================================

fn bench_event_executor(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_executor");

    for num_events in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*num_events as u64));
        group.bench_with_input(
            BenchmarkId::new("events", num_events),
            num_events,
            |b, &num_events| {
                b.iter(|| {
                    let mut exec = EventSubgraphExecutor::new(1);
                    exec.add_node(1, Box::new(BenchCounterNode::new(1)));

                    exec.init();

                    // Schedule events
                    for i in 0..num_events as u64 {
                        exec.schedule_event(Event::message(
                            i,
                            0,
                            "ext",
                            1,
                            "in",
                            serde_json::json!({}),
                        ));
                    }

                    black_box(exec.run_until(num_events as SimTime));
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Engine Benchmarks
// ============================================================================

fn bench_engine_single_subgraph(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_single");

    for steps in [100, 500, 1000].iter() {
        group.throughput(Throughput::Elements(*steps as u64));
        group.bench_with_input(BenchmarkId::new("steps", steps), steps, |b, &steps| {
            b.iter(|| {
                let mut engine = SimulationEngine::new(1);

                let mut exec = TickSubgraphExecutor::new(1, 1);
                for i in 0..10 {
                    exec.add_node(i, Box::new(BenchCounterNode::new(i)));
                }
                exec.set_execution_order((0..10).collect());
                engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(1, 1));

                engine.init();
                black_box(engine.run(steps));
            });
        });
    }

    group.finish();
}

fn bench_engine_multiple_subgraphs(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_multi");

    for num_subgraphs in [2, 4, 8].iter() {
        group.throughput(Throughput::Elements(*num_subgraphs as u64));
        group.bench_with_input(
            BenchmarkId::new("subgraphs", num_subgraphs),
            num_subgraphs,
            |b, &num_subgraphs| {
                b.iter(|| {
                    let mut engine = SimulationEngine::new(10);

                    for sg in 0..num_subgraphs {
                        let mut exec = TickSubgraphExecutor::new(sg as u32 + 1, 10);
                        let base_node = (sg * 10) as NodeId;
                        for i in 0..10 {
                            exec.add_node(base_node + i, Box::new(BenchCounterNode::new(base_node + i)));
                        }
                        exec.set_execution_order((base_node..base_node + 10).collect());
                        engine.add_subgraph(
                            Box::new(exec),
                            SubgraphConfig::tick(sg as u32 + 1, 10),
                        );
                    }

                    engine.init();
                    black_box(engine.run(1000));
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Parallel vs Sequential Benchmark
// ============================================================================

fn bench_parallel_vs_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_comparison");

    let num_subgraphs = 4;
    let nodes_per_subgraph = 50;
    let simulation_time = 1000;

    // Sequential engine
    group.bench_function("sequential", |b| {
        b.iter(|| {
            let mut engine = SimulationEngine::new(10);

            for sg in 0..num_subgraphs {
                let mut exec = TickSubgraphExecutor::new(sg + 1, 10);
                let base_node = (sg as u64 * nodes_per_subgraph as u64) as NodeId;
                for i in 0..nodes_per_subgraph {
                    exec.add_node(
                        base_node + i as u64,
                        Box::new(BenchCounterNode::new(base_node + i as u64)),
                    );
                }
                exec.set_execution_order(
                    (base_node..base_node + nodes_per_subgraph as u64).collect(),
                );
                engine.add_subgraph(Box::new(exec), SubgraphConfig::tick(sg + 1, 10));
            }

            engine.init();
            black_box(engine.run(simulation_time));
        });
    });

    // Parallel engine
    group.bench_function("parallel", |b| {
        b.iter(|| {
            let mut engine = ParallelSimulationEngine::new(10);

            for sg in 0..num_subgraphs {
                let mut exec = TickSubgraphExecutor::new(sg + 1, 10);
                let base_node = (sg as u64 * nodes_per_subgraph as u64) as NodeId;
                for i in 0..nodes_per_subgraph {
                    exec.add_node(
                        base_node + i as u64,
                        Box::new(BenchCounterNode::new(base_node + i as u64)),
                    );
                }
                exec.set_execution_order(
                    (base_node..base_node + nodes_per_subgraph as u64).collect(),
                );
                engine.add_subgraph(Box::new(exec), ParallelSubgraphConfig::tick(sg + 1, 10));
            }

            engine.init();
            black_box(engine.run(simulation_time));
        });
    });

    group.finish();
}

// ============================================================================
// Event Queue Benchmarks
// ============================================================================

fn bench_event_queue(c: &mut Criterion) {
    use akiku::executor::event::EventQueue;

    let mut group = c.benchmark_group("event_queue");

    for num_events in [1000, 10000, 100000].iter() {
        group.throughput(Throughput::Elements(*num_events as u64));

        // Push benchmark
        group.bench_with_input(
            BenchmarkId::new("push", num_events),
            num_events,
            |b, &num_events| {
                b.iter(|| {
                    let mut queue = EventQueue::new();
                    for i in 0..num_events as u64 {
                        queue.push(Event::message(i, 0, "out", 1, "in", serde_json::json!({})));
                    }
                    black_box(queue.len());
                });
            },
        );

        // Pop benchmark
        group.bench_with_input(
            BenchmarkId::new("pop", num_events),
            num_events,
            |b, &num_events| {
                b.iter_batched(
                    || {
                        let mut queue = EventQueue::new();
                        for i in 0..num_events as u64 {
                            queue.push(Event::message(i, 0, "out", 1, "in", serde_json::json!({})));
                        }
                        queue
                    },
                    |mut queue| {
                        while queue.pop().is_some() {}
                        black_box(queue.len());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    benches,
    bench_tick_executor,
    bench_tick_executor_throughput,
    bench_event_executor,
    bench_engine_single_subgraph,
    bench_engine_multiple_subgraphs,
    bench_parallel_vs_sequential,
    bench_event_queue,
);

criterion_main!(benches);

//! Statistics collection and export for the simulation framework.
//!
//! This module provides comprehensive statistics tracking and multiple
//! export formats (JSON, CSV) for simulation analysis.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use crate::types::{SimTime, SubgraphId};

/// Aggregate statistics for a simulation run.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SimulationStats {
    /// Simulation metadata
    pub metadata: SimulationMetadata,

    /// Engine-level statistics
    pub engine: EngineStats,

    /// Per-subgraph statistics
    pub subgraphs: HashMap<SubgraphId, SubgraphStats>,

    /// Timing statistics
    pub timing: TimingStats,
}

/// Metadata about the simulation run.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SimulationMetadata {
    /// Simulation name/description
    pub name: String,

    /// Start time (wall clock)
    pub start_time: Option<String>,

    /// End time (wall clock)
    pub end_time: Option<String>,

    /// Simulation version
    pub version: String,

    /// Configuration file used (if any)
    pub config_file: Option<String>,
}

/// Engine-level statistics.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EngineStats {
    /// Final simulation time
    pub final_time: SimTime,

    /// Total time steps executed
    pub steps_executed: u64,

    /// Total events dispatched between subgraphs
    pub events_dispatched: u64,

    /// Total events processed
    pub events_processed: u64,

    /// Events dropped due to errors
    pub events_dropped: u64,

    /// Number of subgraphs
    pub subgraph_count: usize,

    /// Number of channels
    pub channel_count: usize,
}

/// Statistics for a single subgraph.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SubgraphStats {
    /// Subgraph identifier
    pub id: SubgraphId,

    /// Whether this is a tick or event subgraph
    pub mode: String,

    /// Tick period (for tick subgraphs)
    pub tick_period: Option<SimTime>,

    /// Final local time
    pub final_time: SimTime,

    /// For tick subgraphs: number of ticks executed
    pub ticks_executed: Option<u64>,

    /// For tick subgraphs: total node invocations
    pub node_invocations: Option<u64>,

    /// For event subgraphs: events processed
    pub events_processed: Option<u64>,

    /// For event subgraphs: events generated
    pub events_generated: Option<u64>,

    /// Incoming events from other subgraphs
    pub incoming_events: u64,

    /// Outgoing events to other subgraphs
    pub outgoing_events: u64,

    /// Peak queue size (for event subgraphs)
    pub peak_queue_size: Option<usize>,

    /// Number of nodes in subgraph
    pub node_count: usize,
}

/// Timing/performance statistics.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TimingStats {
    /// Total wall-clock time in milliseconds
    pub total_wall_time_ms: f64,

    /// Simulation time per wall-clock second
    pub sim_time_per_second: f64,

    /// Events processed per second
    pub events_per_second: f64,

    /// Ticks executed per second
    pub ticks_per_second: f64,
}

impl SimulationStats {
    /// Creates a new empty statistics container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the simulation name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.metadata.name = name.into();
        self
    }

    /// Records the start time.
    pub fn record_start(&mut self) {
        self.metadata.start_time = Some(chrono_now());
    }

    /// Records the end time.
    pub fn record_end(&mut self) {
        self.metadata.end_time = Some(chrono_now());
    }

    /// Updates timing statistics based on wall clock time.
    pub fn compute_timing(&mut self, wall_time_ms: f64) {
        self.timing.total_wall_time_ms = wall_time_ms;

        if wall_time_ms > 0.0 {
            let seconds = wall_time_ms / 1000.0;
            self.timing.sim_time_per_second = self.engine.final_time as f64 / seconds;
            self.timing.events_per_second = self.engine.events_processed as f64 / seconds;

            let total_ticks: u64 = self
                .subgraphs
                .values()
                .filter_map(|s| s.ticks_executed)
                .sum();
            self.timing.ticks_per_second = total_ticks as f64 / seconds;
        }
    }

    /// Exports statistics to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Exports statistics to JSON file.
    pub fn to_json_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let json = self.to_json().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        std::fs::write(path, json)
    }

    /// Exports summary statistics to CSV.
    pub fn to_csv(&self) -> String {
        let mut csv = String::new();

        // Header
        csv.push_str("metric,value\n");

        // Engine stats
        csv.push_str(&format!("final_time,{}\n", self.engine.final_time));
        csv.push_str(&format!("steps_executed,{}\n", self.engine.steps_executed));
        csv.push_str(&format!("events_dispatched,{}\n", self.engine.events_dispatched));
        csv.push_str(&format!("events_processed,{}\n", self.engine.events_processed));
        csv.push_str(&format!("events_dropped,{}\n", self.engine.events_dropped));
        csv.push_str(&format!("subgraph_count,{}\n", self.engine.subgraph_count));
        csv.push_str(&format!("channel_count,{}\n", self.engine.channel_count));

        // Timing stats
        csv.push_str(&format!("wall_time_ms,{:.2}\n", self.timing.total_wall_time_ms));
        csv.push_str(&format!("sim_time_per_second,{:.2}\n", self.timing.sim_time_per_second));
        csv.push_str(&format!("events_per_second,{:.2}\n", self.timing.events_per_second));

        csv
    }

    /// Exports summary statistics to CSV file.
    pub fn to_csv_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        std::fs::write(path, self.to_csv())
    }

    /// Exports per-subgraph statistics to CSV.
    pub fn subgraphs_to_csv(&self) -> String {
        let mut csv = String::new();

        // Header
        csv.push_str("subgraph_id,mode,final_time,ticks_executed,events_processed,incoming_events,outgoing_events\n");

        // Data rows
        for (id, stats) in &self.subgraphs {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                id,
                stats.mode,
                stats.final_time,
                stats.ticks_executed.map(|v| v.to_string()).unwrap_or_default(),
                stats.events_processed.map(|v| v.to_string()).unwrap_or_default(),
                stats.incoming_events,
                stats.outgoing_events,
            ));
        }

        csv
    }

    /// Exports per-subgraph statistics to CSV file.
    pub fn subgraphs_to_csv_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        std::fs::write(path, self.subgraphs_to_csv())
    }

    /// Writes a human-readable summary to a writer.
    pub fn write_summary<W: Write>(&self, mut w: W) -> std::io::Result<()> {
        writeln!(w, "=== Simulation Statistics ===")?;
        writeln!(w)?;

        if !self.metadata.name.is_empty() {
            writeln!(w, "Name: {}", self.metadata.name)?;
        }
        if let Some(ref start) = self.metadata.start_time {
            writeln!(w, "Started: {}", start)?;
        }
        if let Some(ref end) = self.metadata.end_time {
            writeln!(w, "Ended: {}", end)?;
        }
        writeln!(w)?;

        writeln!(w, "--- Engine ---")?;
        writeln!(w, "Final simulation time: {}", self.engine.final_time)?;
        writeln!(w, "Steps executed: {}", self.engine.steps_executed)?;
        writeln!(w, "Events dispatched: {}", self.engine.events_dispatched)?;
        writeln!(w, "Events processed: {}", self.engine.events_processed)?;
        writeln!(w, "Events dropped: {}", self.engine.events_dropped)?;
        writeln!(w, "Subgraphs: {}", self.engine.subgraph_count)?;
        writeln!(w, "Channels: {}", self.engine.channel_count)?;
        writeln!(w)?;

        writeln!(w, "--- Timing ---")?;
        writeln!(w, "Wall time: {:.2} ms", self.timing.total_wall_time_ms)?;
        writeln!(w, "Sim time/sec: {:.2}", self.timing.sim_time_per_second)?;
        writeln!(w, "Events/sec: {:.2}", self.timing.events_per_second)?;
        writeln!(w)?;

        writeln!(w, "--- Subgraphs ---")?;
        for (id, stats) in &self.subgraphs {
            writeln!(w, "Subgraph {} ({}):", id, stats.mode)?;
            writeln!(w, "  Final time: {}", stats.final_time)?;
            if let Some(ticks) = stats.ticks_executed {
                writeln!(w, "  Ticks: {}", ticks)?;
            }
            if let Some(events) = stats.events_processed {
                writeln!(w, "  Events processed: {}", events)?;
            }
            writeln!(w, "  Incoming: {}, Outgoing: {}", stats.incoming_events, stats.outgoing_events)?;
        }

        Ok(())
    }

    /// Returns a summary string.
    pub fn summary(&self) -> String {
        let mut buf = Vec::new();
        self.write_summary(&mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }
}

/// A simple timer for measuring wall-clock time.
#[derive(Debug)]
pub struct Timer {
    start: std::time::Instant,
}

impl Timer {
    /// Starts a new timer.
    pub fn start() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }

    /// Returns elapsed time in milliseconds.
    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    /// Returns elapsed time in seconds.
    pub fn elapsed_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::start()
    }
}

/// Returns current timestamp as string.
fn chrono_now() -> String {
    // Simple timestamp without external dependency
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    format!("{}s", duration.as_secs())
}

/// Statistics collector that can be attached to an engine.
#[derive(Debug, Default)]
pub struct StatsCollector {
    stats: SimulationStats,
    timer: Option<Timer>,
}

impl StatsCollector {
    /// Creates a new collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the simulation name.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.stats.metadata.name = name.into();
    }

    /// Starts timing.
    pub fn start(&mut self) {
        self.timer = Some(Timer::start());
        self.stats.record_start();
    }

    /// Stops timing and computes final statistics.
    pub fn stop(&mut self) {
        self.stats.record_end();
        if let Some(ref timer) = self.timer {
            self.stats.compute_timing(timer.elapsed_ms());
        }
    }

    /// Updates engine statistics from JSON export.
    pub fn update_from_json(&mut self, json: &serde_json::Value) {
        if let Some(engine) = json.get("engine") {
            self.stats.engine.final_time = engine["current_time"].as_u64().unwrap_or(0);
            self.stats.engine.steps_executed = engine["steps_executed"].as_u64().unwrap_or(0);
            self.stats.engine.events_dispatched = engine["events_dispatched"].as_u64().unwrap_or(0);
            self.stats.engine.events_processed = engine["events_processed"].as_u64().unwrap_or(0);
            self.stats.engine.events_dropped = engine["events_dropped"].as_u64().unwrap_or(0);
            self.stats.engine.subgraph_count = engine["subgraph_count"].as_u64().unwrap_or(0) as usize;
            self.stats.engine.channel_count = engine["channel_count"].as_u64().unwrap_or(0) as usize;
        }

        if let Some(subgraphs) = json.get("subgraphs").and_then(|s| s.as_object()) {
            for (id_str, sg_json) in subgraphs {
                if let Ok(id) = id_str.parse::<SubgraphId>() {
                    let mut sg_stats = SubgraphStats::default();
                    sg_stats.id = id;
                    sg_stats.final_time = sg_json["current_time"].as_u64().unwrap_or(0);

                    // Detect mode from available fields
                    if sg_json.get("ticks_executed").is_some() {
                        sg_stats.mode = "tick".to_string();
                        sg_stats.ticks_executed = sg_json["ticks_executed"].as_u64();
                        sg_stats.node_invocations = sg_json["node_invocations"].as_u64();
                        sg_stats.tick_period = sg_json["tick_period"].as_u64();
                    } else {
                        sg_stats.mode = "event".to_string();
                        sg_stats.events_processed = sg_json["events_processed"].as_u64();
                        sg_stats.events_generated = sg_json["events_generated"].as_u64();
                        sg_stats.peak_queue_size = sg_json["peak_queue_size"].as_u64().map(|v| v as usize);
                    }

                    sg_stats.incoming_events = sg_json["incoming_events"].as_u64().unwrap_or(0);
                    sg_stats.outgoing_events = sg_json["outgoing_events"].as_u64().unwrap_or(0);

                    self.stats.subgraphs.insert(id, sg_stats);
                }
            }
        }
    }

    /// Returns the collected statistics.
    pub fn stats(&self) -> &SimulationStats {
        &self.stats
    }

    /// Consumes the collector and returns the statistics.
    pub fn into_stats(self) -> SimulationStats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_creation() {
        let stats = SimulationStats::new()
            .with_name("Test Simulation");

        assert_eq!(stats.metadata.name, "Test Simulation");
    }

    #[test]
    fn test_stats_json_export() {
        let mut stats = SimulationStats::new();
        stats.engine.final_time = 1000;
        stats.engine.steps_executed = 100;

        let json = stats.to_json().unwrap();
        assert!(json.contains("1000"));
        assert!(json.contains("100"));
    }

    #[test]
    fn test_stats_csv_export() {
        let mut stats = SimulationStats::new();
        stats.engine.final_time = 1000;
        stats.engine.events_processed = 500;

        let csv = stats.to_csv();
        assert!(csv.contains("final_time,1000"));
        assert!(csv.contains("events_processed,500"));
    }

    #[test]
    fn test_subgraph_stats_csv() {
        let mut stats = SimulationStats::new();

        let mut sg = SubgraphStats::default();
        sg.id = 1;
        sg.mode = "tick".to_string();
        sg.final_time = 1000;
        sg.ticks_executed = Some(100);
        stats.subgraphs.insert(1, sg);

        let csv = stats.subgraphs_to_csv();
        assert!(csv.contains("1,tick,1000,100"));
    }

    #[test]
    fn test_timer() {
        let timer = Timer::start();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 10.0);
    }

    #[test]
    fn test_stats_collector() {
        let mut collector = StatsCollector::new();
        collector.set_name("Test");
        collector.start();

        std::thread::sleep(std::time::Duration::from_millis(5));

        collector.stop();

        let stats = collector.stats();
        assert_eq!(stats.metadata.name, "Test");
        assert!(stats.timing.total_wall_time_ms >= 5.0);
    }

    #[test]
    fn test_stats_collector_from_json() {
        let mut collector = StatsCollector::new();

        let json = serde_json::json!({
            "engine": {
                "current_time": 1000,
                "steps_executed": 100,
                "events_dispatched": 50,
                "events_processed": 45,
                "events_dropped": 5,
                "subgraph_count": 2,
                "channel_count": 1
            },
            "subgraphs": {
                "1": {
                    "current_time": 1000,
                    "ticks_executed": 100,
                    "node_invocations": 200,
                    "tick_period": 10,
                    "incoming_events": 10,
                    "outgoing_events": 20
                },
                "2": {
                    "current_time": 1000,
                    "events_processed": 30,
                    "events_generated": 25,
                    "peak_queue_size": 15,
                    "incoming_events": 20,
                    "outgoing_events": 10
                }
            }
        });

        collector.update_from_json(&json);

        let stats = collector.stats();
        assert_eq!(stats.engine.final_time, 1000);
        assert_eq!(stats.engine.steps_executed, 100);
        assert_eq!(stats.subgraphs.len(), 2);

        let sg1 = stats.subgraphs.get(&1).unwrap();
        assert_eq!(sg1.mode, "tick");
        assert_eq!(sg1.ticks_executed, Some(100));

        let sg2 = stats.subgraphs.get(&2).unwrap();
        assert_eq!(sg2.mode, "event");
        assert_eq!(sg2.events_processed, Some(30));
    }

    #[test]
    fn test_summary_output() {
        let mut stats = SimulationStats::new().with_name("Summary Test");
        stats.engine.final_time = 1000;
        stats.engine.steps_executed = 100;

        let summary = stats.summary();
        assert!(summary.contains("Summary Test"));
        assert!(summary.contains("1000"));
    }
}


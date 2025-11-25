//! Event definitions for the simulation framework.
//!
//! Events are the primary mechanism for communication between nodes,
//! both within and across subgraphs.

use serde::{Deserialize, Serialize};

use crate::types::{NodeId, SimTime};

/// Represents an event in the simulation.
///
/// Events carry a timestamp and a payload, and are the fundamental
/// unit of communication in event-driven simulation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    /// The simulation time at which this event occurs
    pub time: SimTime,
    /// The event payload containing the actual data
    pub payload: EventPayload,
}

impl Event {
    /// Creates a new event with the given time and payload.
    pub fn new(time: SimTime, payload: EventPayload) -> Self {
        Self { time, payload }
    }

    /// Creates a new message event.
    pub fn message(
        time: SimTime,
        src_node: NodeId,
        src_port: impl Into<String>,
        dst_node: NodeId,
        dst_port: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        Self {
            time,
            payload: EventPayload::Message {
                src: (src_node, src_port.into()),
                dst: (dst_node, dst_port.into()),
                data,
            },
        }
    }

    /// Creates a custom event.
    pub fn custom(time: SimTime, data: impl Into<String>) -> Self {
        Self {
            time,
            payload: EventPayload::Custom(data.into()),
        }
    }
}

/// The payload of an event.
///
/// Events can carry different types of payloads depending on the use case.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EventPayload {
    /// A message between nodes.
    ///
    /// Contains source and destination node/port pairs, plus arbitrary data.
    Message {
        /// Source: (node_id, port_name)
        src: (NodeId, String),
        /// Destination: (node_id, port_name)
        dst: (NodeId, String),
        /// The message data (JSON value for flexibility)
        data: serde_json::Value,
    },

    /// A custom event with arbitrary string data.
    Custom(String),
}

impl EventPayload {
    /// Returns the destination node ID if this is a message event.
    pub fn dst_node(&self) -> Option<NodeId> {
        match self {
            EventPayload::Message { dst, .. } => Some(dst.0),
            EventPayload::Custom(_) => None,
        }
    }

    /// Returns the source node ID if this is a message event.
    pub fn src_node(&self) -> Option<NodeId> {
        match self {
            EventPayload::Message { src, .. } => Some(src.0),
            EventPayload::Custom(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = Event::new(
            100,
            EventPayload::Message {
                src: (1, "out".to_string()),
                dst: (2, "in".to_string()),
                data: serde_json::json!({"value": 42}),
            },
        );

        assert_eq!(event.time, 100);
        assert_eq!(event.payload.src_node(), Some(1));
        assert_eq!(event.payload.dst_node(), Some(2));
    }

    #[test]
    fn test_event_message_helper() {
        let event = Event::message(
            200,
            1,
            "data_out",
            2,
            "data_in",
            serde_json::json!({"packet": "hello"}),
        );

        assert_eq!(event.time, 200);
        if let EventPayload::Message { src, dst, data } = &event.payload {
            assert_eq!(src, &(1, "data_out".to_string()));
            assert_eq!(dst, &(2, "data_in".to_string()));
            assert_eq!(data["packet"], "hello");
        } else {
            panic!("Expected Message payload");
        }
    }

    #[test]
    fn test_custom_event() {
        let event = Event::custom(300, "timer_expired");

        assert_eq!(event.time, 300);
        if let EventPayload::Custom(data) = &event.payload {
            assert_eq!(data, "timer_expired");
        } else {
            panic!("Expected Custom payload");
        }
    }

    #[test]
    fn test_event_serialization() {
        let event = Event::message(100, 1, "out", 2, "in", serde_json::json!(42));
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();

        assert_eq!(event.time, deserialized.time);
    }
}


//! AI chat relay module.
//!
//! Handles streaming communication with the internal AI service via NDJSON.

pub mod stream;

pub use stream::{
    AiClient, AiEvent, AiStream, HistoryItem, NdjsonParser, StartError, AI_UNAVAILABLE_MESSAGE,
    DEFAULT_IDLE_TIMEOUT, DEFAULT_TOTAL_TIMEOUT,
};

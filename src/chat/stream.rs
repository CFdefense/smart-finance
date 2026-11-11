//! AI chat streaming client and NDJSON line parser.
//!
//! Relays requests to the internal AI service at `POST /chat/stream` and parses
//! incoming NDJSON events with total and per-chunk idle timeout protection.

use std::{collections::VecDeque, time::Duration};

use serde::{Deserialize, Serialize};

/// Default total deadline for the entire AI chat stream (120 seconds).
pub const DEFAULT_TOTAL_TIMEOUT: Duration = Duration::from_secs(120);

/// Default idle timeout between consecutive stream chunks (15 seconds).
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(15);

/// User-facing error message emitted when the AI service fails mid-stream or times out.
pub const AI_UNAVAILABLE_MESSAGE: &str =
    "Sorry, the assistant is having trouble right now. Please try again shortly.";

/// A single turn in the chat history sent to the AI service.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryItem {
    /// Role of the message author (`user` or `assistant`).
    pub role: String,
    /// Text content of the message.
    pub content: String,
}

/// An event emitted by the AI service NDJSON stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AiEvent {
    /// A tool invocation status notification.
    Status {
        /// Name of the tool being executed (e.g. `web_search`).
        tool: String,
    },
    /// An incremental token of assistant reply text.
    Token {
        /// Partial text chunk.
        text: String,
    },
    /// Successful completion of the turn with full assistant response text.
    Done {
        /// The complete assistant response text.
        response: String,
    },
    /// A turn failure event emitted by the AI service.
    Error {
        /// Machine-readable error code (e.g. `ai_failed`).
        code: String,
        /// Human-readable error description.
        message: String,
    },
}

/// Errors that can occur when initiating the AI stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    /// The AI service is not running, returned a non-2xx status (including 503), or refused connection.
    Unavailable,
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "AI service is unavailable"),
        }
    }
}

impl std::error::Error for StartError {}

/// Line-buffered parser for NDJSON streams of [`AiEvent`]s.
#[derive(Debug, Default)]
pub struct NdjsonParser {
    buffer: Vec<u8>,
}

impl NdjsonParser {
    /// Creates a new, empty line parser.
    #[must_use]
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Pushes new bytes into the parser and returns any complete [`AiEvent`]s parsed from new lines.
    ///
    /// Blank lines and unparseable lines are ignored without logging to protect user privacy.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<AiEvent> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.buffer.drain(..=pos).collect();
            if let Some(event) = Self::parse_line(&line_bytes) {
                events.push(event);
            }
        }

        events
    }

    /// Flushes any remaining bytes in the buffer as a final line without requiring a trailing newline.
    pub fn finish(&mut self) -> Vec<AiEvent> {
        let mut events = Vec::new();
        if !self.buffer.is_empty() {
            let remaining = std::mem::take(&mut self.buffer);
            if let Some(event) = Self::parse_line(&remaining) {
                events.push(event);
            }
        }
        events
    }

    fn parse_line(bytes: &[u8]) -> Option<AiEvent> {
        let s = std::str::from_utf8(bytes).ok()?.trim();
        if s.is_empty() {
            return None;
        }
        serde_json::from_str::<AiEvent>(s).ok()
    }
}

/// HTTP client for relaying chat prompts to the internal AI service.
#[derive(Clone, Debug)]
pub struct AiClient {
    http: reqwest::Client,
    base_url: String,
    total_timeout: Duration,
    idle_timeout: Duration,
}

impl Default for AiClient {
    fn default() -> Self {
        Self::from_env()
    }
}

impl AiClient {
    /// Builds an `AiClient` configured from the `AI_SERVICE_URL` environment variable.
    ///
    /// Defaults to `http://127.0.0.1:8000` when unset.
    ///
    /// # Panics
    ///
    /// Panics if the underlying `reqwest::Client` cannot be built.
    #[must_use]
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("AI_SERVICE_URL").unwrap_or_else(|_| "http://127.0.0.1:8000".to_string());
        Self::with_base_url(base_url)
    }

    /// Builds an `AiClient` pointing to a specific base URL (used to point tests at a local mock).
    ///
    /// # Panics
    ///
    /// Panics if the underlying `reqwest::Client` cannot be built.
    #[must_use]
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .build()
                .expect("failed to build reqwest client"),
            base_url: base_url.into(),
            total_timeout: DEFAULT_TOTAL_TIMEOUT,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        }
    }

    /// Overrides the total and idle timeouts for the client.
    #[must_use]
    pub fn with_timeouts(mut self, total: Duration, idle: Duration) -> Self {
        self.total_timeout = total;
        self.idle_timeout = idle;
        self
    }

    /// Initiates a streaming chat turn against the AI service.
    ///
    /// Sends `POST {base_url}/chat/stream` with `{"user_id", "message", "history"}`.
    /// Non-2xx responses (including 503) or connection failures return [`StartError::Unavailable`].
    ///
    /// # Errors
    ///
    /// Returns [`StartError::Unavailable`] if the AI service fails to accept the connection or returns non-2xx.
    pub async fn start(
        &self,
        user_id: i32,
        message: &str,
        history: Vec<HistoryItem>,
    ) -> Result<AiStream, StartError> {
        let body = serde_json::json!({
            "user_id": user_id,
            "message": message,
            "history": history,
        });

        let resp = self
            .http
            .post(format!("{}/chat/stream", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|_| StartError::Unavailable)?;

        if !resp.status().is_success() {
            return Err(StartError::Unavailable);
        }

        let total_deadline = tokio::time::Instant::now() + self.total_timeout;

        Ok(AiStream {
            response: resp,
            parser: NdjsonParser::new(),
            buffered_events: VecDeque::new(),
            total_deadline,
            idle_timeout: self.idle_timeout,
            eof_reached: false,
            terminated: false,
            emitted_terminal: false,
        })
    }
}

/// An active NDJSON event stream from the AI service.
#[derive(Debug)]
pub struct AiStream {
    response: reqwest::Response,
    parser: NdjsonParser,
    buffered_events: VecDeque<AiEvent>,
    total_deadline: tokio::time::Instant,
    idle_timeout: Duration,
    eof_reached: bool,
    terminated: bool,
    emitted_terminal: bool,
}

impl AiStream {
    /// Reads and returns the next [`AiEvent`] from the stream.
    ///
    /// Returns `None` once the stream has completed (after yielding a `Done` or `Error` event),
    /// or if the stream has terminated.
    ///
    /// If the stream encounters an idle timeout, total deadline expiration, transport error,
    /// or ends without a `Done` or `Error` event, it yields an `AiEvent::Error` with code
    /// `"ai_failed"` once, then subsequent calls return `None`.
    pub async fn next_event(&mut self) -> Option<AiEvent> {
        if self.terminated {
            return None;
        }

        loop {
            if let Some(event) = self.buffered_events.pop_front() {
                if matches!(event, AiEvent::Done { .. } | AiEvent::Error { .. }) {
                    self.terminated = true;
                    self.emitted_terminal = true;
                    self.buffered_events.clear();
                }
                return Some(event);
            }

            if self.eof_reached {
                self.terminated = true;
                if !self.emitted_terminal {
                    self.emitted_terminal = true;
                    return Some(AiEvent::Error {
                        code: "ai_failed".to_string(),
                        message: AI_UNAVAILABLE_MESSAGE.to_string(),
                    });
                }
                return None;
            }

            let now = tokio::time::Instant::now();
            if now >= self.total_deadline {
                self.terminated = true;
                if !self.emitted_terminal {
                    self.emitted_terminal = true;
                    return Some(AiEvent::Error {
                        code: "ai_failed".to_string(),
                        message: AI_UNAVAILABLE_MESSAGE.to_string(),
                    });
                }
                return None;
            }

            let total_remaining = self.total_deadline - now;
            let chunk_timeout = self.idle_timeout.min(total_remaining);

            match tokio::time::timeout(chunk_timeout, self.response.chunk()).await {
                Ok(Ok(Some(chunk))) => {
                    let events = self.parser.push(&chunk);
                    self.buffered_events.extend(events);
                }
                Ok(Ok(None)) => {
                    self.eof_reached = true;
                    let remaining_events = self.parser.finish();
                    self.buffered_events.extend(remaining_events);
                }
                Ok(Err(_err)) => {
                    self.terminated = true;
                    if !self.emitted_terminal {
                        self.emitted_terminal = true;
                        return Some(AiEvent::Error {
                            code: "ai_failed".to_string(),
                            message: AI_UNAVAILABLE_MESSAGE.to_string(),
                        });
                    }
                    return None;
                }
                Err(_elapsed) => {
                    self.terminated = true;
                    if !self.emitted_terminal {
                        self.emitted_terminal = true;
                        return Some(AiEvent::Error {
                            code: "ai_failed".to_string(),
                            message: AI_UNAVAILABLE_MESSAGE.to_string(),
                        });
                    }
                    return None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::StatusCode, response::IntoResponse, routing::post, Router};

    #[test]
    fn test_parser_single_and_multiple_lines() {
        let mut parser = NdjsonParser::new();
        let chunk =
            b"{\"type\":\"token\",\"text\":\"Hello\"}\n{\"type\":\"token\",\"text\":\" world\"}\n";
        let events = parser.push(chunk);
        assert_eq!(
            events,
            vec![
                AiEvent::Token {
                    text: "Hello".to_string()
                },
                AiEvent::Token {
                    text: " world".to_string()
                }
            ]
        );
        assert!(parser.finish().is_empty());
    }

    #[test]
    fn test_parser_split_multibyte_char_across_chunks() {
        let line = "{\"type\":\"token\",\"text\":\"🔍\"}\n".as_bytes();
        let emoji_start = line.iter().position(|b| *b == 0xF0).unwrap();
        let mut parser = NdjsonParser::new();
        assert!(parser.push(&line[..emoji_start + 2]).is_empty());
        assert_eq!(
            parser.push(&line[emoji_start + 2..]),
            vec![AiEvent::Token {
                text: "🔍".to_string()
            }]
        );
    }

    #[test]
    fn test_parser_split_lines_across_chunks() {
        let mut parser = NdjsonParser::new();
        let chunk1 = b"{\"type\":\"token\",\"te";
        let chunk2 = b"xt\":\"split token\"}\n{\"type\":\"done\",\"res";
        let chunk3 = b"ponse\":\"complete\"}\n";

        let ev1 = parser.push(chunk1);
        assert!(ev1.is_empty());

        let ev2 = parser.push(chunk2);
        assert_eq!(
            ev2,
            vec![AiEvent::Token {
                text: "split token".to_string()
            }]
        );

        let ev3 = parser.push(chunk3);
        assert_eq!(
            ev3,
            vec![AiEvent::Done {
                response: "complete".to_string()
            }]
        );

        assert!(parser.finish().is_empty());
    }

    #[test]
    fn test_parser_ignores_blank_lines() {
        let mut parser = NdjsonParser::new();
        let chunk = b"\n\r\n   \n{\"type\":\"status\",\"tool\":\"web_search\"}\n\n";
        let events = parser.push(chunk);
        assert_eq!(
            events,
            vec![AiEvent::Status {
                tool: "web_search".to_string()
            }]
        );
        assert!(parser.finish().is_empty());
    }

    #[test]
    fn test_parser_ignores_unparseable_lines() {
        let mut parser = NdjsonParser::new();
        let chunk = b"invalid json line\n{\"unknown\":\"structure\"}\n{\"type\":\"token\",\"text\":\"valid\"}\nnot-json\n";
        let events = parser.push(chunk);
        assert_eq!(
            events,
            vec![AiEvent::Token {
                text: "valid".to_string()
            }]
        );
        assert!(parser.finish().is_empty());
    }

    #[test]
    fn test_parser_finish_without_trailing_newline() {
        let mut parser = NdjsonParser::new();
        let chunk = b"{\"type\":\"done\",\"response\":\"final response\"}";
        let events = parser.push(chunk);
        assert!(events.is_empty());

        let final_events = parser.finish();
        assert_eq!(
            final_events,
            vec![AiEvent::Done {
                response: "final response".to_string()
            }]
        );
    }

    #[test]
    fn test_parser_all_event_types() {
        let mut parser = NdjsonParser::new();
        let chunk = b"{\"type\":\"status\",\"tool\":\"web_search\"}\n{\"type\":\"token\",\"text\":\"abc\"}\n{\"type\":\"error\",\"code\":\"test_err\",\"message\":\"something failed\"}\n{\"type\":\"done\",\"response\":\"fin\"}\n";
        let events = parser.push(chunk);
        assert_eq!(
            events,
            vec![
                AiEvent::Status {
                    tool: "web_search".to_string()
                },
                AiEvent::Token {
                    text: "abc".to_string()
                },
                AiEvent::Error {
                    code: "test_err".to_string(),
                    message: "something failed".to_string()
                },
                AiEvent::Done {
                    response: "fin".to_string()
                }
            ]
        );
    }

    async fn spawn_mock_server(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn test_relay_normal_flow() {
        let router = Router::new().route(
            "/chat/stream",
            post(|| async {
                let stream = futures_util::stream::iter(vec![
                    Ok::<_, std::io::Error>(axum::body::Bytes::from(
                        "{\"type\":\"status\",\"tool\":\"web_search\"}\n",
                    )),
                    Ok::<_, std::io::Error>(axum::body::Bytes::from(
                        "{\"type\":\"token\",\"text\":\"Hello \"}\n",
                    )),
                    Ok::<_, std::io::Error>(axum::body::Bytes::from(
                        "{\"type\":\"token\",\"text\":\"there\"}\n",
                    )),
                    Ok::<_, std::io::Error>(axum::body::Bytes::from(
                        "{\"type\":\"done\",\"response\":\"Hello there\"}\n",
                    )),
                ]);
                Body::from_stream(stream).into_response()
            }),
        );

        let url = spawn_mock_server(router).await;
        let client = AiClient::with_base_url(url);

        let mut stream = client
            .start(1, "Hi", vec![])
            .await
            .expect("stream should start");

        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Status {
                tool: "web_search".to_string()
            })
        );
        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Token {
                text: "Hello ".to_string()
            })
        );
        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Token {
                text: "there".to_string()
            })
        );
        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Done {
                response: "Hello there".to_string()
            })
        );
        assert_eq!(stream.next_event().await, None);
    }

    #[tokio::test]
    async fn test_relay_503_unavailable() {
        let router = Router::new().route(
            "/chat/stream",
            post(|| async {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "AI assistant is not available.",
                )
                    .into_response()
            }),
        );

        let url = spawn_mock_server(router).await;
        let client = AiClient::with_base_url(url);

        let res = client.start(1, "Hi", vec![]).await;
        assert_eq!(res.unwrap_err(), StartError::Unavailable);
    }

    #[tokio::test]
    async fn test_relay_connection_refused_unavailable() {
        // Unallocated / closed port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let client = AiClient::with_base_url(format!("http://{addr}"));
        let res = client.start(1, "Hi", vec![]).await;
        assert_eq!(res.unwrap_err(), StartError::Unavailable);
    }

    #[tokio::test]
    async fn test_relay_mid_stream_error_line() {
        let router = Router::new().route(
            "/chat/stream",
            post(|| async {
                let stream = futures_util::stream::iter(vec![
                    Ok::<_, std::io::Error>(axum::body::Bytes::from(
                        "{\"type\":\"token\",\"text\":\"Starting...\"}\n",
                    )),
                    Ok::<_, std::io::Error>(axum::body::Bytes::from(
                        "{\"type\":\"error\",\"code\":\"provider_error\",\"message\":\"Rate limit exceeded\"}\n",
                    )),
                    Ok::<_, std::io::Error>(axum::body::Bytes::from(
                        "{\"type\":\"token\",\"text\":\"ignored\"}\n",
                    )),
                ]);
                Body::from_stream(stream).into_response()
            }),
        );

        let url = spawn_mock_server(router).await;
        let client = AiClient::with_base_url(url);

        let mut stream = client
            .start(1, "Hi", vec![])
            .await
            .expect("stream should start");

        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Token {
                text: "Starting...".to_string()
            })
        );
        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Error {
                code: "provider_error".to_string(),
                message: "Rate limit exceeded".to_string()
            })
        );
        // After an Error event, stream must terminate and return None
        assert_eq!(stream.next_event().await, None);
    }

    #[tokio::test]
    async fn test_relay_stream_ends_without_done() {
        let router = Router::new().route(
            "/chat/stream",
            post(|| async {
                let stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>(
                    axum::body::Bytes::from("{\"type\":\"token\",\"text\":\"Incomplete\"}\n"),
                )]);
                Body::from_stream(stream).into_response()
            }),
        );

        let url = spawn_mock_server(router).await;
        let client = AiClient::with_base_url(url);

        let mut stream = client
            .start(1, "Hi", vec![])
            .await
            .expect("stream should start");

        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Token {
                text: "Incomplete".to_string()
            })
        );
        // Stream ended without Done/Error -> must yield ai_failed error once
        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Error {
                code: "ai_failed".to_string(),
                message: AI_UNAVAILABLE_MESSAGE.to_string(),
            })
        );
        // Next event is None
        assert_eq!(stream.next_event().await, None);
    }

    #[tokio::test]
    async fn test_relay_idle_timeout() {
        let router = Router::new().route(
            "/chat/stream",
            post(|| async {
                let stream = futures_util::stream::unfold(0, |state| async move {
                    if state == 0 {
                        Some((
                            Ok::<_, std::io::Error>(axum::body::Bytes::from(
                                "{\"type\":\"token\",\"text\":\"First\"}\n",
                            )),
                            1,
                        ))
                    } else if state == 1 {
                        // Delay longer than client's idle timeout
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        Some((
                            Ok::<_, std::io::Error>(axum::body::Bytes::from(
                                "{\"type\":\"done\",\"response\":\"First\"}\n",
                            )),
                            2,
                        ))
                    } else {
                        None
                    }
                });
                Body::from_stream(stream).into_response()
            }),
        );

        let url = spawn_mock_server(router).await;
        let client = AiClient::with_base_url(url)
            .with_timeouts(Duration::from_secs(5), Duration::from_millis(50));

        let mut stream = client
            .start(1, "Hi", vec![])
            .await
            .expect("stream should start");

        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Token {
                text: "First".to_string()
            })
        );
        // Idle timeout triggers ai_failed error
        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Error {
                code: "ai_failed".to_string(),
                message: AI_UNAVAILABLE_MESSAGE.to_string(),
            })
        );
        assert_eq!(stream.next_event().await, None);
    }

    #[tokio::test]
    async fn test_relay_total_timeout() {
        let router = Router::new().route(
            "/chat/stream",
            post(|| async {
                let stream = futures_util::stream::unfold(0, |state| async move {
                    if state == 0 {
                        Some((
                            Ok::<_, std::io::Error>(axum::body::Bytes::from(
                                "{\"type\":\"token\",\"text\":\"One\"}\n",
                            )),
                            1,
                        ))
                    } else if state == 1 {
                        tokio::time::sleep(Duration::from_millis(40)).await;
                        Some((
                            Ok::<_, std::io::Error>(axum::body::Bytes::from(
                                "{\"type\":\"token\",\"text\":\"Two\"}\n",
                            )),
                            2,
                        ))
                    } else if state == 2 {
                        tokio::time::sleep(Duration::from_millis(40)).await;
                        Some((
                            Ok::<_, std::io::Error>(axum::body::Bytes::from(
                                "{\"type\":\"done\",\"response\":\"OneTwo\"}\n",
                            )),
                            3,
                        ))
                    } else {
                        None
                    }
                });
                Body::from_stream(stream).into_response()
            }),
        );

        let url = spawn_mock_server(router).await;
        // Total timeout 60ms, idle timeout 50ms.
        // Chunk 1 at 0ms. Chunk 2 at 40ms. Next chunk at 80ms > 60ms total deadline.
        let client = AiClient::with_base_url(url)
            .with_timeouts(Duration::from_millis(60), Duration::from_millis(50));

        let mut stream = client
            .start(1, "Hi", vec![])
            .await
            .expect("stream should start");

        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Token {
                text: "One".to_string()
            })
        );
        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Token {
                text: "Two".to_string()
            })
        );
        // Total deadline expiration triggers ai_failed error
        assert_eq!(
            stream.next_event().await,
            Some(AiEvent::Error {
                code: "ai_failed".to_string(),
                message: AI_UNAVAILABLE_MESSAGE.to_string(),
            })
        );
        assert_eq!(stream.next_event().await, None);
    }
}

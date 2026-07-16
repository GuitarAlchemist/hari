//! Error type for the NL-extraction surface.

use thiserror::Error;

/// All failure modes of [`crate::MercuryExtractor::extract`].
///
/// The crate prefers errors over silent fallbacks: a wrong HexValue or
/// hallucinated proposition would silently corrupt Hari's BeliefNetwork,
/// so every parse/validation step bails out loudly.
#[derive(Debug, Error)]
pub enum ExtractError {
    /// No `INCEPTION_API_KEY` in the environment (or empty), and the
    /// caller didn't supply one explicitly via `MercuryConfig::api_key`.
    /// Surfaces at construction time so a missing key never appears as a
    /// runtime 500 deep in a CognitiveLoop cycle.
    #[error("INCEPTION_API_KEY is not set (or is empty); export it or set MercuryConfig::api_key explicitly")]
    MissingApiKey,

    /// Caller passed an empty or whitespace-only note.
    #[error("empty input note — nothing to extract")]
    EmptyInput,

    /// `reqwest::Client::builder()` rejected the timeout/feature combo.
    /// Realistically only fires on completely broken host TLS configs.
    #[error("failed to build HTTP client: {0}")]
    HttpClientBuild(#[source] reqwest::Error),

    /// Network-level failure (DNS, TCP, TLS, body decode).
    #[error("HTTP request to Inception failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Inception returned a non-2xx status.
    #[error("Inception API returned HTTP {status}: {body}")]
    ApiStatus {
        status: reqwest::StatusCode,
        body: String,
    },

    /// Response parsed at the transport level but `choices` was empty.
    /// Indicates a vendor-side schema regression — shouldn't happen with
    /// `response_format = json_object` set, but failing loudly is correct.
    #[error("Inception response had no choices")]
    EmptyResponse,

    /// Model emitted something that doesn't fit the structured schema.
    /// `got` is the raw assistant content so operators can debug prompt
    /// drift without re-running the extraction.
    #[error("Inception JSON does not match expected shape: {source}\n--- raw content ---\n{got}")]
    JsonShape {
        got: String,
        #[source]
        source: serde_json::Error,
    },

    /// Required field absent. We name the field rather than echoing the
    /// raw payload to keep log noise low.
    #[error("missing required field in extraction: {0}")]
    MissingField(&'static str),

    /// HexValue token outside the six-valued alphabet. Distinct from
    /// `JsonShape` so callers can decide whether to retry with a
    /// reminder-stuffed prompt vs treat as a hard error.
    #[error("invalid HexValue token: {0} (expected one of True/Probable/Unknown/Doubtful/False/Contradictory)")]
    InvalidHexValue(String),

    /// Relation token outside the closed set declared by `hari-lattice`.
    #[error("invalid Relation token: {0} (expected Supports/Contradicts/Implies)")]
    InvalidRelation(String),
}

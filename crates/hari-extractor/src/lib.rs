//! # hari-extractor — NL → ResearchEvent boundary
//!
//! Converts free-form natural-language research notes into the typed
//! [`hari_core::ResearchEvent`] objects that Hari's CognitiveLoop replays.
//! Sits one layer above `hari-core`'s public input surface; **never imported
//! by `hari-lattice`, `hari-cognition`, `hari-swarm`, or `hari-core`** — this
//! preserves Hari's deterministic-replay property. The extractor is opt-in
//! sidecar machinery for the IX/operator boundary, not part of Hari's
//! substrate.
//!
//! ## Why this exists
//!
//! Today an IX agent (or a human) writes JSON directly into a fixture file
//! like `fixtures/ix/conflicting_benchmark.json`. That's fine for replay
//! testing but cumbersome at the live boundary. This crate gives IX a way
//! to emit notes in plain English ("benchmark-x is probably reliable, 5
//! runs all passed") and get back a typed `ResearchEvent` that Hari can
//! consume verbatim.
//!
//! ## Two-flag opt-in (mirrors the ga subagent pattern)
//!
//! 1. `INCEPTION_API_KEY` env var must be set (or `Inception:ApiKey` in a
//!    config file the caller supplies).
//! 2. The caller must explicitly enable extraction by constructing the
//!    [`MercuryExtractor`] — there is no global activation, no DI side-effect.
//!    The substrate stays LLM-free even if the env var is set.
//!
//! ## Architecture
//!
//! ```text
//! NL note ──► MercuryExtractor::extract ──► ResearchEvent ──► hari-core
//!                  │                              ▲
//!                  └─ Mercury (OpenAI-compat ──┐  │
//!                     HTTPS, json_object)     │  │ TypedExtractor (future)
//!                                              ▼  │ — closed-form rules
//!                                       error ─►─┘
//! ```
//!
//! A future `TypedExtractor` can short-circuit the common patterns ("X is
//! True with N runs of evidence Y" etc.) the same way ga's
//! `TypedMusicalQueryExtractor` handles 96% of voicing queries before any
//! LLM call. v1 of this crate ships only the Mercury path.

use std::time::Duration;

use hari_core::{ResearchEvent, ResearchEventPayload};
use hari_lattice::HexValue;
use serde::{Deserialize, Serialize};

pub mod error;
pub use error::ExtractError;

/// Default Inception Labs Mercury endpoint. Override via [`MercuryConfig::base_url`].
pub const DEFAULT_BASE_URL: &str = "https://api.inceptionlabs.ai/v1";

/// Default Mercury model id. Override via [`MercuryConfig::model`].
pub const DEFAULT_MODEL: &str = "mercury-2";

/// Environment variable holding the Inception bearer token. The crate
/// deliberately does NOT read this on its own — callers pass the key
/// explicitly via [`MercuryConfig::api_key`]. This constant exists so the
/// caller and the operator agree on a name without re-deriving it.
pub const API_KEY_ENV_VAR: &str = "INCEPTION_API_KEY";

/// Configuration for the Mercury-backed extractor.
///
/// `api_key` is required; missing/empty keys cause [`MercuryExtractor::new`]
/// to return an error at construction time rather than first call — same
/// lesson as ga's `AnthropicProvider` (PR #151): deferred construction
/// hides a missing key behind opaque 500s on the first request.
#[derive(Debug, Clone)]
pub struct MercuryConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    /// Per-request timeout. Mercury's 2026-05-13 benchmark p95 was 729 ms;
    /// 30s is comfortably above the 99.9-th percentile of any reasonable
    /// vendor-side hiccup.
    pub timeout: Duration,
}

impl MercuryConfig {
    /// Build a config from the `INCEPTION_API_KEY` env var. Returns
    /// [`ExtractError::MissingApiKey`] if the env var is unset or empty.
    pub fn from_env() -> Result<Self, ExtractError> {
        let api_key = std::env::var(API_KEY_ENV_VAR).map_err(|_| ExtractError::MissingApiKey)?;
        if api_key.trim().is_empty() {
            return Err(ExtractError::MissingApiKey);
        }
        Ok(Self {
            api_key,
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout: Duration::from_secs(30),
        })
    }

    /// True when an `INCEPTION_API_KEY` env var is present and non-empty.
    /// Callers should gate construction of [`MercuryExtractor`] on this so
    /// a fresh clone without credentials degrades to a no-extraction path
    /// rather than erroring at startup.
    pub fn is_configured() -> bool {
        std::env::var(API_KEY_ENV_VAR)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
    }
}

/// NL → `ResearchEvent` extractor backed by Mercury 2.
///
/// The instance owns a `reqwest::Client` for connection reuse. Cheap to
/// hold across many extractions; build once at app startup.
#[derive(Debug, Clone)]
pub struct MercuryExtractor {
    config: MercuryConfig,
    http: reqwest::Client,
}

impl MercuryExtractor {
    /// Construct an extractor. Validates the config (non-empty key) eagerly.
    pub fn new(config: MercuryConfig) -> Result<Self, ExtractError> {
        if config.api_key.trim().is_empty() {
            return Err(ExtractError::MissingApiKey);
        }
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(ExtractError::HttpClientBuild)?;
        Ok(Self { config, http })
    }

    /// Extract a `ResearchEvent` from a free-text note.
    ///
    /// The `cycle` and `source` fields aren't inferable from text alone, so
    /// the caller supplies them. Mercury fills in the typed `payload`.
    ///
    /// On any error (network, malformed JSON, schema mismatch) the
    /// extractor returns `Err` rather than guessing — Hari's belief network
    /// would silently misroute on a wrong proposition, so failing loudly
    /// here is the right tradeoff.
    pub async fn extract(
        &self,
        cycle: u64,
        source: &str,
        note: &str,
    ) -> Result<ResearchEvent, ExtractError> {
        if note.trim().is_empty() {
            return Err(ExtractError::EmptyInput);
        }

        let raw = self.complete(note).await?;
        let parsed: RawExtraction =
            serde_json::from_str(&raw).map_err(|e| ExtractError::JsonShape {
                got: raw.clone(),
                source: e,
            })?;

        let payload = parsed.into_payload()?;
        Ok(ResearchEvent {
            cycle,
            source: source.to_string(),
            payload,
        })
    }

    /// Lower-level chat-completion call. Public for benchmarks/tests that
    /// want to drive Mercury directly without the parse step.
    pub async fn complete(&self, note: &str) -> Result<String, ExtractError> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "model": self.config.model,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user",   "content": note }
            ],
            "temperature": 0,
            "response_format": { "type": "json_object" }
        });

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(ExtractError::Http)?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ExtractError::ApiStatus { status, body });
        }

        let parsed: ChatCompletionResponse = resp.json().await.map_err(ExtractError::Http)?;
        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or(ExtractError::EmptyResponse)
    }
}

// ---------------------------------------------------------------------------
// Prompt + wire types
// ---------------------------------------------------------------------------

/// System prompt for the Hari NL → ResearchEvent extractor.
///
/// Constrains output to the JSON schema `RawExtraction` parses. Mirrors
/// the closed-vocabulary discipline of ga's `LlmMusicalQueryExtractor`:
/// HexValue, payload type, and "evidence" fields are all closed sets.
const SYSTEM_PROMPT: &str = r#"You extract structured research events from free-text notes for the Hari belief substrate.
Respond with JSON only — no prose, no code fences.

Schema:
{
  "type": "belief_update" | "experiment_result" | "agent_vote" | "retraction" | "goal_update" | "relation_declaration",
  "proposition": "<short kebab-case identifier of the claim, e.g. benchmark-x-is-reliable>",
  "value": "True" | "Probable" | "Unknown" | "Doubtful" | "False" | "Contradictory",
  "evidence_note": "<short factual summary of the supporting observation, max 120 chars>",
  "evidence_runs": <integer count of independent runs / observations, 0 if not stated>,
  "reason": "<retraction reason, only when type is retraction>",
  "goal_key": "<kebab-case goal identifier, only when type is goal_update>",
  "goal_description": "<short description, only when type is goal_update>",
  "goal_priority": <0.0–1.0 priority, only when type is goal_update>,
  "relation_from": "<source proposition, only when type is relation_declaration>",
  "relation_to": "<target proposition, only when type is relation_declaration>",
  "relation": "Supports" | "Contradicts" | "Implies"
}

Rules:
- "value" maps the user's confidence to one of the six HexValue tokens. Hedged
  positives ("probably", "looks like", "five clean runs") map to Probable, not True.
  Conflicting or uncertain accounts map to Contradictory or Unknown respectively.
- "proposition" is the canonical claim. Normalize tense and remove agent identity
  ("agent-A says X" → proposition is X, not "agent-A says X").
- "evidence_runs" is 0 when the user doesn't supply a count; do not invent.
- Omit fields irrelevant to the chosen type (don't include "reason" unless type
  is retraction).
- Return only the JSON object, no surrounding prose.
"#;

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    content: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum RawType {
    BeliefUpdate,
    ExperimentResult,
    AgentVote,
    Retraction,
    GoalUpdate,
    RelationDeclaration,
}

#[derive(Debug, Deserialize)]
struct RawExtraction {
    #[serde(rename = "type")]
    kind: RawType,
    #[serde(default)]
    proposition: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    evidence_note: Option<String>,
    #[serde(default)]
    evidence_runs: Option<u32>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    goal_key: Option<String>,
    #[serde(default)]
    goal_description: Option<String>,
    #[serde(default)]
    goal_priority: Option<f64>,
    #[serde(default)]
    relation_from: Option<String>,
    #[serde(default)]
    relation_to: Option<String>,
    #[serde(default)]
    relation: Option<String>,
}

impl RawExtraction {
    fn into_payload(self) -> Result<ResearchEventPayload, ExtractError> {
        // Build the Evidence JSON from the optional fields the prompt
        // exposes — kept narrow so the LLM has a small target. Downstream
        // analyzers can attach additional structure via a separate
        // post-processing step.
        let evidence = build_evidence(self.evidence_note.as_deref(), self.evidence_runs);

        match self.kind {
            RawType::BeliefUpdate => Ok(ResearchEventPayload::BeliefUpdate {
                proposition: self
                    .proposition
                    .ok_or(ExtractError::MissingField("proposition"))?,
                value: parse_hex_value_required(self.value.as_deref())?,
                evidence,
            }),
            RawType::ExperimentResult => Ok(ResearchEventPayload::ExperimentResult {
                proposition: self
                    .proposition
                    .ok_or(ExtractError::MissingField("proposition"))?,
                value: parse_hex_value_required(self.value.as_deref())?,
                evidence,
            }),
            RawType::AgentVote => Ok(ResearchEventPayload::AgentVote {
                proposition: self
                    .proposition
                    .ok_or(ExtractError::MissingField("proposition"))?,
                value: parse_hex_value_required(self.value.as_deref())?,
                evidence,
            }),
            RawType::Retraction => Ok(ResearchEventPayload::Retraction {
                proposition: self
                    .proposition
                    .ok_or(ExtractError::MissingField("proposition"))?,
                reason: self.reason.ok_or(ExtractError::MissingField("reason"))?,
                // The extractor maps flat records; the `retracts` selector
                // is a structured belief-revision field (issue #16) not yet
                // surfaced by this extraction path.
                retracts: None,
            }),
            RawType::GoalUpdate => Ok(ResearchEventPayload::GoalUpdate {
                key: self
                    .goal_key
                    .ok_or(ExtractError::MissingField("goal_key"))?,
                description: self
                    .goal_description
                    .ok_or(ExtractError::MissingField("goal_description"))?,
                priority: self.goal_priority.unwrap_or(0.5),
                status: self.value.as_deref().map(parse_hex_value).transpose()?,
            }),
            RawType::RelationDeclaration => {
                let from = self
                    .relation_from
                    .ok_or(ExtractError::MissingField("relation_from"))?;
                let to = self
                    .relation_to
                    .ok_or(ExtractError::MissingField("relation_to"))?;
                let relation = parse_relation(self.relation.as_deref())?;
                Ok(ResearchEventPayload::RelationDeclaration { from, to, relation })
            }
        }
    }
}

/// Six HexValue tokens, case-insensitive, leading/trailing whitespace
/// tolerated. Anything else is rejected — silent fallback to Unknown would
/// hide schema drift between this crate and `hari-lattice`.
fn parse_hex_value(raw: &str) -> Result<HexValue, ExtractError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" => Ok(HexValue::True),
        "probable" => Ok(HexValue::Probable),
        "unknown" => Ok(HexValue::Unknown),
        "doubtful" => Ok(HexValue::Doubtful),
        "false" => Ok(HexValue::False),
        "contradictory" => Ok(HexValue::Contradictory),
        _ => Err(ExtractError::InvalidHexValue(raw.to_string())),
    }
}

/// `parse_hex_value` lifted to the `Option<&str>` shape the BeliefUpdate /
/// ExperimentResult / AgentVote variants need (where the field is required).
fn parse_hex_value_required(s: Option<&str>) -> Result<HexValue, ExtractError> {
    parse_hex_value(s.ok_or(ExtractError::MissingField("value"))?)
}

fn parse_relation(s: Option<&str>) -> Result<hari_lattice::Relation, ExtractError> {
    let raw = s.ok_or(ExtractError::MissingField("relation"))?;
    match raw.trim() {
        "Supports" => Ok(hari_lattice::Relation::Supports),
        "Contradicts" => Ok(hari_lattice::Relation::Contradicts),
        "Implies" => Ok(hari_lattice::Relation::Implies),
        _ => Err(ExtractError::InvalidRelation(raw.to_string())),
    }
}

fn build_evidence(note: Option<&str>, runs: Option<u32>) -> hari_core::Evidence {
    // `Evidence` is `BTreeMap<String, serde_json::Value>`. Build it directly
    // so we don't pay an unnecessary Value::Object → serde_json::from_value round trip.
    let mut evidence: hari_core::Evidence = std::collections::BTreeMap::new();
    if let Some(n) = note.filter(|s| !s.trim().is_empty()) {
        evidence.insert("note".into(), serde_json::Value::String(n.to_string()));
    }
    if let Some(r) = runs.filter(|r| *r > 0) {
        evidence.insert(
            "runs".into(),
            serde_json::Value::Number(serde_json::Number::from(r)),
        );
    }
    evidence
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_from_env_requires_non_empty_key() {
        // Capture and restore the env var so other tests aren't perturbed.
        let prev = std::env::var(API_KEY_ENV_VAR).ok();

        // SAFETY: setting/removing env vars is process-global; tests in this
        // module use one config-related env var and serialize via #[test] —
        // a parallel test runner that touches the same var would race, but
        // no other test here does. Wrapped in unsafe per Rust 2024 edition.
        unsafe {
            std::env::remove_var(API_KEY_ENV_VAR);
        }
        assert!(matches!(
            MercuryConfig::from_env(),
            Err(ExtractError::MissingApiKey)
        ));
        assert!(!MercuryConfig::is_configured());

        unsafe {
            std::env::set_var(API_KEY_ENV_VAR, "");
        }
        assert!(matches!(
            MercuryConfig::from_env(),
            Err(ExtractError::MissingApiKey)
        ));
        assert!(!MercuryConfig::is_configured());

        unsafe {
            std::env::set_var(API_KEY_ENV_VAR, "sk_fake-for-test");
        }
        let cfg = MercuryConfig::from_env().expect("non-empty key should succeed");
        assert_eq!(cfg.model, DEFAULT_MODEL);
        assert!(MercuryConfig::is_configured());

        match prev {
            Some(v) => unsafe { std::env::set_var(API_KEY_ENV_VAR, v) },
            None => unsafe { std::env::remove_var(API_KEY_ENV_VAR) },
        }
    }

    #[test]
    fn new_rejects_empty_key() {
        let cfg = MercuryConfig {
            api_key: "   ".to_string(),
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout: Duration::from_secs(5),
        };
        assert!(matches!(
            MercuryExtractor::new(cfg),
            Err(ExtractError::MissingApiKey)
        ));
    }

    #[test]
    fn hex_value_parses_all_six_tokens_case_insensitive() {
        for (raw, expected) in [
            ("True", HexValue::True),
            ("probable", HexValue::Probable),
            ("UNKNOWN", HexValue::Unknown),
            (" Doubtful ", HexValue::Doubtful),
            ("false", HexValue::False),
            ("Contradictory", HexValue::Contradictory),
        ] {
            assert_eq!(parse_hex_value(raw).unwrap(), expected, "raw={raw}");
        }
        assert!(matches!(
            parse_hex_value("maybe"),
            Err(ExtractError::InvalidHexValue(_))
        ));
        assert!(matches!(
            parse_hex_value_required(None),
            Err(ExtractError::MissingField(_))
        ));
    }

    #[test]
    fn raw_extraction_belief_update_round_trip() {
        // Simulates what Mercury would emit; exercises the schema we
        // promise the model produces.
        let json = r#"{
            "type": "belief_update",
            "proposition": "benchmark-x-is-reliable",
            "value": "Probable",
            "evidence_note": "stable pass-rate across initial runs",
            "evidence_runs": 5
        }"#;
        let raw: RawExtraction = serde_json::from_str(json).expect("schema parses");
        let payload = raw.into_payload().expect("payload builds");
        match payload {
            ResearchEventPayload::BeliefUpdate {
                proposition, value, ..
            } => {
                assert_eq!(proposition, "benchmark-x-is-reliable");
                assert_eq!(value, HexValue::Probable);
            }
            other => panic!("expected BeliefUpdate, got {other:?}"),
        }
    }

    #[test]
    fn raw_extraction_retraction_requires_reason() {
        let json = r#"{
            "type": "retraction",
            "proposition": "benchmark-x-is-reliable"
        }"#;
        let raw: RawExtraction = serde_json::from_str(json).unwrap();
        assert!(matches!(
            raw.into_payload(),
            Err(ExtractError::MissingField("reason"))
        ));
    }

    #[tokio::test]
    async fn extract_empty_input_is_error() {
        let cfg = MercuryConfig {
            api_key: "sk_fake".to_string(),
            base_url: "http://127.0.0.1:1".to_string(), // unused — empty-input check fires first
            model: DEFAULT_MODEL.to_string(),
            timeout: Duration::from_millis(50),
        };
        let extractor = MercuryExtractor::new(cfg).unwrap();
        assert!(matches!(
            extractor.extract(0, "test", "   ").await,
            Err(ExtractError::EmptyInput)
        ));
    }
}

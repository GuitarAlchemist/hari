//! # Operator model — Giskard Track G1 (ops face) tracer bullet
//!
//! Mentalics-of-the-human: nobody in the fleet models the *operator*. This
//! module is the minimal substrate for that — a **seen / acknowledged /
//! stale** registry over the signals surfaced to a human operator
//! (presence-snapshot alerts, algedonic-inbox items) plus exactly one
//! consuming decision: *don't re-notify what's already acknowledged*.
//!
//! Scope pinned by the BACKLOG G1 tracer — this is the hari **substrate half
//! only**. The two GA-side halves are a later slice and a cross-repo
//! contract (tribunal required):
//! - the GA *producer* that emits `Surfaced` events when it puts a signal in
//!   front of the operator, and
//! - the GA *consumer* that reads [`should_notify`] before re-notifying.
//!
//! Until those land, this model *exists and is honest*: it records what was
//! surfaced and acknowledged and answers the re-notify question, but no
//! automated GA decision consumes it yet. Same scoping discipline as
//! [`crate::reliability`].
//!
//! Design constraints (inherited from `forecast.rs` / `reliability.rs`):
//! - Append-only JSONL ledger, one file per emission day, under
//!   `HARI_STATE_DIR/operator-model/`.
//! - Timestamps are *injected* (never read from the clock inside the logic)
//!   so folds and decisions are deterministic under test, and are validated
//!   against [`crate::forecast::is_canonical_utc`] at the write boundary.
//! - Honest degradation over panics: an ack for a never-surfaced signal is
//!   dropped (there is nothing to attach it to); a malformed timestamp makes
//!   [`is_stale`] return `false` rather than guess; an unacknowledged signal
//!   always notifies.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Events — the append-only log. `signal_id` is the join key across both kinds.
// ---------------------------------------------------------------------------

/// One entry in the operator-model log. Serialized with a `"event"` tag so a
/// single JSONL file can interleave both kinds in chronological append order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum OperatorEvent {
    /// A signal was put in front of the operator. Re-surfacing the same
    /// `signal_id` is a *fresh* occurrence — the fold resets any prior
    /// acknowledgement (see [`state`]).
    Surfaced {
        signal_id: String,
        /// Free-form producer category, e.g. `presence-snapshot` or
        /// `algedonic-inbox`. Carried through to [`SignalState`] verbatim.
        kind: String,
        surfaced_at: String,
        /// Time-to-live in seconds. `Some` enables staleness; `None` means
        /// the signal never goes stale on its own.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ttl_secs: Option<u64>,
    },
    /// The operator acknowledged a signal. Matches a `Surfaced` by
    /// `signal_id`; an ack with no prior surfacing has nothing to attach to
    /// and is dropped by the fold.
    Acknowledged {
        signal_id: String,
        acknowledged_at: String,
    },
}

impl OperatorEvent {
    /// The join key — the signal this event is about.
    pub fn signal_id(&self) -> &str {
        match self {
            OperatorEvent::Surfaced { signal_id, .. }
            | OperatorEvent::Acknowledged { signal_id, .. } => signal_id,
        }
    }

    /// The event's own canonical-UTC timestamp. The ledger day file is named
    /// after this value's date, so a surface and its later ack can land in
    /// different day files and still fold in the right order.
    pub fn timestamp(&self) -> &str {
        match self {
            OperatorEvent::Surfaced { surfaced_at, .. } => surfaced_at,
            OperatorEvent::Acknowledged {
                acknowledged_at, ..
            } => acknowledged_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Folded state + the consuming decision.
// ---------------------------------------------------------------------------

/// Current registry state for one signal, folded from the event log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalState {
    pub kind: String,
    /// Timestamp of the *most recent* surfacing (last-surfaced-wins).
    pub surfaced_at: String,
    /// `Some` once the operator has acknowledged the current surfacing;
    /// reset to `None` when the signal is re-surfaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acknowledged_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
}

/// Fold the ordered event log into per-signal current state.
///
/// Rules, applied in log order:
/// - `Surfaced` (re)initialises the signal: last surfacing wins and any prior
///   acknowledgement is cleared (a re-surface is a new thing to notice).
/// - `Acknowledged` stamps the current surfacing if the signal is present; an
///   ack for a never-surfaced signal is dropped (honest — there is no state to
///   attach it to).
pub fn state(events: &[OperatorEvent]) -> BTreeMap<String, SignalState> {
    let mut out: BTreeMap<String, SignalState> = BTreeMap::new();
    for event in events {
        match event {
            OperatorEvent::Surfaced {
                signal_id,
                kind,
                surfaced_at,
                ttl_secs,
            } => {
                out.insert(
                    signal_id.clone(),
                    SignalState {
                        kind: kind.clone(),
                        surfaced_at: surfaced_at.clone(),
                        acknowledged_at: None,
                        ttl_secs: *ttl_secs,
                    },
                );
            }
            OperatorEvent::Acknowledged {
                signal_id,
                acknowledged_at,
            } => {
                if let Some(s) = out.get_mut(signal_id) {
                    s.acknowledged_at = Some(acknowledged_at.clone());
                }
            }
        }
    }
    out
}

/// The one consuming decision: **don't re-notify what's already acknowledged.**
/// Returns `false` iff the signal has been acknowledged, `true` otherwise
/// (including for an unknown/unacknowledged signal — the honest default is to
/// notify).
///
/// `now` is accepted so a caller can pass the same timestamp to
/// `should_notify` and [`is_stale`] from one decision site. The re-notify
/// rule itself is time-independent — an acknowledgement is permanent within a
/// surfacing — so `now` does not change the result today.
pub fn should_notify(state: &SignalState, now: &str) -> bool {
    let _ = now;
    state.acknowledged_at.is_none()
}

/// Whether a signal is *stale*: unacknowledged **and** past its TTL deadline.
///
/// Only meaningful when `ttl_secs` is `Some`; a `None` TTL never goes stale.
/// The deadline is `surfaced_at + ttl_secs`, compared against `now` in
/// seconds-since-epoch. Degrades honestly: an already-acknowledged signal, a
/// missing TTL, or a malformed `surfaced_at`/`now` all return `false` — the
/// model refuses to declare staleness it cannot verify.
pub fn is_stale(state: &SignalState, now: &str) -> bool {
    if state.acknowledged_at.is_some() {
        return false;
    }
    let Some(ttl) = state.ttl_secs else {
        return false;
    };
    let (Some(surfaced), Some(now_epoch)) = (
        parse_utc_to_epoch(&state.surfaced_at),
        parse_utc_to_epoch(now),
    ) else {
        // Malformed timestamp: cannot compute a deadline, so don't guess.
        return false;
    };
    now_epoch > surfaced.saturating_add(ttl as i64)
}

/// Per-signal status: the folded [`SignalState`] plus the two derived
/// decisions, keyed by signal id. This is what the `status` CLI prints.
///
/// `Deserialize` exists for wire consumers of the Phase 6 `operator_status`
/// response (`crate::protocol::Response` derives it) — hari itself never
/// reads status reports back. Same convention as
/// [`crate::reliability::ReliabilityReport`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalStatus {
    #[serde(flatten)]
    pub state: SignalState,
    pub should_notify: bool,
    pub is_stale: bool,
}

/// Fold the log and evaluate both decisions for every signal at `now`.
pub fn status_report(events: &[OperatorEvent], now: &str) -> BTreeMap<String, SignalStatus> {
    state(events)
        .into_iter()
        .map(|(id, s)| {
            let should_notify = should_notify(&s, now);
            let is_stale = is_stale(&s, now);
            (
                id,
                SignalStatus {
                    state: s,
                    should_notify,
                    is_stale,
                },
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Ledger — append-only JSONL, one file per emission day, under
// `HARI_STATE_DIR/operator-model/` (default `state/operator-model/`).
// ---------------------------------------------------------------------------

pub fn ledger_dir() -> PathBuf {
    std::env::var_os("HARI_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("state"))
        .join("operator-model")
}

/// Append one event to the day file named after its own timestamp date.
/// The timestamp must be canonical UTC (`YYYY-MM-DDTHH:MM:SSZ`) — rejected at
/// the write boundary for the same lexicographic-ordering reason as the
/// forecast ledger (see [`crate::forecast::is_canonical_utc`]).
pub fn append(dir: &Path, event: &OperatorEvent) -> io::Result<PathBuf> {
    let ts = event.timestamp();
    if !crate::forecast::is_canonical_utc(ts) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("timestamp is not canonical YYYY-MM-DDTHH:MM:SSZ UTC: {ts:?}"),
        ));
    }
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.jsonl", &ts[..10]));
    let line =
        serde_json::to_string(event).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{line}")?;
    Ok(path)
}

/// Load the whole ledger in append order (day-file name, then line order —
/// which is chronological because timestamps are injected in time order).
/// Malformed lines are skipped with a count so a corrupt line can't take the
/// ledger down. A missing directory is an empty ledger.
pub fn load(dir: &Path) -> io::Result<(Vec<OperatorEvent>, usize)> {
    let mut events = Vec::new();
    let mut skipped = 0usize;
    if !dir.exists() {
        return Ok((events, 0));
    }
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    for path in files {
        for line in fs::read_to_string(&path)?.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<OperatorEvent>(line) {
                Ok(ev) => events.push(ev),
                Err(_) => skipped += 1,
            }
        }
    }
    Ok((events, skipped))
}

// ---------------------------------------------------------------------------
// Clock helper — canonical-UTC → seconds-since-epoch. std only; the inverse of
// `forecast::rfc3339_now`'s civil arithmetic (Howard Hinnant's days_from_civil).
// ---------------------------------------------------------------------------

/// Parse a canonical `YYYY-MM-DDTHH:MM:SSZ` string to seconds since the Unix
/// epoch. Returns `None` for any non-canonical input (delegating the shape
/// check to [`crate::forecast::is_canonical_utc`]) so callers degrade instead
/// of panicking.
pub fn parse_utc_to_epoch(ts: &str) -> Option<i64> {
    if !crate::forecast::is_canonical_utc(ts) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| ts[r].parse::<i64>().ok();
    let (year, month, day) = (n(0..4)?, n(5..7)? as u32, n(8..10)? as u32);
    let (hour, min, sec) = (n(11..13)?, n(14..16)?, n(17..19)?);
    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3600 + min * 60 + sec)
}

/// (year, month, day) → days since 1970-01-01. Howard Hinnant's
/// `days_from_civil`; inverse of `forecast::civil_from_days`.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64; // Mar=0..Feb=11
    let doy = (153 * mp + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forecast::uuidv7;

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("hari-operator-{tag}-{}", uuidv7()))
    }

    fn surfaced(id: &str, at: &str, ttl: Option<u64>) -> OperatorEvent {
        OperatorEvent::Surfaced {
            signal_id: id.to_string(),
            kind: "presence-snapshot".to_string(),
            surfaced_at: at.to_string(),
            ttl_secs: ttl,
        }
    }

    fn acked(id: &str, at: &str) -> OperatorEvent {
        OperatorEvent::Acknowledged {
            signal_id: id.to_string(),
            acknowledged_at: at.to_string(),
        }
    }

    #[test]
    fn append_and_load_roundtrip() {
        let dir = temp_dir("roundtrip");
        append(&dir, &surfaced("sig-a", "2026-07-04T18:00:00Z", Some(3600))).unwrap();
        append(&dir, &acked("sig-a", "2026-07-04T18:05:00Z")).unwrap();

        let (events, skipped) = load(&dir).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].signal_id(), "sig-a");
        assert!(matches!(events[1], OperatorEvent::Acknowledged { .. }));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ack_after_surface_flips_should_notify_to_false() {
        let before = state(&[surfaced("s", "2026-07-04T18:00:00Z", None)]);
        assert!(should_notify(&before["s"], "2026-07-04T18:01:00Z"));

        let after = state(&[
            surfaced("s", "2026-07-04T18:00:00Z", None),
            acked("s", "2026-07-04T18:02:00Z"),
        ]);
        assert!(!should_notify(&after["s"], "2026-07-04T18:03:00Z"));
        assert_eq!(
            after["s"].acknowledged_at.as_deref(),
            Some("2026-07-04T18:02:00Z")
        );
    }

    #[test]
    fn re_surface_updates_surfaced_at_and_clears_ack() {
        let st = state(&[
            surfaced("s", "2026-07-04T18:00:00Z", Some(60)),
            acked("s", "2026-07-04T18:00:30Z"),
            // A fresh occurrence of the same signal: last-surfaced wins and
            // the prior ack no longer applies → notify again.
            surfaced("s", "2026-07-04T19:00:00Z", Some(120)),
        ]);
        assert_eq!(st["s"].surfaced_at, "2026-07-04T19:00:00Z");
        assert_eq!(st["s"].ttl_secs, Some(120));
        assert!(st["s"].acknowledged_at.is_none());
        assert!(should_notify(&st["s"], "2026-07-04T19:00:01Z"));
    }

    #[test]
    fn is_stale_true_only_past_ttl_and_unacknowledged() {
        let st = state(&[surfaced("s", "2026-07-04T18:00:00Z", Some(3600))]);
        let s = &st["s"];
        // Deadline is 19:00:00Z.
        assert!(!is_stale(s, "2026-07-04T18:59:59Z")); // before deadline
        assert!(!is_stale(s, "2026-07-04T19:00:00Z")); // exactly at deadline
        assert!(is_stale(s, "2026-07-04T19:00:01Z")); // past deadline

        // No TTL → never stale.
        let no_ttl = state(&[surfaced("t", "2026-07-04T18:00:00Z", None)]);
        assert!(!is_stale(&no_ttl["t"], "2027-01-01T00:00:00Z"));

        // Acknowledged → never stale, even long past the deadline.
        let ack = state(&[
            surfaced("u", "2026-07-04T18:00:00Z", Some(1)),
            acked("u", "2026-07-04T18:00:00Z"),
        ]);
        assert!(!is_stale(&ack["u"], "2026-07-04T23:00:00Z"));
    }

    #[test]
    fn unknown_or_unacknowledged_signal_notifies() {
        let st = state(&[surfaced("s", "2026-07-04T18:00:00Z", None)]);
        // An unacknowledged signal — the "unknown to the operator" case — notifies.
        assert!(should_notify(&st["s"], "2026-07-04T18:00:00Z"));
    }

    #[test]
    fn orphan_ack_is_dropped_not_fatal() {
        // An ack with no prior surfacing has nothing to attach to.
        let st = state(&[acked("ghost", "2026-07-04T18:00:00Z")]);
        assert!(st.is_empty());
    }

    #[test]
    fn empty_and_missing_dir_are_empty_state() {
        // Missing directory.
        let missing = temp_dir("missing");
        let (events, skipped) = load(&missing).unwrap();
        assert!(events.is_empty());
        assert_eq!(skipped, 0);
        assert!(state(&events).is_empty());

        // Existing but empty directory.
        let empty = temp_dir("empty");
        fs::create_dir_all(&empty).unwrap();
        let (events, skipped) = load(&empty).unwrap();
        assert!(events.is_empty());
        assert_eq!(skipped, 0);
        fs::remove_dir_all(&empty).unwrap();
    }

    /// The load-bearing rule, pinned: `should_notify` is `false` *exactly*
    /// when the signal has been acknowledged, and `true` otherwise.
    #[test]
    fn should_notify_false_iff_acknowledged() {
        let unacked = SignalState {
            kind: "k".into(),
            surfaced_at: "2026-07-04T18:00:00Z".into(),
            acknowledged_at: None,
            ttl_secs: None,
        };
        let acked_state = SignalState {
            acknowledged_at: Some("2026-07-04T18:01:00Z".into()),
            ..unacked.clone()
        };
        let now = "2026-07-04T18:02:00Z";
        assert!(should_notify(&unacked, now));
        assert!(!should_notify(&acked_state, now));
        // Rule is time-independent: same answers at any `now`.
        assert!(should_notify(&unacked, "2030-01-01T00:00:00Z"));
        assert!(!should_notify(&acked_state, "2030-01-01T00:00:00Z"));
    }

    #[test]
    fn append_rejects_non_canonical_timestamp() {
        let dir = temp_dir("noncanon");
        let bad = surfaced("s", "2026-07-04T18:00:00.500Z", None);
        let err = append(&dir, &bad).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(!dir.exists());
    }

    #[test]
    fn parse_utc_to_epoch_anchors_and_rejects_non_canonical() {
        assert_eq!(parse_utc_to_epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_utc_to_epoch("1970-01-02T00:00:00Z"), Some(86_400));
        assert_eq!(parse_utc_to_epoch("1970-01-01T01:00:00Z"), Some(3600));
        // Non-canonical forms are rejected (same gate the CLI's --now uses).
        assert!(parse_utc_to_epoch("2026-07-04T18:00:00.500Z").is_none());
        assert!(parse_utc_to_epoch("2026-07-04 18:00:00Z").is_none());
        assert!(parse_utc_to_epoch("not-a-timestamp").is_none());
    }

    #[test]
    fn is_stale_degrades_on_malformed_timestamps() {
        let malformed = SignalState {
            kind: "k".into(),
            surfaced_at: "garbage".into(),
            acknowledged_at: None,
            ttl_secs: Some(1),
        };
        // Cannot compute a deadline → not stale, and no panic.
        assert!(!is_stale(&malformed, "2026-07-04T18:00:00Z"));
        let ok = SignalState {
            surfaced_at: "2026-07-04T18:00:00Z".into(),
            ..malformed
        };
        assert!(!is_stale(&ok, "garbage"));
    }

    #[test]
    fn status_report_carries_state_and_both_decisions() {
        let events = vec![
            surfaced("fresh", "2026-07-04T18:00:00Z", Some(3600)),
            surfaced("done", "2026-07-04T18:00:00Z", Some(3600)),
            acked("done", "2026-07-04T18:05:00Z"),
            surfaced("stale", "2026-07-04T10:00:00Z", Some(60)),
        ];
        let report = status_report(&events, "2026-07-04T18:30:00Z");

        assert!(report["fresh"].should_notify && !report["fresh"].is_stale);
        assert!(!report["done"].should_notify && !report["done"].is_stale);
        assert!(report["stale"].should_notify && report["stale"].is_stale);

        // Status serializes the flattened state plus the two decisions.
        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&report["stale"]).unwrap()).unwrap();
        assert_eq!(v["kind"], "presence-snapshot");
        assert_eq!(v["should_notify"], true);
        assert_eq!(v["is_stale"], true);
    }
}

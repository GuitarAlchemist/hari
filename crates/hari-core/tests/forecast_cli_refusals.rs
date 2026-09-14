//! Subprocess-level tests for `hari-core forecast emit`'s predicate guard.
//!
//! `resolve` only scores `== <literal>` / `!= <literal>`; anything else
//! resolves `void`. `emit` used to accept any string, so the two 2026-07-20
//! forecasts (`< 4`, `>= 2026-08-01`) were written as unscorable and the
//! mistake only surfaced 30 days later, at horizon, as a red forecast-check
//! tripwire. The guard moves that failure to emission time.
//!
//! Template: `replay_cli_refusals.rs`. The binary comes from
//! `CARGO_BIN_EXE_hari-core`, so no extra build step is needed.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn state_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hari-forecast-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn emit(state: &Path, predicate: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hari-core"))
        .env("HARI_STATE_DIR", state)
        .args([
            "forecast",
            "emit",
            "--belief",
            "cli-guard-test",
            "--probability",
            "0.5",
            "--source",
            "ga:state/fleet/presence.json",
            "--field",
            "/limbs/id=sensor:quality-snapshot/status",
            "--predicate",
            predicate,
            "--horizon",
            "2026-07-03T18:00:00Z",
        ])
        .output()
        .expect("spawn hari-core forecast emit")
}

fn ledger_has_records(state: &Path) -> bool {
    std::fs::read_dir(state.join("forecasts"))
        .map(|entries| entries.flatten().next().is_some())
        .unwrap_or(false)
}

#[test]
fn an_ordered_comparison_predicate_is_refused_before_it_reaches_the_ledger() {
    for predicate in ["< 4", ">= 2026-08-01", "green"] {
        let state = state_dir("refused");
        let out = emit(&state, predicate);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{predicate}: {stderr}");
        assert!(
            stderr.contains("not mechanically scorable"),
            "{predicate}: {stderr}"
        );
        assert!(
            !ledger_has_records(&state),
            "{predicate}: refused forecast was still written to the ledger"
        );
        let _ = std::fs::remove_dir_all(&state);
    }
}

#[test]
fn an_equality_predicate_is_still_emitted() {
    let state = state_dir("accepted");
    let out = emit(&state, "== green");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(ledger_has_records(&state));
    let _ = std::fs::remove_dir_all(&state);
}

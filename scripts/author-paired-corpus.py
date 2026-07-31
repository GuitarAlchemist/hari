"""Author the #35 section 9.3 paired corpus to the committed mix (b35d89e).

Structure is mechanical; the evidence, the propositions and the ground truth
are authored.  Every pair's two halves are byte-identical but for the single
declared trigger, which is what makes the contrast isolable:

  G1 replication      the labeled events are IDENTICAL payloads bar the
                      proposition name.  The only difference is that the act
                      half's proposition already has an independent
                      corroborating experiment_result earlier in the trace and
                      the abstain half's does not.
  G2 commitability    act half carries a corroborated proposition; abstain half
                      is a goal_update, which carries none.
  G3 withdrawn basis  both halves re-assert a claim that a lab reported.  The
                      abstain half's supporting result was retracted (targeted
                      selector) between the report and the re-assertion.

Cycle stamps track trace position so exp(-lambda*age) < theta_wait never fires
and no abstain half is satisfiable by decay arithmetic.
"""
import json
import io
import os

THEMES = [
    dict(
        slug="latency",
        title="p99 latency budget",
        metric="p99_ms",
        good=174,
        budget=200,
        bench="latency-p99",
    ),
    dict(
        slug="throughput",
        title="sustained decode throughput",
        metric="tokens_per_s",
        good=1180,
        budget=1000,
        bench="decode-throughput",
    ),
    dict(
        slug="accuracy",
        title="eval-suite accuracy",
        metric="exact_match",
        good=0.913,
        budget=0.90,
        bench="suite-exact-match",
    ),
    dict(
        slug="memory",
        title="peak resident set",
        metric="peak_rss_mb",
        good=742,
        budget=900,
        bench="rss-peak",
    ),
    dict(
        slug="cache",
        title="prefix cache hit rate",
        metric="hit_rate",
        good=0.871,
        budget=0.80,
        bench="prefix-cache-hit",
    ),
    dict(
        slug="cost",
        title="cost per 1k requests",
        metric="usd_per_1k",
        good=2.41,
        budget=3.00,
        bench="unit-cost",
    ),
]


def ev(cycle, source, payload):
    return {"cycle": cycle, "source": source, "payload": payload}


def experiment(prop, value, source, bench, metric, val, budget, runs, note):
    return {
        "type": "experiment_result",
        "proposition": prop,
        "value": value,
        "evidence": {
            "benchmark": bench,
            "runs": runs,
            metric: val,
            "budget": budget,
            "note": note,
        },
    }


def belief(prop, value, bench, metric, val, budget, runs, note):
    return {
        "type": "belief_update",
        "proposition": prop,
        "value": value,
        "evidence": {
            "benchmark": bench,
            "runs": runs,
            metric: val,
            "budget": budget,
            "note": note,
        },
    }


def build(theme, variant):
    """variant == "isolation": act halves stay Probable, so the ONLY difference
    between a pair's two labeled events is prior corroboration (section 9.3.1).

    variant == "task": act halves assert True, which is what a source reporting
    a reproduced effect does (section 9.3.2).  Bundles asserted confidence with
    reproduction, faithful to section 2's "real regression vs injected
    variance".  Abstain halves are identical in both variants.
    """
    act_value = "Probable" if variant == "isolation" else "True"
    t = theme["slug"]
    bench, metric, good, budget = (
        theme["bench"],
        theme["metric"],
        theme["good"],
        theme["budget"],
    )
    events, labels, claims = [], [], []

    def push(source, payload):
        # Cycle tracks position: stamp == 1-based index, so age stays ~0.
        events.append(ev(len(events) + 1, source, payload))
        return len(events) - 1

    # ---------------- G1: replication -------------------------------------
    # Trigger: whether an independent runner has already reported the same
    # result.  The two labeled events are otherwise identical.
    for i in range(1, 4):
        real = f"g1-{t}-{i}-real"
        flaky = f"g1-{t}-{i}-flaky"
        # The trigger itself: prior independent corroboration, for `real` only.
        push(
            "ix-runner-beta",
            experiment(
                real,
                act_value,
                "ix-runner-beta",
                f"{bench}-replication",
                metric,
                good,
                budget,
                24,
                "independent runner reports the same result",
            ),
        )
        act_i = push(
            "ix-runner-alpha",
            experiment(
                real,
                act_value,
                "ix-runner-alpha",
                bench,
                metric,
                good,
                budget,
                24,
                "clears budget",
            ),
        )
        abstain_i = push(
            "ix-runner-alpha",
            experiment(
                flaky,
                "Probable",
                "ix-runner-alpha",
                bench,
                metric,
                good,
                budget,
                24,
                "clears budget",
            ),
        )
        labels += [
            {"event_index": act_i, "pair": f"g1-{t}-{i}", "expect": "act"},
            {"event_index": abstain_i, "pair": f"g1-{t}-{i}", "expect": "abstain"},
        ]
        # Ground truth by construction: `flaky` is injected variance.
        claims += [
            {"proposition": real, "outcome": "stood"},
            {"proposition": flaky, "outcome": "withdrawn"},
        ]

    # ---------------- G2: commitability -----------------------------------
    # Trigger: whether the event carries a proposition at all.
    for i in range(1, 4):
        prop = f"g2-{t}-{i}"
        push(
            "ix-runner-beta",
            experiment(
                prop,
                act_value,
                "ix-runner-beta",
                f"{bench}-corroboration",
                metric,
                good,
                budget,
                24,
                "independent runner reports the same result",
            ),
        )
        act_i = push(
            "ix-agent-evaluator",
            belief(prop, act_value, bench, metric, good, budget, 24, "clears budget"),
        )
        abstain_i = push(
            "ix-runner-orchestrator",
            {
                "type": "goal_update",
                "key": f"g2-{t}-{i}-characterise",
                "description": f"Characterise {theme['title']} across regions",
                "priority": 0.9,
                "status": "Unknown",
            },
        )
        labels += [
            {"event_index": act_i, "pair": f"g2-{t}-{i}", "expect": "act"},
            {"event_index": abstain_i, "pair": f"g2-{t}-{i}", "expect": "abstain"},
        ]
        claims.append({"proposition": prop, "outcome": "stood"})

    # ---------------- G3: withdrawn basis ---------------------------------
    # Trigger: a targeted retraction of the claim's only support, landing
    # between the lab's report and the re-assertion.
    for i in range(1, 4):
        intact = f"g3-{t}-{i}-intact"
        pulled = f"g3-{t}-{i}-pulled"

        support_cycle = len(events) + 1
        push(
            "ix-lab-gamma",
            experiment(
                intact,
                act_value,
                "ix-lab-gamma",
                f"{bench}-lab",
                metric,
                good,
                budget,
                24,
                "lab run supports the claim",
            ),
        )
        act_i = push(
            "ix-agent-evaluator",
            belief(
                intact,
                act_value,
                bench,
                metric,
                good,
                budget,
                24,
                "re-asserted on the lab result",
            ),
        )

        pulled_cycle = len(events) + 1
        push(
            "ix-lab-gamma",
            experiment(
                pulled,
                act_value,
                "ix-lab-gamma",
                f"{bench}-lab",
                metric,
                good,
                budget,
                24,
                "lab run supports the claim",
            ),
        )
        push(
            "ix-lab-gamma",
            {
                "type": "retraction",
                "proposition": pulled,
                "reason": "lab withdrew the supporting run: harness misconfiguration",
                "retracts": {"source": "ix-lab-gamma", "cycle": pulled_cycle},
            },
        )
        abstain_i = push(
            "ix-agent-evaluator",
            belief(
                pulled,
                act_value,
                bench,
                metric,
                good,
                budget,
                24,
                "re-asserted on the lab result",
            ),
        )
        assert support_cycle  # documents that the intact support is un-retracted
        labels += [
            {"event_index": act_i, "pair": f"g3-{t}-{i}", "expect": "act"},
            {"event_index": abstain_i, "pair": f"g3-{t}-{i}", "expect": "abstain"},
        ]
        claims += [
            {"proposition": intact, "outcome": "stood"},
            {"proposition": pulled, "outcome": "withdrawn"},
        ]

    comment = [
        f"#35 section 9.3 paired fixture ({variant}) - {theme['title']}.",
        "",
        "9 pairs, 3 per ground, equal weight, per the mix committed in b35d89e.",
        VARIANT_NOTE[variant],
        "Pair ids encode the ground (g1-/g2-/g3-) so the per-ground breakdown",
        "the mix requires derives from PairedScore::per_pair.",
        "",
        G1_NOTE[variant],
        "G2 commitability: act half carries a corroborated proposition; abstain",
        "  half is a goal_update, which carries none. Nothing to commit to.",
        "G3 withdrawn basis: both halves re-assert a claim a lab reported. The",
        "  abstain half's only support was retracted (targeted selector) between",
        "  the report and the re-assertion, so the claim now rests on nothing.",
        "",
        "Cycle stamps track trace position, so exp(-lambda*age) < theta_wait",
        "never fires and no abstain half is satisfiable by decay arithmetic.",
        "",
        "Claim outcomes are ground truth by construction, not derived from any",
        "arm: '-flaky' is injected variance and '-pulled' lost its support, so",
        "both are 'withdrawn' and withholding on them is excused under 5.3.",
    ]

    return {
        "_comment": comment,
        "trace": {"dimension": 4, "events": events},
        "labels": labels,
        "claims": claims,
    }


G1_NOTE = {
    "isolation": (
        "G1 replication: the two labeled events are IDENTICAL payloads bar the "
        "proposition name - same asserted value, same source, same evidence "
        "keys. The ONLY difference is that the act half's claim already has an "
        "independent corroborating experiment_result earlier in the trace. "
        "Abstain because one unreplicated run is what the flaky-vs-real task "
        "exists to catch."
    ),
    "task": (
        "G1 replication: the act half asserts True AND has an independent "
        "corroborating experiment_result; the abstain half asserts Probable "
        "with no corroboration. TWO observables of section 2's one injected "
        "trigger - a real effect both reproduces and is reported with "
        "confidence. The payloads are NOT identical here; see the isolation "
        "variant for the single-variable contrast."
    ),
}

VARIANT_NOTE = {
    "isolation": (
        "ISOLATION variant (section 9.3.1): act halves assert Probable, the "
        "same as abstain halves, so the ONLY difference between a pair's two "
        "labeled events is prior corroboration. Authored before any arm was "
        "replayed. Measures whether an arm detects reproduction alone."
    ),
    "task": (
        "TASK variant (section 9.3.2): act halves assert True, which is what a "
        "source reporting a reproduced effect does. Bundles asserted confidence "
        "with reproduction, faithful to section 2's 'real regression vs "
        "injected variance'. Never pooled with the isolation variant."
    ),
}


def main():
    out = "fixtures/ix/paired"
    os.makedirs(out, exist_ok=True)
    for variant in ("isolation", "task"):
      for theme in THEMES:
        doc = build(theme, variant)
        path = os.path.join(out, f"{theme['slug']}-{variant}.json")
        with io.open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(doc, f, indent=2, ensure_ascii=False)
            f.write("\n")
        print(
            f"{path}: {len(doc['trace']['events'])} events, "
            f"{len(doc['labels'])//2} pairs, {len(doc['claims'])} claims"
        )


main()

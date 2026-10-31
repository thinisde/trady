# Synthetic stress-scenario author for offline evaluation (proposed)

Status: **offline-only** scenario author, no real data feed, no paper/live execution simulation activated by this documentation. All narrative generation uses **OpenCodeGo `deepseek-v4.1-flash`** only under [model policy](./model-routing-and-governance.md), with no second generative model or external agent framework.

## What this adds

[Weekly optimization](../weekly-optimization.md) already specifies slippage/gap/latency stress and mathematical proofs. This new agent **authors diverse, explicitly fictional multi-event failure narratives**, converts them to constrained machine-readable test inputs and helps identify uncovered resilience cases. It does **not** create an independent market forecast, calibrate financial outcomes from fiction or allow synthetic samples in real performance reports.

Candidate stories: overnight 8% market gap, halted instrument and delayed price feed; unexpected CPI surprise during broker outage; issuer guidance reversal with a concurrent spoofed X headline; partial fill combined with cancelled native stop acknowledgement; delayed corporate-action adjustment; degraded database and delayed model inference.

## Input and output isolation

Input: typed immutable approved trading policy, current known risk invariants, *non-sensitive synthetic* account sizes, allowed simulation clocks/calendars, broker contract constraints, prior incident taxonomy and known test gaps. No production trading credentials, raw unmasked broker transactions, private datasets, or actual live account/order IDs.

Output is a versioned StressCase, not a research artifact usable for entry:
- fictional=true, case id, originating Flash request hash;
- triggering event sequence and explicit timing/order uncertainties;
- simulated market/broker/source states, availability and missingness;
- expected invariants (e.g., no second entry, no oversell, native protection retained);
- property-based assertions and independent fixture/evaluator responsibility;
- severity, uncertainty and unsupported mechanics requiring manual review.

## Example contract

```json
{
  "schema_version": 1,
  "case_id": "stress-fictitious-42",
  "fictional": true,
  "model_id": "deepseek-v4.1-flash",
  "environment": "OFFLINE_SIMULATION",
  "events": [
    {"at_ms": 0, "kind": "LIMIT_BUY_SUBMITTED"},
    {"at_ms": 20, "kind": "BROKER_RESPONSE_LOST"},
    {"at_ms": 60, "kind": "PARTIAL_FILL_OBSERVED"}
  ],
  "invariants": ["NO_BLIND_RETRY", "PROTECT_CONFIRMED_QUANTITY"],
  "expected_realized_return": null,
  "may_submit_orders": false
}
```

Synthetic example; expected invariant assertions are **candidate tests** until trusted simulator implementation verifies them.

## Offline workflow

1. Rust accepts a bounded owner-selected fault category and frozen policy version, then Flash drafts up to N cases using only supported event types.
2. Validate against a fixed versioned DSL allowlist. The LLM **may not generate executable shell commands, unsafe code or arbitrary test scripts with run authority**.
3. A trusted human/reviewer maps descriptions to deterministic replay fixtures. The simulator rejects impossible broker states and maintains separate virtual account/time/data namespaces.
4. Run through local deterministic Rust harness and offline approved QuantConnect LEAN models if relevant. Compare expected vs observed behavior; artifacts are test-only and cannot cross into original prediction/trading labels.
5. Preserve every crash/failure and "invalid scenario" result, dedupe equivalent cases and track which code path was stressed. An untested scenario is not a passed test.
6. Coding agent may propose a regression testcase based on the result; owner/deployment review remains required before changing production safety policy or live code.

## Model/agent budget

All case drafting/rephrasing/review uses Flash-only. Pro is not required; no automatic code-execution-capable agent or external generative provider. Max cases, event count, repeated runs, token spend and storage bounded by offline task manifest. Timeout -> incomplete case; do not save it as a successful test. The generated narratives should be scoped to stress tests, never forwarded to news/evidence channels as factual market events.

## Validation / acceptance

Metrics: novel valid failure coverage, duplicate-case rate, fraction producing actionable deterministic regression tests, false-positive impossible cases, simulator runtime/spend and confirmed invariants under defined assumptions. Run known failure fixtures as positive controls, compare to curated deterministic stress baseline. Hard cases: overlapping partial fills, quote gaps, lease fencing, duplicated jobs, corporate actions, wrong timezone, order cancel/fill race, stale model output, unauthorized tool-call attempt and prompt-injected fictional article.

Mandatory property: **no simulator path can reach live broker endpoints**, consume real entry slots or overwrite original forecasts. Generated model scenario confidence cannot count as market evidence, and no test result alone can independently promote a live strategy.

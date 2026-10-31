# Thesis falsification agent (proposed)

Status: additional LLM research feature, no existing automatic falsification agent. Generative calls are **OpenCodeGo / `deepseek-v4.1-flash` only**, with the optional unavailable Pro reserved for reviewed exceptional tasks per [model policy](./model-routing-and-governance.md).

## Objective

Convert a company's bullish or bearish investment thesis into **pre-registered conditions that would weaken or invalidate it**, and proactively search for contrary evidence. Distinct from the existing [holding watchdog](../watchdogs-and-x.md): a watchdog notices events, whereas this agent defines explicit testable rejection conditions **before** monitoring starts.

Examples: "margin expansion is driven by persistent customer demand"; falsifiers include guidance cuts, order cancellations, falling verified unit volumes, deteriorating pricing or new financing constraints. Differentiate an observed fact (negative) from an unobserved condition (unknown), and an opinion from a factual contradiction.

## Lifecycle and state

- On a new reviewed thesis, require stable thesis_id/version, source/filing ids, issuer, economic mechanism, time horizon, baseline factual assumptions, evidence strength and expiry.
- Flash converts each assumption into an independently testable trigger: observable metric/event, measurement unit/source, direction, threshold (if justified), exact review period, competing benign explanation and limitations.
- Rust stores immutable ThesisCheckpoint records and reviewed conditions; agents cannot directly amend existing trading plans.
- On new source events, select only checkpoints relevant to that event's issuer/period. Fetch/verify permitted data and ask Flash to label `SUPPORTED`, `CHALLENGED`, `REFUTED`, `UNKNOWN` with evidence ids and caveats.
- Force a source/economic-context review before assigning `REFUTED`. Model disagreement triggers independent source verification, not a simple agent vote.
- On closing a position, compare original registered falsifiers against observed outcomes, including never-triggered and false-positive checks. Weekly optimization may analyze logs, not rewrite historic checkpoints.

## Proposed contract

```json
{
  "schema_version": 1,
  "thesis_id": "thesis-77",
  "thesis_version": 2,
  "assumption": "Customer demand remains robust",
  "falsifier_id": "falsifier-3",
  "expected_observation": "issuer guidance versus previous guidance",
  "review_deadline": "2026-11-05T21:00:00Z",
  "supporting_evidence_ids": ["filing-1"],
  "contradicting_evidence_ids": [],
  "assessment": "UNKNOWN",
  "model_id": "deepseek-v4.1-flash",
  "entry_authorized": false
}
```

Synthetic. Thresholds may be numbers only if justified by sources and reviewed policy, not hallucinated from a prompt.

## Integrations

Analyst service: `thesis.register_falsifiers` (draft only), `thesis.evaluate_falsifier` (read-only evidence check) and existing `research.store_artifact` after schema approval. Event watchers trigger a research queue, not executor orders. Source comparison confirms independence; the existing [prediction engine](../prediction-fusion.md) may use a **verified research revision** but not raw language verdict as probability. Original stops, trailing states and deadlines remain unchanged unless the existing independent holding-policy process approves a permitted tightening or exit.

## Limits and failure handling

No monitor every second; trigger on materially new filing/price/regulatory evidence, changed profile revision, original review deadline or owner request. Enforce per-thesis checkpoint count, source budget, deduplication, cache TTL and prompt tokens. Unavailable evidence -> UNKNOWN; contradictory primary facts and official corrections invalidate old assessments. Pro may be considered for complicated multi-period disputes only after exact availability is independently verified; otherwise Flash plus review/abstention.

## Acceptance tests and efficacy

Test positive vs negative evidence not conflated; company-specific metric unit/period alignment; edited/corrected filing; duplicate syndicated coverage; stale evidence; counterexamples for benign fluctuations; owner-thesis version changes; memory retrieval doesn't selectively exclude failed theses. Evaluate false refutations, time to notice evidence, coverage of material downside and paper trading net utility vs normal watchdog. A good falsifier is a *testable hypothesis*, not a guaranteed profitable sell signal.

## Safeguards

Never automatically sell merely because the model outputs REFUTED; Rust manages orders, native protection and entry gating. Fail closed on unsupported claims; preserve observed negative results so weekly reviews cannot select only successful warnings.

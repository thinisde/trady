# Multi-horizon research and forecast consistency reviewer (proposed)

Status: explanatory LLM quality-control feature, not a new calibrated forecaster. The existing [Perpetual/prediction architecture](../prediction-fusion.md) already binds numerical predictions to specific scenarios and horizons; this feature tests **whether LLM research theses remain temporally coherent** across those horizons. The LLM is only OpenCodeGo `deepseek-v4.1-flash`, with [governance](./model-routing-and-governance.md).

## Objective

For 1-session, 3-session and 5-session research questions, identify conflicts such as near-term bearish earnings risk versus longer-term fundamental optimism. It is legitimate for signs to differ at distinct horizons. The reviewer should articulate causal timing, information maturity and trade feasibility, not collapse three opinions into an unsupported average "confidence".

## Input contracts

Exact issuer, current portfolio/entry policy, snapshot revision, exchange calendar, scenario binding (entry/stop/target), event availability, forecast origin and model/calibrator versions, outcome partition and each horizon's actual number of *exchange sessions*. Capture unresolved quote/FX readiness separately from research. Distinguish original short-term forecasts from subsequent evaluations.

An LLM is not permitted to infer a complete probability distribution from an existing binary target-hit forecast or to create fake 3/5-day predictions. All numerical probabilities must be drawn verbatim from independent approved, horizon-matched calibrated records or remain null.

## Review process

1. Rust selects a single frozen research snapshot and gathers forecasts *only* for their exact registered horizons.
2. Flash writes independently reasoned thesis summaries: dominant catalyst, plausible reaction latency, uncertainty, strongest contrary evidence and what changes by session.
3. Flash compares narratives and classifies differences as `COMPATIBLE_TIMING`, `EXPLICIT_CONFLICT`, `DIFFERENT_QUESTION`, `MISSING_FORECAST` or `UNSUPPORTED`. No forced consensus.
4. Rust verifies horizon/date mapping, scenario costs and source/model freshness. Overlapping forecast labels can be pending and cannot be used as independent mature observations.
5. Output a conflict map for the reviewed research artifact; only an approved predictor and existing formula/utility process decide admissible numerical entries. A longer optimistic horizon cannot loosen the original stop or extend a holding beyond policy.
6. Monitor later outcomes for calibration and explanation accuracy, retaining original forecasts unchanged.

## Example result

```json
{
  "schema_version": 1,
  "issuer_id": "issuer-42",
  "as_of": "2026-10-30T19:00:00Z",
  "model_id": "deepseek-v4.1-flash",
  "horizons": [
    {"exchange_sessions": 1, "research_thesis": "event uncertainty", "scenario_probability": null},
    {"exchange_sessions": 3, "research_thesis": "demand reaction may mature", "scenario_probability": null},
    {"exchange_sessions": 5, "research_thesis": "expiry/overnight risk", "scenario_probability": null}
  ],
  "conflict_status": "DIFFERENT_QUESTION",
  "trade_authority": false
}
```

Synthetic and qualitative only. Horizon 5 sessions is not permission to modify the existing deadline.

## Implementation ports and budgets

Analyst-side `forecasts.read_original`, `forecasts.compare_horizons`, `forecasts.review_contradictions` are future restricted tools. Reuse prediction history and application context manifests; no new model, endpoint or full replay of raw historical transcripts. Cap horizons/candidate cases, LLM turns and token usage. No Pro is ordinarily needed, and there is no runtime Pro fallback. Expose clear "missing" vs "incompatible" states in owner reports.

## Evaluation and acceptance

Human-review coherence/attribution accuracy and compare forecast ranking with existing horizon-specific baselines under walk-forward splits, embargoing overlapping outcome labels. Assess whether this feature catches impossible event order or prevents unsupported confidence improvements. Report cost and any increase in abstention.

Acceptance: conflicting horizons aren't automatically errors; 1 calendar day != 1 exchange session; early close/holiday respected; unsupported 5-day model is null; pending immature outcomes never counted as wins; later observed results not used to rewrite an original narrative; one model cannot relabel a binary probability as full scenario probability. Any violation blocks new entry analysis; native protection remains independent.

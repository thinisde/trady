# Economic causal-impact reasoning agent (proposed)

Status: optional research hypothesis engine, not a validated causal estimator. Only **OpenCodeGo `deepseek-v4.1-flash`** is normally invoked. Complex multi-hop paths could request exact `deepseek-v4.1-pro` solely after provider verification and deterministic owner approval per [model governance](./model-routing-and-governance.md); otherwise abstain or run Flash.

## Distinct function

Existing [company relationships](../company-intelligence.md) capture directional public statements and economic links. This feature builds and critiques **typed economic transmission hypotheses**: how a verified event *could* affect revenue, costs, inventory, capital allocation and finally stock valuation across a documented horizon. It is **not** license to claim a causal price effect from a correlated chart.

Examples: chip export restriction -> eligible product/geography -> supplier volumes -> margins -> market expectation; energy price shock -> energy inputs -> industry profitability; interest-rate surprise -> refinancing cost -> investment/earnings. A model must separate mechanism, sign, lag, magnitude, preconditions and potential confounders.

## Graph and evidence contracts

Nodes: event/issuer/segment/commodity/metric/region/financial result/market price; edges: `SUPPLIES`, `SELLS_TO`, `REVENUE_EXPOSURE`, `COST_EXPOSURE`, `CONSTRAINT`, `ASSUMED_EFFECT`. Each edge has an original source id, effective interval, direction, confidence *in extraction*, uncertainty, rights id and status `OBSERVED`/`INFERRED`/`UNVERIFIED`.

A causal hypothesis requires: affected population, intervention/shock, measurable outcome, time interval, control group/counterfactual, known prior common causes, possible reverse causality and assumptions. Missing assumptions must remain visible. LLM-generated conjectures cannot be marked verified.

## Proposed pipeline

1. Ingest a source-verified event and issuer mapping with precise public availability and known affected business operations.
2. Flash drafts **minimal paths** from event to issuer fundamentals, capped at 3–4 hops to reduce invented links. Each intermediate node must cite an independent permitted source or state `UNKNOWN`.
3. Static Rust verifier checks graph node identity, directed edge schemas, cyclic self-support, temporal ordering, geography/segment fit and duplication. Repeated media coverage of one source counts once.
4. Flash builds alternatives: what if the event affects competitors instead, what if inventories absorb disruption, what if this risk was already priced in? Record plausible confounders and negative controls.
5. Optional trusted time-series evaluator performs explicit event-study or quasi-experimental calculation **only** if data/design assumptions are valid. Name the result as observational association when causal identification fails.
6. Read-only output goes to company memory, [counterfactual scenarios](./counterfactual-scenarios.md) or an approved prospective feature. The existing [prediction engine](../prediction-fusion.md) and Rust policy retain all numerical/financial authority.

## Example artifact

```json
{
  "schema_version": 1,
  "hypothesis_id": "causal-42",
  "event_id": "event-regulation-1",
  "target_issuer_id": "issuer-42",
  "paths": [
    {"edges": ["restriction->regional_sales", "regional_sales->revenue"],
     "mechanism": "revenue exposure", "estimated_magnitude": null,
     "source_ids": ["filing-exposure-1"], "evidence_status": "INFERRED"}
  ],
  "confounders": ["simultaneous earnings release"],
  "identification_quality": "OBSERVATIONAL_ONLY",
  "model_id": "deepseek-v4.1-flash",
  "trade_authority": false
}
```

Synthetic fixture. If confounders/causal identification remain unresolved, no numeric causal claim.

## Port/service ownership

`causal.propose_paths`, `causal.inspect_exposure`, `causal.test_counterevidence` are *future* analyst-side tools with checked schemas; no SQL, filesystem, direct model route or broker mutations. Source graph revisions are immutable; replace superseded edges with versioned corrections. The complex-case escalation gateway may build a separate Pro child run **only** if exact OpenCodeGo ID is confirmed; model provenance and original Flash drafts remain linked.

## Experiments and acceptance

Evaluate graph edge precision and mechanism plausibility against human-reviewed disclosed supply chains, including intentionally false plausible links. Compare to a naive sector exposure baseline and manual analyst research. For observational returns, use time-aligned competing-news controls, event exclusion zones and holdout studies. Report no effect where identification fails. Track LLM token and data retrieval cost.

Tests: false economic link based on name similarity, backwards effect, missing geographic exposure, unsupported magnitude, multi-source duplicated claim, same-day simultaneous shock, cyclic graph, stale counterparty relationship, post-event market data leakage and synthetic scenario mislabeled actual. Lack of evidence -> `INCOMPLETE`/`ABSTAIN`; never propose independent BUY/SELL orders.

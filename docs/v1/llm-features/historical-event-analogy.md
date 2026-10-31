# Historical event analogy engine (proposed)

Status: new optional retrieval + LLM interpretation capability; no point-in-time historical event similarity service is implemented by this text. LLM: **OpenCodeGo `deepseek-v4.1-flash`**, no other generative provider, under [model governance](./model-routing-and-governance.md).

## Purpose and boundaries

Given a new earnings/guidance, regulatory, supply-chain or unusual market event, retrieve previous **comparable events** and explain similarity and difference without cherry-picking only historical winners. The existing research memory stores facts and outcomes; this extension adds a versioned **event canonicalization, analogy search and comparison evaluator**. Retrieval does not prove a causal mechanism and must not automatically become a price forecast.

## Historical dataset

Point-in-time EventCase:
- stable issuer and industry as of event; company capitalization/financial condition as of then;
- event class, primary source id, actual published time, earliest allowed ingestion time;
- market regime, sector, realized volatility, liquidity and pre-event reaction at decision cutoff;
- historical evidence availability mask and licensed retention rights;
- future outcome labels **in a separate outcome table** with maturity/censor status, trading costs and corp-action adjustment.

Build the searchable case descriptor **only from pre-cutoff inputs**. Keep later outcomes inaccessible to the retrieval/ranking agent until the candidate set and similarity method are frozen. Avoid today's S&P 500 membership and current management profiles for former constituents.

## Algorithm and prompt workflow

1. Extract current event type and verified available-time features using Flash and deterministic validation.
2. Query a versioned index with explicit filters: instrument eligibility at event, sector/group, region, event type, days to next disclosure, capitalization bucket and observed regime. Search may be structured/lexical/statistical and does **not** require a different embedding model; if embeddings are used later they must comply with the owner model restriction and be separately approved.
3. Freeze K candidate case ids **without examining future outcomes**. Include nearest dissimilar controls, missing-match warnings and out-of-sample relevance. Cap lookback and number returned.
4. Flash summarizes comparable/noncomparable factors and economic plausibility, citing each original case source id.
5. A trusted evaluator joins matured outcomes after freezing retrieval and reports weighted/unweighted distributions, uncertainty intervals, non-fills and losing cases. Never let the LLM fill missing labels or compute winning percentages from remembered anecdotes.
6. Candidate use in fusion requires separately fitted, holdout-calibrated mapping and independence analysis; until then the analogy stays explanatory research.

## Proposed output

```json
{
  "schema_version": 1,
  "current_event_id": "evt-100",
  "cutoff_at": "2026-10-30T19:00:00Z",
  "retrieval_version": "case-matcher-v1",
  "model_id": "deepseek-v4.1-flash",
  "analogs": [
    {"historical_event_id": "evt-010", "similarity_reasons": ["guidance revision"], "differences": ["liquidity regime"], "source_ids": ["src-10"]}
  ],
  "excluded_case_counts": {"outcome_unmatured": 3},
  "outcome_summary": null,
  "status": "EXPLANATORY_ONLY"
}
```

Synthetic; `outcome_summary` is populated **only** by the trusted evaluation port.

## Tools, storage and cost

New read-only analyst tools `history.find_event_cases`, `history.read_case`, `history.evaluate_frozen_set`; allow the evaluator only approved immutable frozen-case IDs and authorized time windows, with independent role permissions. Use existing PostgreSQL research tables and a separate outcome/label store as specified in [weekly optimization](../weekly-optimization.md). Retrieval index, schema and contents must be versioned and corrected with availability history. Bounded K (e.g. 5–20) and output bytes avoid token overrun. No hidden internet search or alternative generative model.

## Validation

Use walk-forward and leave-issuer-out evaluations, compare retrieved-analogy performance to random cases and simple sector/event matched baselines, and assess recall of actual contextual differences, calibration improvement and net costs. Test changing K, ambiguous event labels, stale filings, same event duplicated across outlets, non-public earnings leaks, survivor bias and post-outcome search contamination. A result made from hindsight annotations must be quarantined as explanatory, never shown as a valid historical forecast.

**Fail closed:** no valid analogs, rights denial or imprecise event time -> INCOMPLETE or ABSTAIN; do not invent a historical case. Trading remains independently controlled by Rust.

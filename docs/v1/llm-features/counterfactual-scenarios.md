# Counterfactual scenario analyst (proposed)

Status: new optional research hypothesis author, no calibrated counterfactual forecaster or trade authority. All LLM drafting uses **OpenCodeGo `deepseek-v4.1-flash`**; exact `deepseek-v4.1-pro` reserved only for verified/owner-approved complex work under [model policy](./model-routing-and-governance.md).

## Goal and distinction

The existing [prediction fusion](../prediction-fusion.md) uses bounded predefined financial scenarios with outcome distributions; this feature helps researchers **formulate alternative world states and test their assumptions**. It should explain what would need to happen for a thesis to hold, collapse or become untradeable. It cannot create expected return or missing probability distributions from persuasive prose.

Illustrative branches: CPI above/below contemporaneous consensus; chip export restrictions partially lifted vs extended; data center capex guidance revised; supply disruption resolved vs prolonged. Distinguish the observed baseline from hypothetical variants and avoid combining mutually incompatible assumptions.

## Input and validity rules

Input: frozen researched event, original source ids and as-of time, relevant issuer relationships, recent market data vintage, current holding thesis, question, branch count and scenario horizon. A scenario node includes trigger, exogenous assumption, conditional company exposure, timing, costs, competing news, contradiction markers and proposed observation that would discriminate branches. Numerical values must refer to known facts, trusted bounded math or explicitly labeled analyst assumptions.

## Research algorithm

1. Establish **BASELINE_AS_OBSERVED** state using timestamped evidence and approved feature snapshots.
2. Flash proposes at most 3–5 mutually distinct branches, including a neutral/no-material-change and a downside branch, with verified and hypothetical claims separated.
3. Test for internal contradictions, impossible event order, missing economic transmission and overlapping scenarios. Use the [causal agent](./causal-impact-reasoning.md) for optional pathway checks.
4. Distinguish outcome partition (what happens first to the **approved** stop/target/deadline) from event narrative branches (what external conditions might happen); they are **not interchangeable partitions**.
5. A trusted numerical evaluator applies existing Rust limits and, only if an independent calibrated model exists, maps validated scenario features to outcome distributions. LLM-proposed likelihood remains `null` and must not be silently filled.
6. Keep scenario provenance immutable; later observed outcomes are linked for offline evaluation, not inserted into the earlier hypothetical input.
7. Return next observations worth tracking and an abstention reason if no branch can be supported.

## Suggested artifact

```json
{
  "schema_version": 1,
  "scenario_set_id": "scenario-42",
  "as_of": "2026-10-30T18:00:00Z",
  "model_id": "deepseek-v4.1-flash",
  "branches": [
    {"branch_id": "baseline", "kind": "OBSERVED_BASELINE",
     "assumptions": ["publicly reported current guidance"],
     "outcome_probability": null},
    {"branch_id": "demand_falls", "kind": "HYPOTHETICAL",
     "assumptions": ["customer spending declines 10%"],
     "outcome_probability": null}
  ],
  "status": "UNPRICED_HYPOTHESES",
  "order_authority": false
}
```

Synthetic; the number 10% is an **assumed shock for test**, not a forecast.

## Runtime and tools

Proposed bounded analyst tools `scenario.propose`, `scenario.validate_assumptions`, `scenario.compare_to_policy`. Rust stores scenario sets with structural version, source snapshot, task and model identities; numerical calculator runs only on independently bound price/fee/FX inputs. Do not copy candidate narrative branches into current prediction schema as complete probabilities. A Pro child run may be requested only with exact verified model ID and isolated model-binding lineage; no automatic substitution.

## Validation and failure handling

Use human-reviewed real policy/earnings historical situations, but separate retrospective explanation from genuine prerelease predictions. Check branch diversity, contradictory hypotheses, economic plausibility, correct as-of evidence, forecast-calibration abstention, sensitivity to extreme assumptions, model spend and whether a later factual observation updates the proper branch without rewriting history. No data -> no branches; inconsistent branches -> return review-needed; high-output token use -> stop. **Never generate an execution command or extend existing holding deadlines**.

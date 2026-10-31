# Autonomous research budget and value-of-information optimizer (proposed)

Status: an **additional planning policy**, not an unrestricted autonomous agent. Trady already has bounded model/tool rounds and token reservations under [OpenCodeGo harness](../../opencode/README.md). This feature chooses **which information request matters next** under those existing ceilings, instead of simply consuming the entire allotted budget.

All generative decisions use **OpenCodeGo / `deepseek-v4.1-flash`** only; Pro cannot be used to optimize its own routing and is unavailable unless separately verified/approved via [model policy](./model-routing-and-governance.md).

## Objective

Minimize unnecessary tool/API/model spending while maintaining evidence completeness. Estimate the chance a missing source will change a decision, its cost, freshness and success probability. An LLM estimate is not a calibrated economic value of information: use a deterministic policy initially with bounded heuristic utility and empirical updates.

Questions: Is retrieving consensus more useful than another copied news article? Is the company filing missing or already available? Should the team stop researching because the required primary evidence cannot legally be obtained? Is research of an alternative stock cheaper than continuing an unsupported candidate?

## Deterministic decision boundary

The Rust ResearchBudgetPolicy owns hard limits: per-provider quota/requests, max model turns (existing default six), tool rounds, total calls, deadline, reserved finalization turn, cost ceiling, current position priority and minimum required source gates. The LLM suggests a *ranking*, never changes budgets or permissions. No new agent work may starve executor protection or consume reserved finalization capacity.

Candidate task fields: task_id, issuer, required source gap, existing evidence ids, incremental expected decision relevance, authorization, provider capability, estimated tokens/API credits/latency, time-to-event, data rights, dependency graph and cached-snapshot availability.

A safe initial score can be:

```text
score(task) =
  bounded_relevance_estimate
  * source_success_probability
  * expiry_urgency_factor
  - cost_weight * estimated_total_cost
  - latency_weight * expected_delay
```

All weights are reviewed heuristics **not financial expected utility**; unavailable source rights or mandatory prerequisite failure yields INELIGIBLE, not negative cost. The actual caller must validate and reserve the cost ceiling before executing a tool; incomplete requests still consume appropriately reserved credits.

## Agent workflow

1. Rust identifies unresolved mandatory checks and optional enrichments from existing research artifacts.
2. Flash ranks 2–5 approved next investigative questions with explicit reason, expected *information gain*, source independence and alternative no-action.
3. Rust rejects tools not registered under the approved skill/tool manifest, absent source rights, spent provider budget, wrong account, stale snapshot or impossible deadline; executes highest permitted batch through current bounded harness.
4. Update source availability and costs from actual results. Run another ranking only while run limits and paid-call reserve permit.
5. Stop if primary research finished, expected incremental benefit is too small, budget exceeded or independent evidence cannot be obtained. Preserve `ABSTAIN` as a valid output.
6. Record which skipped calls might have changed information coverage to support offline comparison, but never reconstruct fabricated tool outputs.

## Example plan

```json
{
  "schema_version": 1,
  "run_id": "run-42",
  "model_id": "deepseek-v4.1-flash",
  "available_tool_ids": ["sec.read_filing", "news.find_primary"],
  "candidates": [
    {"tool": "sec.read_filing", "reason": "required source missing",
     "estimated_credits": 1, "expected_information_gain": "HIGH"}
  ],
  "selected": "sec.read_filing",
  "stop_if_no_eligible_tools": true,
  "may_change_policy": false
}
```

Synthetic planned contract; no named tool should be assumed already registered in production.

## Storage, monitoring and risks

Log proposed/skipped/executed tools, frozen input snapshot, model and reviewed routing policy, incremental actual provider spend, coverage, uncertainty and acceptance result. Prevent the LLM from learning to choose fewer calls simply to report better cost ratios while skipping required source verification. Train/calibrate tool-success likelihood from historical **original attempts**, not only successful ones. Add issuer/event-type strata and confidence intervals; cached data is not necessarily current enough for entry.

## Evaluation and acceptance

Offline replay tool choices against same immutable source availability, simulate credit budgets and measure required-check completion, evidence quality, total spend, average latency, missed critical facts, owner follow-ups and change in net research utility. Use deterministic fixed-query baseline and random eligible-query baseline. Never claim faster research is profitable trading.

Tests: paid call budget exceeded, multiple agents race on last quota slot, cancelled request after reservation, uncertain usage, cached stale filing, duplicate same-origin article, urgent held-position research, fast-model output invalid, tool not allowlisted, owner model override attempted. **Fail closed** into incomplete/no new entry; position protection unaffected.

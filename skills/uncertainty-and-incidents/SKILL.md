---
name: uncertainty-and-incidents
description: >-
  Handle Trady research gaps, conflicting claims, failed or stale tools, budget
  exhaustion and uncertain account/protection state without inventing readiness.
metadata:
  version: "1.0.0"
---

# Uncertainty and incidents

## Identify the actual blocker

Separate factual uncertainty, missing source rights, inaccessible content, stale market/account data, exhausted budgets and unknown broker state. State what is known, its timestamp and exactly what remains unresolved. Do not turn a tool error into an empty result or a rumor into verified news.

Use a permitted independent source or narrower query when it can answer the missing fact within bounds. Check a primary correction, issuer/counterparty release or differently sourced report when claims conflict. Do not evade a denied tool through another URL, arbitrary HTTP, a renamed function or a memory note.

## Apply the scenario

| Scenario | Required analysis response |
|---|---|
| Same rumor on many sites | Group one origin; seek actual primary/independent evidence; block entry until verified |
| Old story reshared today | Record the original event date; assess current relevance without calling it a new catalyst |
| Material facts conflict | Preserve disagreement and seek cited resolution; no majority-vote confidence |
| High score but stale quote/account/FX | No entry; request fresh authorized observations, then reassess |
| Source/tool/run budget exhausted | Report incomplete coverage and no entry permission; retain useful records for a later run |
| Unknown order, quantity or protection | Report recovery state with permitted identifiers; no model-originated order/retry/cancel |
| Deposit while already holding | Explain the new cash balance; do not add shares or reset loss/entry guards |
| Bad news during a holding | Review the active plan/version and propose only allowed changes; never cancel a latched exit |

## Result and escalation

Populate final JSON blockers, uncertainty and failed/unknown readiness checks with actual tool/evidence references. Use `NEEDS_INPUT` for an owner clarification that is necessary; use a no-action/incomplete outcome for missing external facts or expired bounds. Do not ask for a secret in model conversation or request policy relaxation to bypass an ordinary failed check.

The application sends incident notifications; the model has no arbitrary messaging authority. Native protection and Rust exits remain active independently. Follow [operations](../../docs/v1/operations.md#incident-responses), [trading policy](../../docs/v1/strategy.md) and the [JSON protocol](../../docs/v1/llm-and-tools.md).

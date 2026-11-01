---
name: position-review
description: >-
  Reassess Trady's existing holding after scheduled or material news updates and
  propose a justified hold or threshold update without cancelling a triggered exit.
metadata:
  version: "1.1.1"
---

# Position review

## Establish the active state

Read actual held quantity, current plan/version, stop, target, trailing peak/floor, original deadline, pending orders and protection. Use fresh market/FX observations. If an exit has latched, report its state and do not propose continuing the holding or raising its target.

Read the personalized [watchdog](../holding-watchdog.SKILL.md) and prioritize its issuer/people/relationship/sector dependencies. Compare new evidence with the original thesis and accepted/rejected changes. Reuse [news analysis](../news-analysis/SKILL.md) and [source comparison](../source-comparison/SKILL.md) for material claims; consult [Perpetual](../perpetual-review.SKILL.md) on changed plan scenarios. Price movement alone is not proof of an improved product story.

## Decide whether a change is justified

Keep the original formula/fusion revision and deadline with the active plan.
Request numerical changes through the approved calculator and a newly bound
assessment under [prediction fusion](../../docs/v1/prediction-fusion.md); subjective
price hints or a weekly model revision cannot replace active protection, reset
age/entry slots, clear an exit latch or force rotation.

For fast appreciation with new verified support, evaluate a higher target only before the old target is reached and within the computed confidence band. Retain or tighten protection. A higher target is optional; normal 1% taking remains valid.

For a slower rise without material new evidence, retain the current plan and deadline rather than extending until it wins. For deteriorating facts, consider a lower target, tighter stop or shorter deadline through a complete versioned plan update. Do not loosen the stop, add shares, average down or extend the original holding deadline.

An existing holding can need management below the 90-point new-entry floor. Apply separate holding target/protection rules; a lower score is not an unconfigured sell command. An unavailable ML assessment does not authorize changing the plan. In an execution-enabled deployment, supported risk tightening and deterministic exits belong to the executor; verify their state. Record-only shadow review does not activate exits or protection. Route exceptional alternatives through [opportunity comparison](../opportunity-comparison.SKILL.md), preserving entry-slot and rotation gates.

## Result and unavailable services

Return `HOLD`, `PLAN_UPDATE` or `NO_ACTION` in final JSON with the expected position/plan version, actual evidence references, complete proposed thresholds and explicit uncertainties. The execution-enabled Rust executor must evaluate the old plan before accepting an update, with a triggered exit taking precedence. Report only the actual returned decision; a shadow record does not establish that an update or exit executed.

If news/model tools fail, retain the last validated plan and record the coverage gap. If account/quote/protection state is unknown, invoke [uncertainty handling](../uncertainty-and-incidents/SKILL.md). Do not cancel native protection while waiting for more research. Follow [trading policy](../../docs/v1/strategy.md) and the [JSON protocol](../../docs/v1/llm-and-tools.md).

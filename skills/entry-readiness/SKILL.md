---
name: entry-readiness
description: >-
  Finalize a possible Trady entry only after required research, source comparison,
  high computed confidence and current financial checks are complete; otherwise record blockers.
metadata:
  version: "1.1.0"
---

# Entry readiness

## Require a finished dossier

Read the [authoritative readiness gate](../../docs/v1/strategy.md#finalized-research-and-entry-readiness). Inspect the completed scope/search log, claim cards and comparison matrix. Require the pinned mandatory research skills, actual primary verification and at least one independent supporting/reporting lineage. No material contradiction, unknown mandatory field or incomplete required tool result may remain.

Require the thirteen routed skills, effective member/classification snapshot, validated sector/group/finalist records, current company/relationship reviews, prepared watchdog and required usable [Perpetual assessment](../perpetual-review.SKILL.md). Bind its assessed scenario to the final limits; a changed scenario needs a fresh assessment. Probability is separate from the 90-point checklist score. An `OK` fetch does not verify a headline; missing records make the check `UNKNOWN` or `FAIL`.

## Check current execution context

Obtain a fresh account/position/active-plan view, executable quote/FX, actual exchange session and instrument constraints. Confirm the candidate is approved, the account is flat, no unresolved order exists, the session entry slot is unused and entry is not paused. Treat stale or unknown state as a blocker.

Use `calculator.confidence_score` for the evidence-based score and `calculator.entry_budget`/`calculator.plan_economics` for allocation, target, stop and cost/reward checks. Do not substitute your own confidence number or arithmetic. The proposed entry floor is 90/100; the ordinary target stays 1%, with higher caps only under the target/exceptional rules.

Verify that the thesis deadline, required stop/protection and size are supported. An unaffordable minimum, stop inside spread/noise, unacceptable reward/risk or unproven protection means no entry. Do not raise a target, tighten a technical stop arbitrarily or widen the entry range simply to make the calculation pass.

The target [fusion/formula contract](../../docs/v1/prediction-fusion.md) requires
application-resolved approved numerical limits, model/calibrator/fusion revisions
and exact scenario-bound forecasts. Price fields are research hints until Rust
recomputes and binds them. Missing/stale/unsupported required predictions, unknown
uncertainty or absent complete outcomes prevents utility selection. No new tool
becomes available merely because it appears in a design; return blocked/no action
when the needed validated calculator and readiness records do not exist.

## Finalize or block

Return JSON `research.entry_readiness` with each required check, `PASS`/`FAIL`/`UNKNOWN`, evidence/tool/snapshot references, calculated score/economics IDs and blockers. When every mandatory check passes, provide a complete final `ENTRY_PROPOSAL` with the supported entry range, stop, target, evidence and expiry.

If anything mandatory is unresolved, return a no-action/incomplete/clarification outcome and say what is missing. A score above 90 cannot compensate for a failed checklist, and a complete checklist cannot compensate for a score below 90. Do not enter because a budget or deadline is running out.

The application validates these artifacts and constructs readiness records. Rust rechecks current inputs and reserves/submits any eligible entry; the model does not execute it. There is no extra owner approval for each otherwise eligible automated entry, and this skill cannot waive the separate live-deployment authorization.

Use the [JSON/tool protocol](../../docs/v1/llm-and-tools.md). A finalized proposal is still a proposal until the executor accepts it.

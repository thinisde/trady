# LLM investment committee (proposed)

Status: proposed, no running multi-agent committee. Reuses the existing [OpenCodeGo conversation harness](../../opencode/README.md). **Every agent runs `deepseek-v4.1-flash` through OpenCodeGo**, under the [strict model policy](./model-routing-and-governance.md); an unavailable Pro never blocks the normal committee.

## Problem and unique capability

Current Trady research compares evidence and produces one structured analytical proposal. Add a deliberately **independent adversarial review process**: Bull, Bear, Neutral and EvidenceVerifier are role instructions with isolated contexts; none may see the other roles' conclusions before submitting the initial assessment. A deterministic adjudicator identifies unresolved claims, not an LLM majority vote or automatic trade approval.

## Inputs and trust

ResearchTask is bound to one account, instrument, allowed universe, market session and a frozen evidence snapshot:
- reviewed filings, claims and rights-checked source ids, quoted excerpts and publication/availability time;
- existing holdings and sanitized account state, approved policy version and source completeness;
- objective/scenario horizon, paper/shadow mode and deadlines;
- requested question and numeric research budget; no broker trading credential or arbitrary web tool.
Agents can call only the same approved read-only research and calculation tools already allowed for analyst roles. Do not send raw personal tax information or unauthorized X text.

## Execution phases

1. Rust planner validates instrument, snapshot, policy, permissions and max 3 parallel Flash research calls; reserve conservative token/time budgets first.
2. Bull researches upside catalysts, evidence strength, price/reaction caveats and failed bullish cases.
3. Bear researches downside exposure, alternative explanations, disconfirming evidence and market expectations.
4. Neutral builds a chronology, distinguishes facts from projections, and tests whether neither position merits a trade.
5. EvidenceVerifier receives the **sealed** three outputs and original evidence, checks referenced ids, duplicates, unsupported claims, contradictions and staleness. This is also Flash, never a special model.
6. Rust computes intersection/differences of claim ids, sets required follow-up checks and emits either `REVIEW_COMPLETE`, `UNRESOLVED` or `ABSTAIN`. Optional one targeted follow-up round only within original run limits.
7. Existing [prediction fusion](../prediction-fusion.md) and [trading policy](../strategy.md) independently determine numeric constraints and readiness. A committee cannot grant confidence points just because agents agree.

## Typed artifact (proposal, not current schema)

```json
{
  "schema_version": 1,
  "task_id": "committee-task-42",
  "evidence_snapshot_id": "snapshot-42",
  "model_id": "deepseek-v4.1-flash",
  "roles": [
    {"role": "BULL", "claims": ["claim-A"], "unknowns": ["future-demand"]},
    {"role": "BEAR", "claims": ["claim-B"], "unknowns": ["guidance-risk"]},
    {"role": "NEUTRAL", "claims": ["claim-C"], "unknowns": []}
  ],
  "verification": {"unsupported_claim_ids": [], "conflicts": ["claim-A vs claim-B"]},
  "status": "UNRESOLVED",
  "entry_authorized": false
}
```

Synthetic fixture. Role-specific confidence is evidence interpretation quality, **not** a calibrated probability that the stock rises.

## Scheduling, cost and failure

Invoke only after sector screening reduces candidates and relevant original evidence is present; the current holding may receive priority over new opportunities. Default two-pass cap: initial independent role turns plus one verifier round; fail on cost/timeout/source gaps, never prolong a live exit. Reserve one finalization turn under existing run limits; share immutable source snapshots but never reasoning continuation items across role sessions. Duplicate requests dedupe by task/snapshot/role/model/hash. Pro may be separately proposed for unusual irreconcilable evidence, but only under verified user policy; do not assume it works.

## Evaluation / acceptance

Blind-label whether agents surfaced genuine contradictory primary evidence and whether that evidence changed future research readiness; compare ordinary single-Flash research against committee-Flash on same historical snapshots. Measure added unsupported claims, citation precision, abstention, false blocked entries, incremental calibrated decision utility **after costs**, latency and token spend. No future data in arguments.

Test: roles cannot see each other before sealing; 3 identical hallucinations aren't 3 source lineages; invalid verdict cannot authorize order; duplicate source ids remain one origin; missing role/timeout = incomplete; evaluator cannot revise original source record or model ID; restart and resume maintain sealed outputs. No production deployment without prospective paper/shadow evidence and unchanged native protection.

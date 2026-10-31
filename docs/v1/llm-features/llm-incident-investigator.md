# Read-only LLM incident investigator (proposed)

Status: new operational diagnostic assistant, not an incident responder authorized to place orders or alter services. The existing [operations playbooks](../operations.md) and [autonomous execution](../../autonomous.md) govern protective action; the LLM can only explain events based on sanitized immutable evidence. Normal model: **OpenCodeGo `deepseek-v4.1-flash`**; exact `deepseek-v4.1-pro` optional only when verified/approved for a genuinely complex investigation per [model governance](./model-routing-and-governance.md).

## Scope and examples

Analyze: unexplained broker/local cash mismatch, unknown order acknowledgement, partial fill, failed protection check, duplicated research, missing evidence source, stale quote, unexpected loss, expired model calibration or failed weekly promotion. The investigator is **post-event/support-only**. If executor detects unprotected shares, deterministic incident controls and owner alerts act *before* and *without* waiting for any LLM completion.

## Immutable evidence packet

Use a trusted Rust IncidentCollector that builds a bounded, redacted event timeline from:
- deployment and policy revisions, machine/build identity and run IDs;
- fenced database intents, state transitions, broker snapshots, fill references, reconciliation/recovery statuses and timestamps;
- quote/source freshness, provider coverage, retry/timeout records and context manifest hashes;
- original LLM model/skill/tool IDs, prompt/response metadata and permission checks **only** if privacy/rights allow;
- measured ledger P&L and original trade reason, separated from analyst opinions.

Do not send trading credentials, owner tax data, raw account identifiers, full broker payloads or sensitive free-form session messages. Avoid exposing raw API keys through exception traces. For real broker mysteries, absence of a reliable broker snapshot is a hard limitation, not permission to invent events.

## Analysis pipeline

1. Rust detects/receives an incident; classifies severity, attaches correlation IDs, preserves snapshots and initiates native safety actions independent of the LLM.
2. Flash reconstructs a chronological account: `OBSERVED`, `INFERRED`, `UNKNOWN` event markers. Do not sort by wall-clock timestamp alone when clocks drift; retain sequence/fencing generations.
3. Propose 1–3 plausible failure hypotheses, each with direct evidence and falsifying checks. Do not assign blame or claim verified root cause prematurely.
4. An independent deterministic verifier checks that cited events exist, all suggested mitigations are **advisory**, and no step implies retrying an unknown broker submission without reconciliation.
5. Render an owner-facing postmortem draft: impact, timeline, confirmed/possible causes, evidence gaps, counterfactual prevention, recommended engineering tests, current unresolved exposure and escalation contacts.
6. Weekly review may register bounded regression-test hypotheses, but the coding agent or LLM cannot hot-edit policy, restart the executor or activate a new model.

## Proposed artifact

```json
{
  "schema_version": 1,
  "incident_id": "incident-42",
  "severity": "HIGH",
  "model_id": "deepseek-v4.1-flash",
  "evidence_snapshot_id": "redacted-audit-12",
  "timeline": [
    {"event_id": "intent-1", "kind": "OBSERVED", "state": "SUBMISSION_UNKNOWN"}
  ],
  "hypotheses": [
    {"cause": "request response lost", "status": "UNVERIFIED",
     "supporting_event_ids": ["intent-1"], "check": "broker order/fill reconciliation"}
  ],
  "proposed_actions": ["owner review"],
  "may_execute_actions": false
}
```

Synthetic. No open-ended shell/log navigation or repair code execution as model tools.

## Service and budget boundaries

Use `incident.read_timeline`, `incident.inspect_snapshot`, `incident.compare_revisions` read-only tools with tightly scoped incident IDs and retention. High severity gets bounded priority after executor duties, not infinite analysis. Model attempts and optional exact-Pro escalation count against owner-approved costs and separate persisted child-run model bindings. If unavailable, publish deterministic incident summary without a fake LLM root cause. Make provider rights and legal retention explicit for broker/order data.

## Evaluation / hard acceptance

Replay fault-injection records for timeout-after-submit, duplicate fill, split adjustment, stale lease, LLM prompt injection, overdue source API request, FX discrepancy and double-counted broker withholding. Measure citation correctness, hypothesis quality, time-to-understand, false causal attribution and operator actionability. Compare deterministic timeline to LLM augmentation; ambiguous true state remains ambiguous.

Test: unknown broker request never triggers retry; partial fill always prioritizes protection; clock order issue handled; hidden secret in logs is redacted; source entitlement expires; Pro unavailable still produces safe incomplete diagnostics; no action tool listed. Incidents may remain unresolved and explicitly say so. No LLM output changes executor state or financial policy.

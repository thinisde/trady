# Issuer and market narrative drift monitor (proposed)

Status: new longitudinal text research capability; differs from the existing [relationship/statement memory](../company-intelligence.md) by tracking **changes in attribution, factual claims and emphasis** across time. LLM: **only `deepseek-v4.1-flash` through OpenCodeGo** under [model policy](./model-routing-and-governance.md).

## Objective and limitations

Detect when named management, analysts, regulators or permitted news sources change their documented thesis or emphasis: "capacity expansion" becomes "inventory digestion", earlier firm guidance becomes more conditional, or a market narrative switches from "growth" to "margins". A changed phrase can reflect seasonality/editorial context rather than genuine information. This is a **research flag**, not an objective measure of sentiment or proof the speaker is dishonest.

## Data chronology

Use signed published/ingested times, issuer/actor identity, statement type, original text locator/rights, quoted speaker, prepared vs spontaneous Q&A, referenced reporting period, sector/region context and source lineage. Actors with shared names must have confirmed identities. A secondary headline quoting a CEO is not a second independent CEO statement.

## Workflow

1. Segment permitted statements and normalize one attributed **claim frame**: speaker/issuer, subject/metric, polarity, qualification strength, timeframe, factual or projected, source ids.
2. Compare frames within coherent subjects, *not* arbitrary adjacent headlines. Ask Flash to classify: `FACT_CORRECTION`, `GUIDANCE_REVISION`, `CHANGE_IN_EMPHASIS`, `PUBLIC_REACTION`, `UNCHANGED`, `INCOMPARABLE`.
3. Validate true numbers in Rust against comparable primary sources. A generic LLM semantic similarity score does not establish contradiction.
4. Attach evidence of what actually changed versus what appears different because context, fiscal period, reporter or source rights changed. Keep neutral examples and reversals.
5. Produce a narrative timeline with extracted anchors and **uncertainty**, then route major guidance changes to [thesis falsification](./thesis-falsification.md) or [earnings-call research](./earnings-call-intelligence.md).
6. A verified change may trigger prioritized research. It cannot automatically lift confidence, widen an entry limit or cause a direct SELL.

## Proposed record

```json
{
  "schema_version": 1,
  "issuer_id": "issuer-42",
  "actor_id": "executive-5",
  "claim_subject": "capacity_utilization",
  "earlier_statement_id": "stmt-11",
  "latest_statement_id": "stmt-18",
  "comparison_type": "CHANGE_IN_EMPHASIS",
  "numerical_revision": null,
  "counterevidence_ids": [],
  "status": "REVIEW_REQUIRED",
  "model_id": "deepseek-v4.1-flash"
}
```

Synthetic. The earlier and current text spans remain source-attributed and rights-checked.

## Proposed integration and cost

New analyst tools `narrative.compare_statements`, `narrative.timeline` and `narrative.validate_change`, each bound to issuer/actor/window. Use company memory statement IDs and existing PostgreSQL revisions. Cache unchanged pairs; deduplicate repeated articles and coalesce updates by actor/subject. A scheduled daily digest can be proposed but must have approved cadence and quotas. Don't send enormous transcript history in one prompt; retrieve bounded per-topic passages through reviewed tools.

## Quality gates

Evaluate against manually annotated quarterly transcript/filing changes and sector controls. Track claim attribution precision, false revisions from fiscal-period changes, rate of misleading shifted quotes, time to new original disclosure, and whether signals add information beyond news timestamps alone. Include different-language translation limitations and data-rights failure cases.

Acceptance: reused quote on a later day is not a new statement; correction links to original; same surname does not merge speakers; simultaneous sources share a common root; changed guidance period isn't necessarily guidance cut; missing old source -> incomparable; unsupported text -> no change score. Any failed check yields `INCOMPARABLE` or `REVIEW_REQUIRED`; executor native protections unaffected.

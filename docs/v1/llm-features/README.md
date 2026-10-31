# Proposed OpenCodeGo LLM research extensions

Status: **12 optional research designs**, 2026-10-31, not implemented by this documentation. They extend Trady's existing [LLM/tool protocol](../llm-and-tools.md), [source review](../research.md), [approved skills](../runtime-skills.md), [memory](../memory-and-data.md), and [prediction fusion](../prediction-fusion.md). They do **not** add LLM trading authority.

**Mandatory deployment policy:** [OpenCodeGo-only model governance](./model-routing-and-governance.md). All 12 proposals use **\`deepseek-v4.1-flash\`** for every ordinary model call. **\`deepseek-v4.1-pro\`** is only an optional, explicit, bounded complex-task escalation after the exact ID is verified on OpenCodeGo; the documented current model list does not establish it. **No alternative models, generative providers or silent fallback.** Existing non-generative FinBERT/Perpetual remain separate.

## Feature catalog

| Proposed feature | Newly defined behavior | Initial LLM tier |
|---|---|---|
| [1. Investment committee](./investment-committee.md) | Independent Bull/Bear/Neutral/Verifier roles with conflicts and adjudication | Flash |
| [2. Thesis falsification](./thesis-falsification.md) | Pre-register refutable hypotheses, disconfirming evidence and review triggers | Flash |
| [3. Historical event analogy](./historical-event-analogy.md) | Retrieve and compare historical analogs with lookahead-safe statistical evaluation | Flash |
| [4. Earnings-call intelligence](./earnings-call-intelligence.md) | Transcript Q&A/change detection and management-claim lineage | Flash |
| [5. Causal impact reasoning](./causal-impact-reasoning.md) | Typed economic transmission graph with assumptions and confounding | Flash; Pro possible only if verified |
| [6. Research budget optimizer](./research-budget-optimizer.md) | Cost-aware next-question selection and bounded value-of-information testing | Flash |
| [7. Counterfactual scenario analyst](./counterfactual-scenarios.md) | Falsifiable conditional scenarios, not invented outcome probabilities | Flash; Pro possible only if verified |
| [8. Narrative drift](./narrative-drift.md) | Track actor-specific thesis/wording changes across original sources | Flash |
| [9. Multi-horizon forecast reviewer](./multi-horizon-review.md) | Reconcile conflicting 1/3/5-session forecast theses without fabricating probability | Flash |
| [10. Incident investigator](./llm-incident-investigator.md) | Read-only forensic review of trading/model/provider incidents | Flash; Pro possible only if verified |
| [11. Source credibility analyst](./source-credibility-analyst.md) | Versioned provenance quality, historical correction rates and manipulation suspicion | Flash |
| [12. Synthetic stress-scenario author](./synthetic-stress-scenarios.md) | Hypothetical test cases for deterministic stress harness, segregated from real evidence | Flash |

## New architecture and common orchestration

\`\`\`text
Approved data snapshots / existing verified source IDs
    -> rust ResearchOrchestrator (scope, rights, budgets, model allowlist)
    -> OpenCodeGo: deepseek-v4.1-flash (isolated role tasks)
    -> per-agent typed draft artifacts and unresolved evidence
    -> independent Rust validators and optional Flash review
    -> current historical reliability / predictor / policy layers
    -> existing executor-only order workflow OR NO_TRADE
\`\`\`

All model work uses the already implemented OpenCodeGo conversation harness and its approved tool registry; these documents define **future typed tools/ports** and do not imply the tools already exist. Agents are **prompt/persona roles run through one provider**, not independently hosted models. Reuse the existing session, tool-call ID, fencing, context limits and recovery semantics; new orchestration is an application service, not a permission to run hidden third-party frameworks or additional LLM endpoints.

### Minimum shared artifact

Every result must have a version, \`run_id\`, \`agent_role\`, pinned \`provider\`, \`model_id\`, \`model_request_id\`, \`evidence_snapshot_id\`, source ids and observed/available times, \`status\` (\`COMPLETE\`, \`INCOMPLETE\`, \`ABSTAIN\`, \`REVIEW_REQUIRED\`), explicitly typed claims, counterevidence, confidence **in source interpretation** distinct from outcome probabilities, uncertainty, coverage, cost and source-processing rights. A draft is not an executor command, a source verification, a trained forecast or a finalized proposal.

### Trust and research discipline

- Document verifiable facts separately from assumptions, economic hypotheses, event interpretations and hypothetical narratives.
- Evidence lineages must not be counted multiple times merely because agents agree. The committee does not produce probabilities through votes; only approved calibration/history may support them.
- All generated scenarios/historical analogs remain **candidates** for independent evaluators. They cannot change stops, holding deadline, liquidity gates, profit target caps, owner policy, risk checks or broker permissions.
- Source articles, X posts and transcripts are untrusted data. No model outputs skills, tool registrations, executable policy, SQL, filesystem paths or new provider/model routes with authority.
- No production workflow can require Pro until the exact OpenCodeGo \`deepseek-v4.1-pro\` is verified and owner enabled; otherwise bounded Flash/abstention must be usable.
- Privacy, X/news processing rights, provider quotas and existing run/session limits remain binding.
- Maintain original forecasts/failed research and no-trade histories; don't replace them retrospectively with improved explanations.
- Read-only analyst/LLM tools cannot create broker orders; unprotected positions stay a higher-priority executor concern than any research escalation.

## Incremental delivery and experiments

1. Implement the **model allowlist + no-fallback gateway**, validate OpenCodeGo exact model identifiers and pin agent-call provenance.
2. Add a typed research task/child-agent schema, deterministic budget arbitration and separate role outputs; avoid changing existing proposal schema until an independently versioned adapter is approved.
3. Pilot committee + falsification using frozen paper research; add historical analogy and source credibility with point-in-time provenance next.
4. Pilot earnings, narrative, causal and counterfactual services with audited parsers/graph and uncertainty; test extra calls against baseline research outcomes.
5. Add research-budget optimizer, multi-horizon reviewer and incident investigation; stress-case generator feeds offline test harness only.
6. Evaluate with chronological studies and prospective shadow research; compare Flash-only enriched research to existing Flash-only baseline, with costs, missingness, unsupported claims, false alarms and abstentions measured.
7. Activate only after independent release review. No future-return guarantees or mandatory positive signal claims.

## Verification checklist

- Every link resolves; 12 specs contain input contracts, workflow, JSON or structured output examples, failure cases, evaluation and acceptance.
- No doc claims new tools are implemented, no extra provider/model ID is permitted, no unreviewed model escalation.
- Under crashes, stale data, refused rights or incompatible tool schemas, research terminates \`INCOMPLETE\`/\`ABSTAIN\` and existing protection continues.
- Run the repository's standard asset, test and integration checks when **code** is eventually changed. These Markdown files themselves do not run a schedule, API request, background agent or trading action.

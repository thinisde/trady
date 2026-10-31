# OpenCodeGo-only LLM model policy for Trady research extensions

Status: **proposed strict deployment contract**, 2026-10-31. This document specifies the required policy for future LLM agents; writing it does **not** install enforcement or change currently deployed services. The existing [OpenCodeGo provider](../../opencode/README.md) already defaults to \`deepseek-v4.1-flash\` in [config](../../../config/opencode/llm.json), but its current adapter accepts explicitly configured alternate model strings. **Before new agent automation goes live, implementation must add a fail-closed allowlist.**

## Source of truth and scope

- **Only generative LLM provider**: OpenCodeGo through the reviewed authenticated OpenAI-compatible Chat Completions adapter and the configured OpenCodeGo endpoint. No Anthropic, OpenAI-hosted, Gemini, local Ollama, direct DeepSeek API, aggregator, or implicit alternative model usage.
- **Default and required model**: exact request ID \`deepseek-v4.1-flash\` for orchestration, specialist prompts, all ordinary analysis, summaries, extraction, tool planning, verification drafting, simulation narratives and reports.
- **Sole permitted conditional escalation**: literal ID \`deepseek-v4.1-pro\` **only** for bounded complex research tasks, only after verifying that the exact ID is officially exposed by this same OpenCodeGo account/endpoint, the owner explicitly enables that ID, and per-run escalation gates pass. Do not assume a model with a similar marketing name is the same ID.
- **Availability at documentation time**: [OpenCode Go model list](https://dev.opencode.ai/v2/docs/console/go/) describes \`deepseek-v4.1-flash\` and \`deepseek-v4-pro\`, not \`deepseek-v4.1-pro\`. The latter remains **RESERVED_UNAVAILABLE**, with no requests sent until exact provider capability/entitlement verification. \`deepseek-v4-pro\` is NOT an allowed substitute. A model alias, router, or provider-side redirection cannot satisfy the identity guarantee without verification.
- These restrictions apply to **every new generative invocation**, including internal subagents, critique/retry paths, background research, summaries, offline experiments, debugging and Telegram-generated explanations. Inherited models, hard-coded agent defaults, CLI overrides, provider redirects, nondefault endpoint/model aliases and environment configuration must all pass the same checks.
- **Non-LLM distinction**: existing independent non-generative components (FinBERT sentiment classification, Perpetual numerical forecasts, deterministic Rust calculations and Lean/LEAN proofs/backtests) are separate components already defined by Trady v1. They are **not alternate LLMs** and this document does not activate, disable, replace or silently route through them. If the owner's intent extends the ban to all predictive ML, that needs a separate explicit policy change to reconcile with v1.
- Model output cannot authorize trading; retain [LLM/tool contracts](../llm-and-tools.md), [prediction-fusion boundaries](../prediction-fusion.md) and [execution safeguards](../strategy.md).

## Fail-closed routing rules

All model calls go through a **single trusted ModelGateway** in Rust; the LLM cannot set its own model, endpoint or agent authority. Roles are names of prompts and permission bundles, **not separate model deployments**.

1. Validate the provider URL against a pinned **OpenCodeGo endpoint allowlist**, with HTTPS, redirect policy, DNS/SSRF protection and reviewed endpoint version. Do not accept an arbitrary base URL as an alternative provider.
2. Resolve the model using the deployment-approved enum \`Flash\` or \`ProReserved\`, never a free-form request field from model output or a skill.
3. Verify Flash is the requested and available model. For Pro, additionally require exact OpenCodeGo catalog/entitlement evidence, deployment allowlist revision, explicit owner approval for Pro use and one complex-task reason code plus hard tokens/spend cap.
4. Serialize a run manifest containing requested_model_id, returned_model_id (if exposed), provider identity, endpoint fingerprint, policy revision, escalation reason, owner approval, actor/agent, prompt hash, input/context hashes, cost reservation, actual reported usage and completion status. If provider omits/changes reported identity, treat the run as **MODEL_ID_UNVERIFIED**, not a valid financial research result.
5. On invalid/unknown model, unsupported Pro, quota exhaustion, timeout or provider outage: **never** fall back to other models or endpoints. Flash may retry Flash within the existing counted budget for explicitly retryable errors. Pro may not silently retry via Flash to produce a purported Pro-grade result; a new, clearly labeled Flash-only investigation may be separately requested.
6. Saved sessions pin provider + model + endpoint under existing [OpenCodeGo session bindings](../../opencode/README.md). Escalation should create a **bounded child research run with a new explicit model binding**, not mutate an existing Flash session or merge incompatible reasoning continuation. The child returns a cited typed artifact to the parent; both transcripts retain true model identities.
7. When the hard deadline is approaching, do not start escalation. Produce \`INCOMPLETE\`, \`NEEDS_REVIEW\` or \`NO_TRADE\` under current policy, while executor protection continues.

## Complexity classification (examples, not a loophole)

\`deepseek-v4.1-flash\` handles ordinary tasks: extracting issuer facts, screening, summarizing transcripts, source comparisons, basic contradiction checks, one-hop economic links, scenario enumeration, historical candidate retrieval and reports.

A Pro request may only be considered for: densely conflicting multinational evidence; long, multi-hop causal paths with source reconciliation; adversarial committee deadlock after bounded Flash passes; difficult multi-period financial/accounting inconsistencies; or unusual research incident root-cause reconstruction. These are **task-specific**, not a global "always use Pro for finance" flag.

Escalation conditions should combine \`complexity_reason\`, \`evidence_ids\`, \`failed_flash_attempt_ids\`, \`expected_information_gain\`, \`estimated_cost\`, and \`owner_approved_budget\`; require the exact model to be available. No model may decide its own escalation without deterministic gateway approval.

## Proposed deployment configuration (illustrative, NOT current parser schema)

\`\`\`yaml
generative_llm:
  provider: opencodego
  endpoint: https://opencode.ai/inference/go/openai/v1
  default_model: deepseek-v4.1-flash
  allowed_models: [deepseek-v4.1-flash]
  pro_reserved_id: deepseek-v4.1-pro
  pro_enabled: false
  allow_unlisted_models: false
  allow_cross_provider_fallback: false
  allow_automatic_model_upgrade: false
  allow_automatic_pro_escalation: false
  require_returned_model_verification: true
  max_parallel_flash_agents: 3
  max_pro_calls_per_research_run: 0
\`\`\`

**This YAML is a design target**, not something to copy into the existing \`config/opencode/llm.json\` unmodified. When support is proven, an owner-reviewed revision may set \`pro_enabled: true\`, append **only** \`deepseek-v4.1-pro\` to \`allowed_models\`, set a finite per-run Pro-call cap and require reason-coded approval. If no actual ID is exposed, keep Pro disabled. Limit values are example ceilings to be tuned to real quotas. Prices/context windows are provider- and account-dependent and must not be guessed.

## Credential, privacy and tool boundaries

Only the existing \`OPENCODE_API_KEY\` credential route is allowed for these generative calls. Never give model prompts or skills API keys, raw broker trading credentials, tax identity, arbitrary SQL/shell access or order submission. Apply source-processing/retention rights before forwarding X, licensed transcript, market data, private portfolio or tax material. Isolate agent memory and summaries by owner/account/run and reviewed skill manifest. Prompt/tool-injection in source text does not authorize another model, URL or tool.

## Test and monitoring matrix

| Test | Required result |
|---|---|
| Unknown model string, direct DeepSeek base URL or OpenAI/Anthropic endpoint | Refuse at construction and before HTTP; record denied model attempt |
| Prompt asks for different model or alias | Treat as untrusted content; no config change or request |
| Pro asked for complex problem but exact OpenCodeGo ID absent | Explicit \`PRO_UNAVAILABLE\`; Flash-only bounded handling or unresolved review, never \`deepseek-v4-pro\` substitution |
| Pro allowed but owner budget/approval absent | Refuse escalation; keep active Flash run intact |
| Provider returns a different model ID or sends unsupported redirect | Quarantine response; no promotion into trade readiness |
| Agent crash and resumption | Preserve pinned model/endpoint, prior usage and attempt IDs |
| Flash timeout | Count request and optional same-model retry; no other model |
| Parallel committee agents | Each call records Flash ID; no hidden agent default |
| Historical replay/weekly optimization | No retrospective change in model used for an original forecast |
| Token-price change or Go entitlement removed | Fail with budget/capability status; protection independent |

Observability: requests and spend by model/run/role, Pro denials, model-mismatch alerts, rights failures, truncated JSON and audit-ready source-to-assessment references. A feature is **not complete** until the running code enforces these tests. Documentation has no runtime or financial side effects.

## Evidence and related documents

[OpenCode Go documented model list](https://dev.opencode.ai/v2/docs/console/go/); [OpenCode conversation harness](../../opencode/README.md); [Trady agent tool protocol](../llm-and-tools.md); [Trady's prior LLM/config defaults](../../../config/opencode/llm.json); [weekly independent approval](../weekly-optimization.md).

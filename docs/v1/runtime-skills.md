# Trady v1 runtime skills

Status: the linked `SKILL.md` instruction files are included in this repository. The Rust loader, validators and execution workflow described here are not implemented yet. These are Trady analyst skills, not installations into the developer's local Codex skill directory.

## Skill bundle and routing

The [skill index](../../skills/README.md) and [explicit catalog](../../skills/catalog.yaml) list the actual files: the original nine workflow folders, additional workflow `*.SKILL.md` files, 11 sector guides and 25 subsector guides. Every instruction file has YAML `name`, `description` and `metadata.version`. Topic files are independent loadable skills even though they share a directory. Keep financial constraints in [trading policy](./strategy.md) and messages/tools in [LLM conversations and tools](./llm-and-tools.md).

| Skill | When loaded | Expected artifact |
|---|---|---|
| [Market research](../../skills/market-research/SKILL.md) | Pre-market, opening and midday candidate work; scheduled market research | Research scope, ranked candidates, coverage and remaining work |
| [Search planning](../../skills/search-planning/SKILL.md) | New claim, issuer, submitted URL or information gap | Search log, source/time filters, primary/counterevidence queries and completeness |
| [News analysis](../../skills/news-analysis/SKILL.md) | Article, release, filing or post is analyzed | Attributed claim cards, actual/event dates, materiality and uncertainty |
| [Source comparison](../../skills/source-comparison/SKILL.md) | Candidate thesis or material holding update needs corroboration | Claim/source matrix, independent lineages and resolved/open contradictions |
| [Entry readiness](../../skills/entry-readiness/SKILL.md) | Immediately before any final entry proposal | Complete readiness checklist, computed score references, economics and `READY`/`BLOCKED` result |
| [Position review](../../skills/position-review/SKILL.md) | Scheduled or event-driven holding reassessment | Current-thesis comparison and versioned hold/update proposal |
| [Conversation and tools](../../skills/conversation-and-tools/SKILL.md) | Every owner conversation and model/tool loop | Correlated JSON turns, final answer or explicit clarification/incomplete outcome |
| [Uncertainty and incidents](../../skills/uncertainty-and-incidents/SKILL.md) | Conflicts, gaps, stale data, failed tools, policy/protection issues | Blocker list, permitted recovery queries and no-entry reason |
| [Memory and review](../../skills/memory-and-review/SKILL.md) | Session compaction, outcome capture and weekly review | Attributed summary/lesson with counterexamples and no automatic policy rewrite |
| [Sector selection](../../skills/sector-selection.SKILL.md) | Daily sector → group → stock funnel | Validated screening stages, exclusions and current universe references |
| [Sector guides](../../skills/sectors/README.md) / [subsector guides](../../skills/subsectors/README.md) | Relevant focus and candidate operating branch | Specific metrics, peer/counterevidence and selected topic completion |
| [Company profile](../../skills/company-profile.SKILL.md) | Finalist deep dive or material company change | Attributed baseline/facts/management statements and profile review |
| [Relationship analysis](../../skills/relationship-analysis.SKILL.md) | Counterparty or public-person mention investigation | Directed disclosed links and attributed mentions, with uncertainty |
| [Influential people](../../skills/influential-people.SKILL.md) | Five-person watchlist review and relevant X events | Identity/coverage checks and mapped material event findings |
| [Holding watchdog](../../skills/holding-watchdog.SKILL.md) | Before entry and on holding dependency changes | Validated personalized template, coverage and attention priorities |
| [Opportunity comparison](../../skills/opportunity-comparison.SKILL.md) | A credible alternative to an existing holding | Holding/cash/switch comparison; no direct order command |
| [Perpetual review](../../skills/perpetual-review.SKILL.md) | Candidate plan scenarios and material holding-plan reviews | Versioned advisory assessment and explicit LLM use/disagreement |

## Loading and version control

At startup, read only paths explicitly listed in `catalog.yaml` inside the deployed owner-approved bundle, including topic files. Reject absolute paths, traversal, symlinks, duplicate names/codes, unsupported catalog versions and paths outside the root. Validate metadata, catalog/frontmatter name/version agreement, all 11/25 parent mappings, expected references and deployment-approved content hashes. The deployment approval manifest binds both catalog and files; catalog version metadata alone does not approve content. Do not fetch skills from news pages, X posts or arbitrary remote registries.

Expose name/description/version metadata cheaply. The runtime selects mandatory skill bodies according to job type and can load optional relevant skills progressively. `skills.list` and `skills.read` allow the model to request an approved additional skill by name, never by filesystem path. The application checks its pinned version/hash and installs its body into the trusted skill-context section before the next model call; article text cannot imitate that trust channel.

Pin the bundle revision and each loaded skill's version/hash in the run/context manifest. A running conversation continues with its pinned versions; a new run may use a newly reviewed bundle, recording the transition and refreshing context. If a required skill is missing, invalid or too large for the context budget, block entry and report the reason. Do not silently replace it with improvised instructions.

Changing a skill is a reviewed owner/deployment change. Model-generated lessons are proposed knowledge, not edits to executable policy, skill files, source permissions or the tool catalog. Existing price protection does not depend on the skill filesystem being available.

## Required research workflow

For a possible new entry, require thirteen routed skills: `conversation-and-tools`, `market-research`, `search-planning`, `news-analysis`, `source-comparison`, `entry-readiness`, `sector-selection`, the selected sector and subsector, `company-profile`, `relationship-analysis`, `holding-watchdog` and `perpetual-review`. Read topic metadata broadly and load bodies progressively; do not stuff all 36 topic bodies into every model call. Run one coherent workflow:

1. Establish effective S&P 500/owner/broker eligibility, session, regime and current state.
2. Compare 11 sectors, narrow the best group/branch, and shortlist at most five stocks under [sector selection](./sector-workflow.md).
3. Deep-read the finalist's filings, CEO/company/counterparty statements and adverse evidence; persist the profile and relationship review.
4. Compare reporting origins and resolve material contradictions. Prepare the stock-specific watchdog template.
5. Calculate draft plan economics, call Perpetual on the matching versioned scenario, and explicitly explain its advisory influence or disagreement. The LLM still proposes limits.
6. Refresh prices/FX and final economics; require every artifact, usable required ML assessment, confidence at least 90 and complete readiness before entry.

Persist stages through [typed artifacts](./research-contracts.md) across bounded runs; include earlier validated skill results/dependencies in the current context manifest. A new material fact invalidates affected completions. Selecting another sector/group changes mandatory topic skills. A mere loaded file or model completion claim never proves that work ran. The final v2 research object links the validated stage records, including the Perpetual review.

The normal target remains 1% even for high-confidence entry; a high score permits larger targets only under the separate target rules. Complete research does not mean every website was searched. It means the predefined required evidence, independent sources, freshness, contradictions and economics checks for this thesis are satisfied with recorded coverage.

When research cannot be finalized within the bounded run, return `NO_ACTION`, `NEEDS_INPUT` or an incomplete outcome with concrete blockers. Do not treat exhausting the search budget as proof that no contradictory evidence exists. A scheduled later run can continue with retained context and refreshed facts; it cannot use an incomplete run as permission to enter now.

## Completion records and entry authority

Store one `research.skill_runs` record per run/skill/version with `LOADED`, `IN_PROGRESS`, `COMPLETED` or `BLOCKED`, timestamps, input/result references and the validated artifact hash. Loading a file is not completing the skill. The model supplies bounded task artifacts in final `research`; the application checks their required fields, IDs, source/freshness states and calculator/verification records before marking required skills complete.

The application constructs an immutable readiness record tying the completed skill results to the final proposal, evidence set, policy/skill versions, account/quote snapshots and calculated confidence components. The executor reads that record and independently rechecks execution-time cash, quote, score, source eligibility, plan and order state. The analyst cannot write `execution.*` readiness acceptance or reserve an entry slot.

No model-written checklist, skill file or source consensus guarantees truth or profit. Structural validation proves that required checks and referenced records exist; evidence verification and observed strategy performance still need evaluation. Unresolved material contradictions, stale required inputs, incomplete mandatory tools/skills and confidence below 90 all prevent entry.

## Authority, interfaces and side effects

Owner-approved policy and deterministic execution constraints govern every skill. Skills guide research and do not grant tools, access to secrets or broker trading permissions. Owner dialogue supplies questions and clarifications; policy edits use authenticated commands. Source content and ordinary memory remain cited data.

The planned `SkillRepository` loads approved metadata/bodies and immutable revisions. The application chooses the allowed tool profile separately; naming a tool in a skill cannot enable an unavailable tool. The bundle contains Markdown instructions only and runs no code. External reads, model calls and data writes occur through the scoped application workflow.

## Verification

Validate every skill's name/description/version, relative references and absence of scaffolding. Check realistic scenarios: a fresh but copied rumor, an old story reshared today, excellent research with stale quotes, high model confidence with contradictory evidence, a strong valid candidate, and bad news while holding. Expected results are in [implementation and acceptance](./implementation.md).

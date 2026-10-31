# Trady implementation entry point

Use this file as the starting brief for the coding agent implementing Trady v1. Follow the linked specifications, deliver the project in the documented stages, and keep the implementation, documentation and validation evidence consistent.

## Current state and specification authority

[OpenCode Go conversations](./docs/opencode/README.md) now implement model calls,
persistent sessions, approved skill loading and bounded tool/context management.

The repository contains the specifications, 52 Trady runtime skills and an initial
offline Rust foundation. The [implementation record](./docs/foundation.md) lists
implemented contracts, commands and validation evidence. Cargo workspace, JSON
validation, fake adapters and local PostgreSQL recovery exist; trained models,
listeners, full research readiness, provider integrations, protective orders and
live execution remain future work. The later-stage package/tool/table descriptions
remain requirements unless that record explicitly marks their implementation.

Read applicable `AGENTS.md` instructions before working. Inspect the workspace and preserve existing changes. Use the [v1 specification](./docs/v1/README.md) and its linked contracts as the implementation requirements. This index provides navigation and delivery guidance; detailed rules remain in their owning documents. [v1.1.md](./docs/v1/v1.1.md) is historical input, and later v1 decisions replace its superseded requirements. Resolve material contradictions explicitly instead of silently changing policy.

The files under [skills](./skills/README.md) are instructions for Trady's runtime LLM. Implement their loading, versioning and validation through the approved catalog; they do not replace the coding agent's repository instructions.

## Read in this order

| Order | Read | Purpose |
|---|---|---|
| 1 | [v1 specification](./docs/v1/README.md) | Scope, accepted decisions, defaults and outstanding deployment choices |
| 2 | [Architecture](./docs/v1/architecture.md) | Process boundaries, proposed Rust packages, ports, ownership and event flow |
| 3 | [Trading policy](./docs/v1/strategy.md) | Confidence, allocation, costs, thresholds, state machines and deterministic execution |
| 4 | [Implementation and acceptance](./docs/v1/implementation.md) | Delivery sequence, behavioral acceptance cases, requirement traceability and release gates |
| 5 | [Memory and data](./docs/v1/memory-and-data.md) and [operations](./docs/v1/operations.md) | Persistence, database roles, account fencing, recovery, capabilities and environment setup |
| 6 | [LLM conversations and tools](./docs/v1/llm-and-tools.md), [exact JSON contracts](./docs/v1/llm-contracts.md) and [research contracts](./docs/v1/research-contracts.md) | Multi-round sessions, authority, schemas, artifact storage and application/model boundaries |
| 7 | [Runtime skills](./docs/v1/runtime-skills.md), [skill index](./skills/README.md) and [catalog](./skills/catalog.yaml) | Approved skill routing, required artifacts, bundle metadata and entry readiness |

Before implementing each subsystem, read its detailed guides:

| Subsystem | Guides |
|---|---|
| Sources and evidence | [Research pipeline](./docs/v1/research.md), [researched source guides](./docs/v1/research-sources.md), [references](./docs/v1/references.md) |
| Sector → subsector → stock | [Selection workflow](./docs/v1/sector-workflow.md), [11 sector guides](./skills/sectors/README.md), [25 subsector guides](./skills/subsectors/README.md) |
| Company profiles and relationships | [Company intelligence](./docs/v1/company-intelligence.md), [research artifact schemas](./docs/v1/research-contracts.md) |
| X events and holding-specific research | [Watchdogs and X](./docs/v1/watchdogs-and-x.md) |
| Numerical advisory model | [Perpetual integration](./docs/v1/perpetual.md), [Perpetual review skill](./skills/perpetual-review.SKILL.md) |
| Owner controls and notifications | [Telegram interface](./docs/v1/telegram.md) |

## Rules the implementation must preserve

- **Rust owns execution.** Use the proposed `trady-domain`, `trady-policy`, `trady-application`, `trady-storage`, `trady-adapters` and `trady-cli` package boundaries. Run `trady analyst`, `trady executor` and `trady gateway` as separate services with distinct credentials and restricted database roles. The analyst has no broker trading credential.
- **The LLM proposes; the executor validates and acts.** The model chooses a stock and proposes entry, stop, target and subsequent plan updates through strict JSON contracts. It has no `BUY` or `SELL` tool. Preserve correlated tool turns, persistent sessions, schema validation, bounded runs and audit records. Untrusted source text cannot grant tools or authority.
- **Research must be complete before entry.** Apply the effective S&P 500 membership, owner permissions and broker support intersection. Narrow through sector and subsector, compare independent evidence and counterevidence, and store company profiles, attributed mentions and relationships. Require every documented readiness gate and a deterministic confidence score of at least 90/100; the score is not a probability. A no-trade result is valid.
- **Use the documented cash and risk policy.** Start with one cash account, one distinct stock at a time, no shorting or leverage, and no additions to a holding. Allocate at most 98% of reconciled spendable cash to total estimated entry cost, including fees and FX. Deposits affect sizing only after reconciliation. Use checked decimal arithmetic and currency-aware calculations.
- **Targets are bounded objectives.** Normally aim for a 1% net position gain, 2–3% when justified, and at most a 10% configured target for a verified exceptional catalyst. These are not guaranteed daily returns. Apply the documented confidence-dependent thresholds, holding limits, stop protection and session loss guard.
- **Protection remains deterministic.** Continue holding research without delaying price monitoring or broker reconciliation. Latch reached exit triggers before considering optimistic updates. Enforce idempotency, account fencing, partial-fill handling and no-oversell coordination. Rotation starts disabled and requires the separate reviewed policy and executor-owned gates.
- **Persist evidence and state.** PostgreSQL is authoritative; memory files are reconstructable projections. If reusing `paperless-db-1`, provision a separate `trady` database and reviewed roles without altering the Paperless database. Preserve provenance, timestamps, corrections, deletions and uncertainty rather than turning copied or repeated claims into confirmation.
- **Monitor the owner's five accounts with scoped priorities.** Use Trump, Musk, Keith Gill, Ackman and Icahn as specified in [watchdogs and X](./docs/v1/watchdogs-and-x.md). Verify identities and actual API access, include permitted additional Trump channels, and distinguish originals, replies, quotes and reposts. Their qualitative influence ranking grants no trading-universe exception. Daily selection and event-driven holding research coexist.
- **Perpetual is advisory.** Integrate `perpetual-ml/perpetual` through the documented assessment tool and approved immutable model artifacts. The LLM weighs the assessment and still proposes limits; Rust enforces policy. Model output adds no confidence points or independent news confirmation. Unavailable, stale or unsupported assessments block new entry while existing protection continues.
- **Begin with fake adapters and paper operation.** Missing live credentials, provider contracts or deployment choices do not block the foundation. Record unknown capabilities and keep dependent actions blocked. Live release requires the evidence, reviewed configuration and explicit owner authorization already specified in the release gates.

## Delivery sequence

Use the detailed exit criteria in [implementation and acceptance](./docs/v1/implementation.md). This table is a navigation aid, not a replacement for those criteria.

| Stage | Deliver | Advance when |
|---|---|---|
| 1. Contracts and capabilities | Domain types, versioned policy/configuration, schema fixtures, fake clock/calendar and provider ports | Contract meanings are explicit and unknown capabilities cannot become assumed support |
| 2. Rust and persistence | Cargo workspace, process wiring, migrations, restricted roles, leases, outbox and account fencing | Fake-account reconciliation and restart recovery work, and privilege boundaries are demonstrated |
| 3. Research and knowledge | Effective universe/GICS snapshots, sector funnel, news/RSS collectors, company profiles, relationships, X listeners and watchdogs | All 11 sector and 25 subsector guides route correctly, with provenance and coverage gaps retained |
| 4. LLM, skills and Telegram | Persistent JSON sessions/tool loops, approved skill loader, research artifacts, memory, dialogue and notifications | Correlated turns and owner follow-ups survive restart; unauthorized tools and commands have no authority |
| 5. Perpetual and paper execution | Point-in-time ML pipeline, approved model assessment, policy validation, sizing, plans, fills and protection | Required research, advisory, cost, quote, partial-fill and limit-ownership cases pass |
| 6. Read-only and shadow operation | Approved real feeds, broker snapshots, scheduled research, reports and shadow intents | At least 20 exchange sessions meet the documented reconciliation and protection criteria; extend observation for unresolved gaps |
| 7. Restricted live pilot | Account-specific reviewed configuration, capability evidence, incident runbooks and rollback | All acceptance and live release gates pass and the owner explicitly authorizes deployment |

The shadow window is an operational observation requirement, not proof of profitability. Keep fake, paper, shadow and live policy/state-transition behavior consistent; change adapters and execution mode rather than duplicating policy.

## First implementation milestone

Start with stages 1 and 2. Deliver a runnable offline foundation that later stages can extend:

1. Inspect the repository, read the required specifications above, and record a concrete implementation plan against the documented acceptance cases.
2. Create the proposed Rust workspace and three service entry points. Keep domain and pure policy logic independent of network clients, database drivers and provider DTOs. Pin dependencies and verify current library documentation according to repository instructions.
3. Turn the documented JSON Schemas into versioned machine-readable assets and typed boundaries. Validate the worked examples and reject unsupported fields, invalid calendar dates, incompatible versions and model-supplied application authority. Keep schema sources and documentation synchronized.
4. Add fake broker, quote and time adapters, plus explicit capability/configuration records. Incomplete proposals and unknown capabilities must remain blocked.
5. Add reviewed PostgreSQL migrations and role grants for durable state, account ownership, jobs and outbox handling. Exercise them against an isolated development database using the deployment guidance.
6. Demonstrate fake-account reconciliation, persistence and restart recovery. Show that replay or competing workers cannot create duplicate execution intents and that a model-facing process cannot acquire trading authority.

Record actual setup, run and validation commands when they exist. Update documentation to distinguish implemented behavior from future stages. This first milestone establishes the foundation; it does not satisfy the later research, ML, execution or release gates.

## Validation and implementation handoff

- Treat the [behavioral acceptance cases](./docs/v1/implementation.md) as requirements to implement and exercise, not as tests that already ran. Map implementation evidence to case IDs as each stage becomes available.
- Use meaningful domain/policy tests, schema fixtures, database integration checks and fake-provider scenarios. Include stale data, duplicated events, restarts, partial fills, concurrent workers, exit/update races, source outages and ML abstention where relevant.
- Run formatting, linting and applicable Rust tests once the workspace exists. Validate migration/role boundaries and restoration separately from pure policy checks. Record failures and unresolved capability gaps accurately.
- Validate local Markdown links, catalog metadata and schema examples after changing docs or skills. Do not claim live subscriptions, a populated company database or a trained model from documentation alone.
- At each completed stage, report what works, which commands and acceptance cases passed, what remains, and the next concrete stage. Keep provider secrets out of source files, logs and research artifacts.

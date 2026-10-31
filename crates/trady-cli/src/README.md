# CLI commands and wiring

## Overview

[main.rs](./main.rs) contains private command handlers composing public package boundaries.
[autonomous.rs](./autonomous.rs) composes the gated autonomous executor separately
from the foundation startup probes and paper demonstrations. Its private
[analyst worker](./autonomous_worker.rs) runs screening and native inference in a
separate child process.

## Interface

`[weekly.rs:run]` exposes real `weekly check|run|export|forecast|bridge` commands;
`arithmetic-bridge` and `formula-bridge` support executable Lean/Rust parity. `[release.rs:run]`
exposes candidate, independent verification, scoped evidence, explicit owner
approval, compare-and-swap advancement, inspection and ACTIVE formula export.

`[financial.rs:run]` exposes reviewed provider collection, rights-aware snapshots
and status through the [financial-data contract](../../../docs/financial-data.md).

`[inputs_service.rs:HELP]`, `run(args: &[String]) -> Result<Value>` expose local
`inputs-service check/import/review/inspect/export-universe/deadline/prune` and
explicit `serve`. `[model_lab.rs:HELP]`, `run` expose offline
`model-lab fit/evaluate/inspect/bundle/offline-infer`; see the
[operator contract](../../../services/inputs/README.md). Both reject unknown or
duplicate options. The HTTP server uses `TRADY_INPUTS_SERVICE_TOKEN` through the
central credential loader and binds loopback only. Import/review/output commands
write private immutable files, evaluation durably consumes the sealed interval,
and explicit pruning removes expired licensed content. Fit/evaluate do not approve
or deploy a model; TEST ONLY artifacts cannot receive production approval.

`[finbert/mod.rs:HELP]` and `[finbert/mod.rs:run]` add `sentiment status --config FILE`
and `sentiment score --config FILE --input FILE`; see the
[component contract](./finbert/README.md). The
[FinBERT guide](../../../docs/v1/finbert/README.md) defines source-item input, optional
confidence features, dormant deployment and the distinction between a preview and
persisted automated plan integration.

`[opencode/mod.rs:HELP]`, `run` add `chat`, `sessions list/show/close` and
`skill-manifest`; see the [component contract](./opencode/README.md).
The [OpenCode guide](../../../docs/opencode/README.md) describes the approved bundle,
model configuration, environment credentials, follow-ups, recovery and context files.

`[news.rs:HELP]` and `[news.rs:run]` add `news status`, bounded `news collect --once`,
`news read`, `news x`, `news resolve-x` and offline `news parse-feed`. The [news guide](../../../docs/news.md)
documents flags, registry review, environment references and source coverage limits.

`[news.rs:run]` also exposes `counts-x`, `budget-x`, `reconcile-x FILE` and
`export-x [--as-of UTC] [--limit 1..100]`. `[news.rs:client]` composes `MeteredHttp`
with the shared analyst cost repository; existing X commands use the same gate.

`[telegram/mod.rs:HELP]`, `run` expose `telegram discover`, `check --config FILE`
and `run|worker --config FILE [--once]`; see the [component contract](./telegram/README.md).
Expanded run uses the gateway identity; worker separately handles analyst
research/dialogue jobs (`[telegram/gateway.rs:Gateway::open]`,
`[telegram/gateway.rs:worker]`).

`[autonomous.rs:HELP]`, `run(args: &[String]) -> Result<Value>` expose:

```text
trady autonomous check --config FILE
trady autonomous run --config FILE [--once]
trady autonomous analyst-service --config FILE
trady autonomous holding --config FILE --position ID
trady autonomous review-holding --config FILE --request FILE
trady autonomous recover-order --config FILE --intent ID --broker-order ID
```

`[autonomous.rs:RuntimeConfig]` requires `config_version`, `enabled`,
`interval_seconds`, `trading`, `broker`, `inputs`, `universe`, `models`, `controls`.
Each model entry has `manifest` and `artifact_path`; artifact paths resolve from
the deployment file's directory. The shipped
[example](../../../config/autonomous.example.json) is disabled and has no approved artifacts.
`check` validates configuration/artifacts and reports disabled runtime/universe,
empty model/skill registries and missing/invalid active formula as blockers.
Optional fields are `analyst_socket`, `shadow_directory` and `release_candidate`.
Live requires the socket and exact release candidate; local native model hashes
must match its model map. Expired native artifacts are excluded from inference.

`[autonomous.rs:holding_advice]` uses `TRADY_ANALYST_DSN`. `holding --position ID`
reads an executor-attested holding. `review-holding --request FILE` reads at most
4 MiB of UTF-8 JSON with exactly `context`, `proposed`, `research`, `screening`
and `review_reason`; unknown fields fail. The position reference comes from
`context.scenario.binding.arguments.position_id`, and the calculation identity
is checked against the current holding. Review persists screening, dossier and
context, performs/persists native assessment and review, then queues a
`HoldingUpdate` for later executor activation checks. It does not activate the plan.

`[autonomous_worker.rs:IsolatedInputs]` implements `TradingInputs` using the
read-only `MarketInputs` for session/quote/cost reads and a lazy analyst child for
`candidate`. The internal `autonomous analyst-worker` entrypoint accepts only its
bounded stdin protocol; it is not a deployment command. `Boot` contains reviewed
configuration, the absolute deployment configuration file path, analyst DSN and
optional service token.
Each subsequent request is an `AccountSnapshot`; `Reply` returns that snapshot ID,
a failure flag and an optional candidate. Both directions use one JSON line per
message with a 4 MiB limit, including the newline. `Reply` can also carry
`ModelIdentity` with `revision_id` and
`artifact_sha256` (`[autonomous_worker.rs:Boot]`, `[autonomous_worker.rs:Reply]`,
`[autonomous_worker.rs:ModelIdentity]`).

`[main.rs:HELP]` and `[main.rs:run]` define:

```text
trady analyst|executor|gateway --once [--config FILE]
trady demo
trady demo-postgres
trady migrate
trady credentials init|status
trady t212 snapshot --config FILE
trady validate-json model|results|envelope|artifact|artifact-result FILE
```

`--help`, `help` and `-h` display help. `[main.rs:service]` uses
`TRADY_ANALYST_DSN`, `TRADY_EXECUTOR_DSN` or `TRADY_GATEWAY_DSN` when present.
Migration requires `TRADY_MIGRATOR_DSN`; the database demo requires the executor DSN.
All credential names resolve through the [central store](../../../docs/credentials.md)
with environment overrides. `credentials init` creates a blank private file;
`credentials status` prints names and availability without values or network calls.
`[main.rs:trading212_snapshot]` selects `TRADY_T212_DEMO_API_KEY` /
`TRADY_T212_DEMO_API_SECRET` or `TRADY_T212_LIVE_API_KEY` /
`TRADY_T212_LIVE_API_SECRET` from the config's explicit `environment`.
The file contains only a non-secret `Trading212Config`.
See the [Trading 212 guide](../../../docs/trading212.md).

## Invariants

`[weekly.rs:run]` invokes `run_weekly` with `PgWeeklyStore`, pinned `HelperLab` and a
durable holdout claim/finish wrapper. The database start time survives restart.
Weekly decisions never authorize trading. Release verification executes a pinned
independent helper and resolves real pinned evidence files; no raw model flag is
accepted as an approval. Release roles have separate DSNs and owner scope binds
account/mode, policy, candidate/model/source hashes and expiry.
`[weekly.rs:exact_options]` rejects duplicate/unknown options;
`[weekly.rs:read]` accepts bounded UTF-8 JSON from a file or `-` stdin, with a
4 MiB limit. Weekly leases must be 1–3,600 seconds and strictly exceed twice the
longest helper timeout plus thirty seconds (`[weekly.rs:run]`).
`[release.rs:IndependentReleaseVerification::validate]` requires PRODUCTION
FORMULA_V1 evidence, the ten named checks and matching pinned artifact hashes.

`[financial.rs:run]` requires `collect --once`, source selection for
snapshot/signals/features/import, and limits 1–500. `[financial.rs:bounded]`
bounds file input to 2 MiB. `[model_lab.rs:run]` reports offline inference as
diagnostic with `deployment_eligible:false`.

- `[news.rs:run]` inspects disabled pilot status offline. Counts require a selected
  search source; billing reconciliation requires a bounded provider-evidence file.
  `[news.rs:client]` requires a shared durable cost policy for paid X reads.
- `export-x` uses stored, rights-checked evidence and no source client. Its output
  denies training permission and identifies missing verification/feature availability.

- `[finbert/mod.rs:run]` bounds file reads, requires explicit enabled configuration
  for scoring and always reports `entry_authorized=false`. `status` makes no
  network request. Optional thresholds use supplied features and default policy;
  these are unverified previews, not completed readiness or an active plan update.
  Flags must appear in the documented exact order; only score includes
  `evidence_verified=false`, while status omits it.

- `[opencode/mod.rs:binding]` requires an explicit local owner and validated paper
  account/policy. `[opencode/mod.rs:chat]` loads a reviewed skill manifest, binds
  the configured provider and permits one question/file/resume action.
- `[news.rs:run]` requires explicit source selection for lookup/resolve/offline parsing;
  validates registry/flags; reserves source allowance before provider requests; and
  rejects disabled X sources. Offline parsing does not require source enablement.
  Collection/read results grant no entry authority; successful identity resolution
  instead reports `identity_approved=false` for operator review.
- `news read --for-model` applies storage/model-processing rights and retention.
  Unavailable sources return explicit statuses instead of fabricated post/article text.
  `news x` returns persisted, redacted records. Explicit post-unavailability failures
  invalidate known copies and remove stored expanded quotations.

- `[main.rs:service]` validates paper configuration and schemas. Supplied DSNs must
  use the matching restricted role; absent DSNs yield `not_configured`.
- `[main.rs:begin_demo]` uses prepared paper buy fixtures and reports duplicate
  reservation behavior; it does not assert the second reservation's result.
  `[main.rs:demo_postgres]` closes/reopens its connection and recovers one durable intent.
- `[main.rs:finish_demo]` reports unresolved broker responses and unprotected filled
  positions without submission/protection claims. All entry authority remains blocked.
- `[main.rs:main]` returns a failure exit code on invalid input or operational failure.
- `[main.rs:trading212_snapshot]` permits information reads from an explicitly bound
  demo/live account and reports `entry_authorized=false`. It exposes no order command.
- `[autonomous.rs:RuntimeConfig::validate]` requires version 1, a 5–30-second
  interval, matching currency, validated input/universe
  configuration and at most 32 models. Shadow may use a distinct trading account;
  matching broker/trading account and mode are required in the broker-bound
  execution branch. Configuration is a regular UTF-8 file
  bounded to 2 MiB; model artifacts are bounded to 16 MiB and checked against
  approved native model manifests/hashes/runtime/format.
- `[autonomous.rs:run]` requires `enabled=true` for run/recovery. The demo/paper
  and live executor paths need broker environment credentials plus
  `TRADY_EXECUTOR_DSN`; run additionally needs
  `TRADY_ANALYST_DSN` only for the child compatibility path. The external analyst
  service loads its own DSN. Optional input authentication uses
  `TRADY_TRADING_INPUTS_TOKEN`, all through the central credential loader.
  Database-backed paths require migrations 1–9; startup does not migrate. Database transport
  uses verified remote TLS or explicit local plaintext with restricted roles.
- `[autonomous.rs:run]` wires Trading 212 demo/paper or live. Shadow uses a
  separate namespace and read-only broker reader, without an executor DSN,
  execution journal or executor lease. Its child compatibility path alone needs
  the analyst DSN. New entries need an approved formula,
  skills, model artifacts, finalized dossier, screening and fresh session/quote/
  account facts. Deployments must supply approved universe rights/provider and
  input-service data; neither is replaced by a built-in constituent/data fallback.
  The analyst/executor/gateway probes do not supply that input service.
- `[autonomous.rs:run]` check performs no credential, broker or database access and
  reports `entry_authorized=false`; it does not establish operational readiness.
  Demo/paper and live runs, including `--once`, can submit real broker orders. Unknown outcomes
  remain blocked until exact-ID observation/recovery; they are not replayed.
- `[autonomous_worker.rs:WorkerProcess]` starts the current executable with a
  cleared environment and anonymous pipes. The child receives the analyst DSN
  and service token, with no broker/executor credential or credential-store load.
  Pipe readers/writers use bounded queues. A request has a 30-second monotonic
  deadline, including first-worker initialization; shutdown cancels its wait.
  Timeout, malformed output or a mismatched snapshot terminates and reaps the
  child before a later cycle can start a replacement. Entry validation still
  checks the current quote, FX, account, formula, evidence and release gates.
  The engine invokes candidate analysis only while flat; holdings use market
  reads and never wait for inference. The child inherits the executor's OS identity
  and filesystem access. Live uses `[autonomous_worker.rs:serve_external]` under
  a separate identity, with a private `0660` Unix socket and bounded I/O.
  That I/O deadline does not preempt the external server's synchronous analysis
  (`[autonomous_worker.rs:serve_external]`).
- `[main.rs:HELP]` distinguishes bounded foundation probes from the explicitly
  configured autonomous demo/live execution and read-only shadow paths.

## Side Effects

`[weekly.rs:run]` reads private lab credentials and invokes offline helpers;
ledger/release commands use their narrowly selected PostgreSQL roles. Formula
bridges compute in process without database, market-provider or broker access.
Owner approval records approval evidence; the separate releaser must advance
the state and satisfy twenty real sessions plus fresh readiness before ACTIVE.
Weekly store lease release is attempted after `run_weekly` returns, including its
error path (`[weekly.rs:run]`).

`[financial.rs:run]` status inspects the central credential store without network
access. Every non-status financial command connects as the analyst and
synchronizes the registry before its specific work.

Explicit `sentiment score` reads files/time and posts permitted normalized text to
the configured FinBERT service. It prints JSON and makes no database or broker writes.

OpenCode chat performs model HTTP calls and analyst-role session writes. Sessions
can be listed/read/closed without model credentials; closure retains audit history.

News commands can perform read-only provider HTTPS calls and analyst-role PostgreSQL
writes/reads/content purges. `status` and offline `parse-feed` need neither credentials
nor database access; input file and response sizes are bounded.
`[news.rs:sync_registry]` can write registry state before `news read` fetches items
and before `export-x` reads evidence (`[news.rs:run]`).

`[telegram/mod.rs:run]` discover/run resolve credentials and contact Telegram.
Run also locks/writes local inbox/offset/outbox state, sends authorized replies
and can read broker information. Check only reads/validates local configuration.

Reads files/environment/time/process ID and prints JSON or errors. The memory demo
mutates memory. The database demo creates a timestamp-named paper account and durable
fixture records. Migration applies database DDL through the dedicated role.
The Trading 212 snapshot command performs authenticated HTTPS GETs without database writes.

The demo/paper and live executor paths in `[autonomous.rs:run]` load deployment
configuration and credentials, acquire a
120-second executor lease, read broker state and configured market services,
and persist executor state/audit/outbox records. The analyst child loads model
artifacts, reads input/universe services and persists analyst research.
It can submit Day limit buys, GTC protective stops, market exits and cancellations
through the configured demo/live broker. A separate analyst retention connection
purges every 50 seconds with a 10-second statement timeout, including during analysis.
Linux socket connections verify the peer UID; all socket ancestors must be trusted
directories without symlinks. Replies bind the actual persisted native model hash
to the live deployment release. Deployment must also run
retention cleanup when the runtime/child is absent. Recovery reads exact broker facts and updates the journal
without submitting an order or requiring analyst credentials/model loading.

Continuous demo/paper and live execution prints cycle failures, renews its fence and retries
after the configured interval; renewal failure terminates it. `--once` attempts
one cycle and propagates cycle errors. Argument and setup validation precedes
lease acquisition; runtime cleanup attempts lease release and stops the child. Shutdown's
`broker_protection_retained=true` field is unconditional and does not prove that
a newly submitted entry was filled or protected before a once-mode stop.

`[autonomous_shadow.rs:run]` instead writes immutable local cycle/proposal files
and uses only a broker information reader. Cycle errors are recorded as
`cycle.result.status="blocked"`; when persistence/output succeeds, `--once`
returns success with `status="shadow_stopped"`. Automation must inspect the cycle
result to distinguish a blocked shadow cycle. Shadow never submits/cancels orders
or writes an executor journal; setup, persistence and output errors still propagate.
An immutable account/session slot is reserved before recording the proposal;
failure after that reservation can consume the session's slot
(`[autonomous_shadow.rs:run]`).

## Verification

Run `cargo test -p trady-cli --test weekly_cli` for arithmetic CLI and malformed
input gates, plus the root disposable PostgreSQL checks for durable lifecycle
semantics. No proof or historical performance is inferred from these fixtures.

Run `cargo test -p trady-cli --locked`; the disposable database script also runs all
three role-specific startup probes and the PostgreSQL demonstration.
Run `cargo test --workspace --locked` from the repository root for workspace verification.

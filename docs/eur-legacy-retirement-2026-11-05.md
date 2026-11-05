# Legacy runtime retirement, 5 November 2026

The owner explicitly requested source cleanup after preparing the bounded EUR
evidence continuation. This supersedes the earlier plan to defer all legacy
retirement until EUR acceptance. It grants no activation or missing-evidence
waiver. The native partial-entry, positive paper, independent acceptance and
exact-hash owner gates still apply.

The old IEX30/Alpaca CLI runtime, private analyst/IPC worker, local SHADOW runner,
Alpaca data client, watchdog CLI, account/cold-start CLI and synthetic
`demo-entry` command are removed. Their dedicated integration tests and obsolete
private-worker dependencies are retired with them. `autonomous check|run`
remains as a thin EUR-only entrypoint because the current EUR service uses it.
Legacy configuration shapes and old actions fail before credentials or providers.

The unused `AutomaticInputs`/`MarketInputs` pipeline and trading-input config
validator are removed. Its five research wire documents remain in
[input_documents.rs](../crates/trady-adapters/src/input_documents.rs), consumed
by the reviewed input service and offline model tools. The reserved
`alpaca_iex30` provider now fails before HTTP; its fabricated ISIN/classification
membership factory is removed. Shared domain IEX/index/currency guards remain.

The generic preparation helper now emits only disabled EUR SHADOW configuration.
Old analyst/executor unit templates are [archived as text](./archive/legacy-runtime/README.md).
No historical database migration, journal record, failed/native broker evidence
or sealed release is deleted or relabelled.

Shared application/storage autonomous contracts, `EurExecutionPort`, native
broker transport, exact-ID recovery and foundation role/database diagnostics
remain because current EUR or retained tests import them. Canonical IEX/formula
JSON stays at its existing paths for hash-sensitive mathematical replay and
integrity checks. Yahoo history/news, Alpha research, the separate Yahoo wire
diagnostic, IBKR sources and weekly/model libraries remain. Historical guides
describe their recorded period; they do not make retired commands executable.

The working tree and the owner's existing uncommitted evidence update were
checkpointed before edits. The source snapshot and dependency assessment are
retained privately under `var/eur-legacy-cleanup-20261105`. Current validation,
review and SHADOW deployment status are recorded in the
[cleanup evidence](./evidence/eur-legacy-cleanup-2026-11-05.json).

The authorized next-session continuation must retain its one-run self-disable
guard, observation window, single EUR 2,500 entry-attempt cap and complete
evidence/approval gates. Its reviewed source/binary references must be rebound
to any rebuilt deployment before it runs.

# Workflow remediation — 2026-10-31

## Outcome

Collection/parser defects are repaired and the dedicated diagnostic database's
nine Trady login passwords are rotated. The complete trading decision workflow
has **not passed**. Its deployment approvals remain unavailable; the owner chose
a pending deployment package. Current source freshness and provider quotas also
prevent a fresh production sentiment/decision demonstration.

The original [live diagnostic](./live-demo-run-2026-10-31.md) describes the earlier
run. This document records the subsequent repairs without changing that history.

## Pending deployment

[autonomous.pending.json](../config/autonomous.pending.json) is a native runtime
configuration with `enabled=false`, `mode=shadow`, `pause_entries=true` and a
separate `shadow_pending_deployment` account namespace. Its demo broker binding
comes from the earlier read-only diagnostic; it is not new broker capability or
release evidence.

[deployment.pending.json](../config/deployment.pending.json) is an operator
handoff manifest, not a runtime input or an approval. It lists the exact missing
input service, source/reviewer registries, native model artifacts, formula
authority, universe, skills and capability evidence. Credentials are references
only. No source rights, model approval or formula evidence was invented.

The native configuration check returns these expected blockers:

```text
runtime_disabled
approved_model_registry_empty
approved_input_service_unconfigured
universe_source_disabled
approved_skill_hashes_missing
approved_formula_unavailable
```

Run from the repository root:

```sh
target/debug/trady autonomous check --config config/autonomous.pending.json
```

This succeeds as a configuration inspection with `entry_authorized=false` and
zero approved models. It is not a successful trading cycle. The inputs-service
example now reports `approved input source registry is empty` rather than
mislabeling that omission as an invalid bounds configuration.

## X collection

The adapter now requests `tweet.fields`, including author, reference and edit
metadata, and uses the documented expansion paths. Omitted optional reference
metadata on an original post maps to an empty reference list. Explicit `null`
remains unknown, and enabled repost exclusion rejects actual repost references
and unknown metadata. Replies and quotes retain their reference context checks.

The collector no longer turns local error cadence into a provider retry deadline.
A forced repair also recognizes the exact legacy synthetic deadline for
`x_repost_exclusion_not_verified`; provider deadlines, daily request reservations
and the durable X monetary ledger remain enforced.

The live retry reached the existing **daily source request cap** and made no new
X requests. Its durable retry is `2026-11-01T00:00:00Z`. No cap was raised or
accounting reset. Positive provider acceptance of the repaired eight-post batch
therefore remains unverified; realistic payload regression tests pass.

## Sentiment inputs

RSS channel language and Atom `xml:lang` now reach source records, including
inheritance, entry/content overrides and explicit resets. Mixed-language XHTML
is kept unknown. Regional English tags such as `en-US` and `en-GB` are accepted
by sentiment processing; missing/non-English metadata, missing reference context
and evidence older than 24 hours remain blocked before inference.

Language changes now affect source content hashes, and the parser version is
`trady-news-2`. Existing stored records are not relabeled or given new publication
times. The old Fed items remain too old. A fresh public Fed-feed diagnostic also
encountered an HTTP error, so this follow-up does not claim current production
sentiment readiness. The original direct FinBERT inference success remains a
separate service diagnostic.

## GDELT

A direct transport probe took about nine seconds to establish TLS, exceeding
the previous five-second connection bound. GDELT now has bounded 15-second
connection and 30-second total request deadlines. Other hosts retain their
existing bounds. Automatic retries, HTTPS/public-DNS checks and body limits stay
enforced; timeout/connection diagnostics expose safe reason codes only.

The repaired native health probe reached HTTP 200 in about 15.2 seconds but did
not receive a valid article response. Subsequent public probes returned HTTP 429
with GDELT's throttle notice. That known notice is now recognized even when
delivered with HTTP 200, and collection retains a provider backoff. Transport is
no longer the only explanation for failure; **GDELT article collection is still
not verified healthy**.

## Database credential rotation

Scope: the dedicated `trady-live-demo-db-20261031` PostgreSQL container and the
nine DSN passwords in the ignored central credential store. The separate
Paperless database was not modified.

[rotate-postgres-credentials.py](../scripts/rotate-postgres-credentials.py) uses
psql's private password prompt over stdin, stages a private atomic store update,
checks for concurrent edits, restores prior verifiers on pre-commit failure and
verifies both new-password acceptance and old-password rejection. It terminates
existing Trady-role sessions after successful verification. Secrets are not
included in command arguments, reports or logs.

All nine roles were rotated and verified against SCRAM authentication; the
private store mode is `0600`. DSN endpoints were preserved: diagnostic host
connections still explicitly select the container's published loopback port,
rather than claiming that central endpoint settings are a deployed service.
This verification does not establish rotation on any other server that may have
reused an exposed password.

The [development authentication fixture](../config/dev-pg_hba.conf) now requires
SCRAM for Trady logins. The disposable test runner generates passwords and its
integration tests consume the corresponding DSNs.

## Verification

- `cargo test --workspace --locked`: 516 tests passed; 21 database tests were
  skipped in that run and passed separately below.
- `python3 scripts/test-postgres.py`: all 21 database integration tests and
  service/CLI scenarios passed with generated passwords and SCRAM.
- Final focused news-adapter suite: 27 passed, including original X posts,
  explicit-null/repost rejection, RSS/Atom language, language-sensitive hashes
  and GDELT plain-text throttling.
- Collector recovery: five application tests passed, including provider backoff
  and durable daily-cap enforcement.
- Sentiment: four tests passed, including regional English and stale/non-English
  rejection before provider calls.
- API health: seven tests passed, including safe error redaction and HTTP-200
  throttle recognition.
- Credential rotation: four unit tests passed for success/private storage,
  pre-commit rollback, concurrent edits and rejection of trust authentication.
- Pending configuration: native check passed with the six blockers above.
- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.

Full-tree EGM bootstrapping is complete across **51 code-directory contracts**:
49 covered by the three fresh-context workers, plus the repository and Rust
workspace blueprints. The test-only `crates/trady-adapters/src/telegram/spool`
directory is excluded from the code scope; its existing README is preserved.
All worker reports are reconciled, and their recorded tool/read incidents are
resolved.

The final pass fully reread all 51 contracts and three additional guides,
checked required sections, Markdown tables/fences, Cargo package targets and
1,553 source-file citations, and found no missing cited files or unresolved
contract placeholders. The final adapter source/contract hashes match its
completed review. `python3 scripts/verify-assets.py` passed for 11 canonical
JSON assets, 52 skill records, 188 Markdown files and 1,378 local links;
`git diff --check` also passed. These documentation checks do not establish a
successful live trading decision workflow.

The owner-requested [EGM drift addendum](../2026-10-31_DRIFT_REPORT.md#egm-contract-bootstrap-drift-addendum--2026-10-31)
reviews the 48 modified README contracts. It records public-interface coverage
gaps in four contracts: input-service limits/payload helpers, application
fingerprint/invalidation helpers, supporting intelligence types, and storage
gateway/release APIs. The implementations exist; the missing coverage is in the
documentation. The other 44 reviewed contracts have no detected drift. Earlier
financial-data findings in that report are preserved.

## Implementation references

Wire-field behavior was checked against the current
[X field documentation](https://docs.x.com/x-api/fundamentals/fields) and
[expansion documentation](https://docs.x.com/x-api/fundamentals/expansions).
Bounded request/error handling follows the
[Reqwest client API](https://docs.rs/reqwest/0.12.28/reqwest/blocking/struct.ClientBuilder.html).
The private password operation uses PostgreSQL's documented
[psql password command](https://www.postgresql.org/docs/18/app-psql.html#APP-PSQL-META-COMMAND-PASSWORD).

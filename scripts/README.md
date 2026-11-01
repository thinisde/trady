# Foundation verification scripts

## Overview

Repeatable asset and database checks for the offline milestone, plus an explicit
operational PostgreSQL credential-rotation utility.

## Interface

- [verify-assets.py](./verify-assets.py): `verify_schemas`, `verify_catalog` and
  `verify_markdown`, `heading_anchors`; compares canonical JSON copies, checks the
  52 skill records including 11 sector/25 group mappings, and verifies local Markdown
  links, heading anchors and fences.
- [test-postgres.py](./test-postgres.py): `main`; provisions a new disposable
  PostgreSQL 18.3 container and runs ignored storage integration tests plus the
  CLI `opencode`, `autonomous_cli` and `telegram_jobs_cli` integration targets
  and service probes/demo.
  The latter verifies the actual analyst child with restricted PostgreSQL access,
  no inherited credentials and absent formula authority; it performs no broker calls.
  `[test-postgres.py:run]` runs checked text subprocesses; `[test-postgres.py:sql]`
  executes SQL through `docker exec -i ... psql -v ON_ERROR_STOP=1`, defaulting to
  database `postgres` and discarding command stdout.
  `[test-postgres.py:rotation_check]` checks and applies credential rotation only
  against that disposable container and verifies the saved endpoints through
  subsequent analyst/executor/gateway logins.
- [requirements-dev.txt](./requirements-dev.txt): pinned PyYAML for asset verification.
- [rotate-postgres-credentials.py](./rotate-postgres-credentials.py): `main` accepts
  required `--container`, optional `--credentials FILE` (default
  `config/credentials.local.json` under the resolved repository root), and opt-in `--apply`.
  `[rotate-postgres-credentials.py:rotate]` checks a nine-role rotation plan by
  default and applies it only with `--apply`; the CLI prints structured status.
  `[rotate-postgres-credentials.py:main]` reports `rotation_plan_checked` or
  `rotated_and_verified`; endpoint synchronization reports `endpoint_plan_checked`
  or `endpoints_synchronized`. Caught operational failures emit suppressed-detail
  JSON with `rotation_failed` or `endpoint_sync_failed` and exit 1. Argument
  parsing failures use stderr/exit 2. `--roles` requires `--sync-endpoints`.

## Invariants

`[test-postgres.py:main]` creates its own container, publishes only on loopback,
uses an ephemeral data directory and attempts container removal in `finally`
after a successful start. Removal is unchecked and can fail; `--rm` and this
cleanup attempt do not guarantee removal.
Its development authentication requires SCRAM for the nine Trady logins;
`[test-postgres.py:main]` generates private passwords through psql prompts and
supplies those passwords in its disposable DSNs. The remaining development trust
rules are limited to that disposable server.
`[verify-assets.py:verify_catalog]` checks name/version agreement and rejects
catalog traversal/symlinks; it does not approve a bundle for runtime use.

`[verify-assets.py:verify_schemas]` compares parsed JSON with the normative fenced
examples and fixtures. `[verify-assets.py:verify_markdown]` checks root README/index
plus Markdown under docs, skills, crates, schemas, migrations, scripts, config,
lab and services. It skips URI-scheme links and fenced examples, checks local targets exist and
checks Markdown fragments with `[verify-assets.py:heading_anchors]`. These are
equality and regex/assertion checks, without a complete schema or Markdown parser.
The assertion-based checks require Python assertions to remain enabled
(`[verify-assets.py:verify_schemas]`, `[verify-assets.py:verify_catalog]`,
`[verify-assets.py:verify_markdown]`).

`[test-postgres.py:main]` requires a Docker daemon, Cargo/Rust, local libpq 18 or
later and the repository's development authentication config. It mounts that config read-only, uses tmpfs
for PostgreSQL data and allocates an ephemeral loopback port. It polls local and
TCP readiness at most 150 times, sleeping 0.2 seconds after unsuccessful polls;
the subprocesses themselves have no timeout. The runner executes role provisioning
and creates `trady` plus a separate `paperless_fixture` database. It provisions
roles through `migrations/0000_roles.sql` and `migrations/0000_lab_roles.sql`.
It sets `TRADY_TEST_ISOLATED=1`, `TRADY_TEST_PORT` and the
migrator/executor/analyst/gateway DSNs to the new container's Trady database,
plus `TRADY_LAB_DSN`, `TRADY_VERIFIER_DSN`, `TRADY_SHADOW_DSN`,
`TRADY_RELEASE_DSN` (the `trady_releaser` role) and `TRADY_RELEASE_OWNER_DSN`.
It sets `RUST_TEST_THREADS=1` because expiry cleanup intentionally sweeps all
scopes in the shared disposable database. Concurrency regressions still
interleave their own separate connections explicitly; unrelated tests cannot
purge their deliberately expired fixtures during an assertion.

`[verify-assets.py:ROOT]` and `[test-postgres.py:ROOT]` resolve the repository root
from the script location. `[test-postgres.py:NAME]` includes the runner process ID
in the container name.

`[rotate-postgres-credentials.py:rotate]` requires a non-symlink credential file
with all nine role-matched keyword DSNs and a `trady` database, plus existing
nonsuperuser login roles without database/role creation, replication or RLS bypass
privileges. `[rotate-postgres-credentials.py:read_private]` opens without following
the final symlink and requires an owner-matched regular file with exact mode `0600`.
`[rotate-postgres-credentials.py:parse_dsn]` supports a narrow single-line form of
libpq single-quote/backslash syntax, rejects duplicate fields and non-ASCII
whitespace, and retains the password span for replacement.
`[rotate-postgres-credentials.py:sync_endpoints]`, selected with `--sync-endpoints`,
checks the selected container's published loopback port and verifies each saved
role/password against that exact server before updating DSN port values. Plan mode
is the default; `--apply` writes an atomic private credential-store update and
preserves passwords and unrelated credentials. `--roles ROLE...` explicitly scopes
this endpoint operation and requires `trady_analyst` for the shared server lock.
Roles outside that selection retain their existing database binding. Parsing and
authentication failures suppress connection-string and exception details.
Scoped synchronization still requires `validate_hba` coverage for all nine roles
in `ROLES` (`[rotate-postgres-credentials.py:sync_endpoints]`,
`[rotate-postgres-credentials.py:validate_hba]`).
`[rotate-postgres-credentials.py:container_target]` requires a local Unix-socket
Docker daemon and pins the inspected container ID.
`[rotate-postgres-credentials.py:validate_endpoints]` requires explicit literal
loopback hosts and ports published by that container's PostgreSQL listener;
remote hosts, service indirection, mismatched hostaddr overrides and public port
bindings are rejected. DNS names and Unix-socket DSNs are outside this helper's scope.
`[rotate-postgres-credentials.py:validate_hba]` checks every role and rejects
potentially matching permissive rules before covering SCRAM host rules for both
IP families. Only explicit `trady` database and role names establish coverage;
ambiguous `all` tokens, group and regex matches are treated conservatively. An unproven safe
configuration is rejected. The server must generate SCRAM verifiers, and all nine
current passwords must succeed while random passwords fail before rotation.
`[rotate-postgres-credentials.py:saved_connection]` requires local `libpq.so.5`
version 18 or later and authenticates through each saved DSN in the caller's
network namespace with `require_auth=scram-sha-256`. `same_server` verifies a
random live session marker, database, role and backend PID through the selected
container's administrative connection. `admin` clears the container environment
and explicitly selects socket `/var/run/postgresql`, port `5432`, database/user
`postgres`; other administrative layouts are outside this helper's scope.
`rotation_lock` holds a persistent owned `0600` sibling `.rotation.lock` file's
exclusive lock across the operation. `database_lock` holds one server advisory
lock across different credential paths. Its `AdministrativeSession` disables
idle-session timeout at startup and executes every protected catalog, password,
commit-verification and rollback command on that same connection. Losing the
connection stops further commands; restoration cannot switch to a new session
and overwrite another rotation. Session commands have a thirty-second timeout
and a 256 KiB output limit. Competing helper invocations fail before
password mutation. The credential text/inode is rechecked after acquiring the
server lock and immediately before replacement; protection against editors that
ignore these locks is best effort, with a remaining check/replace race.
Saved-verifier restoration is attempted on failures before file replacement and
can itself fail; every role's restoration is attempted using SQL on stdin, with
verifiers absent from subprocess arguments. Later failures leave the committed rotation in place. Successful rotation
requires every new password to authenticate and every old password to fail.

## Side Effects

Asset verification reads files. Database verification downloads an image when
needed, starts/removes a container, creates fixture databases/roles, invokes Cargo,
and writes disposable PostgreSQL state. The runner supplies disposable Trady DSNs;
inherited Docker/Cargo settings govern its subprocesses (`[test-postgres.py:main]`).
Cargo can also fetch dependencies and write build/cache files. Asset verification
prints counts without modifying the checked assets (`[verify-assets.py:__main__]`).

`[rotate-postgres-credentials.py:rotate]` reads the selected credential JSON and
queries Docker metadata, role catalogs, HBA rules and current authentication even
in plan mode. It creates/reuses the stable file lock and acquires the server lock
for plans too; keep the lock file in place for cooperating writers.
Opt-in application changes nine role passwords, writes/fsyncs a staged 0600
file, replaces the credential file, fsyncs its directory, reloads server
configuration, checks old/new authentication and terminates
matching role sessions after verification. `[rotate-postgres-credentials.py:invoke]`
uses captured Docker subprocesses with a thirty-second timeout. Protected rotation
commands use the persistent lock-owning Docker/psql session, with stdin-only SQL.

`[rotate-postgres-credentials.py:sync_endpoints]` reads container metadata and
authenticates under the same file/server locks. Its apply mode changes only selected
DSN ports in the private store; it changes no server passwords and ends no sessions.

## Verification

Run `cargo test --workspace --locked` from the repository root for Rust verification.
The database scenario is opt-in through `[test-postgres.py:main]` below.

From the repository root, run `python3 scripts/verify-assets.py` and
`python3 scripts/test-postgres.py`. Install the pinned Python development dependency
in a virtual environment with `python3 -m pip install -r scripts/requirements-dev.txt`.
Run `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` for rotation
rollback, concurrent-edit, SCRAM and private-store checks without a database.
The disposable runner also exercises plan/apply rotation for all nine roles and
uses the resulting saved DSNs for actual published-endpoint service probes.

`[test-postgres.py:main]` runs these subprocesses from the repository root with
its disposable-database environment:

- `cargo test --locked -p trady-storage --tests -- --ignored --nocapture`
- `cargo test --locked -p trady-cli --test opencode -- --ignored --nocapture`
- `cargo test --locked -p trady-cli --test autonomous_cli -- --ignored --nocapture`
- `cargo test --locked -p trady-cli --test telegram_jobs_cli -- --ignored --nocapture`
- `cargo run --locked --quiet --bin trady -- analyst --once`
- `cargo run --locked --quiet --bin trady -- executor --once`
- `cargo run --locked --quiet --bin trady -- gateway --once`
- `cargo run --locked --quiet --bin trady -- demo-postgres`
- `cargo run --locked --quiet --bin trady -- news collect --once`
- `cargo run --locked --quiet --bin trady -- news read --limit 5 --for-model`

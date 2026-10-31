# Foundation verification scripts

## Overview

Repeatable asset and database checks for the offline milestone.

## Interface

- [verify-assets.py](./verify-assets.py): `verify_schemas`, `verify_catalog` and
  `verify_markdown`, `heading_anchors`; compares canonical JSON copies, checks the
  52 skill records including 11 sector/25 group mappings, and verifies local Markdown
  links, heading anchors and fences.
- [test-postgres.py](./test-postgres.py): `main`; provisions a new disposable
  PostgreSQL 18.3 container and runs ignored storage integration tests plus the
  CLI `opencode` integration target and service probes/demo.
  `[test-postgres.py:run]` runs checked text subprocesses; `[test-postgres.py:sql]`
  executes SQL through `docker exec -i ... psql -v ON_ERROR_STOP=1`, defaulting to
  database `postgres` and discarding command stdout.
- [requirements-dev.txt](./requirements-dev.txt): pinned PyYAML for asset verification.

## Invariants

`[test-postgres.py:main]` creates its own container, publishes only on loopback,
uses an ephemeral data directory and attempts container removal in `finally`
after a successful start. Removal is unchecked and can fail; `--rm` and this
cleanup attempt do not guarantee removal.
Its development trust authentication is limited to that disposable server.
`[verify-assets.py:verify_catalog]` checks name/version agreement and rejects
catalog traversal/symlinks; it does not approve a bundle for runtime use.

`[verify-assets.py:verify_schemas]` compares parsed JSON with the normative fenced
examples and fixtures. `[verify-assets.py:verify_markdown]` checks root README/index
plus Markdown under docs, skills, crates, schemas, migrations, scripts and config;
it skips URI-scheme links and fenced examples, checks local targets exist and
checks Markdown fragments with `[verify-assets.py:heading_anchors]`. These are
equality and regex/assertion checks, without a complete schema or Markdown parser.

`[test-postgres.py:main]` requires a Docker daemon, Cargo/Rust and the repository's
development authentication config. It mounts that config read-only, uses tmpfs
for PostgreSQL data and allocates an ephemeral loopback port. It polls local and
TCP readiness at most 150 times, sleeping 0.2 seconds after unsuccessful polls;
the subprocesses themselves have no timeout. The runner executes role provisioning
and creates `trady` plus a separate `paperless_fixture` database. It sets
`TRADY_TEST_ISOLATED=1`, `TRADY_TEST_PORT` and the migrator/executor/analyst/gateway
DSNs to the new container's Trady database.

`[verify-assets.py:ROOT]` and `[test-postgres.py:ROOT]` resolve the repository root
from the script location. `[test-postgres.py:NAME]` includes the runner process ID
in the container name.

## Side Effects

Asset verification reads files. Database verification downloads an image when
needed, starts/removes a container, creates fixture databases/roles, invokes Cargo,
and writes disposable PostgreSQL state. The runner supplies disposable Trady DSNs;
inherited Docker/Cargo settings govern its subprocesses (`[test-postgres.py:main]`).
Cargo can also fetch dependencies and write build/cache files. Asset verification
prints counts without modifying the checked assets (`[verify-assets.py:__main__]`).

## Verification

Run `cargo test --workspace` from the repository root for Rust verification.
The database scenario is opt-in through `[test-postgres.py:main]` below.

From the repository root, run `python3 scripts/verify-assets.py` and
`python3 scripts/test-postgres.py`. Install the pinned Python development dependency
in a virtual environment with `python3 -m pip install -r scripts/requirements-dev.txt`.

`[test-postgres.py:main]` runs these subprocesses from the repository root with
its disposable-database environment:

- `cargo test --locked -p trady-storage --tests -- --ignored --nocapture`
- `cargo test --locked -p trady-cli --test opencode -- --ignored --nocapture`
- `cargo run --locked --quiet --bin trady -- analyst --once`
- `cargo run --locked --quiet --bin trady -- executor --once`
- `cargo run --locked --quiet --bin trady -- gateway --once`
- `cargo run --locked --quiet --bin trady -- demo-postgres`
- `cargo run --locked --quiet --bin trady -- news collect --once`
- `cargo run --locked --quiet --bin trady -- news read --limit 5 --for-model`

# PostgreSQL credentials

## Overview

The nine DSNs are connection strings for separate database login roles.
Obtain the host/socket, port, database name and each role's password from the
administrator of your Trady PostgreSQL installation. Database provisioning is
described in the [operations runbook](../../../docs/v1/operations.md) and
[migration guide](../../../migrations/README.md).

## Interface

The named DSNs below select separate runtime and offline-lab database identities.
`[../../../scripts/rotate-postgres-credentials.py:ROLES]` enumerates the nine
credential-to-login bindings used by the scoped rotation helper.

For a new installation, provision the roles first. An authorized administrator
can then set each password inside an administrative `psql` session:

```text
\password trady_analyst
\password trady_executor
\password trady_gateway
\password trady_migrator
```

These commands prompt for the passwords instead of putting them in SQL command
text; see PostgreSQL's
[official password-command documentation](https://www.postgresql.org/docs/current/app-psql.html#APP-PSQL-META-COMMAND-PASSWORD).
Store each resulting connection string in `config/credentials.local.json` using
the [central store setup](../README.md).

For a local Trady container, the scoped
[rotation helper](../../../scripts/rotate-postgres-credentials.py) first checks a
plan, then explicitly applies it:

```sh
python3 scripts/rotate-postgres-credentials.py --container EXPLICIT_TRADY_CONTAINER
python3 scripts/rotate-postgres-credentials.py --container EXPLICIT_TRADY_CONTAINER --apply
```

It requires an owner-matched `0600` credential file, the nine restricted Trady
roles and SCRAM authentication/encryption. Every DSN must use a literal loopback
host and the selected local container's published PostgreSQL port; DNS names,
Unix-socket DSNs, remote Docker daemons and public port bindings are rejected.
It requires local `libpq.so.5` version 18 or later, pins the container ID, checks
covering SCRAM rules with explicit database/role names and verifies current
authentication through every saved endpoint before changing passwords. A random
live session marker confirms those connections reach the selected server from
the caller's network namespace. Administrative psql commands clear the container
environment and explicitly use socket `/var/run/postgresql`, port `5432` and
database/user `postgres`. This helper supports that standard administrative
layout and preserves DSN endpoints.
Applying it rotates all nine passwords through private psql prompts,
atomically replaces the private store with mode `0600`, verifies old/new password
behavior and terminates existing Trady-role sessions. The helper does not operate
on remote/shared servers through their DSNs. The disposable database runner tests
plan/apply rotation and actual logins through the saved published endpoints. The
earlier
[diagnostic rotation record](../../../docs/workflow-remediation-2026-10-31.md#database-credential-rotation)
records the historical behavior before this endpoint hardening.

The examples below contain placeholders. Replace the port/password for your
installation. Runtime role checks require the database `trady`. Explicit loopback
hosts and Unix sockets can use plaintext. Remote hosts or non-loopback `hostaddr`
overrides require `sslmode=require`; Trady verifies the certificate chain and
server hostname with the platform trust store and requires TLS 1.2 or later.
Install your database CA in that trust store and use its matching server name.
For example, a remote DSN starts with
`host=db.example.com sslmode=require dbname=trady` before its role and credentials.

## TRADY_ANALYST_DSN

Use the `trady_analyst` login for research and durable model conversations:

```text
host=127.0.0.1 port=5432 dbname=trady user=trady_analyst password='REPLACE_LOCALLY'
```

## TRADY_EXECUTOR_DSN

Use the `trady_executor` login for execution persistence and the database demo:

```text
host=127.0.0.1 port=5432 dbname=trady user=trady_executor password='REPLACE_LOCALLY'
```

## TRADY_GATEWAY_DSN

Use the `trady_gateway` login for the gateway's restricted database access:

```text
host=127.0.0.1 port=5432 dbname=trady user=trady_gateway password='REPLACE_LOCALLY'
```

## TRADY_MIGRATOR_DSN

Use the `trady_migrator` login for explicit migrations:

```text
host=127.0.0.1 port=5432 dbname=trady user=trady_migrator password='REPLACE_LOCALLY'
```

The migrator is a member of the non-login `trady_owner` role and changes role
during migrations. Runtime logins remain separate; using an administrator or a
different application's connection string does not satisfy Trady's role checks.

Keep quoting/escaping appropriate to PostgreSQL connection strings when a password
contains special characters, then escape the complete value as a JSON string when
editing the private store. `credentials status` checks availability only; database
commands perform actual connection and role validation.

## Offline lab and release DSNs

An administrator separately provisions the roles in
[0000_lab_roles.sql](../../../migrations/0000_lab_roles.sql) before migration 6.
Set their passwords through the same private `psql` password prompt, and store
each connection string under its exact central name:

| Credential | PostgreSQL login | Purpose |
| --- | --- | --- |
| `TRADY_LAB_DSN` | `trady_lab` | Frozen weekly runs and cumulative trial/holdout ledgers |
| `TRADY_VERIFIER_DSN` | `trady_verifier` | Independent content-bound verification evidence |
| `TRADY_SHADOW_DSN` | `trady_shadow` | Genuine prospective session evidence |
| `TRADY_RELEASE_OWNER_DSN` | `trady_release_owner` | Explicit scoped owner approval |
| `TRADY_RELEASE_DSN` | `trady_releaser` | Compare-and-swap release transitions |

These use database `trady` and the same transport checks above. The supplied lab
service permits only Unix sockets. Give each identity only its own credential;
the lab cannot receive verifier, owner, releaser or broker secrets. The
[runtime deployment contract](../../../services/runtime/README.md) describes the
role and process boundaries. Provisioning credentials does not approve evidence,
advance a release or enable trading.

## Invariants

`[../../../scripts/rotate-postgres-credentials.py:rotate]` requires matching
restricted login roles in database `trady`, preserves DSN endpoints and refuses
to apply without SCRAM authentication/encryption. Its staged file uses mode
`0600`. A persistent sibling `.rotation.lock` file serializes cooperating writers
to that path, and a server advisory lock serializes rotations across different
credential paths. Both locks cover preflight, mutation, commit, verification and
rollback; competing invocations fail before password changes. The server-lock connection disables
idle-session timeout at startup and performs all protected commands itself;
connection loss stops later password changes and rollback. It never reconnects
to restore a stale verifier snapshot over another rotation. Do not remove the
lock file while a helper may be running. Text/inode rechecks detect some external
edits and trigger verifier restoration before commit; editors ignoring the locks
can still race with the final check/replace. Restoration attempts every role and
can itself fail. Failures after replacement retain the committed passwords/store.
Password prompts and verifier restoration use stdin rather than secret-bearing
subprocess arguments. The narrow single-line DSN parser rejects non-ASCII
whitespace and duplicate fields.
The runtime and lab identities retain their separate authorization boundaries.

## Side Effects

Reading this guide has none. Checking a rotation plan creates/reuses the private
lock file, opens database connections and acquires the server advisory lock.
Explicitly applying
`[../../../scripts/rotate-postgres-credentials.py:rotate]` changes the nine role
passwords and private credential store, fsyncs the staged file and parent
directory, reloads authentication configuration,
performs old/new login checks and terminates sessions for those Trady roles.

## Verification

Run `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` for the rotation
failure-path checks, and `python3 scripts/test-postgres.py` for isolated SCRAM
database integration. The helper's default invocation checks its plan without
rotating credentials; `--apply` performs the explicit scoped operation.

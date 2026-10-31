# PostgreSQL credentials

The nine DSNs are connection strings for separate database login roles.
Obtain the host/socket, port, database name and each role's password from the
administrator of your Trady PostgreSQL installation. Database provisioning is
described in the [operations runbook](../../../docs/v1/operations.md) and
[migration guide](../../../migrations/README.md).

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

# Archived intelligence audit queries

## Overview

[sample-queries.sql](./sample-queries.sql) inspects a disposable audit database
after synthetic imports. The accompanying [report](./report.md) and
[remediation notes](./remediation.md) record the dated audit context. These
inspection queries do not import evidence or perform invalidation.

## Interface

- `[sample-queries.sql:observations]` groups retained
  `research.intelligence_observations` by `feature` and returns observation counts.
- `[sample-queries.sql:retained_tax_rows]` counts rows with `feature = 'TAX'`.
- `[sample-queries.sql:content_erased]` returns `observation_id`, `feature` and
  `document IS NULL` for every visible observation, ordered by observation ID.
- `[sample-queries.sql:can_select]` calls `has_table_privilege` for
  `trady_analyst`, `trady_executor`, `trady_gateway`, `trady_shadow`, `trady_lab`
  and `trady_verifier`, reporting SELECT privilege on the observation table.
- `[sample-queries.sql:base_tables]` groups visible `information_schema.tables`
  base-table counts by `table_schema`, excluding exactly `pg_catalog` and
  `information_schema`. Other visible schemas can appear.

## Invariants

`[sample-queries.sql:BEGIN READ ONLY]` encloses all five SELECT statements in a
read-only transaction; `[sample-queries.sql:ROLLBACK]` ends it. The comments
expect nine persistable feature groups, zero retained TAX rows, erased content
where rights were revoked, and analyst-only SELECT among the listed roles.
The queries display counts and booleans without asserting those expectations or
raising a failure when results differ. They do not enforce a feature whitelist
or filter the erasure listing to revoked records.

## Side Effects

`[sample-queries.sql:SELECT]` reads database/catalog state and returns result
sets. BEGIN/ROLLBACK change session transaction state. The file contains no data,
schema, grant, import or invalidation mutations. Connecting and running it
requires a separately selected disposable database and suitable read privileges.

## Verification

From the repository root, `cargo test --workspace --locked` verifies the Rust
workspace. It does not execute this inspection file. Validate this contract's
source references and relative links; inspect actual query results only against
the disposable audit database after its separately prepared synthetic imports.

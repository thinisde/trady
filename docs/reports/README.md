# Verification reports

## Overview

Review records for checked repository contracts. Reports describe the source at
their recorded date and do not approve a financial deployment.

## Interface

- [Implementation completion](./2026-10-31_IMPLEMENTATION.md): concrete work areas,
  validation results and real activation/proof dependencies.
- [2026-10-31 README drift report](./2026-10-31_DRIFT_REPORT.md): comparisons of
  the README contracts updated during autonomous-runtime implementation.

## Invariants

Each report distinguishes remaining differences from verified contract behavior.
Runtime and deployment requirements remain in the [v1 specification](../v1/README.md).

## Side Effects

None at runtime. These records do not authorize broker calls or change configuration.

## Verification

Run `python3 scripts/verify-assets.py` from the repository root to check local links.
Recheck source when using a dated report as implementation evidence.

# Versioned JSON contracts

## Overview

Machine-readable copies of the canonical specification schemas and worked examples.

## Interface

- [LLM contracts v2](./llm-contracts.v2.json): `[llm-contracts.v2.json:$defs.ModelTurn]`,
  `ToolResults` and `ApplicationProposalEnvelope`.
- [Research contracts v1](./research-contracts.v1.json):
  `[research-contracts.v1.json:$defs.StoreArtifactArguments]`, result and calculator definitions.
- `fixtures/` contains nine worked JSON documents copied from the specification.
  No executable code or directory contract is inferred for those fixtures.
- [Application validator](../crates/trady-application/src/README.md) compiles the
  exact schemas with calendar and decimal format assertions.

## Invariants

`[llm-contracts.v2.json:$defs.Final]` separates model output from application-owned
metadata. Unknown fields are rejected at defined boundaries. The research document's
own schema version is distinct from LLM protocol version and proposal-body version.
`[../scripts/verify-assets.py:verify_schemas]` checks copies against the normative Markdown.
Schema validity alone does not prove evidence or readiness.

## Side Effects

None. These are static JSON assets embedded by the application validator.

## Verification

Run `python3 scripts/verify-assets.py` and `cargo test -p trady-application --locked`
from the repository root.

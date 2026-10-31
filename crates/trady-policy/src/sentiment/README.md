# Sentiment threshold policy

## Overview

Pure sentiment adjustment of an existing confidence score. The public module
remains `trady_policy::sentiment`; the
[v1 design](../../../../docs/v1/finbert/README.md#threshold-calculation-version-1)
defines the initial thresholds and their integration limits.

## Interface

- `[mod.rs:validate_probabilities]`: bounded decimal distributions summing to one
  within `0.000001`.
- `[mod.rs:sentiment_thresholds]`, `SentimentThresholds`: adjusted score, target
  ceiling, entry/review flags and reported sentiment context.

## Invariants

Checks policy validity, schema/model identity and revision shape; exact approved
revision binding belongs to callers. Assessment age is at most 900 seconds,
observations at most 86,400 seconds old and all evidence remains unexpired.
Item IDs and lineages are distinct, with valid hashes and probability values.

Maximum negative probability at least `0.60` subtracts 10 points with saturation,
requests review and prevents target increases. At least `0.80` vetoes entry;
otherwise adjusted points must meet the policy floor. Positive tone adds no
points. Mean net sentiment is reported without changing decisions.

## Side Effects

None. Returned calculations do not mutate a policy or plan and grant no order
authority. Callers explicitly apply the overlay and other readiness checks.

## Verification

Run `cargo test -p trady-policy --test sentiment --locked` from the repository root.

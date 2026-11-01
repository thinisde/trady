# Sentiment threshold policy

## Overview

Pure sentiment adjustment of an existing confidence score. The public module
remains `trady_policy::sentiment`; the
[v1 design](../../../../docs/v1/finbert/README.md#threshold-calculation-version-1)
defines the initial thresholds and their integration limits.
The supplied `[../lib.rs:ConfidenceScore]` is calculated from verified features;
its checked journal deserialization validates component sets, not evidence rights
or provenance. See the [policy source contract](../README.md).

## Interface

- `[mod.rs:validate_probabilities]`: bounded decimal distributions summing to one
  within `0.000001`, returning `Result<()>`.
- `[mod.rs:sentiment_thresholds]`, `SentimentThresholds`: adjusted score, target
  ceiling, entry/review flags and reported sentiment context.
  `[mod.rs:SentimentThresholds]` exposes `calculation_version` (1 in results
  returned by `sentiment_thresholds`),
  `base_score`, `adjusted_score`, `negative_probability_max`, `mean_net_sentiment`,
  `target_ceiling_fraction`, `entry_score_passes`, `target_increase_allowed` and
  `review_required`.
  `sentiment_thresholds` returns `Result<SentimentThresholds>` and propagates
  policy, evidence, arithmetic and target-ceiling errors.
  `[mod.rs:SentimentThresholds]` derives serialization without deserialization.
  `[mod.rs:sentiment_thresholds]` passes its caller-supplied
  `exceptional_verified` flag and the adjusted score to `target_ceiling`.

## Invariants

`[mod.rs:sentiment_thresholds]` checks policy validity, schema/model identity and
revision shape; exact approved revision binding belongs to callers. Assessment
age is at most 900 seconds,
observations at most 86,400 seconds old and all evidence remains unexpired.
Evidence count is 1–16, item IDs and lineages are nonempty and distinct, and
observations cannot follow `scored_at`. Text hashes have 64 ASCII hexadecimal
bytes; this checks shape without recomputing a digest. Expiry is strictly after
`now`, and probability values pass `[mod.rs:validate_probabilities]`.

`[mod.rs:sentiment_thresholds]` requires a model revision of exactly 40 ASCII
hexadecimal bytes. It reports the maximum negative probability across evidence
and the arithmetic mean of each item's positive-minus-negative probability;
checked arithmetic errors reject the calculation.

`[mod.rs:sentiment_thresholds]` uses caller-supplied `now`; call it again with
current time before using the result. Public `SentimentThresholds` fields allow
manual construction and do not enforce the function's returned values.

`[mod.rs:sentiment_thresholds]` subtracts 10 points with saturation when maximum
negative probability is at least `0.60`, requests review and prevents target
increases. At least `0.80` vetoes entry;
otherwise adjusted points must meet the policy floor. Positive tone adds no
points. Target increases additionally require adjusted score at least 70.
Mean net sentiment is reported without changing decisions.

## Side Effects

None. `[mod.rs:sentiment_thresholds]` calculations do not mutate a policy or plan and grant no order
authority. Callers explicitly apply the overlay and other readiness checks.
`[../holding.rs:activate_plan]` accepts the caller's checked adjusted score and
target-increase permission; it still evaluates old thresholds and preserves any
existing exit latch before considering an update.

## Verification

Run `cargo test -p trady-policy --test sentiment --locked` from the repository root.
Run `cargo test --workspace --locked` for workspace verification.

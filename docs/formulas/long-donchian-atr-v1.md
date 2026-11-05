# `long_donchian_atr_v1`

This is Trady's historical pre-registered IEX30 research baseline. The IEX30
runtime is [retired](../eur-legacy-retirement-2026-11-05.md); shared offline replay
remains. The 5 November code audit corrected opening-target exit precedence in
Rust and the Python evaluator. The frozen integrity manifest and evidence hashes
have not been reapproved for the changed implementation: the integrity verifier
deliberately reports a hash mismatch. Recompute and independently validate a new
package before using the corrected implementation as release evidence.

It is a long-only,
daily regular-session Donchian breakout with Wilder ATR risk distances. FinBERT
and model fusion are disabled for this authority.

The signal uses the highest high of the previous 20 completed bars. The current
bar is excluded. Wilder ATR(14) is seeded with the arithmetic mean of the first
14 true ranges, where true range includes the previous close. A signal requires
35 valid completed bars and `close > previous_20_bar_high`.

The next regular-session open is accepted when it is no more than `0.10 ATR`
above the signal close. A lower open uses the first executable price. The initial
risk unit is `1.50 ATR`; the target is `1.50R`, or `2.25 ATR`. Stop and target
touches in one bar resolve stop first when the open is between the thresholds.
An opening gap through either stop or target resolves at the opening price
before considering later intrabar touches. Unresolved positions exit at the close of the fifth
holding session. There is no trailing, averaging, pyramiding, leverage, or
reopening without a new signal event.

The frozen universe is the 30-symbol `IEX30` list in
[`config/iex30.universe.json`](../../config/iex30.universe.json). The authority
and its exact hashes are in
[`config/formulas/long-donchian-atr-v1.authority.json`](../../config/formulas/long-donchian-atr-v1.authority.json).
The runtime copy is embedded in
[`config/autonomous.iex30.demo.json`](../../config/autonomous.iex30.demo.json),
which remains disabled and pauses entries.

The Rust signal and plan implementation is
[`donchian.rs`](../../crates/trady-policy/src/limits/donchian.rs). The generic
limits calculator routes this authority through the ATR-based entry and target
path in [`limits/mod.rs`](../../crates/trady-policy/src/limits/mod.rs). A
deterministic model-free proposal is accepted without a prediction/fusion
payload when this authority declares fusion disabled.

The historical validation package used Alpaca Basic IEX daily bars from 2024-01-02
through 2026-11-02, a frozen 20 bps round-trip cost proxy, and these chronological
periods:

| Period | Closed trades | Mean net return | Compounded return | Maximum drawdown |
| --- | ---: | ---: | ---: | ---: |
| Development, 2024 | 190 | 0.5070% | 107.26% | 35.65% |
| Holdout, 2025 | 241 | 0.4633% | 108.65% | 57.14% |
| 2026 retrospective shadow proxy | 172 | 0.0163% | -34.87% | 65.02% |

That package's Rust/Python signal and unrounded price-plan parity report passed three
representative IEX30 cases with a `1e-25` numeric tolerance. Unit/property checks
cover the Wilder seed, look-ahead exclusion, warm-up, policy bounds, target
ordering, and disabled model fusion. The displayed results predate the exit
correction and are not results for the current implementation.

The authority is therefore registered as a **research baseline**, not a live
release. The current package still needs a prospective post-registration shadow
run and real paper-fill cost/slippage evidence. The negative 2026 retrospective
proxy is retained as a blocker rather than optimized away. Reports are generated
under `var/formula/long_donchian_atr_v1/` and are ignored runtime artifacts.

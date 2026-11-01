# Cold-start run record — 1 November 2026

The initializer was run with `config/cold-start.demo.json` against the Trading
212 demo endpoint. The private machine-readable bundle is under
`var/cold-start/cold_start_20261101T130046Z/`; its summary is immutable and
reports `entry_authorized:false`.

The provider discovery returned broker account `52211463`, base currency EUR,
available cash `5000.00 EUR`, no positions and no pending orders. The history
adapter returned no prior orders or fills. Per-fill fee and FX records are not
available before a genuine execution, so those fields remain unknown.

The independent Twelve Data import contains 13,610 OHLCV rows across the five
research instruments, with strictly increasing timestamps and no duplicate or
large missing-candle windows. Its existing research-only dataset and holdout
reports are pinned into the seed market-model manifest; they do not establish
deployment eligibility or execution performance.

The broker metadata call returned an invalid response during this run. The
universe artifact therefore contains no broker candidates and no invented
allowlist. The first-order command recorded an immutable intent with zero
broker mutations and returned `FIRST_ORDER_BLOCKED`.

OpenCode Go independently reviewed the runtime payloads. Its verdict was
`REQUEST_CHANGES`: reproducible validation and parity evidence still need to be
attached, and fills, fees, FX, slippage and prospective evidence remain
unknown. The review did not authorize an order. Later bounded retries were
unavailable/time-limited and were retained as unavailable rather than treated as
approval.

The five deployment blockers now have concrete cold-start artifacts and remain
explicitly bounded:

| Blocker | Current state |
| --- | --- |
| Reviewed input service | HTTPS boundary configured, but endpoint review and fresh quote/cost response are still pending. |
| Native market model | Seed manifest registered, research-only and shadow-only; holdout report says no measured advantage. |
| Formula authority | Deterministic seed formula, risk policy and reproducibility report registered; independent parity/release approval pending. |
| Dynamic universe | Broker intersection and five-name cap implemented; metadata response must be repaired before any candidate is tradable. |
| Runtime skills/capabilities | OpenCode review recorded; cash identity is verified, while quote, order-kind, protection, cancellation, partial-fill, fee and FX capabilities remain unknown. |

The existing autonomous recorder remains the next safe stage. No autonomous
entry is promoted until the metadata/input service issues are resolved, the
shadow sample is collected, and a new independent review accepts the bound
artifacts.

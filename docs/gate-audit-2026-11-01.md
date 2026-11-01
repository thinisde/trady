# Cold-start gate audit — 2026-11-01

Status: **NOT READY FOR FIRST COLD-START**. The shadow configuration remains
unchanged and no `first-order` command was run.

| Gate | Result | Evidence |
| --- | --- | --- |
| PG identity | Correct broker candidate identified, release still blocked | [`pg-crosswalk-review-2026-11-01.json`](./pg-crosswalk-review-2026-11-01.json): `PGC_US_EQ` is rejected as Peapack-Gladstone; the account-bound catalog contains exact `PG_US_EQ` → `US7427181091`/USD. Current membership effective date and licensed source hash are still missing. FMP returned 402 and Massive returned 403. |
| Input service | Blocked | `inputs-service check --config services/inputs/config.example.json` rejects the empty source registry. Alpaca credentials are valid and historical IEX bars are available; the paper market clock is closed until the next NYSE session, so the latest IEX quote is stale for the 10-second gate. The Twelve Data EUR/USD probe was future-dated and rejected. The current free-tier limits and feature probes are recorded in [`provider-api-research-2026-11-01.md`](./provider-api-research-2026-11-01.md). No private approved endpoint, TLS proxy, reviewer registry, account/calendar binding, or fresh quote import is present. |
| Credentials | Passed presence check | `credentials status` reports `OPENCODE_API_KEY` and broker/input credentials configured from the private file; values were not printed. |
| OpenCode review | Reviewed, changes requested | The original 560 KB one-shot request timed out. The bounded hash-preserving sections plus finalization fallback now complete in three turns; latest run `cold_start_20261101T193134Z` returned `runtime_review.status=REVIEWED` with verdict `REQUEST_CHANGES`. This remains shadow evidence only. |
| Model bundle | Blocked | Existing artifacts report `deployment_eligible=false`; holdout status is `NO_MEASURED_HOLDOUT_ADVANTAGE`; `model-lab inspect` rejects the existing candidate/native files as the wrong artifact schema; execution-data scans contain no filled-quantity, fill-price, slippage, FX-rate, or non-fill records. |
| Formula/cost | Shadow only | Seed formula and cost artifacts are unapproved; fees, FX and slippage remain unavailable until a genuine fill. Fresh proof/parity checks pass for the current source hash, but they do not create model, cost or owner approval. |
| Broker capabilities | Blocked | The independently reviewed `PG_US_EQ` record now imports and promotes only cash-account and exact-instrument identity. Quote, order kinds, protection, cancellation, partial fills, fees and FX remain `UNKNOWN`; the runtime remains fail-closed. |
| Paper deployment | Blocked | `autonomous check --config config/autonomous.demo-no-submit.json` returns `configuration_ready=false`, `approved_models=0`, and blockers `approved_model_registry_empty`, `approved_input_service_unconfigured`, `universe_source_disabled`, `entry_capabilities_unverified`, and `approved_formula_unavailable`. Alpaca volatility research selected `XOM_US_EQ` (23.998906% annualized close-return volatility) for the €50 supervised trade, but this is not a fresh quote or model approval. |
| First order | Not run | The requested guard was honored. No broker mutation or retry occurred. |

The latest cold-start run is `cold_start_20261101T193134Z`; its account snapshot
was EUR 5,000 with no orders or fills. It is evidence of account discovery only,
not execution capability or release approval. The selected Trading 212 Invest
profile and provider limits are recorded in [`provider-api-research-2026-11-01.md`](./provider-api-research-2026-11-01.md).
The private cold-start configuration now selects `XOM_US_EQ` as the highest-
volatility candidate and caps the supervised order at EUR 50.00. No preflight
intent or broker mutation was run while the NYSE session was closed.

The formula parity run covered 671 exact cases plus 265 malformed-input cases.
The fresh proof run covered 26 theorem bindings and two accepted witnesses using
source hash `16b5768762cfbc248ed42b1e9a139398d4d15e12fe5bf3ad2fa4770f1a83acf2`.
These checks are frozen verification artifacts; no parameters were widened.

Repository verification also passed `cargo test --workspace --locked`,
`cargo clippy --locked -p trady-cli --bin trady -- -D warnings`,
`cargo fmt --all -- --check`, `python3 scripts/verify-assets.py`, and the
offline proof input tests. PostgreSQL integration tests remain explicitly
ignored until the disposable database runner is invoked.

Independent-review automation is recorded in
[`independent-review-policy.md`](./independent-review-policy.md). It records
timeouts and rejections as unavailable evidence rather than approval.

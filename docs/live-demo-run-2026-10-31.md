# Live demo verification — 31 October 2026

One bounded run used the actual Trading 212 **demo** account and live external services. No broker submissions, cancellations, protective orders or other broker mutations were sent. The account remained at **EUR 5,000.00 cash, zero positions and zero pending orders**.

The complete trading workflow is **blocked before candidate selection and limit calculation**. The shipped deployment has no approved input service, native prediction model, formula authority or tradable universe. Saturday also supplies no verified open exchange session. No stock was selected, and no executable buy limit, stop trigger, stop-limit price, target or quantity was produced.

## Broker isolation and account comparison

Only `Trading212Reader` and the existing recording-only shadow runner were used. The shadow path constructs no executable broker adapter. Account observations used matching demo credentials; the shadow journal has a separate account namespace.

| Observation | UTC | Cash | Positions | Pending orders |
|---|---|---:|---:|---:|
| Before | 2026-10-31 20:11:49 | EUR 5,000.00 | 0 | 0 |
| After | 2026-10-31 20:19:20 | EUR 5,000.00 | 0 | 0 |

Account, mode, spendable cash, positions and orders matched exactly. This is a comparison of returned account state plus the selected read-only code paths; it is not a broker-side audit of all historical HTTP traffic.

The native shadow cycle returned:

```json
{
  "mode": "shadow",
  "order_mutations": 0,
  "result": {
    "reason": "trading input provider unavailable",
    "status": "blocked"
  }
}
```

The configuration check separately returned `approved_model_registry_empty`, `universe_source_disabled`, `approved_skill_hashes_missing` and `approved_formula_unavailable`. The input-service example fails its bounded configuration check because its source/reviewer registries are intentionally empty. No source reviews, model approvals, capability evidence or exchange sessions were invented.

## Actual integrations

| Integration | Actual result |
|---|---|
| FinBERT container | Built, started and healthy at `http://127.0.0.1:8091/health`; three actual inference results returned. |
| Telegram | Configured owner matched a private chat via `getChat`. Native monitor delivered one startup/status notification and completed one poll. A final result notification was also delivered to the reverified owner. No continuous polling process was left running. |
| OpenCode Go | Persisted session confirms `deepseek-v4.1-flash`; run completed with a real reply, no proposal and no tool calls. |
| X | Native metered identity lookup, two counts measurements and recent search contacted the provider. Final counts estimated eight posts; search returned eight post reads. The parser rejected the batch with `x_repost_exclusion_not_verified`, so no posts entered research storage. |
| SEC, FRED, Twelve Data, Finnhub, Alpha Vantage, FMP, CoinGecko, Tiingo, Massive | Each returned HTTP 200 with the expected health-probe payload. These probes do not activate the disabled financial collection registries. |
| Google News RSS, BBC business RSS | Both health probes returned HTTP 200 and parseable feed records. |
| GDELT | Transport failure, `http_transport_failed`; no HTTP status available. |
| yfinance | Offline import path only; no live client exists in this configuration. |
| Additional financial collectors | Sources remain disabled, capability status UNKNOWN, and collection readiness false. |
| Optional intelligence | All ten features remain disabled with no reviewed source registry. Their offline contracts were exercised by the workspace tests. |

The combined API-health command made 13 network requests: 12 healthy responses and one GDELT transport failure. Its X entry deliberately reports `requires_existing_cost_gate`; the separate native X commands above used that gate.

The isolated X ledger recorded **USD 0.100000** against a **USD 0.25** diagnostic ceiling: eight ordinary post reads, five user reads and two counts requests. No uncertain-cost reservations remain. This is a local estimate for this run, not the app's cumulative spend or final provider invoice. Rates were checked against the [official X pricing documentation](https://docs.x.com/x-api/getting-started/pricing).

Initial temporary source caps were too small for the adapters' conservative reservations: four possible requests for RSS, and two for each X counts or collection operation. They were corrected. One isolated source retry was cleared after raising its local cap; request counters and the entire X cost ledger were preserved. The changed X registry invalidated its prior counts, so a fresh counts measurement ran before search. No provider backoff, cost balance, rule verification or content gate was bypassed.

## Information gathering and FinBERT results

The native collector retrieved ten Federal Reserve press-feed items and stored permitted text in the isolated research database. It reported PARTIAL with `rss_snapshot_not_full_history`, `feed_item_budget_exhausted_no_history_cursor` and `page_budget_exhausted`. This was a bounded feed snapshot, not complete historical coverage. Board-authored text was used under the [Board's reuse policy](https://www.federalreserve.gov/disclaimer.htm).

Model: `ProsusAI/finbert`, revision `0bc2ac10dda6929bcb1f77ccc43c7ec5c424f031`. CPU-only container, two threads, 2 GiB memory, UID/GID 10001, read-only root filesystem. The inference response identity, item hashes, probability sums and truncation flags were checked.

| Actual source text | Published UTC | Negative | Neutral | Positive |
|---|---|---:|---:|---:|
| [Bank of Nova Scotia application approval](https://www.federalreserve.gov/newsevents/pressreleases/orders20261029a.htm) | 2026-10-29 20:30 | 0.027076375 | 0.879452109 | 0.093471549 |
| [FOMC statement](https://www.federalreserve.gov/newsevents/pressreleases/monetary20261028a.htm) | 2026-10-28 18:00 | 0.074908398 | 0.899334848 | 0.025756782 |
| [Enforcement-action terminations](https://www.federalreserve.gov/newsevents/pressreleases/enforcement20261016a.htm) | 2026-10-16 15:00 | 0.922517955 | 0.063065670 | 0.014416309 |

Scored at 2026-10-31 20:16:12 UTC. No document was truncated. These are container inference diagnostics on actual historical text, not trading probabilities. The negative score on a termination headline also shows why a raw sentiment label requires context.

The production `sentiment score` command rejected the source items with `sentiment requires distinct, current English evidence with available context`: publication times exceeded its 24-hour gate and the RSS adapter supplied no language metadata. The direct container diagnostic did not change those records or grant production sentiment readiness. Primary-source verification also remains absent from the collected records.

## Calculations and selection

| Calculation or planner output | Result |
|---|---|
| Actual spendable demo cash | EUR 5,000.00 |
| Configured allocation fraction | 0.98 |
| Allocation ceiling before costs and other constraints | 5,000.00 × 0.98 = **EUR 4,900.00** |
| Minimum unallocated buffer | 5,000.00 − 4,900.00 = **EUR 100.00** |
| Stock selection | Unavailable; candidate stage not reached |
| Buy limit, stop trigger, stop-limit price, target, quantity | Unavailable; production calculator not reached |
| Entry authorization | **false** |

The allocation ceiling is an arithmetic bound from the shipped policy, not an order budget approved for execution. ATR, executable bid/ask and FX, instrument increments, costs, approved formula parameters and calibrated predictions are still required for actual limits. The shipped trading policy caps holdings at five sessions; the earlier 6–12 month research-report horizon does not change that runtime policy.

## What OpenCode Go returned

Provider endpoint: `https://opencode.ai/inference/go/openai/v1/chat/completions`. Persisted provider model: **`deepseek-v4.1-flash`**. Session `session_0febd2ff56f74cadb9b800e9facab9be`, run status `COMPLETED`, charged tokens 12,574, `entry_authorized=false`, proposal `null`.

Actual model reply:

> Demo run summary (broker snapshot 2026-10-31T20:11:49Z, demo environment, 0 order mutations).
>
> Cash budget: 5,000.00 EUR cash; the 98% entry fraction yields a maximum entry spend of 4,900.00 EUR before cost constraints, leaving a 100.00 EUR minimum unallocated buffer.
>
> FinBERT diagnostic (ProsusAI/finbert, revision 0bc2ac1; three scored texts, none truncated):
> 1) 2026-10-29 Bank of Nova Scotia application approval — negative 0.027076375, neutral 0.879452109, positive 0.093471549.
> 2) 2026-10-28 FOMC statement — negative 0.074908398, neutral 0.899334848, positive 0.025756782.
> 3) 2026-10-16 enforcement-action terminations — negative 0.922517955, neutral 0.063065670, positive 0.014416309.
>
> The production planner returned no stock selection, buy limit, stop trigger, target sell limit or quantity; each is null. Entry is unauthorized: production_sentiment_readiness is false, with native rejection of aged evidence and missing language metadata, alongside blockers — no approved input service, native prediction model, formula authority or tradable universe, unknown broker capabilities, and Saturday with no verified open exchange session. No order is authorized or proposed.

The null values in that summary came from the supplied diagnostic context's unavailable fields. The native planner itself stopped before calling the calculator. The LLM summarized observed diagnostics; it did not independently gather market data or select a trade.

## Verification and service state

- `cargo build --locked --bin trady`: passed.
- `cargo test --locked --workspace`: **506 passed, zero failed, 21 ignored**.
- `python3 scripts/test-postgres.py`: **21 additional checks passed**, zero failed; migrations through version 10 and role/CLI checks ran against a separate disposable PostgreSQL container.
- `python3 -m unittest discover -s services/finbert/tests`: **six passed**.
- Actual demo before/after comparison: unchanged; zero broker mutations performed by this run.

FinBERT remains running. Inspect it with `docker compose -f services/finbert/compose.yaml --profile finbert ps` and stop it with `docker compose -f services/finbert/compose.yaml stop finbert`.

The live research database is isolated in `trady-live-demo-db-20261031` on a loopback-only random port. No pre-existing database was migrated or modified. It has no automatic restart policy and is not a production deployment. Local run configurations, logs, original responses, the shadow journal and the source-policy correction are in `/tmp/trady-live-demo-20261031/`; that directory is owner-only. Unrelated Paperless and pcbparts containers were preserved.

A prior diagnostic exposed database credential values in tool output. They were excluded from these artifacts and later inspection was redacted. Those credentials still require rotation; this run did not rotate shared credentials.

Machine-readable results: [live-demo-run-2026-10-31.json](./live-demo-run-2026-10-31.json).

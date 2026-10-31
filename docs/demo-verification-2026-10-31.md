# Demo verification — 2026-10-31

The demo account reads, offline tests and calculations passed. `deepseek-v4.1-flash`
is pinned in the [configuration](../config/opencode/llm.json) and
[adapter default](../crates/trady-adapters/src/opencode/llm.rs). Real OpenCode Go
requests to that model returned correct arithmetic and completed a persisted,
streamed two-turn conversation. A separate `glm-5.2` diagnostic also succeeded.

## Scope and side effects

Trading 212 was contacted only through authenticated **GET** requests to the fixed
demo host: account summary, positions and pending orders. No buy, sell, cancel,
protection change, live-account read, autonomous execution or Telegram message was
requested. The successful broker CLI returned `entry_authorized=false`.

Ordinary tests used an empty private credentials fixture. Database scenarios used
new PostgreSQL 18.3 containers with loopback ports and temporary storage. Model
probes received only the OpenCode credential and local test database credentials;
they received no broker credentials. Test containers were removed afterward.

## Verification results

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo test --workspace --locked --no-fail-fast` | 357 passed; 20 intentionally ignored database tests |
| `python3 scripts/test-postgres.py` | All 20 reserved database/CLI tests passed; disposable service and paper-demo probes passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `python3 -m unittest discover -s lab/tests` | 154 passed |
| `python3 -m unittest discover -s services/finbert/tests` | 6 passed |
| `python3 scripts/verify-assets.py` | Passed before bootstrap changes; final documentation validation recorded below |
| Post-fix isolated OpenCode CLI dialogue regression | Passed |

These suites contain **537 distinct passing tests**. The additional post-fix
dialogue regression repeats an already counted database test. Rust formatting,
workspace tests and Clippy passed again after the session-header fix.

The first workspace attempt stopped because the empty test credential file had
non-private permissions. Setting that temporary file to mode `0600` resolved it;
no project behavior was changed to bypass the credential check.

## Actual demo account response

The direct demo account-summary GET returned HTTP 200. Account identity is omitted
here; the CLI separately bound the actual numeric account and EUR currency.

```json
{
  "currency": "EUR",
  "totalValue": 5000.0,
  "cash": {
    "availableToTrade": 5000.0,
    "reservedForOrders": 0,
    "inPies": 0
  },
  "investments": {
    "currentValue": 0,
    "totalCost": 0,
    "realizedProfitLoss": 0,
    "unrealizedProfitLoss": 0
  }
}
```

The Rust command used a temporary demo configuration with that account binding:

```sh
target/debug/trady t212 snapshot --config /tmp/trady-verification-20261031/trading212.demo.json
```

Its normalized response, omitting only the generated snapshot ID:

```json
{
  "entry_authorized": false,
  "environment": "demo",
  "provider": "trading212",
  "snapshot": {
    "account_id": "paper_t212_verification",
    "mode": "paper",
    "observed_at": "2026-10-31T18:26:53.595930461Z",
    "orders": [],
    "positions": [],
    "spendable_cash": {"amount": "5000.00", "currency": "EUR"}
  }
}
```

## Production calculator outputs

The actual Rust `weekly formula-bridge` executed the production
[calculator](../crates/trady-policy/src/limits/mod.rs) with EUR 5,000 cash and the
[existing proof fixture](../lab/proofs/fixtures.py). **All prices, volatility,
confidence, timestamps and market evidence in this example are hypothetical.**
The fixture is dated 2026-10-30; its returned limits are expired and provide no
current market or order authority.

| Returned field | Exact result |
| --- | --- |
| Status / authority | `ACCEPTED` / `FORMULA_V1` |
| Maximum cash budget | EUR `4900` |
| Hypothetical limit / quantity | EUR `100.1` / `41.36` |
| Estimated acquisition cost | EUR `4144.476136` |
| Hypothetical stop / target | EUR `99.1` / `103.32` |
| Estimated proceeds at stop | EUR `4094.477224` |
| Estimated proceeds at target | EUR `4268.8418848` |
| Account risk | EUR `49.998912` |
| Planned loss fraction | `0.0120639883930556187427399389` |
| Reward to risk | `2.4873691011516410597094592778` |
| Target net fraction | `0.03` |

The calculator reduced quantity below the 98% cash allocation to satisfy the
fixture's 1% account-risk bound. An independent Python `Decimal` recomputation
exactly matched acquisition cost, stop proceeds, target proceeds and account risk;
it also confirmed acquisition cost stayed within EUR 4,900, risk stayed within
EUR 50 and target proceeds covered the modeled 3% net return. The unused cash in
that hypothetical case was EUR `855.523864`. Separate checked-decimal calls returned:

```json
{"status":"DECIMAL","value":"0.3"}
{"status":"DECIMAL","value":"4900"}
```

Those are `0.10 + 0.20` and `5000 × 0.98`, respectively. Negative checks returned:

```json
{"status":"REJECTED","error":"formula requires current, warmed, causal volatility and an eligible signal"}
{"status":"REJECTED","error":"decimal division overflow or zero denominator"}
```

The offline in-memory recovery demo also passed: duplicate creation was false,
the missing response became `RECOVERY_REQUIRED`, a fake reconciled fill became
`OPEN_UNPROTECTED`, unresolved intents were zero, and entry authority stayed false.
That example used a fake broker and changed no actual demo holdings.

## What OpenCode Go returned

The endpoint was `https://opencode.ai/inference/go/openai/v1/chat/completions`.
The first request exposed a missing required routing header:

```json
{"type":"error","error":{"type":"MissingSessionID","message":"Request is missing x-opencode-session and cannot be routed efficiently. Please see https://opencode.ai/v2/docs/console/go/#where-can-i-use-it"}}
```

The [official Go documentation](https://github.com/anomalyco/opencode/blob/dev/services/www/src/docs/content/console/go.mdx)
requires a stable conversation ID in `x-opencode-session`. The application now
passes the persisted session ID in `ModelRequest`, and the adapter sends it for
every request. The HTTP regression checks that tool rounds keep the same ID.

Before the model ID was corrected, a request to `deepseek-v4-flash` returned HTTP
400. That ID is no longer configured; the current `deepseek-v4.1-flash` succeeds.
The earlier response was:

```json
{"error":{"type":"server_error","message":"Upstream request failed: Model is unavailable."}}
```

The real Rust chat command handled that earlier failure as a persisted failed run,
with no proposal:

```json
{
  "run_status": "FAILED",
  "entry_authorized": false,
  "final": {
    "protocol_version": 1,
    "message_type": "FINAL",
    "reply": {
      "status": "INCOMPLETE",
      "text": "Provider request failed (provider_request_rejected).",
      "evidence_ids": [],
      "snapshot_ids": [],
      "questions": []
    },
    "research": null,
    "proposal": null
  }
}
```

Requests to the current `deepseek-v4.1-flash` model and a separate `glm-5.2`
diagnostic both returned HTTP 200 and this exact parsed result. There is no
automatic model fallback.

```json
{
  "budget_eur": "4900.00",
  "reserve_eur": "100.00",
  "risk_eur": "50.00",
  "entry_authorized": false
}
```

| Tested model | Finish reason | Input / output tokens | Elapsed |
| --- | --- | --- | --- |
| `deepseek-v4.1-flash` | `stop` | 109 / 140 | 1.58 seconds |
| `glm-5.2` | `stop` | 90 / 289 | 3.04 seconds |

Both responses contained no tool calls and matched the independently computed
decimal values. A GET to the Go `/models` path returned HTTP 404; that path was
not used as evidence of model availability.

## Streamed Rust conversation and continuation

A configuration selecting the now-pinned `deepseek-v4.1-flash` used streaming.
The real `trady chat` command used the reviewed 52-file runtime skill
manifest, normal JSON validation and an isolated PostgreSQL analyst role. It
completed two runs in the same persisted session. Both returned
`entry_authorized=false`, `research=null`, `proposal=null`, and reply status
`ANSWER`; no tools were called.

The first public reply was:

```text
Using EUR 5000.00 as a supplied demo cash fixture (not a verified account balance or quote): the 98% entry budget is EUR 4900.00, the remaining 2% reserve is EUR 100.00, and 1% account risk is EUR 50.00. These are arithmetic on the fixture only, computed without tools; they are not a verified account state, an executable market observation, or trade authority.
```

The follow-up public reply was:

```text
Using EUR 5100.00 as the supplied arithmetic demo cash fixture (not a verified balance or quote): the 98% entry budget is EUR 4998.00, the remaining 2% reserve is EUR 102.00, and 1% account risk is EUR 51.00. These are fixture arithmetic only, computed without tools; they are not a verified account state or trade authority.
```

Persistence inspection confirmed two `COMPLETED` runs, four dialogue messages and
provider identity `opencode-go` / `deepseek-v4.1-flash`. Recorded provider usage
was 9,512 input / 329 output tokens, followed by 9,710 input / 167 output tokens.
The session was explicitly closed and its temporary database removed.

## Limits and documentation verification

No current quote, market-entry readiness, strategy performance or deployed FinBERT
model was established by these checks.

The adapter EGM worker completed 17 fresh-context scouts and corrected 14 README
contracts. Its report verified source citations, links and command targets, with
no scout failures. The parent explicitly reread all 14 contracts and checked their
required sections, code fences and relative links. The parent also documented the
session-header change that landed while the scouts were running.

The core worker also completed its 15-directory audit using 21 fresh-context
scouts, with no scout or Markdown/target failures. The parent explicitly reread
all 15 contracts and rechecked all 64 relative links. The updated application
contract correctly describes the new stable `ModelRequest::session_id`; its scout
read `harness.rs` after the parent change landed.

That completes 29 package contracts. The runtime worker is still running; its
report and the final full-tree verification will be added when available.

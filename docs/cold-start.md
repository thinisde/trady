# Cold-start account workflow

`trady account cold-start` is the read-only bootstrap path for a demo account
with no assumed execution history. It discovers the provider account identity,
captures the account snapshot and available history, validates an independent
OHLCV import, intersects the broker's current instrument metadata with the
bounded research candidate pool, and writes private immutable artifacts under
the configured artifact directory.

The command registers three separate seed artifacts:

- `market-prediction-model.json` describes the historical feature/label model;
- `execution-cost-model.json` contains only the bounded prior for spread, fees,
  FX and slippage, with all unobserved components marked unknown;
- `trading-decision-model.json` binds the reviewed model and cost hashes to the
  deterministic risk formula in `seed-formula.json`.

All four artifacts are `SHADOW_ONLY`. The historical candidate is explicitly
research-only and cannot claim execution performance. The broker capability
report verifies cash-account identity and records quote, order-kind,
protection, cancellation, partial-fill, fee and FX capabilities separately. A
successful account response does not promote any of those unknown fields.

The dynamic universe has no permanent owner allowlist. Runtime research may
use news and X through the approved OpenCode Go tools, but a candidate must
still resolve to a current broker instrument and remain inside the five-name
bound. The initializer records an empty candidate pool when the broker
metadata endpoint is unavailable; it never invents an instrument identifier.

The command also writes an independent review request. With `review.enabled`
or `--llm-review`, OpenCode Go receives hash-preserving gate sections rather
than the full broker exclusion list, then performs two evidence turns and a
final verdict turn. The result is stored as a separate immutable artifact. A
review can approve shadow-only use; it never authorizes an order. Provider
timeouts or unfinished reasoning remain unavailable and cannot count as
approval.

The current provider limits, observed free-tier responses, fees and the selected
Trading 212 Invest/EUR/no-leverage profile are recorded in
[`provider-api-research-2026-11-01.md`](./provider-api-research-2026-11-01.md).
The broker catalog refresh used by `capability-evidence` is rate-limited per
account; an HTTP 429 is retained as unavailable evidence and is never worked
around by replaying a mutation.

Account-bound broker capability evidence is imported separately:

```text
trady account cold-start capability-evidence \
  --config /private/cold-start.paper-approved.json \
  --file /private/broker-capability-evidence.json
```

The record must include the exact broker account, instrument ID, share-class
ISIN, currency, venue, observation/expiry window, provider source hash and an
independent reviewer hash. The command cross-checks those fields against the
current broker catalog and stores an immutable copy; it never promotes a
`UNKNOWN` support value. Set `capability_evidence_path` in the private
cold-start config to the imported record before attempting `first-order`.
Market/limit order, attached protection, cancellation, partial-fill, fee and
FX observations are all required for an eligible first order. Fractional
support is required when the computed quantity is fractional.

The supervised first-order path is separate:

```text
trady account cold-start first-order \
  --config config/cold-start.demo.json \
  --require-manual-approval
```

It refreshes the account, positions, pending orders, broker metadata, quote and
execution costs, computes the risk-limited quantity and protected prices, and
records one immutable intent. It returns `FIRST_ORDER_BLOCKED` until a fresh
reviewed input service and independently verified protection capability exist.
The optional `--approve` flag records the operator's explicit approval after
the intent has been inspected; it does not bypass the other gates.
When review is enabled in the config, the intent is also sent to the separate
OpenCode Go reviewer; an unavailable or non-approving review remains a blocker.
Even with `--approve`, the current Trading 212 adapter refuses to mutate when
attached protection cannot be proven. No first-order command retries an
unknown submission.

After a genuine fill, bind the exact broker order ID with the read-only
reconciliation command:

```text
trady account cold-start reconcile \
  --config config/cold-start.demo.json \
  --intent var/cold-start/first-order/intent_<timestamp>.json \
  --broker-order-id <provider-id>
```

The reconciliation artifact records the broker response, terminal state and
every normalized historical fill returned for the exact broker order, including
fill price/quantity and any wallet-impact FX, net value, realized P/L and tax
rows. Missing broker fields remain unknown; the records must be complete before
cost-model calibration or prospective promotion.

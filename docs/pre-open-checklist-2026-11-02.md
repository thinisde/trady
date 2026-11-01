# Pre-open checklist — 2026-11-02

The rotated Alpaca credentials passed read-only checks on 1 November 2026:
the paper account returned `ACTIVE`, and the Alpaca market clock returned
`is_open=false` with the next open at `2026-11-02T09:30:00-05:00`. The selected
demo candidate is `XOM_US_EQ`, capped at EUR 50 in the private cold-start
configuration.

## Work that can be completed before the open

- Configure a real deployment-owned HTTPS input-service origin and approved TLS
  proxy. `inputs.trady.example` remains a placeholder and
  `inputs-service check` still rejects the empty source registry.
- Register Alpaca Basic as a reviewed source with its IEX-only real-time scope,
  personal-use rights, retention and reviewer identity. Decide whether IEX is
  acceptable for the execution reference or obtain a licensed consolidated/SIP
  entitlement.
- Prepare the XOM universe/crosswalk import, the EUR exchange calendar, the
  Trading 212 EUR account binding and the reviewed source/reviewer registries.
- Confirm Trading 212 demo permissions for Invest order placement, stop/stop-limit
  protection, cancellation and fill reporting. Documentation alone cannot turn
  these runtime fields from `UNKNOWN` into `VERIFIED`.
- Keep the existing model/formula/cost artifacts explicitly shadow-only. The
  current historical-bar candidate has no measured holdout advantage and must
  not be approved by assumption.

## Work that must wait for the open session or a genuine fill

- Capture a fresh Alpaca XOM bid/ask and current EUR/USD quote within the runtime
  age limits; prior-session quotes are rejected.
- Import, independently review and inspect the quote, FX and cost records, then
  require `currently_reviewed_and_usable=true`.
- Generate the immutable EUR 50 intent, inspect its exact quantity/limit/stop/
  target/account, and submit only after the explicit manual approval step.
- Reconcile the exact Trading 212 order ID. Only that fill can supply actual
  fee, FX, spread and slippage evidence for cost calibration.
- Build and evaluate the genuine execution dataset, then complete the required
  prospective sample (twenty calendar sessions) before release acceptance.

No queued or random order is used as a substitute for the fresh quote,
protection, reconciliation and cost gates.


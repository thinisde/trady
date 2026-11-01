# Tax-aware after-tax trade comparison (research implementation)

Status: deterministic owner-only research implementation, 2026-10-31; enabled by default for research. [Policy](../../../crates/trady-policy/src/intelligence/tax.rs) uses explicitly reviewed dated rules, lots, FX, fees and annual context to illustrate no-action, target and permitted replacement scenarios while preserving the safety snapshot. The [ephemeral CLI comparator](./implementation-report.md) stores no raw tax context and exposes no general model tool. Legal rule maintenance and broker-statement reconciliation remain owner responsibilities; the output is illustrative.

## Purpose and explicit separation

Trady's [operations](../operations.md) already records withholding and notes that final personal tax liability is unknown. This extension adds a **hypothetical, user-configured, jurisdiction-aware after-tax decision comparator**, rather than merely reporting taxes after trades. It should answer: "Under clearly stated assumptions, does rotating a holding or realizing a gain meaningfully change expected after-tax value?" It must never become an excuse to violate stops, lock in a loss unexpectedly, or override entry limits.

The tax engine must not assume legal tax residence solely from the user's current city, citizenship, browser location or broker's country. Residence and account treatment require owner-declared context, professional review where appropriate, and effective-date-specific rules.

## Input contracts

Inputs: explicitly selected tax jurisdiction, tax year, account wrapper/type, tax currency, broker entity, account statements and timestamps, purchase/sale lots with fees and FX, withholding detail, cash dividends, corporate actions, carryforward information if reliably known and user-confirmed allowances. Each provider/account tax rule has a dated source and version. **Unknown exemptions, loss carryforwards, tax lots or jurisdiction -> no reliable after-tax optimization.**

The current one-stock policy is compatible with multiple acquisition lots if a position was built externally or split/merged; never fabricate acquisition cost or assume every position has one lot. Distinguish broker-confirmed tax withholding from estimated final annual tax. Prevent a comparison that subtracts tax already reflected in net cash twice.

~~~json
{
  "schema_version": 1,
  "account_id": "redacted_account",
  "tax_context": {
    "country": "DE",
    "tax_year": 2026,
    "residency_confirmed": false,
    "rule_version": "unapproved",
    "tax_custody_type": "UNKNOWN"
  },
  "position_lots": [
    {"quantity": "0.5", "cost_basis": "55.00", "currency": "EUR", "source": "broker_statement"}
  ],
  "estimates": null,
  "status": "MISSING_APPROVED_CONTEXT"
}
~~~

Synthetic fixture. Do not collect personally identifiable tax data beyond what the owner explicitly provides and the feature genuinely needs.

## Hypothetical calculation stages

1. Reconcile lots, executions, fees, FX and cash events to broker statements; only use **actual** tax-aware account cost basis when reliable, otherwise scenario-only estimates.
2. Calculate gross realized gains by legally approved lot-identification methodology for the selected jurisdiction, account type and effective tax year. No arbitrary "best lot" if law/broker process does not permit it.
3. Apply approved tax rules: treatment of stock gains/losses, dividends, foreign withholding, applicable allowances, loss offset restrictions and currency translation, all as versioned *illustrative* scenarios. German example background includes capital income withholding, solidarity surcharge and possible church tax; exact personal treatment and broker deductions vary.
4. Distinguish estimated **immediate sale cash** from **annual final liability** and the timing value of deferred tax. Treat carryforwards and unused allowances only if documented; do not pretend an estimate is a tax assessment.
5. Net scenario utility: expected pre-tax proceeds − fees/slippage/FX − **incremental estimated tax effect** under stated assumptions. Preserve existing raw pre-tax outcome, modeled planned downside, original stop/target and execution-policy authority.
6. For an approved optional rotation comparison, compute no-action, hold-to-target and sell/replace after-tax illustrations. Keep uncertainty ranges; tax cannot make an otherwise risk-invalid trade feasible.

## Rust architecture and security

trady-domain::tax introduces TaxContext, EffectiveRuleSet, CostBasisLot and TaxScenarioEstimate with provenance and explicit unknown states. trady-application::tax reconciles broker events, stores user-approved context and builds scenarios. trady-policy::tax implements checked-decimal, pure scenario calculations; never hard-code a permanent rate across years or mix FIFO with arbitrary lot selection. Tax rules and updates must be owner-reviewed, source-dated and tested against independent accountant examples. A read-only owner tool account.tax_scenarios returns ranges and limitations; LLM never sees unnecessary identity/tax identifiers or edits tax policy.

Tax context is **separately permissioned** from research memory and should not be provided to general news tools, public exports, or Codex optimization data except anonymized synthetic fixtures. Encryption, access logs, retention and opt-out are prerequisites. The executor can read a validated scenario summary, not personal raw documents, and retains sole trading authority.

## Evaluation and safeguards

This is primarily a deterministic correctness problem rather than a machine-learning alpha signal. Test against official worked examples, broker annual statements, independent professional review and property tests for accounting conservation. For forecasting, compare pre-tax and after-tax ranking on paper portfolios without changing historical strategy rules. Report assumptions and legal uncertainty. Never extrapolate projected statutory rates without source review.

## Acceptance cases

- Account currency conversion uses tax-law-required valuation dates and documented FX source; no silent exchange-rate substitutions.
- Same broker withholding appears in both broker cash and reported tax -> reconciled once, not double charged.
- Purchase lot unknown, foreign account treatment uncertain, jurisdiction missing -> ESTIMATE_UNAVAILABLE, never zero tax.
- Corporate action, stock split, partial sale and fees preserve total quantity/basis within documented rules.
- No auto "loss harvesting" sale when position has an active protective exit, a consumed entry slot or insufficient approved replacement path.
- Proposed tax improvement cannot delay an existing stop/time exit, lower the confidence gate, enable leverage or raise a profit target.
- Tax-rule version changed -> invalidate only prospective estimates; historical decisions retain old rules/assumptions.
- The owner reporting comparator is **enabled by default for research**. It requires explicit reviewed inputs and rules and has no order-triggering influence.

## References

[German Federal Ministry of Finance: Taxes from A to Z](https://www.bundesfinanzministerium.de/Content/DE/Downloads/Broschueren_Bestellservice/steuern-von-a-z.pdf); [existing Trady operating and performance rules](../operations.md); [v1 trading policy](../strategy.md). Rules, exceptions and official references must be checked again for every tax year and deployment jurisdiction.

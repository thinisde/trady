---
name: opportunity-comparison
description: >-
  Compare keeping Trady's holding, moving to cash and a fully researched alternative
  over a common horizon, without bypassing entry slots or deterministic exit gates.
metadata:
  version: "1.0.0"
---

# Holding versus alternative

Read [opportunity rotation](../docs/v1/strategy.md#opportunity-rotation), actual held position/plan, policy, entry-slot use and unresolved orders. An already latched exit wins. If the slot is used under the one-attempt default, record `ENTRY_SLOT_USED`; you may research for a later session but cannot authorize another buy now.

Spend the small alternative-research allowance only on an eligible member with a new verifiable material catalyst and plausible tradability. Finish its sector/subsector/company/relationship/source/readiness workflow. Enthusiasm, a person's popularity or a larger stated target is insufficient.

Compare holding, cash and the alternative at one common future instant within the existing holding's original deadline. Use fresh bid/ask/account/FX observations and `calculator.rotation_economics`. Distinguish remaining forward gains from sunk purchase cost and report realized gains/losses separately. Include exit/entry/ultimate-exit costs, slippage/risk allowance and configured tax effects without double counting. Use scenario intervals, not confidence as a probability or guaranteed expected return.

Store `OPPORTUNITY_COMPARISON` with the alternative dossier and trusted calculator ID, horizon, scenario assumptions, gaps and `RESEARCH_ONLY` or requested `ELIGIBLE_FOR_VALIDATION`. The application/executor decides eligibility. No artifact contains an order command. Existing stop/target evaluation continues throughout.

An enabled rotation requires the deterministic policy gate and confirmed closure of the old stock before a new entry check. Refresh the candidate and available settled/spendable cash after sale; a passed comparison cannot guarantee the second leg. If it expires or fails, remain in cash. Never add a simultaneous holding, loosen a stop, retry an ambiguous order or waive the 10% target ceiling.

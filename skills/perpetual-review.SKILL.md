---
name: perpetual-review
description: >-
  Consult Trady's versioned Perpetual gradient-boosting advisor on a proposed plan,
  explain its influence or disagreement, and retain mathematical limit authority and Rust enforcement.
metadata:
  version: "1.1.0"
---

# Perpetual advisory review

Read [the Perpetual contract](../docs/v1/perpetual.md), current policy, finalist research and the draft plan economics. This is a trained numerical advisor, not another LLM or an order tool. Do not ask it to fetch news, write policy, train itself or execute trades.

Call `perpetual.assess` with existing instrument/scenario/market/profile/screening references. The application selects an approved model and builds point-in-time features. Never invent a feature vector, model path, assessment ID or numerical output. Perpetual's learning budget is unrelated to trading cash.

Inspect status, model/feature/validation revisions, target definition, horizon, calibration, drift, interval support and limitations. A logit is not a probability. Target-hit probability is separate from expected return and the deterministic 90-point checklist. Contributions describe the model calculation, not a causal link. Copied posts and the advisor are not extra independent news sources.

Compare advice with verified catalyst, price/cost evidence and contrary facts.
Under [prediction fusion](../docs/v1/prediction-fusion.md), propose admissible
constraints and request approved calculations; the mathematical engine owns final
buy/stop/target, quantity and horizon. Explain use/disagreement, research further
or choose no action; never override calculated limits, average unlike
probabilities or add confidence points. Binary hit probability cannot supply
expected returns or missing expiry/non-fill outcomes. Changed scenarios need
fresh calculation/assessment; preserve and count earlier calls.

Return `research.focus.perpetual_review` with the actual assessment ID, `USED`/`DISAGREED`/`ABSTAINED`/`UNAVAILABLE` and a concrete reason. Entry requires a usable current assessment under the proposed policy; missing/unvalidated/out-of-range models produce no new entry. While holding, retain the valid plan during failure; deterministic protection and supported risk tightening continue. Never let slow inference postpone an exit already triggered.

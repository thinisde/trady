---
name: perpetual-review
description: >-
  Consult Trady's versioned Perpetual gradient-boosting advisor on a proposed plan,
  explain its influence or disagreement, and retain LLM limit proposals and Rust enforcement.
metadata:
  version: "1.0.0"
---

# Perpetual advisory review

Read [the Perpetual contract](../docs/v1/perpetual.md), current policy, finalist research and the draft plan economics. This is a trained numerical advisor, not another LLM or an order tool. Do not ask it to fetch news, write policy, train itself or execute trades.

Call `perpetual.assess` with existing instrument/scenario/market/profile/screening references. The application selects an approved model and builds point-in-time features. Never invent a feature vector, model path, assessment ID or numerical output. Perpetual's learning budget is unrelated to trading cash.

Inspect status, model/feature/validation revisions, target definition, horizon, calibration, drift, interval support and limitations. A logit is not a probability. Target-hit probability is separate from expected return and the deterministic 90-point checklist. Contributions describe the model calculation, not a causal link. Copied posts and the advisor are not extra independent news sources.

Compare the advice with verified catalyst, price/cost evidence and contrary facts. You still choose the proposed entry/stop/target. You can use the advice, research further, decline a trade or disagree with a supported explanation; do not mechanically average advice into limits or add confidence points. A favorable prediction never waives policy. A changed scenario needs fresh economics and assessment; preserve earlier calls instead of selecting only favorable outputs.

Return `research.focus.perpetual_review` with the actual assessment ID, `USED`/`DISAGREED`/`ABSTAINED`/`UNAVAILABLE` and a concrete reason. Entry requires a usable current assessment under the proposed policy; missing/unvalidated/out-of-range models produce no new entry. While holding, retain the valid plan during failure; deterministic protection and supported risk tightening continue. Never let slow inference postpone an exit already triggered.

# Perpetual as an advisory LLM tool

Status: integration specification and runtime skill, updated 2026-10-31. The
library inspection below records an earlier reviewed source revision; current
workspace manifests/source identify implementation dependencies. No trained or
calibrated financial model, forecast accuracy or live approval is established
here. The [fusion contract](./prediction-fusion.md) supersedes the older
LLM-selected numerical limits while preserving Perpetual's advisory boundary.

## Verified library and Rust boundary

The requested library is [perpetual-ml/perpetual](https://github.com/perpetual-ml/perpetual), a gradient-boosting library with a Rust core. The inspected upstream revision is [fc88385688b9d781249a56fe0e5f3a3da2fd720f](https://github.com/perpetual-ml/perpetual/tree/fc88385688b9d781249a56fe0e5f3a3da2fd720f); its Cargo manifest declares version 2.3.0. This identifies inspected source, not an installed dependency or proof that a published package matches that commit. Pin the reviewed crate/source version and model format during implementation.

The native source exposes `PerpetualBooster`, `Matrix<f64>`, fitting, probability prediction, feature contributions, calibration and drift components. Rust `predict` returns a raw logit for classification; use the verified probability path and matching calibration, not a raw score relabeled as probability. Binding-specific signatures differ. The adapter must compile/check the selected version and parity against recorded fixtures. [Rust prediction source](https://github.com/perpetual-ml/perpetual/blob/fc88385688b9d781249a56fe0e5f3a3da2fd720f/src/booster/predict.rs), [Rust calibration example](https://github.com/perpetual-ml/perpetual/blob/fc88385688b9d781249a56fe0e5f3a3da2fd720f/examples/conformal_prediction.rs)

Run inference inside a bounded Rust analyst worker behind `PerpetualAdvisor`. Train/evaluate in a separate offline Rust job using curated point-in-time data. The LLM has no fit/retrain/load-path/upload-code tool. Inference receives application-built numeric/categorical feature snapshots and an approved signed model revision. CPU/thread/time/memory limits prevent it starving holding work; the executor never waits for inference to run a stop/target.

Perpetual's training `budget` controls learning effort. It is unrelated to trading cash, the 98% allocation, a target probability or risk limit. Upstream benchmark or generalization claims do not establish accuracy, calibration or profitability on financial time series. Do not use Python-only examples as proof of native Rust capability.

## LLM conversation and influence

1. The LLM finishes evidence work and proposes admissible **draft** scenario constraints. The approved mathematical calculator derives numerical limits; the draft is research, not an active plan.
2. It calls `perpetual.assess` with that scenario and existing snapshot/review IDs. Rust builds features, checks model support/freshness and returns a persisted assessment.
3. It reads the target/horizon definition, calibration, drift, uncertainty and contributions; compares the result with verified business facts and counterevidence.
4. It explains use/disagreement and may request further research or reject the setup. It cannot override calculated numerical limits. A price/quantity/cost/horizon change requires a new calculation and assessment; preserve earlier calls and count every scenario trial.
5. Final JSON carries `research.focus.perpetual_review`; Rust validates the research, final scenario binding and all policy gates before activating any plan.

Perpetual informs evidence-aware comparison, caution and the approved fusion
baseline. It adds no confidence points or independent news confirmation and has
no order or final-price authority. A reasoned LLM disagreement is recorded and
may lead to no action, but cannot waive required forecast/readiness gates. A
favorable estimate cannot raise a policy cap or loosen protection.

The initial calibrated binary task remains `target before stop/deadline`, not a
complete distribution of target-first/stop-first/expiry/non-fill events. Keep
unsupported return regression, uncertainty and additional event predictions absent.
Expected-utility selection abstains until a separately trained compatible model
and validated conditional net outcomes exist; do not synthesize them from this
binary probability. Preserve original forecasts for cumulative calibration and
[weekly model/formula experiments](./weekly-optimization.md).

The policy initially requires a usable `OK` assessment for a new entry; missing/unvalidated/stale/out-of-support models block that entry. This is a service/readiness gate, not a hidden numerical prediction veto. There is no universal probability threshold without calibrated strategy evaluation. An `OK` signal still cannot force entry. Research-only reports can show `ABSTAIN`/`UNAVAILABLE`. Holding failures retain the valid plan; protection, exits and supported risk-reducing updates continue. A raised holding target requires the configured fresh assessment and all normal plan checks.

The approved model manifest supplies supported scenario/feature ranges, maximum assessment age and evaluated quote/FX drift tolerances. The proposed paper starting quote tolerance is 0.1%, within all stricter quote-age/entry-range/economics rules; it is not evidence that this tolerance is safe for every model. If no evaluated tolerance exists, a changed relevant feature requires reassessment. A stop/target/horizon edit always creates a new scenario; do not treat a five-minute proposal lifetime as five-minute ML validity.

## Feature and label design

| Feature family | Point-in-time examples | Required controls |
|---|---|---|
| Price/liquidity | Market/group-relative returns, breadth, realized volatility, volume surprise, spread and session phase | Licensed observations available by inference time; same venue/session conventions |
| Company/financial | Effective sector/group, actual segment exposure, filed fundamentals, upcoming event proximity | Publication availability, not fiscal-period end, governs whether a fact was known |
| News/social | Event category, novelty, verified issuer mapping, materiality, stance/uncertainty, actor priority/scope, source lineage and age | Persist extractor/model/prompt versions and evidence; a later revised extraction must not leak into earlier examples |
| Relationships | Disclosed customer/supplier link, named praise/criticism, actual actor versus quoted/reposted author | Avoid ambiguous identities, circular inference and duplicated reports counting as separate events |
| Scenario | Entry/stop/target, remaining horizon, costs/FX, position state | Match trained scenario support; no prediction for unsupported target/horizon/stock regimes |

Use the owner's Trump/Musk/Gill/Ackman/Icahn scope flags as research-priority metadata, not probabilities or labels of expected direction. Missing feed coverage is a feature/state, not zero social activity. Never train on future likes/reposts, edited text, later filings, tomorrow's membership, final-day volumes or post-outcome narrative.

The first proposed task is **probability that a fixed scenario's net profit target is reached before its stop or deadline**, using executable-price/cost conventions consistent with Rust. Specify whether this is a touch or executable-fill label; do not promise a fill from a price touch. Resolve stop/target ordering using adequate observations; ambiguous same-bar events are conservative/unknown, not wins. Include delistings, no-trade candidates, halts, gaps, partial/fill constraints and censored horizons. Avoid training only on LLM-selected winning trades.

A forward net-return regression/interval is optional and stays null until separately trained and validated. Target-hit probability does not imply expected return: losing amounts and execution costs matter. Predictive intervals need the actual trained/calibrated output; never manufacture them by converting a confidence score or feature importance. Model contributions explain the model's calculation, not economic causation or a person's causal influence.

## Evaluation, promotion and failure

Use chronological walk-forward train/validation/test windows, a separate calibration window, a gap/purge covering overlapping label horizons, point-in-time index membership and transformations fitted only on training data. Keep a final untouched period; random row splitting is insufficient for overlapping financial outcomes. Evaluate by regime, sector, horizon and confidence bucket, with sample counts, calibration/Brier/log loss for classification and realized cost-aware strategy performance/drawdown. Compare the LLM strategy with and without ML advice and simple baselines.

Persist predictions before outcomes arrive. Delayed labels update monitoring after the horizon; a label-free drift score is not proof that realized predictive quality is unchanged. Reject unsupported schemas, NaN/infinity, changed feature order, stale data, insufficient calibration and out-of-range inputs. Neural/model extractor changes create new feature versions. Even if continual learning is supported, updates remain offline candidate revisions with replay/evaluation and deployment approval, never automatic live mutation.

Use the common deployment lifecycle `CANDIDATE → VERIFIED → SHADOW → APPROVED → ACTIVE`, then `RETIRED` or `QUARANTINED`. Store dataset lineage/cutoffs, dependency/source revision, feature/label/calibration/fusion versions, artifact hash, training parameters, independent evaluation and supported universe/horizons. Load only approved immutable local artifacts; no model-specified path, remote pickle or arbitrary deserialization. Fail new inference/readiness on incompatible hashes while preserving existing protection.

PostgreSQL relations are `research.ml_models`, `ml_datasets`, `feature_snapshots`, `ml_assessments`, `ml_validation_reports` and `ml_outcomes`. Store the LLM's use/disagreement and all tested scenarios alongside input/evidence/profile/market revisions. Respect inherited content-deletion rules in features and model-training lineage; quarantine/retrain or retire artifacts when applicable source restrictions require it. Financial audit records retain only permitted references.

## Exact normalized tool contract

The root below validates `perpetual.assess` arguments; select `Assessment` for its tool result and `PerpetualReview` for the final research member. The application chooses account, policy, approved model and features; the model supplies existing research/snapshot IDs, never a raw feature matrix or arbitrary model path.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "urn:trady:perpetual:1",
  "$ref": "#/$defs/AssessArguments",
  "$defs": {
    "Id": {
      "type": "string",
      "pattern": "^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$"
    },
    "Ids": {
      "type": "array",
      "items": {
        "$ref": "#/$defs/Id"
      },
      "uniqueItems": true,
      "maxItems": 32
    },
    "Text": {
      "type": "string",
      "minLength": 1,
      "maxLength": 2000
    },
    "Decimal": {
      "type": "string",
      "pattern": "^-?(0|[1-9][0-9]*)(\\.[0-9]+)?$",
      "maxLength": 48
    },
    "Time": {
      "type": "string",
      "format": "date-time",
      "pattern": "^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](\\.[0-9]+)?Z$"
    },
    "AssessArguments": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "instrument_id": {
          "$ref": "#/$defs/Id"
        },
        "scenario_calculation_id": {
          "$ref": "#/$defs/Id"
        },
        "market_snapshot_id": {
          "$ref": "#/$defs/Id"
        },
        "profile_review_id": {
          "$ref": "#/$defs/Id"
        },
        "screening_artifact_id": {
          "$ref": "#/$defs/Id"
        },
        "position_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        }
      },
      "required": [
        "instrument_id",
        "scenario_calculation_id",
        "market_snapshot_id",
        "profile_review_id",
        "screening_artifact_id",
        "position_id"
      ]
    },
    "Contribution": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "feature_name": {
          "$ref": "#/$defs/Id"
        },
        "contribution": {
          "$ref": "#/$defs/Decimal"
        },
        "interpretation": {
          "$ref": "#/$defs/Text"
        }
      },
      "required": [
        "feature_name",
        "contribution",
        "interpretation"
      ]
    },
    "ReturnInterval": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "low_net_return_fraction": {
          "$ref": "#/$defs/Decimal"
        },
        "high_net_return_fraction": {
          "$ref": "#/$defs/Decimal"
        },
        "nominal_coverage": {
          "$ref": "#/$defs/Decimal"
        }
      },
      "required": [
        "low_net_return_fraction",
        "high_net_return_fraction",
        "nominal_coverage"
      ]
    },
    "Assessment": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "assessment_version": {
          "const": 1
        },
        "assessment_id": {
          "$ref": "#/$defs/Id"
        },
        "status": {
          "enum": [
            "OK",
            "ABSTAIN",
            "UNAVAILABLE"
          ]
        },
        "instrument_id": {
          "$ref": "#/$defs/Id"
        },
        "scenario_calculation_id": {
          "$ref": "#/$defs/Id"
        },
        "market_snapshot_id": {
          "$ref": "#/$defs/Id"
        },
        "feature_snapshot_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "model_revision_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "training_data_cutoff_at": {
          "anyOf": [
            {
              "$ref": "#/$defs/Time"
            },
            {
              "type": "null"
            }
          ]
        },
        "validation_report_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "as_of": {
          "$ref": "#/$defs/Time"
        },
        "valid_until": {
          "$ref": "#/$defs/Time"
        },
        "target_definition": {
          "$ref": "#/$defs/Text"
        },
        "target_hit_probability": {
          "anyOf": [
            {
              "$ref": "#/$defs/Decimal"
            },
            {
              "type": "null"
            }
          ]
        },
        "expected_net_return_fraction": {
          "anyOf": [
            {
              "$ref": "#/$defs/Decimal"
            },
            {
              "type": "null"
            }
          ]
        },
        "return_interval": {
          "anyOf": [
            {
              "$ref": "#/$defs/ReturnInterval"
            },
            {
              "type": "null"
            }
          ]
        },
        "calibration_status": {
          "enum": [
            "VALIDATED",
            "INSUFFICIENT",
            "NOT_SUPPORTED",
            "UNKNOWN"
          ]
        },
        "drift_status": {
          "enum": [
            "IN_RANGE",
            "WARNING",
            "OUT_OF_RANGE",
            "UNKNOWN"
          ]
        },
        "top_contributions": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/Contribution"
          },
          "maxItems": 8
        },
        "limitations": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/Text"
          },
          "maxItems": 16
        },
        "limits_authority": {
          "const": "ADVISORY_ONLY"
        }
      },
      "required": [
        "assessment_version",
        "assessment_id",
        "status",
        "instrument_id",
        "scenario_calculation_id",
        "market_snapshot_id",
        "feature_snapshot_id",
        "model_revision_id",
        "training_data_cutoff_at",
        "validation_report_id",
        "as_of",
        "valid_until",
        "target_definition",
        "target_hit_probability",
        "expected_net_return_fraction",
        "return_interval",
        "calibration_status",
        "drift_status",
        "top_contributions",
        "limitations",
        "limits_authority"
      ]
    },
    "PerpetualReview": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "assessment_id": {
          "$ref": "#/$defs/Id"
        },
        "influence": {
          "enum": [
            "USED",
            "DISAGREED",
            "ABSTAINED",
            "UNAVAILABLE"
          ]
        },
        "reason": {
          "$ref": "#/$defs/Text"
        }
      },
      "required": [
        "assessment_id",
        "influence",
        "reason"
      ]
    }
  }
}
```

Validate probability/coverage fractions in `[0,1]`, ordered interval endpoints, finite decimals, time ordering and actual reference existence. `OK` requires an approved compatible model, feature snapshot, validation report and supported non-null prediction, plus current acceptable calibration/drift states. Optional regression values remain null when unsupported. `ABSTAIN`/`UNAVAILABLE` must not carry actionable numerical predictions; limitations explain why. Call success means the wrapper ran, not that an unavailable model became usable.

## Multi-round example

After tools return `economics_scenario_01`, `market_snapshot_05`, `profile_review_03` and `finalist_scan_03`, the model may request:

```json
{
  "protocol_version": 1,
  "message_type": "TOOL_REQUEST",
  "tool_calls": [{
    "call_id": "call_perpetual_01",
    "tool": "perpetual.assess",
    "arguments": {
      "instrument_id": "instrument_03",
      "scenario_calculation_id": "economics_scenario_01",
      "market_snapshot_id": "market_snapshot_05",
      "profile_review_id": "profile_review_03",
      "screening_artifact_id": "finalist_scan_03",
      "position_id": null
    }
  }]
}
```

For example, if no model has been approved, the application can return the following. Values are synthetic; null predictions explicitly prevent a fabricated estimate.

```json
{
  "protocol_version": 1,
  "message_type": "TOOL_RESULTS",
  "results": [{
    "call_id": "call_perpetual_01",
    "tool": "perpetual.assess",
    "status": "OK",
    "data": {
      "assessment_version": 1,
      "assessment_id": "ml_assessment_unavailable_01",
      "status": "UNAVAILABLE",
      "instrument_id": "instrument_03",
      "scenario_calculation_id": "economics_scenario_01",
      "market_snapshot_id": "market_snapshot_05",
      "feature_snapshot_id": null,
      "model_revision_id": null,
      "training_data_cutoff_at": null,
      "validation_report_id": null,
      "as_of": "2026-10-30T14:00:00Z",
      "valid_until": "2026-10-30T14:00:30Z",
      "target_definition": "Requested scenario: net target before stop or deadline; no prediction was made.",
      "target_hit_probability": null,
      "expected_net_return_fraction": null,
      "return_interval": null,
      "calibration_status": "UNKNOWN",
      "drift_status": "UNKNOWN",
      "top_contributions": [],
      "limitations": ["No approved compatible model is available."],
      "limits_authority": "ADVISORY_ONLY"
    },
    "error": null,
    "meta": {
      "as_of": "2026-10-30T14:00:00Z", "snapshot_id": "ml_assessment_unavailable_01",
      "source_ids": ["perpetual_advisor"], "freshness": "CURRENT", "truncated": false, "next_cursor": null
    }
  }]
}
```

The outer `OK` means the wrapper stored an honest unavailable assessment. Its inner status blocks required entry; the LLM reports an incomplete/no-entry result. With a usable result, the LLM can make another bounded call or finalize with its use/disagreement reason. This is a tool loop, not a conversation with a second autonomous trading agent.

## Verification

Check raw-logit/probability confusion, wrong feature order/model hash, future-data leakage, duplicated social lineages, missing feeds, model drift, unsupported target/horizon, unavailable inference, stale scenario after target edits, deliberate LLM disagreement and a target hit during slow inference. None may bypass the old-plan exit latch or let Perpetual set execution limits.


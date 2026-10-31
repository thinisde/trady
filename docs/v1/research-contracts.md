# Trady exact research artifact contracts

Status: normative design schemas with implemented validator/handler subsets
identified in [the foundation](../foundation.md) and [OpenCode guide](../opencode/README.md).
Artifact version 1 and final research schema version 2 support the target
sector/company/watchdog workflow. Complete readiness is separate. Model-turn
protocol and proposal-body versions remain 1.

## Ownership and bounded storage

The [prediction-fusion contract](./prediction-fusion.md) extends the target
application boundary with immutable typed predictions, calibration records and
formula/fusion calculation bindings. Existing artifact/calculator schemas below
remain unchanged research contracts; they do not accept model-written calibration
success, approved formula revisions or execution authority. Application-resolved
records must bind source/model/availability/horizon/target/uncertainty and the
final scenario before any future formula readiness acceptance. A new tool/schema
revision needs explicit handlers, fixtures and deployment review before exposure.

`research.store_artifact` takes exactly `{"artifact": <one typed object>}`. It writes an immutable draft and returns application-generated references plus validation status; it cannot write arbitrary tables, verification results, policy or orders. The handler binds owner/run/source scope from trusted context and deduplicates by run/call ID. No model-provided artifact ID, owner ID, SQL, filesystem path or verification flag is accepted.

Use `StoreArtifactResult` to validate the result. `STORED_DRAFT` and `BLOCKED` cannot satisfy readiness. `VALIDATED` means structural, identity, provenance, freshness, source-rights, scope and workflow checks passed; it does not prove future profit. Fact promotion and profile merge use trusted verifier records and compare-and-swap. Tool writes do not directly complete a skill or an executor readiness acceptance.

Stage artifacts are separate from the small final response. Store an all-sector pass, then a linked group pass, then the finalist result; retain excluded candidates and gaps. Profiles may accumulate multiple bounded reviews before a complete current revision exists. Parent links/dependencies bind reused work to the same effective universe and current source revisions. Never synthesize a final entry by merging incompatible snapshots.

## Canonical artifact and calculator schema

The root validates `research.store_artifact` arguments. Select `FocusRefs`, `StoreArtifactResult`, `PriorityArguments/Result` and `RotationArguments/Result` as separate entry points for their named contracts. All referenced external IDs must exist in this run's retrieval/validation scope. The final model schema embeds the same `FocusRefs` shape in `research.focus`.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "urn:trady:research-artifacts:1",
  "$ref": "#/$defs/StoreArtifactArguments",
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
    "Texts": {
      "type": "array",
      "items": {
        "$ref": "#/$defs/Text"
      },
      "maxItems": 16
    },
    "Time": {
      "type": "string",
      "format": "date-time",
      "pattern": "^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](\\.[0-9]+)?Z$"
    },
    "Decimal": {
      "type": "string",
      "pattern": "^-?(0|[1-9][0-9]*)(\\.[0-9]+)?$",
      "maxLength": 48
    },
    "Currency": {
      "type": "string",
      "pattern": "^[A-Z]{3}$"
    },
    "Coverage": {
      "enum": [
        "COMPLETE",
        "PARTIAL",
        "UNAVAILABLE"
      ]
    },
    "SectorCode": {
      "enum": [
        "10",
        "15",
        "20",
        "25",
        "30",
        "35",
        "40",
        "45",
        "50",
        "55",
        "60"
      ]
    },
    "GroupCode": {
      "enum": [
        "1010",
        "1510",
        "2010",
        "2020",
        "2030",
        "2510",
        "2520",
        "2530",
        "2550",
        "3010",
        "3020",
        "3030",
        "3510",
        "3520",
        "4010",
        "4020",
        "4030",
        "4510",
        "4520",
        "4530",
        "5010",
        "5020",
        "5510",
        "6010",
        "6020"
      ]
    },
    "PriorityRow": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "gics_code": {
          "oneOf": [
            {
              "$ref": "#/$defs/SectorCode"
            },
            {
              "$ref": "#/$defs/GroupCode"
            }
          ]
        },
        "calculation_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "coverage_status": {
          "$ref": "#/$defs/Coverage"
        },
        "reason": {
          "$ref": "#/$defs/Text"
        }
      },
      "required": [
        "gics_code",
        "calculation_id",
        "coverage_status",
        "reason"
      ]
    },
    "Screening": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "artifact_version": {
          "const": 1
        },
        "as_of": {
          "$ref": "#/$defs/Time"
        },
        "evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "blockers": {
          "$ref": "#/$defs/Texts"
        },
        "kind": {
          "const": "SCREENING"
        },
        "stage": {
          "enum": [
            "SECTORS",
            "SUBSECTORS",
            "FINALIST"
          ]
        },
        "prior_screening_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "universe_snapshot_id": {
          "$ref": "#/$defs/Id"
        },
        "market_snapshot_id": {
          "$ref": "#/$defs/Id"
        },
        "rankings": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/PriorityRow"
          },
          "maxItems": 36,
          "minItems": 1
        },
        "selected_sector_code": {
          "anyOf": [
            {
              "$ref": "#/$defs/SectorCode"
            },
            {
              "type": "null"
            }
          ]
        },
        "selected_subsector_code": {
          "anyOf": [
            {
              "$ref": "#/$defs/GroupCode"
            },
            {
              "type": "null"
            }
          ]
        },
        "operating_branch": {
          "anyOf": [
            {
              "$ref": "#/$defs/Text"
            },
            {
              "type": "null"
            }
          ]
        },
        "candidate_instrument_ids": {
          "$ref": "#/$defs/Ids",
          "maxItems": 5
        },
        "peer_instrument_ids": {
          "$ref": "#/$defs/Ids",
          "maxItems": 2
        },
        "finalist_instrument_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "exclusions": {
          "$ref": "#/$defs/Texts"
        }
      },
      "required": [
        "artifact_version",
        "as_of",
        "evidence_ids",
        "blockers",
        "kind",
        "stage",
        "prior_screening_id",
        "universe_snapshot_id",
        "market_snapshot_id",
        "rankings",
        "selected_sector_code",
        "selected_subsector_code",
        "operating_branch",
        "candidate_instrument_ids",
        "peer_instrument_ids",
        "finalist_instrument_id",
        "exclusions"
      ]
    },
    "FactDraft": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "field": {
          "enum": [
            "BUSINESS_MODEL",
            "SEGMENT_EXPOSURE",
            "FINANCIAL_METRIC",
            "RISK",
            "EVENT",
            "MANAGEMENT_STATEMENT"
          ]
        },
        "claim_type": {
          "enum": [
            "FACT",
            "FORECAST",
            "INTERPRETATION",
            "RUMOR"
          ]
        },
        "statement": {
          "$ref": "#/$defs/Text"
        },
        "value": {
          "anyOf": [
            {
              "$ref": "#/$defs/Decimal"
            },
            {
              "type": "null"
            }
          ]
        },
        "currency": {
          "anyOf": [
            {
              "$ref": "#/$defs/Currency"
            },
            {
              "type": "null"
            }
          ]
        },
        "unit": {
          "anyOf": [
            {
              "$ref": "#/$defs/Text"
            },
            {
              "type": "null"
            }
          ]
        },
        "period": {
          "anyOf": [
            {
              "$ref": "#/$defs/Text"
            },
            {
              "type": "null"
            }
          ]
        },
        "support_evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "contradiction_evidence_ids": {
          "$ref": "#/$defs/Ids"
        }
      },
      "required": [
        "field",
        "claim_type",
        "statement",
        "value",
        "currency",
        "unit",
        "period",
        "support_evidence_ids",
        "contradiction_evidence_ids"
      ]
    },
    "ProfileReview": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "artifact_version": {
          "const": 1
        },
        "as_of": {
          "$ref": "#/$defs/Time"
        },
        "evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "blockers": {
          "$ref": "#/$defs/Texts"
        },
        "kind": {
          "const": "PROFILE_REVIEW"
        },
        "issuer_entity_id": {
          "$ref": "#/$defs/Id"
        },
        "expected_profile_revision_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "baseline_item_ids": {
          "$ref": "#/$defs/Ids"
        },
        "fact_drafts": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/FactDraft"
          },
          "maxItems": 24
        },
        "statement_evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "coverage_status": {
          "$ref": "#/$defs/Coverage"
        },
        "gaps": {
          "$ref": "#/$defs/Texts"
        }
      },
      "required": [
        "artifact_version",
        "as_of",
        "evidence_ids",
        "blockers",
        "kind",
        "issuer_entity_id",
        "expected_profile_revision_id",
        "baseline_item_ids",
        "fact_drafts",
        "statement_evidence_ids",
        "coverage_status",
        "gaps"
      ]
    },
    "EconomicEdge": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "subject_entity_id": {
          "$ref": "#/$defs/Id"
        },
        "target_entity_id": {
          "$ref": "#/$defs/Id"
        },
        "relation": {
          "enum": [
            "SUPPLIES",
            "CUSTOMER_OF",
            "PARTNERS_WITH",
            "COMPETES_WITH",
            "INVESTED_IN",
            "REGULATES",
            "LEADS"
          ]
        },
        "statement": {
          "$ref": "#/$defs/Text"
        },
        "valid_from": {
          "anyOf": [
            {
              "$ref": "#/$defs/Time"
            },
            {
              "type": "null"
            }
          ]
        },
        "valid_until": {
          "anyOf": [
            {
              "$ref": "#/$defs/Time"
            },
            {
              "type": "null"
            }
          ]
        },
        "support_evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "contradiction_evidence_ids": {
          "$ref": "#/$defs/Ids"
        }
      },
      "required": [
        "subject_entity_id",
        "target_entity_id",
        "relation",
        "statement",
        "valid_from",
        "valid_until",
        "support_evidence_ids",
        "contradiction_evidence_ids"
      ]
    },
    "Mention": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "speaker_entity_id": {
          "$ref": "#/$defs/Id"
        },
        "target_entity_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "quoted_speaker_entity_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "item_id": {
          "$ref": "#/$defs/Id"
        },
        "locator": {
          "$ref": "#/$defs/Text"
        },
        "event_type": {
          "enum": [
            "PRAISES",
            "CRITICIZES",
            "MENTIONS",
            "QUOTES",
            "REPOSTS"
          ]
        },
        "distribution_kind": {
          "enum": [
            "ORIGINAL",
            "REPLY",
            "QUOTE",
            "REPOST"
          ]
        },
        "tone": {
          "enum": [
            "SUPPORTIVE",
            "CRITICAL",
            "NEUTRAL",
            "MIXED",
            "UNCLEAR"
          ]
        },
        "stance_confidence": {
          "type": "integer",
          "minimum": 0,
          "maximum": 100
        },
        "entity_resolution_confidence": {
          "type": "integer",
          "minimum": 0,
          "maximum": 100
        },
        "event_at": {
          "anyOf": [
            {
              "$ref": "#/$defs/Time"
            },
            {
              "type": "null"
            }
          ]
        },
        "materiality": {
          "enum": [
            "DIRECT",
            "INDIRECT",
            "IMMATERIAL",
            "UNKNOWN"
          ]
        },
        "transmission_hypothesis": {
          "$ref": "#/$defs/Text"
        },
        "support_evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "contradiction_evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "review_after": {
          "$ref": "#/$defs/Time"
        }
      },
      "required": [
        "speaker_entity_id",
        "target_entity_id",
        "quoted_speaker_entity_id",
        "item_id",
        "locator",
        "event_type",
        "distribution_kind",
        "tone",
        "stance_confidence",
        "entity_resolution_confidence",
        "event_at",
        "materiality",
        "transmission_hypothesis",
        "support_evidence_ids",
        "contradiction_evidence_ids",
        "review_after"
      ]
    },
    "RelationshipReview": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "artifact_version": {
          "const": 1
        },
        "as_of": {
          "$ref": "#/$defs/Time"
        },
        "evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "blockers": {
          "$ref": "#/$defs/Texts"
        },
        "kind": {
          "const": "RELATIONSHIP_REVIEW"
        },
        "issuer_entity_id": {
          "$ref": "#/$defs/Id"
        },
        "profile_revision_id": {
          "$ref": "#/$defs/Id"
        },
        "reviewed_entity_ids": {
          "$ref": "#/$defs/Ids"
        },
        "edges": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/EconomicEdge"
          },
          "maxItems": 16
        },
        "mentions": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/Mention"
          },
          "maxItems": 24
        },
        "coverage_status": {
          "$ref": "#/$defs/Coverage"
        },
        "gaps": {
          "$ref": "#/$defs/Texts"
        }
      },
      "required": [
        "artifact_version",
        "as_of",
        "evidence_ids",
        "blockers",
        "kind",
        "issuer_entity_id",
        "profile_revision_id",
        "reviewed_entity_ids",
        "edges",
        "mentions",
        "coverage_status",
        "gaps"
      ]
    },
    "TopicTerm": {
      "type": "string",
      "minLength": 1,
      "maxLength": 128
    },
    "WatchRule": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "rule_key": {
          "$ref": "#/$defs/Id"
        },
        "category": {
          "enum": [
            "COMPANY",
            "PEOPLE",
            "THESIS",
            "RELATIONSHIP",
            "SECTOR"
          ]
        },
        "source_ids": {
          "$ref": "#/$defs/Ids"
        },
        "entity_ids": {
          "$ref": "#/$defs/Ids"
        },
        "social_account_ids": {
          "$ref": "#/$defs/Ids"
        },
        "topic_terms": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/TopicTerm"
          },
          "maxItems": 12
        },
        "severity": {
          "enum": [
            "INFO",
            "MATERIAL",
            "CRITICAL"
          ]
        },
        "collect_seconds": {
          "enum": [
            60,
            300,
            900,
            3600
          ]
        },
        "cooldown_seconds": {
          "type": "integer",
          "minimum": 0,
          "maximum": 3600
        },
        "mandatory": {
          "type": "boolean"
        },
        "reason": {
          "$ref": "#/$defs/Text"
        }
      },
      "required": [
        "rule_key",
        "category",
        "source_ids",
        "entity_ids",
        "social_account_ids",
        "topic_terms",
        "severity",
        "collect_seconds",
        "cooldown_seconds",
        "mandatory",
        "reason"
      ]
    },
    "WatchdogTemplate": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "artifact_version": {
          "const": 1
        },
        "as_of": {
          "$ref": "#/$defs/Time"
        },
        "evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "blockers": {
          "$ref": "#/$defs/Texts"
        },
        "kind": {
          "const": "WATCHDOG_TEMPLATE"
        },
        "instrument_id": {
          "$ref": "#/$defs/Id"
        },
        "issuer_entity_id": {
          "$ref": "#/$defs/Id"
        },
        "expected_template_revision_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "thesis_evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "profile_review_id": {
          "$ref": "#/$defs/Id"
        },
        "relationship_review_id": {
          "$ref": "#/$defs/Id"
        },
        "mandatory_source_ids": {
          "$ref": "#/$defs/Ids"
        },
        "rules": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/WatchRule"
          },
          "maxItems": 24,
          "minItems": 1
        },
        "review_after": {
          "$ref": "#/$defs/Time"
        },
        "expires_at": {
          "$ref": "#/$defs/Time"
        }
      },
      "required": [
        "artifact_version",
        "as_of",
        "evidence_ids",
        "blockers",
        "kind",
        "instrument_id",
        "issuer_entity_id",
        "expected_template_revision_id",
        "thesis_evidence_ids",
        "profile_review_id",
        "relationship_review_id",
        "mandatory_source_ids",
        "rules",
        "review_after",
        "expires_at"
      ]
    },
    "OpportunityComparison": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "artifact_version": {
          "const": 1
        },
        "as_of": {
          "$ref": "#/$defs/Time"
        },
        "evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "blockers": {
          "$ref": "#/$defs/Texts"
        },
        "kind": {
          "const": "OPPORTUNITY_COMPARISON"
        },
        "held_instrument_id": {
          "$ref": "#/$defs/Id"
        },
        "alternative_instrument_id": {
          "$ref": "#/$defs/Id"
        },
        "position_id": {
          "$ref": "#/$defs/Id"
        },
        "expected_plan_version": {
          "type": "integer",
          "minimum": 1
        },
        "alternative_dossier_id": {
          "$ref": "#/$defs/Id"
        },
        "rotation_calculation_id": {
          "$ref": "#/$defs/Id"
        },
        "horizon_at": {
          "$ref": "#/$defs/Time"
        },
        "requested_status": {
          "enum": [
            "RESEARCH_ONLY",
            "ELIGIBLE_FOR_VALIDATION"
          ]
        },
        "uncertainties": {
          "$ref": "#/$defs/Texts"
        }
      },
      "required": [
        "artifact_version",
        "as_of",
        "evidence_ids",
        "blockers",
        "kind",
        "held_instrument_id",
        "alternative_instrument_id",
        "position_id",
        "expected_plan_version",
        "alternative_dossier_id",
        "rotation_calculation_id",
        "horizon_at",
        "requested_status",
        "uncertainties"
      ]
    },
    "Artifact": {
      "oneOf": [
        {
          "$ref": "#/$defs/Screening"
        },
        {
          "$ref": "#/$defs/ProfileReview"
        },
        {
          "$ref": "#/$defs/RelationshipReview"
        },
        {
          "$ref": "#/$defs/WatchdogTemplate"
        },
        {
          "$ref": "#/$defs/OpportunityComparison"
        }
      ]
    },
    "StoreArtifactArguments": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "artifact": {
          "$ref": "#/$defs/Artifact"
        }
      },
      "required": [
        "artifact"
      ]
    },
    "StoreArtifactResult": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "artifact_id": {
          "$ref": "#/$defs/Id"
        },
        "artifact_kind": {
          "enum": [
            "SCREENING",
            "PROFILE_REVIEW",
            "RELATIONSHIP_REVIEW",
            "WATCHDOG_TEMPLATE",
            "OPPORTUNITY_COMPARISON"
          ]
        },
        "validation_status": {
          "enum": [
            "STORED_DRAFT",
            "VALIDATED",
            "BLOCKED"
          ]
        },
        "profile_revision_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "blockers": {
          "$ref": "#/$defs/Texts"
        }
      },
      "required": [
        "artifact_id",
        "artifact_kind",
        "validation_status",
        "profile_revision_id",
        "blockers"
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
    },
    "FocusRefs": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "universe_snapshot_id": {
          "$ref": "#/$defs/Id"
        },
        "screening_artifact_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "profile_review_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "relationship_review_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "watchdog_template_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "opportunity_comparison_id": {
          "anyOf": [
            {
              "$ref": "#/$defs/Id"
            },
            {
              "type": "null"
            }
          ]
        },
        "perpetual_review": {
          "anyOf": [
            {
              "$ref": "#/$defs/PerpetualReview"
            },
            {
              "type": "null"
            }
          ]
        }
      },
      "required": [
        "universe_snapshot_id",
        "screening_artifact_id",
        "profile_review_id",
        "relationship_review_id",
        "watchdog_template_id",
        "opportunity_comparison_id",
        "perpetual_review"
      ]
    },
    "ComponentInput": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "component": {
          "enum": [
            "CATALYST",
            "BREADTH",
            "TRANSMISSION",
            "TRADABILITY",
            "COVERAGE"
          ]
        },
        "requested_grade": {
          "anyOf": [
            {
              "type": "integer",
              "minimum": 0,
              "maximum": 4
            },
            {
              "type": "null"
            }
          ]
        },
        "reference_ids": {
          "$ref": "#/$defs/Ids"
        },
        "reason": {
          "$ref": "#/$defs/Text"
        }
      },
      "required": [
        "component",
        "requested_grade",
        "reference_ids",
        "reason"
      ]
    },
    "PriorityArguments": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "universe_snapshot_id": {
          "$ref": "#/$defs/Id"
        },
        "market_snapshot_id": {
          "$ref": "#/$defs/Id"
        },
        "gics_code": {
          "oneOf": [
            {
              "$ref": "#/$defs/SectorCode"
            },
            {
              "$ref": "#/$defs/GroupCode"
            }
          ]
        },
        "components": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/ComponentInput"
          },
          "maxItems": 5,
          "minItems": 5
        }
      },
      "required": [
        "universe_snapshot_id",
        "market_snapshot_id",
        "gics_code",
        "components"
      ]
    },
    "PriorityResult": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "calculation_id": {
          "$ref": "#/$defs/Id"
        },
        "rubric_version": {
          "$ref": "#/$defs/Id"
        },
        "validated_components": {
          "type": "array",
          "items": {
            "$ref": "#/$defs/ComponentInput"
          },
          "maxItems": 5,
          "minItems": 5
        },
        "priority_sum": {
          "anyOf": [
            {
              "type": "integer",
              "minimum": 0,
              "maximum": 20
            },
            {
              "type": "null"
            }
          ]
        },
        "coverage_status": {
          "$ref": "#/$defs/Coverage"
        },
        "blockers": {
          "$ref": "#/$defs/Texts"
        }
      },
      "required": [
        "calculation_id",
        "rubric_version",
        "validated_components",
        "priority_sum",
        "coverage_status",
        "blockers"
      ]
    },
    "Scenario": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "instrument_id": {
          "$ref": "#/$defs/Id"
        },
        "currency": {
          "$ref": "#/$defs/Currency"
        },
        "low_exit_price": {
          "$ref": "#/$defs/Decimal"
        },
        "high_exit_price": {
          "$ref": "#/$defs/Decimal"
        },
        "evidence_ids": {
          "$ref": "#/$defs/Ids"
        },
        "reason": {
          "$ref": "#/$defs/Text"
        }
      },
      "required": [
        "instrument_id",
        "currency",
        "low_exit_price",
        "high_exit_price",
        "evidence_ids",
        "reason"
      ]
    },
    "RotationArguments": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "position_id": {
          "$ref": "#/$defs/Id"
        },
        "expected_plan_version": {
          "type": "integer",
          "minimum": 1
        },
        "alternative_dossier_id": {
          "$ref": "#/$defs/Id"
        },
        "account_snapshot_id": {
          "$ref": "#/$defs/Id"
        },
        "market_snapshot_id": {
          "$ref": "#/$defs/Id"
        },
        "horizon_at": {
          "$ref": "#/$defs/Time"
        },
        "held_scenario": {
          "$ref": "#/$defs/Scenario"
        },
        "alternative_scenario": {
          "$ref": "#/$defs/Scenario"
        }
      },
      "required": [
        "position_id",
        "expected_plan_version",
        "alternative_dossier_id",
        "account_snapshot_id",
        "market_snapshot_id",
        "horizon_at",
        "held_scenario",
        "alternative_scenario"
      ]
    },
    "RotationResult": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "calculation_id": {
          "$ref": "#/$defs/Id"
        },
        "currency": {
          "$ref": "#/$defs/Currency"
        },
        "current_liquidation_equity": {
          "$ref": "#/$defs/Decimal"
        },
        "switch_wealth_low": {
          "$ref": "#/$defs/Decimal"
        },
        "hold_wealth_high": {
          "$ref": "#/$defs/Decimal"
        },
        "cash_wealth": {
          "$ref": "#/$defs/Decimal"
        },
        "safety_buffer_amount": {
          "$ref": "#/$defs/Decimal"
        },
        "advantage_fraction": {
          "anyOf": [
            {
              "$ref": "#/$defs/Decimal"
            },
            {
              "type": "null"
            }
          ]
        },
        "hurdle_fraction": {
          "$ref": "#/$defs/Decimal"
        },
        "policy_eligible": {
          "type": "boolean"
        },
        "blockers": {
          "$ref": "#/$defs/Texts"
        },
        "input_snapshot_ids": {
          "$ref": "#/$defs/Ids"
        },
        "assumptions_valid_until": {
          "$ref": "#/$defs/Time"
        }
      },
      "required": [
        "calculation_id",
        "currency",
        "current_liquidation_equity",
        "switch_wealth_low",
        "hold_wealth_high",
        "cash_wealth",
        "safety_buffer_amount",
        "advantage_fraction",
        "hurdle_fraction",
        "policy_eligible",
        "blockers",
        "input_snapshot_ids",
        "assumptions_valid_until"
      ]
    }
  }
}
```

## Semantic checks

- **Screening:** SECTORS covers all 11 distinct current sector codes. SUBSECTORS covers every group of the chosen sector and links a validated sector pass. FINALIST links both stages and selects an eligible instrument from the bounded candidates; the selected group belongs to the selected sector and actual current issuer classification. Single-group sectors supply an operating branch. Unknown rows retain a null/incomplete calculation and cannot be the focus; all missing required coverage is explicit.
- **Priority:** exactly one of each of the five components, with allowed evidence and a consistent market interval. Requested grades are hypotheses. The handler validates support and the rubric before calculating; unresolved components yield a null sum. The tool returns no trade-confidence number.
- **Profile:** baseline items and evidence belong to the resolved issuer; financial metrics have value, units, currency where meaningful and a period. Text facts use null numeric fields. Evidence supports the requested attribution/claim type. Material unresolved gaps prevent a validated entry review. The expected profile revision must match, or a fresh merge/review is required.
- **Relationships:** resolve speaker/target independently. Economic edges need verified disclosure support; quoted speech and repost distribution cannot establish agreement, a contract or source independence. Null/ambiguous targets can be stored as unresolved mention drafts but cannot satisfy material entry claims. Stance/entity confidence is not trading confidence.
- **Watchdog:** all referenced sources/accounts are approved, supported and within budget; mandatory-source coverage exists; every literal term and rule is bounded; expiry/review is in the future and consistent with the thesis. The application adds nonoptional price/order/protection/market-shock monitoring separately, and binds actual position/plan only after a fill. A template cannot disable those safety tasks.
- **Rotation:** distinct eligible instruments, actual position/version, complete alternative dossier, common valid horizon, positive ordered scenario intervals, fresh executable quotes/account/FX, explicit cost/slippage/tax assumptions and no unresolved material forecast gap. The handler computes quantities/wealth in account currency; it never uses model confidence as a probability. A numeric advantage alone cannot validate unsupported price forecasts. Policy eligibility is recalculated by the executor.
- **All:** reject unknown extra fields, unsupported IDs/enums, malformed/impossible times, duplicate local rule keys and too-large arrays. Enforce text/content rights, financial decimals and dependency invalidation. A source correction/deletion/expiry can invalidate a previously validated dependent artifact.

`FocusRefs` is nullable in incomplete or ordinary conversational research. For entry all members except `opportunity_comparison_id` are non-null and refer to compatible validated records; `perpetual_review` follows [the advisory contract](./perpetual.md) and records use/disagreement with a usable matching assessment. `screening_artifact_id` links the final stage and its prior passes. Rotation additionally references the validated comparison. An alternative under research while a stock is held must retain the flat-account blocker; it cannot claim final entry eligibility. Position updates bind applicable current profile/watchdog/position records.

## Worked subsector tool request

All IDs are synthetic examples assumed to have been returned by prior tools. The missing semiconductor coverage does not become a complete row; the software focus still requires its own full company/readiness workflow.

```json
{
  "artifact": {
    "artifact_version": 1,
    "kind": "SCREENING",
    "as_of": "2026-10-30T14:00:00Z",
    "evidence_ids": [
      "evidence_01"
    ],
    "blockers": [],
    "stage": "SUBSECTORS",
    "prior_screening_id": "sector_scan_01",
    "universe_snapshot_id": "universe_01",
    "market_snapshot_id": "market_snapshot_05",
    "rankings": [
      {
        "gics_code": "4510",
        "calculation_id": "priority_4510",
        "coverage_status": "COMPLETE",
        "reason": "Verified customer demand and current economics support further software research."
      },
      {
        "gics_code": "4520",
        "calculation_id": "priority_4520",
        "coverage_status": "COMPLETE",
        "reason": "Hardware evidence is mixed."
      },
      {
        "gics_code": "4530",
        "calculation_id": null,
        "coverage_status": "PARTIAL",
        "reason": "Current restriction and customer context is unresolved."
      }
    ],
    "selected_sector_code": "45",
    "selected_subsector_code": "4510",
    "operating_branch": "Enterprise software",
    "candidate_instrument_ids": [
      "instrument_03"
    ],
    "peer_instrument_ids": [
      "instrument_07"
    ],
    "finalist_instrument_id": null,
    "exclusions": [
      "No semiconductor candidate is finalized while material restriction context is missing."
    ]
  }
}
```

A successful storage result can be:

```json
{
  "artifact_id": "subsector_scan_02",
  "artifact_kind": "SCREENING",
  "validation_status": "VALIDATED",
  "profile_revision_id": null,
  "blockers": []
}
```

This validates the stage under its stated scope, not an entry. The [final JSON contract](./llm-contracts.md) still requires all mandatory skill artifacts, primary/independent evidence, complete readiness and current economics. Research-only opportunities and incomplete drafts cannot create an order.


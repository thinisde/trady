# Trady v1 exact JSON contracts

Status: normative documentation for protocol/proposal version 1 and research schema version 2; no runtime validator is implemented. The JSON Schema below and its examples define canonical model payloads. A provider adapter may translate into a supported native schema/wire format, but local validation uses this full contract.

## Direction and ownership

| Record | Writer and allowed fields |
|---|---|
| `ModelTurn` | Model: exactly `TOOL_REQUEST` or `FINAL`, as defined below |
| `ToolResults` | Application: matching tool outputs; a model cannot fabricate this message type |
| `ProposalBody` | Model: research kind, referenced inputs, proposed thresholds and supporting explanation |
| `ApplicationProposalEnvelope` | Application only: generated IDs, context/policy binding, accepted research-dossier reference, creation/expiry and the validated body |

The model never returns proposal/run/session IDs, owner/account identity, an authoritative policy version, a readiness acceptance flag or proposal expiry as application-envelope fields. External evidence/snapshot/calculation references must be IDs actually supplied in context/tool results. New `claim_id` values are local labels within this research object, checked for uniqueness and mapped to stored records by the application. `artifact_refs` are JSON Pointer fragments into the same final message, not invented database IDs. `expected_plan_version` is an optimistic-concurrency reference, not authority to select another plan. The application creates an envelope after validating a final body; the executor creates its own readiness acceptance/rejection record.

`research.schema_version` is exactly 2; proposal `schema_version` stays 1. Research v2 adds the required nullable `focus` member linking typed sector/profile/relationship/watchdog artifacts. No deployed runtime exists to migrate; reject research-v1 output under this revised contract. `research.workflow_status` is `DRAFT`, `BLOCKED` or `FINAL`; all are model assessments requiring validation. `skill_findings.status` is likewise requested `COMPLETED` or `BLOCKED`, never a direct database write. A missing field uses its explicitly nullable form, not an invented success or an omitted required key.

## Canonical schema

The root validates model turns. The same document's `ToolResults` and `ApplicationProposalEnvelope` definitions are separate application-side entry points. Currency/ID existence, decimal arithmetic, expiry/freshness, exact mandatory skill/check coverage, source truth/independence and score thresholds require the semantic checks described after the schema.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "urn:trady:llm-contracts:2",
  "$ref": "#/$defs/ModelTurn",
  "$defs": {
    "Id": {"type": "string", "pattern": "^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$"},
    "Ids": {"type": "array", "items": {"$ref": "#/$defs/Id"}, "uniqueItems": true, "maxItems": 32},
    "Text": {"type": "string", "maxLength": 2000},
    "Texts": {"type": "array", "items": {"$ref": "#/$defs/Text"}, "maxItems": 16},
    "Time": {"type": "string", "format": "date-time", "pattern": "^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](\\.[0-9]+)?Z$"},
    "Decimal": {"type": "string", "pattern": "^-?(0|[1-9][0-9]*)(\\.[0-9]+)?$", "maxLength": 48},
    "Currency": {"type": "string", "pattern": "^[A-Z]{3}$"},
    "NullableId": {"anyOf": [{"$ref": "#/$defs/Id"}, {"type": "null"}]},
    "NullableTime": {"anyOf": [{"$ref": "#/$defs/Time"}, {"type": "null"}]},
    "NullableText": {"anyOf": [{"$ref": "#/$defs/Text"}, {"type": "null"}]},
    "NullableDecimal": {"anyOf": [{"$ref": "#/$defs/Decimal"}, {"type": "null"}]},
    "Coverage": {"enum": ["COMPLETE", "PARTIAL", "UNAVAILABLE"]},
    "ArtifactPointers": {"type": "array", "items": {"type": "string", "pattern": "^#/research/", "maxLength": 256}, "uniqueItems": true, "maxItems": 16},
    "ModelTurn": {"oneOf": [{"$ref": "#/$defs/ToolRequest"}, {"$ref": "#/$defs/Final"}]},
    "ToolCall": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "call_id": {"$ref": "#/$defs/Id"},
        "tool": {"type": "string", "pattern": "^[a-z][a-z0-9_]*(\\.[a-z][a-z0-9_]*)+$", "maxLength": 96},
        "arguments": {"type": "object"}
      },
      "required": ["call_id", "tool", "arguments"]
    },
    "ToolRequest": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "protocol_version": {"const": 1}, "message_type": {"const": "TOOL_REQUEST"},
        "tool_calls": {"type": "array", "items": {"$ref": "#/$defs/ToolCall"}, "minItems": 1, "maxItems": 4}
      },
      "required": ["protocol_version", "message_type", "tool_calls"]
    },
    "Reply": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "status": {"enum": ["ANSWER", "NEEDS_INPUT", "INCOMPLETE"]},
        "text": {"type": "string", "minLength": 1, "maxLength": 6000},
        "evidence_ids": {"$ref": "#/$defs/Ids"}, "snapshot_ids": {"$ref": "#/$defs/Ids"},
        "questions": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 500}, "maxItems": 3}
      },
      "required": ["status", "text", "evidence_ids", "snapshot_ids", "questions"],
      "if": {"properties": {"status": {"const": "NEEDS_INPUT"}}},
      "then": {"properties": {"questions": {"minItems": 1}}},
      "else": {"properties": {"questions": {"maxItems": 0}}}
    },
    "Scope": {
      "type": "object", "additionalProperties": false,
      "properties": {"question": {"$ref": "#/$defs/Text"}, "instrument_ids": {"$ref": "#/$defs/Ids"}, "as_of": {"$ref": "#/$defs/Time"}},
      "required": ["question", "instrument_ids", "as_of"]
    },
    "SearchFilters": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "instrument_ids": {"$ref": "#/$defs/Ids"}, "start_at": {"$ref": "#/$defs/NullableTime"}, "end_at": {"$ref": "#/$defs/NullableTime"},
        "source_domains": {"type": "array", "items": {"type": "string", "maxLength": 253}, "uniqueItems": true, "maxItems": 8}
      },
      "required": ["instrument_ids", "start_at", "end_at", "source_domains"]
    },
    "SearchRecord": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "source_id": {"$ref": "#/$defs/Id"}, "query": {"$ref": "#/$defs/Text"}, "filters": {"$ref": "#/$defs/SearchFilters"},
        "tool_call_ids": {"$ref": "#/$defs/Ids"}, "result_item_ids": {"$ref": "#/$defs/Ids"}, "coverage_status": {"$ref": "#/$defs/Coverage"}, "gaps": {"$ref": "#/$defs/Texts"}
      },
      "required": ["source_id", "query", "filters", "tool_call_ids", "result_item_ids", "coverage_status", "gaps"]
    },
    "Claim": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "claim_id": {"$ref": "#/$defs/Id"}, "instrument_id": {"$ref": "#/$defs/NullableId"}, "statement": {"$ref": "#/$defs/Text"},
        "claim_type": {"enum": ["FACT", "FORECAST", "INTERPRETATION", "RUMOR"]},
        "published_at": {"$ref": "#/$defs/NullableTime"}, "event_at": {"$ref": "#/$defs/NullableTime"},
        "materiality": {"enum": ["DIRECT", "INDIRECT", "IMMATERIAL", "UNKNOWN"]},
        "support_item_ids": {"$ref": "#/$defs/Ids"}, "contradiction_item_ids": {"$ref": "#/$defs/Ids"},
        "primary_verification_ids": {"$ref": "#/$defs/Ids"}, "unknowns": {"$ref": "#/$defs/Texts"}
      },
      "required": ["claim_id", "instrument_id", "statement", "claim_type", "published_at", "event_at", "materiality", "support_item_ids", "contradiction_item_ids", "primary_verification_ids", "unknowns"]
    },
    "Comparison": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "claim_id": {"$ref": "#/$defs/Id"}, "primary_item_ids": {"$ref": "#/$defs/Ids"}, "independent_lineage_ids": {"$ref": "#/$defs/Ids"},
        "agreed_facts": {"$ref": "#/$defs/Texts"}, "disputed_facts": {"$ref": "#/$defs/Texts"}, "resolution_evidence_ids": {"$ref": "#/$defs/Ids"},
        "unresolved_material_conflicts": {"$ref": "#/$defs/Texts"}, "coverage_status": {"$ref": "#/$defs/Coverage"}
      },
      "required": ["claim_id", "primary_item_ids", "independent_lineage_ids", "agreed_facts", "disputed_facts", "resolution_evidence_ids", "unresolved_material_conflicts", "coverage_status"]
    },
    "SkillFinding": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "skill_name": {"type": "string", "pattern": "^[a-z][a-z0-9-]{0,63}$"},
        "skill_version": {"type": "string", "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$"},
        "status": {"enum": ["COMPLETED", "BLOCKED"]}, "artifact_refs": {"$ref": "#/$defs/ArtifactPointers"}, "blockers": {"$ref": "#/$defs/Texts"}
      },
      "required": ["skill_name", "skill_version", "status", "artifact_refs", "blockers"]
    },
    "ReadinessCheck": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "code": {"enum": ["RESEARCH_FINAL", "REQUIRED_SKILLS", "PRIMARY_VERIFIED", "INDEPENDENT_COMPARISON", "NO_MATERIAL_CONFLICTS", "CONFIDENCE_HIGH", "MARKET_ECONOMICS", "ACCOUNT_ELIGIBLE"]},
        "status": {"enum": ["PASS", "FAIL", "UNKNOWN"]}, "evidence_refs": {"$ref": "#/$defs/Ids"}
      },
      "required": ["code", "status", "evidence_refs"]
    },
    "EntryReadiness": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "checks": {"type": "array", "items": {"$ref": "#/$defs/ReadinessCheck"}, "minItems": 1, "maxItems": 8},
        "confidence_calculation_id": {"$ref": "#/$defs/NullableId"}, "economics_calculation_id": {"$ref": "#/$defs/NullableId"}, "blockers": {"$ref": "#/$defs/Texts"}
      },
      "required": ["checks", "confidence_calculation_id", "economics_calculation_id", "blockers"]
    },
    "Observation": {
      "type": "object", "additionalProperties": false,
      "properties": {"statement": {"$ref": "#/$defs/Text"}, "reference_ids": {"$ref": "#/$defs/Ids"}},
      "required": ["statement", "reference_ids"]
    },
    "Lesson": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "hypothesis": {"$ref": "#/$defs/Text"}, "evidence_ids": {"$ref": "#/$defs/Ids"}, "counterexamples": {"$ref": "#/$defs/Texts"},
        "sample_count": {"type": "integer", "minimum": 0}, "next_test": {"$ref": "#/$defs/Text"}
      },
      "required": ["hypothesis", "evidence_ids", "counterexamples", "sample_count", "next_test"]
    },
    "Review": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "summary": {"$ref": "#/$defs/Text"}, "observations": {"type": "array", "items": {"$ref": "#/$defs/Observation"}, "maxItems": 16},
        "lessons": {"type": "array", "items": {"$ref": "#/$defs/Lesson"}, "maxItems": 8}
      },
      "required": ["summary", "observations", "lessons"]
    },
    "PerpetualReview": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "assessment_id": {"$ref": "#/$defs/Id"}, "influence": {"enum": ["USED", "DISAGREED", "ABSTAINED", "UNAVAILABLE"]},
        "reason": {"type": "string", "minLength": 1, "maxLength": 2000}
      },
      "required": ["assessment_id", "influence", "reason"]
    },
    "FocusRefs": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "universe_snapshot_id": {"$ref": "#/$defs/Id"},
        "screening_artifact_id": {"$ref": "#/$defs/NullableId"},
        "profile_review_id": {"$ref": "#/$defs/NullableId"},
        "relationship_review_id": {"$ref": "#/$defs/NullableId"},
        "watchdog_template_id": {"$ref": "#/$defs/NullableId"},
        "opportunity_comparison_id": {"$ref": "#/$defs/NullableId"},
        "perpetual_review": {"anyOf": [{"$ref": "#/$defs/PerpetualReview"}, {"type": "null"}]}
      },
      "required": ["universe_snapshot_id", "screening_artifact_id", "profile_review_id", "relationship_review_id", "watchdog_template_id", "opportunity_comparison_id", "perpetual_review"]
    },
    "Research": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "schema_version": {"const": 2}, "scope": {"$ref": "#/$defs/Scope"}, "workflow_status": {"enum": ["DRAFT", "BLOCKED", "FINAL"]},
        "focus": {"anyOf": [{"$ref": "#/$defs/FocusRefs"}, {"type": "null"}]},
        "search_log": {"type": "array", "items": {"$ref": "#/$defs/SearchRecord"}, "maxItems": 16},
        "claims": {"type": "array", "items": {"$ref": "#/$defs/Claim"}, "maxItems": 8},
        "source_comparison": {"type": "array", "items": {"$ref": "#/$defs/Comparison"}, "maxItems": 8},
        "skill_findings": {"type": "array", "items": {"$ref": "#/$defs/SkillFinding"}, "maxItems": 24},
        "entry_readiness": {"anyOf": [{"$ref": "#/$defs/EntryReadiness"}, {"type": "null"}]},
        "review": {"anyOf": [{"$ref": "#/$defs/Review"}, {"type": "null"}]}
      },
      "required": ["schema_version", "scope", "workflow_status", "focus", "search_log", "claims", "source_comparison", "skill_findings", "entry_readiness", "review"]
    },
    "ContextRefs": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "account_snapshot_id": {"$ref": "#/$defs/NullableId"}, "market_snapshot_id": {"$ref": "#/$defs/NullableId"},
        "confidence_calculation_id": {"$ref": "#/$defs/NullableId"}, "economics_calculation_id": {"$ref": "#/$defs/NullableId"}
      },
      "required": ["account_snapshot_id", "market_snapshot_id", "confidence_calculation_id", "economics_calculation_id"]
    },
    "CommonProposal": {
      "type": "object",
      "properties": {
        "schema_version": {"const": 1}, "kind": {"enum": ["ENTRY_PROPOSAL", "PLAN_UPDATE", "HOLD", "NO_ACTION"]},
        "instrument_id": {"$ref": "#/$defs/NullableId"}, "context_refs": {"$ref": "#/$defs/ContextRefs"},
        "evidence_ids": {"$ref": "#/$defs/Ids"}, "reason": {"$ref": "#/$defs/Text"}, "thesis": {"$ref": "#/$defs/NullableText"},
        "invalidation_conditions": {"$ref": "#/$defs/Texts"}, "uncertainties": {"$ref": "#/$defs/Texts"}
      },
      "required": ["schema_version", "kind", "instrument_id", "context_refs", "evidence_ids", "reason", "thesis", "invalidation_conditions", "uncertainties"]
    },
    "EntryRange": {
      "type": "object", "additionalProperties": false,
      "properties": {"currency": {"$ref": "#/$defs/Currency"}, "minimum": {"$ref": "#/$defs/Decimal"}, "maximum": {"$ref": "#/$defs/Decimal"}},
      "required": ["currency", "minimum", "maximum"]
    },
    "Trailing": {
      "type": "object", "additionalProperties": false,
      "properties": {"activation_net_fraction": {"$ref": "#/$defs/Decimal"}, "distance_fraction": {"$ref": "#/$defs/Decimal"}},
      "required": ["activation_net_fraction", "distance_fraction"]
    },
    "ExceptionalCatalyst": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "claim_ids": {"$ref": "#/$defs/Ids"}, "reason": {"$ref": "#/$defs/Text"}, "expires_at": {"$ref": "#/$defs/Time"},
        "primary_evidence_ids": {"$ref": "#/$defs/Ids"}, "independent_lineage_ids": {"$ref": "#/$defs/Ids"}
      },
      "required": ["claim_ids", "reason", "expires_at", "primary_evidence_ids", "independent_lineage_ids"]
    },
    "Thresholds": {
      "type": "object",
      "properties": {
        "currency": {"$ref": "#/$defs/Currency"}, "stop_price": {"$ref": "#/$defs/Decimal"}, "target_net_fraction": {"$ref": "#/$defs/Decimal"},
        "target_price_hint": {"$ref": "#/$defs/NullableDecimal"}, "trailing": {"$ref": "#/$defs/Trailing"}, "holding_deadline_session": {"$ref": "#/$defs/Id"},
        "exceptional_catalyst": {"anyOf": [{"$ref": "#/$defs/ExceptionalCatalyst"}, {"type": "null"}]}
      },
      "required": ["currency", "stop_price", "target_net_fraction", "target_price_hint", "trailing", "holding_deadline_session", "exceptional_catalyst"]
    },
    "EntryProposal": {
      "allOf": [
        {"$ref": "#/$defs/CommonProposal"}, {"$ref": "#/$defs/Thresholds"},
        {"properties": {"kind": {"const": "ENTRY_PROPOSAL"}, "instrument_id": {"$ref": "#/$defs/Id"}, "entry_range": {"$ref": "#/$defs/EntryRange"}}, "required": ["entry_range"]}
      ],
      "unevaluatedProperties": false
    },
    "PlanUpdate": {
      "allOf": [
        {"$ref": "#/$defs/CommonProposal"}, {"$ref": "#/$defs/Thresholds"},
        {"properties": {"kind": {"const": "PLAN_UPDATE"}, "instrument_id": {"$ref": "#/$defs/Id"}, "position_id": {"$ref": "#/$defs/Id"}, "expected_plan_version": {"type": "integer", "minimum": 1}}, "required": ["position_id", "expected_plan_version"]}
      ],
      "unevaluatedProperties": false
    },
    "Hold": {
      "allOf": [
        {"$ref": "#/$defs/CommonProposal"},
        {"properties": {"kind": {"const": "HOLD"}, "instrument_id": {"$ref": "#/$defs/Id"}, "position_id": {"$ref": "#/$defs/Id"}, "expected_plan_version": {"type": "integer", "minimum": 1}}, "required": ["position_id", "expected_plan_version"]}
      ],
      "unevaluatedProperties": false
    },
    "NoAction": {"allOf": [{"$ref": "#/$defs/CommonProposal"}, {"properties": {"kind": {"const": "NO_ACTION"}}}], "unevaluatedProperties": false},
    "ProposalBody": {"oneOf": [{"$ref": "#/$defs/EntryProposal"}, {"$ref": "#/$defs/PlanUpdate"}, {"$ref": "#/$defs/Hold"}, {"$ref": "#/$defs/NoAction"}]},
    "Final": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "protocol_version": {"const": 1}, "message_type": {"const": "FINAL"}, "reply": {"$ref": "#/$defs/Reply"},
        "research": {"anyOf": [{"$ref": "#/$defs/Research"}, {"type": "null"}]}, "proposal": {"anyOf": [{"$ref": "#/$defs/ProposalBody"}, {"type": "null"}]}
      },
      "required": ["protocol_version", "message_type", "reply", "research", "proposal"],
      "allOf": [
        {"if": {"properties": {"reply": {"properties": {"status": {"enum": ["NEEDS_INPUT", "INCOMPLETE"]}}}}}, "then": {"properties": {"proposal": {"type": "null"}}}},
        {"if": {"properties": {"proposal": {"type": "object", "properties": {"kind": {"const": "ENTRY_PROPOSAL"}}, "required": ["kind"]}}}, "then": {"properties": {"research": {"allOf": [{"$ref": "#/$defs/Research"}, {"properties": {"workflow_status": {"const": "FINAL"}, "entry_readiness": {"$ref": "#/$defs/EntryReadiness"}}}]}}}}
      ]
    },
    "ToolError": {
      "type": "object", "additionalProperties": false,
      "properties": {"code": {"$ref": "#/$defs/Id"}, "message": {"$ref": "#/$defs/Text"}, "retryable": {"type": "boolean"}},
      "required": ["code", "message", "retryable"]
    },
    "ToolMeta": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "as_of": {"$ref": "#/$defs/NullableTime"}, "snapshot_id": {"$ref": "#/$defs/NullableId"}, "source_ids": {"$ref": "#/$defs/Ids"},
        "freshness": {"enum": ["CURRENT", "STALE", "UNKNOWN"]}, "truncated": {"type": "boolean"},
        "next_cursor": {"anyOf": [{"type": "string", "maxLength": 2000}, {"type": "null"}]}
      },
      "required": ["as_of", "snapshot_id", "source_ids", "freshness", "truncated", "next_cursor"]
    },
    "ToolResult": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "call_id": {"$ref": "#/$defs/Id"}, "tool": {"type": "string", "maxLength": 96},
        "status": {"enum": ["OK", "PARTIAL", "STALE", "ERROR", "DENIED", "UNAVAILABLE", "CANCELLED"]},
        "data": {"type": ["object", "array", "null"]}, "error": {"anyOf": [{"$ref": "#/$defs/ToolError"}, {"type": "null"}]}, "meta": {"$ref": "#/$defs/ToolMeta"}
      },
      "required": ["call_id", "tool", "status", "data", "error", "meta"],
      "if": {"properties": {"status": {"const": "OK"}}}, "then": {"properties": {"error": {"type": "null"}}}, "else": {"properties": {"error": {"$ref": "#/$defs/ToolError"}}}
    },
    "ToolResults": {
      "type": "object", "additionalProperties": false,
      "properties": {"protocol_version": {"const": 1}, "message_type": {"const": "TOOL_RESULTS"}, "results": {"type": "array", "items": {"$ref": "#/$defs/ToolResult"}, "minItems": 1, "maxItems": 4}},
      "required": ["protocol_version", "message_type", "results"]
    },
    "ApplicationProposalEnvelope": {
      "type": "object", "additionalProperties": false,
      "properties": {
        "envelope_version": {"const": 1}, "proposal_id": {"$ref": "#/$defs/Id"}, "run_id": {"$ref": "#/$defs/Id"}, "session_id": {"$ref": "#/$defs/Id"},
        "policy_version": {"type": "integer", "minimum": 1}, "context_manifest_id": {"$ref": "#/$defs/Id"}, "account_snapshot_id": {"$ref": "#/$defs/NullableId"},
        "research_dossier_id": {"$ref": "#/$defs/NullableId"}, "created_at": {"$ref": "#/$defs/Time"}, "valid_until": {"$ref": "#/$defs/Time"}, "body": {"$ref": "#/$defs/ProposalBody"}
      },
      "required": ["envelope_version", "proposal_id", "run_id", "session_id", "policy_version", "context_manifest_id", "account_snapshot_id", "research_dossier_id", "created_at", "valid_until", "body"]
    }
  }
}
```

## Semantic validation beyond schema

Run full schema validation with date/time format checking, then tool-specific argument/output schemas from the advertised registry. The generic `arguments`/`data` containers above are deliberately validated a second time against the exact selected tool/version; they are not permission to forward arbitrary fields to a provider.

Canonical timestamps are UTC with a `Z` suffix. The syntax pattern rejects prose and offsets, and a mandatory calendar/time parser rejects impossible dates. Some schema libraries treat `format` only as an annotation unless an appropriate checker is installed; startup validation must demonstrate actual rejection of malformed timestamps and impossible calendar dates rather than assuming the annotation is enforced.

For entry, require exactly one of each of the eight readiness codes, every status `PASS`, no blockers, and thirteen routed mandatory skill findings: six shared skills, sector-selection, chosen sector and subsector, company-profile, relationship-analysis, holding-watchdog and perpetual-review. Resolve names against the pinned approved catalog; a regex-valid unknown fails. Earlier findings carry forward only with validated current dependencies in the context manifest. Resolve every pointer and external record against actual scope; an empty artifact proves nothing. Require unique claims, current account/market/calculation references, primary/independent evidence, no material conflicts and recomputed score at least 90. Duplicate identities fail.

`focus` must be non-null for entry and resolve all fields except the normally null opportunity comparison to compatible validated records under [research contracts](./research-contracts.md). Require effective membership/classification, complete screening stages, current profile/relationship reviews, prepared watchdog and a usable [Perpetual assessment](./perpetual.md) matching final scenario/economics and current features. `USED` or a reasoned `DISAGREED` is valid; ML advice cannot set limits or add confidence points. Refresh/recheck assessment dependencies after quote/account changes; material changes require reassessment. An alternative while an old position is held cannot claim `ACCOUNT_ELIGIBLE=PASS`; complete readiness only after flat reconciliation.

Validate decimal signs/ranges and cross-field relationships in Rust: positive prices, ordered entry range, matching currencies, permitted target at most `0.10`, policy-compatible trailing settings, economics/reward checks and a supported holding deadline. `PLAN_UPDATE` must include every threshold field; its stop/deadline cannot loosen and its expected version must match. `HOLD` has no threshold fields, so it cannot implicitly alter them.

Only `ANSWER` may carry a proposal. A `NO_ACTION` proposal is an analyzed outcome with a normal answer; an `INCOMPLETE`/`NEEDS_INPUT` response instead has a null proposal and can retain blocked research. `NO_ACTION` cannot smuggle threshold fields because its schema rejects extras. Model-assessed `FINAL` status still cannot override a failed semantic check.

The application binds creation/expiry to trusted time, with proposal lifetime at most five minutes and no extension of evidence/catalyst validity. It creates non-null dossier/account bindings for entry. Executor refreshes create a separate accepted-readiness record without rewriting the model's original references. Tool-call correlation, allowed source/account scope, resource budgets and unresolved calls are checked outside JSON Schema too.

## Blocked research example

All identifiers below are illustrative. The articles and posts share one origin, so the comparison remains blocked. A structured result preserves the research without proposing entry.

```json
{
  "protocol_version": 1,
  "message_type": "FINAL",
  "reply": {
    "status": "INCOMPLETE",
    "text": "The product announcement is readable, but all returned coverage repeats the issuer. Independent confirmation is missing; no entry is proposed.",
    "evidence_ids": ["evidence_01"], "snapshot_ids": [], "questions": []
  },
  "research": {
    "schema_version": 2,
    "focus": null,
    "scope": {"question": "Is the example product a verified new catalyst?", "instrument_ids": ["instrument_03"], "as_of": "2026-10-30T14:00:00Z"},
    "workflow_status": "BLOCKED",
    "search_log": [
      {
        "source_id": "news_api_01", "query": "Example issuer product launch confirmation",
        "filters": {"instrument_ids": ["instrument_03"], "start_at": "2026-10-30T00:00:00Z", "end_at": null, "source_domains": []},
        "tool_call_ids": ["call_news_01"], "result_item_ids": ["primary_item_01", "copied_item_02"],
        "coverage_status": "PARTIAL", "gaps": ["No independent factual confirmation found in the bounded results."]
      }
    ],
    "claims": [
      {
        "claim_id": "claim_01", "instrument_id": "instrument_03", "statement": "The issuer announced a product launch.", "claim_type": "FACT",
        "published_at": "2026-10-30T13:00:00Z", "event_at": "2026-10-30T13:00:00Z", "materiality": "DIRECT",
        "support_item_ids": ["primary_item_01", "copied_item_02"], "contradiction_item_ids": [], "primary_verification_ids": ["verification_01"],
        "unknowns": ["Independent confirmation of commercial availability."]
      }
    ],
    "source_comparison": [
      {
        "claim_id": "claim_01", "primary_item_ids": ["primary_item_01"], "independent_lineage_ids": [],
        "agreed_facts": ["The issuer published the announcement."], "disputed_facts": [], "resolution_evidence_ids": [],
        "unresolved_material_conflicts": [], "coverage_status": "PARTIAL"
      }
    ],
    "skill_findings": [
      {"skill_name": "source-comparison", "skill_version": "1.0.0", "status": "BLOCKED", "artifact_refs": ["#/research/source_comparison"], "blockers": ["Independent reporting lineage missing."]}
    ],
    "entry_readiness": {
      "checks": [{"code": "INDEPENDENT_COMPARISON", "status": "FAIL", "evidence_refs": ["verification_01"]}],
      "confidence_calculation_id": null, "economics_calculation_id": null, "blockers": ["Research is not finalized."]
    },
    "review": null
  },
  "proposal": null
}
```

## Final entry-research example

This synthetic enterprise-software example assumes the tool/verifier records and validated sector/group/profile/watchdog artifacts exist, calculations report score 93 and acceptable costs/risk, and all snapshots/policy/skill versions pass. A score of 93 can arise from 25 + 20 + 8 + 20 + 10 + 10 points. The payload demonstrates shape, not evidence of a real eligible trade. The application verifies these assumptions and the executor refreshes execution inputs.

```json
{
  "protocol_version": 1,
  "message_type": "FINAL",
  "reply": {
    "status": "ANSWER", "text": "Research is finalized for the example candidate. A normal 1% net target is proposed, subject to the executor's current readiness and price checks.",
    "evidence_ids": ["evidence_01", "evidence_02"], "snapshot_ids": ["account_snapshot_04", "market_snapshot_05"], "questions": []
  },
  "research": {
    "schema_version": 2,
    "focus": {
      "universe_snapshot_id": "universe_01", "screening_artifact_id": "finalist_scan_03", "profile_review_id": "profile_review_03",
      "relationship_review_id": "relations_review_03", "watchdog_template_id": "watchdog_template_03", "opportunity_comparison_id": null,
      "perpetual_review": {"assessment_id": "ml_assessment_03", "influence": "USED", "reason": "The validated scenario assessment supports retaining the ordinary 1% target; final limits remain the LLM proposal."}
    },
    "scope": {"question": "Evaluate an approved example stock for entry.", "instrument_ids": ["instrument_03"], "as_of": "2026-10-30T14:00:00Z"},
    "workflow_status": "FINAL",
    "search_log": [
      {
        "source_id": "issuer_source_01", "query": "Example issuer product release and limitations",
        "filters": {"instrument_ids": ["instrument_03"], "start_at": "2026-10-30T00:00:00Z", "end_at": null, "source_domains": []},
        "tool_call_ids": ["call_primary_01"], "result_item_ids": ["primary_item_01"], "coverage_status": "COMPLETE", "gaps": []
      },
      {
        "source_id": "news_api_01", "query": "Example issuer launch independent confirmation correction delays",
        "filters": {"instrument_ids": ["instrument_03"], "start_at": "2026-10-30T00:00:00Z", "end_at": null, "source_domains": []},
        "tool_call_ids": ["call_news_02"], "result_item_ids": ["report_item_02"], "coverage_status": "COMPLETE", "gaps": []
      }
    ],
    "claims": [
      {
        "claim_id": "claim_01", "instrument_id": "instrument_03", "statement": "The product launch has primary and independent supporting evidence.", "claim_type": "FACT",
        "published_at": "2026-10-30T13:00:00Z", "event_at": "2026-10-30T13:00:00Z", "materiality": "DIRECT",
        "support_item_ids": ["primary_item_01", "report_item_02"], "contradiction_item_ids": [], "primary_verification_ids": ["verification_01"], "unknowns": []
      }
    ],
    "source_comparison": [
      {
        "claim_id": "claim_01", "primary_item_ids": ["primary_item_01"], "independent_lineage_ids": ["lineage_reporter_02"],
        "agreed_facts": ["A new commercial launch is documented."], "disputed_facts": [], "resolution_evidence_ids": [],
        "unresolved_material_conflicts": [], "coverage_status": "COMPLETE"
      }
    ],
    "skill_findings": [
      {"skill_name": "conversation-and-tools", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/scope"], "blockers": []},
      {"skill_name": "market-research", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/scope", "#/research/claims"], "blockers": []},
      {"skill_name": "search-planning", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/search_log"], "blockers": []},
      {"skill_name": "news-analysis", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/claims"], "blockers": []},
      {"skill_name": "source-comparison", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/source_comparison"], "blockers": []},
      {"skill_name": "entry-readiness", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/entry_readiness"], "blockers": []},
      {"skill_name": "sector-selection", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/focus/screening_artifact_id"], "blockers": []},
      {"skill_name": "sector-information-technology", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/focus/screening_artifact_id"], "blockers": []},
      {"skill_name": "subsector-software-services", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/focus/screening_artifact_id"], "blockers": []},
      {"skill_name": "company-profile", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/focus/profile_review_id"], "blockers": []},
      {"skill_name": "relationship-analysis", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/focus/relationship_review_id"], "blockers": []},
      {"skill_name": "holding-watchdog", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/focus/watchdog_template_id"], "blockers": []},
      {"skill_name": "perpetual-review", "skill_version": "1.0.0", "status": "COMPLETED", "artifact_refs": ["#/research/focus/perpetual_review"], "blockers": []}
    ],
    "entry_readiness": {
      "checks": [
        {"code": "RESEARCH_FINAL", "status": "PASS", "evidence_refs": ["claim_01"]},
        {"code": "REQUIRED_SKILLS", "status": "PASS", "evidence_refs": ["skill_bundle_01"]},
        {"code": "PRIMARY_VERIFIED", "status": "PASS", "evidence_refs": ["verification_01"]},
        {"code": "INDEPENDENT_COMPARISON", "status": "PASS", "evidence_refs": ["lineage_reporter_02"]},
        {"code": "NO_MATERIAL_CONFLICTS", "status": "PASS", "evidence_refs": ["evidence_01", "evidence_02"]},
        {"code": "CONFIDENCE_HIGH", "status": "PASS", "evidence_refs": ["confidence_01"]},
        {"code": "MARKET_ECONOMICS", "status": "PASS", "evidence_refs": ["economics_01", "market_snapshot_05"]},
        {"code": "ACCOUNT_ELIGIBLE", "status": "PASS", "evidence_refs": ["account_snapshot_04"]}
      ],
      "confidence_calculation_id": "confidence_01", "economics_calculation_id": "economics_01", "blockers": []
    },
    "review": null
  },
  "proposal": {
    "schema_version": 1, "kind": "ENTRY_PROPOSAL", "instrument_id": "instrument_03",
    "context_refs": {"account_snapshot_id": "account_snapshot_04", "market_snapshot_id": "market_snapshot_05", "confidence_calculation_id": "confidence_01", "economics_calculation_id": "economics_01"},
    "evidence_ids": ["evidence_01", "evidence_02"], "reason": "Verified evidence and deterministic checks support the bounded setup.",
    "thesis": "A verified material launch has current price confirmation.", "invalidation_conditions": ["A verified correction contradicts commercial availability."], "uncertainties": [],
    "entry_range": {"currency": "USD", "minimum": "99.80", "maximum": "100.00"},
    "currency": "USD", "stop_price": "99.70", "target_net_fraction": "0.01", "target_price_hint": null,
    "trailing": {"activation_net_fraction": "0.005", "distance_fraction": "0.005"},
    "holding_deadline_session": "NYSE:2026-11-05", "exceptional_catalyst": null
  }
}
```

## Verification

Compile this schema under JSON Schema 2020-12, validate the worked messages in [LLM conversations and tools](./llm-and-tools.md), and validate the research-bearing examples below. Negative cases must reject model-supplied envelope fields, unknown artifact fields, incomplete replacement thresholds and invalid final/reply combinations. Static schema validation does not establish that referenced external evidence is true or current.

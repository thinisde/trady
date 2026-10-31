# Earnings-call and management Q&A intelligence (proposed)

Status: optional specialist research feature, no transcript ingestion or model-authoritative analysis shipped. **Only `deepseek-v4.1-flash` through OpenCodeGo** for LLM extraction/analysis, with the narrowly controlled Pro reservation in [model policy](./model-routing-and-governance.md).

## Distinction and goals

Trady's [company intelligence](../company-intelligence.md) plans earnings releases and management statements. This feature specifically studies **multi-quarter call transcripts, analyst questions, management answers and the difference between words and audited numbers**. It should detect changed commitments, unanswered specific questions, repeated qualifiers and quantitative contradictions with filings. It cannot reliably infer lying or management emotion from words, punctuation or speech rate.

## Ingestion and licensing

Input sources: issuer investor-relations transcripts, permitted licensed call transcript archives, official slides, audio only if a provider offers a **rights-approved transcript**. Do not silently invoke another speech-recognition/generative model: under the user's model restriction, unsupported audio becomes `TRANSCRIPT_UNAVAILABLE`. If text transcription is later offered by the approved OpenCodeGo exact model and endpoint, verify that capability and rights explicitly first.

Preserve call start/end, speaker, title, employer, speech segment boundary, moderator/analyst role, question-answer pairing, correction, publication time, language, original URL and rights revision. Do not treat a third-party unlicensed transcript as public-domain text.

## Pipeline

1. Verify issuer/period and map transcripts to filing/release events as of decision time.
2. Segment call by official speaker and Q&A, using deterministic delimiters where reliable. Flash may propose segment/speaker extraction with uncertainty and must quote exact permitted paragraph locators.
3. Extract commitments as typed records: metric definition, numerical value/range, period, conditional language, owner/speaker and source. Keep a literal verifiable quote fragment inside rights boundaries (do not forward full transcript to unauthorized providers).
4. Compare each current statement with prior quarter's comparable metric/statement, allowing units, fiscal timing, acquisitions and guidance bases to change. Separate **new fact**, **changed estimate**, **changed emphasis**, **ambiguous wording**, **direct contradiction**.
5. Inspect analyst questions with incomplete answers; mark `UNRESOLVED_QUESTION` instead of inferring intentional evasion.
6. Cross-check numerical claims against SEC facts when source timing and units match. LLM proposes explanations; Rust validates number transformations.
7. Output a concise evidence-backed call analysis to research memory. A revised thesis goes through [thesis falsification](./thesis-falsification.md) and the normal policy gate.

## Proposed typed record

```json
{
  "schema_version": 1,
  "call_id": "issuer42-2026q3",
  "segment_id": "qna-12",
  "speaker_role": "CFO",
  "statement_type": "FORWARD_GUIDANCE",
  "metric": "operating_margin",
  "period": "2027-FY",
  "source_ids": ["licensed-transcript-42"],
  "prior_comparison_id": "issuer42-2026q2",
  "assessment": "REVIEW_CHANGED_ASSUMPTION",
  "evidence_status": "UNVERIFIED",
  "model_id": "deepseek-v4.1-flash"
}
```

Synthetic. A speech assessment is not an independently measured future return.

## Implementation and constraints

Add reviewed `calls.read_segments`, `calls.compare_quarters` and `issuer.statement_history` tools to the analyst registry with token quotas. A Rust transcript parser ensures evidence locators/permission checks; no new audio model or external LLM provider. Index original segments with stable ids, corrections, source lineage and retention; redact personally identifying information irrelevant to public market statements. Bound concurrent calls, prioritize current position, and cache issuer/quarter extraction by content hash.

## Evaluation, failure and acceptance

Create human-reviewed transcript question/answer and guidance-change benchmarks. Measure correct speaker attribution, metric extraction precision, period/units correctness, false contradiction alerts, recall of missed risk changes, source/rights violations and timing. Compare Flash transcript feature to filings-only research and calculate additional cost. Apply chronological historical tests for usefulness; a good extracted difference does not prove profit.

Acceptance: multiple speakers with same surname; corrected transcripts; missing Q&A; nonstandard fiscal periods; ambiguous metric names; after-market call; unsupported language; no transcript rights; audio-only event. Fail as UNAVAILABLE/REVIEW_REQUIRED rather than fabricating a speaker, transcript or numerical result. No direct trade instruction; native stop/target and order controls stay independent.

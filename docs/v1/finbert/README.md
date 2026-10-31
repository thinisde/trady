# Self-hosted FinBERT sentiment

Status: Docker files, Rust provider/application adapters, a CLI scoring command and
a pure threshold calculation are implemented. The service has not been built or
started and no weights have been downloaded. Durable sentiment jobs, readiness
dossier binding, LLM tool registration and automatic plan activation remain v1
integration work. Automated trading is still unavailable.

## Ownership and evidence flow

The [prediction-fusion contract](../prediction-fusion.md) treats this output as
financial-text sentiment. Positive/negative/neutral class probabilities are never
target-hit probabilities, expected returns or extra independent news lineages.
The existing adverse overlay remains a mandatory constraint on formula selection.
Original item/model/time bindings feed cumulative calibration and ablation; no
learned fusion weight or sentiment-to-return mapping is approved by this document.

Trady sends permitted English X posts, news, issuer releases and other related
texts to its own FinBERT service. The analyst owns collection, relevance selection
and inference. Rust policy uses the resulting observations when calculating entry
eligibility and profit-target ceilings. The executor alone validates and activates
a changed plan. FinBERT has no broker, database, Telegram or LLM credentials.

The Rust application remains Rust. The isolated Python inference container uses
PyTorch and Hugging Face Transformers to serve `ProsusAI/finbert`; it is separate
from the [native Rust Perpetual advisor](../perpetual.md). Sentiment describes text
tone, not expected returns, factual truth, exceptional-catalyst verification or a
probability that a trade will win.

The planned production flow is collection → rights/provenance and issuer checks →
one text per reporting lineage → FinBERT → correlated sentiment assessment →
versioned Rust threshold calculation → readiness dossier or holding review →
executor revalidation. No executor lock or financial transaction spans inference.

## Interface

| Implemented boundary | Contract |
|---|---|
| [Domain records](../../../crates/trady-domain/src/sentiment/mod.rs) | `SentimentDocument`, `SentimentProbabilities`, `SentimentPrediction`, `SentimentBatch`, `SentimentEvidence`, `SentimentAssessment` |
| [Application port](../../../crates/trady-application/src/sentiment/mod.rs) | `SentimentProvider::predict`, `model_revision`, `assess_sentiment`, exact UTF-8 `text_sha256` |
| [HTTP adapter](../../../crates/trady-adapters/src/finbert/mod.rs) | `FinBertConfig::validate`, `FinBertClient::new`, bounded `SentimentProvider` implementation |
| [Threshold policy](../../../crates/trady-policy/src/sentiment/mod.rs) | `validate_probabilities`, `sentiment_thresholds`, `SentimentThresholds` |
| [Service contract](../../../services/finbert/README.md) | `POST /v1/sentiment`, `GET /health`, pinned CPU inference |
| [CLI](../../../crates/trady-cli/src/finbert/mod.rs) | `sentiment status --config FILE`, `sentiment score --config FILE --input FILE` |

The model ID is `ProsusAI/finbert`; the shipped immutable revision is
`0bc2ac10dda6929bcb1f77ccc43c7ec5c424f031`. The service loads tokenizer and
model from the same revision, uses safetensors, disables remote model code, sets
evaluation/inference mode and reads class names from `config.id2label`. The
published labels are positive, negative and neutral; array order is never treated
as an unlabeled sentiment score. These details come from the
[pinned model configuration](https://huggingface.co/ProsusAI/finbert/blob/0bc2ac10dda6929bcb1f77ccc43c7ec5c424f031/config.json)
and [Transformers classification API](https://huggingface.co/docs/transformers/v4.57.3/en/tasks/sequence_classification).

The [model repository metadata](https://huggingface.co/ProsusAI/finbert) declares
`CC-BY-NC-SA-4.0`. Record model-use and redistribution terms alongside source-data
permissions before selecting a production deployment. Self-hosting does not grant
rights to process or retain publisher/X content.

## Wire contract and bounds

`POST /v1/sentiment` accepts `application/json`, contract version 1, one to sixteen
documents and at most 4,096 Unicode characters per text. Request bodies are capped
at 512 KiB and Rust responses at 64 KiB. IDs are unique and at most 256 UTF-8 bytes.
The service rejects extra request/document keys, invalid hashes, empty text and
duplicate IDs. Requests have a configured 1–60 second Rust timeout (30 by default),
no redirects and no ambient proxy. HTTP is limited to loopback or the configured
`finbert` Compose hostname; a separately configured remote endpoint requires HTTPS.

Example request:

```json
{
  "schema_version": 1,
  "documents": [{
    "item_id": "article-1",
    "text_sha256": "5339526b9c7d2753c5dc1f42df57f239709908acf7584cd79834a425eeadae24",
    "text": "The company raised its earnings outlook."
  }]
}
```

An illustrative response, not a measured model prediction:

```json
{
  "schema_version": 1,
  "model_id": "ProsusAI/finbert",
  "model_revision": "0bc2ac10dda6929bcb1f77ccc43c7ec5c424f031",
  "predictions": [{
    "item_id": "article-1",
    "text_sha256": "5339526b9c7d2753c5dc1f42df57f239709908acf7584cd79834a425eeadae24",
    "probabilities": {
      "positive": "0.800000000",
      "negative": "0.100000000",
      "neutral": "0.100000000"
    },
    "truncated": false
  }]
}
```

Softmax probabilities cross the wire as decimal strings, each in `[0,1]`, with
sum within `0.000001` of one. Rust checks the exact configured revision, item set
and submitted-text hash. A missing/duplicate prediction, unknown field, invalid
distribution or provider failure is an error. Tokenization is capped at 512 tokens,
including special tokens; the service reports truncation and the application
rejects any truncated result for threshold use. There is no silent neutral fallback.

## Source selection and freshness

`assess_sentiment` consumes normalized `SourceItem` records, including the `items`
object from `news read --for-model`. It reapplies `SourceItem::redact(now, true)`;
both storage and model-processing permission are required, including for this
self-hosted classifier. Disabled or unreviewed source registries remain disabled.

The caller resolves the issuer/instrument and supplies one representative per
independent reporting lineage. Duplicate lineages fail the batch. English must be
explicitly recorded as `language: "en"`; unknown/other languages and X material
with missing conversation context are unusable. FinBERT is not a reliable sarcasm,
meme, repost-endorsement or multilingual truth detector.

Text selection is full text, otherwise permitted nonempty summary, otherwise
permitted title. The hash binds exactly that selected text, rather than a different
source-item content hash. It does not concatenate quoted/reposted context into the
speaker's own statement or infer an economic relationship from tone.

Event time, otherwise known publication time, must be no more than 24 hours old
and cannot be in the future. Re-fetch time cannot rejuvenate an old event. Each
evidence record retains its item/lineage/hash and original expiry. At threshold
calculation time, inference must be at most 15 minutes old, every source must still
be within 24 hours and every expiry must still be in the future. Recompute these
checks at use time; source edits, deletion, changed permission or changed relevance
must invalidate the bound assessment in the future durable pipeline.

## Threshold calculation version 1

The existing six-component [confidence score](../strategy.md#confidence-points)
remains the base. The initial conservative overlay is deterministic and has no
positive-score bonus:

| Observation | Effect |
|---|---|
| Maximum negative probability below `0.60` | Keep base points and their existing target ceiling |
| Any selected lineage has negative probability at least `0.60` | Subtract 10 points with a floor of zero; request a holding/evidence review; disallow target increases |
| Any selected lineage has negative probability at least `0.80` | Also veto entry, even if adjusted points still meet 90 |
| Missing, invalid, stale or unavailable required assessment | No sentiment-dependent entry or target increase; preserve active protection and request refresh |

Use the maximum adverse probability so repeated positive stories cannot average
away one adverse lineage. Report the mean of `p_positive - p_negative` for context;
it does not drive a separate trading rule. Recompute `target_ceiling` from adjusted
points with the existing exceptional-evidence and policy gates. For example, base
95 with `p_negative=0.65` becomes 85: entry fails and the permitted target ceiling
falls from 3% to 2%. Positive tone keeps the base ceiling; it cannot supply missing
primary evidence, price confirmation or readiness points.

`entry_score_passes` and `target_increase_allowed` describe this calculation only.
They do not authorize an order or plan activation. A checked holding update can
lower its target, tighten its stop or shorten expiry; it cannot widen a long stop,
extend the holding deadline, exceed 10%, cancel an exit latch or sell merely
because a post sounds negative. Existing quote, cost, account, reward/risk and
readiness checks remain mandatory. Evaluate the old plan/quote before activating
a replacement; already-triggered protection wins.

These thresholds are initial paper-evaluation settings, not calibrated return
probabilities or demonstrated trading performance. Version any later change and
evaluate it on rights-permitted, point-in-time issuer/X/news fixtures.

## Deployment files and later activation

[services/finbert/compose.yaml](../../../services/finbert/compose.yaml) declares an opt-in `finbert`
profile, localhost port `8091`, a non-root CPU service, two CPU threads, a 2 GiB
memory limit, a read-only root filesystem and the `finbert-cache` named volume.
`restart: "no"` keeps restart activation explicit. First startup loads the pinned
weights into the cache; image building installs dependencies without loading them.
Health becomes ready only after model loading succeeds. No database is required.

[services/finbert/compose.offline.yaml](../../../services/finbert/compose.offline.yaml) is an override
for an already populated cache. It enables local-files-only and Hub/Transformers
offline modes; missing weights make startup fail. This setting prevents model
downloads, not all container network access needed by the HTTP caller.

Safe configuration-only checks, also used for this change:

```sh
docker compose -f services/finbert/compose.yaml --profile finbert config --quiet
docker compose -f services/finbert/compose.yaml -f services/finbert/compose.offline.yaml --profile finbert config --quiet
cargo run --locked --bin trady -- sentiment status --config config/finbert/config.example.json
```

For later manual startup, the command is
`docker compose -f services/finbert/compose.yaml --profile finbert up --build -d finbert`.
It has not been run. Copy the [example configuration](../../../config/finbert/config.example.json)
to a deployment-owned configuration and explicitly set `enabled: true` before
scoring. A host CLI uses `http://127.0.0.1:8091/`; a containerized analyst on the
same private Compose network can use `http://finbert:8000/`.

The scoring command is `trady sentiment score --config FILE --input FILE`.
Its JSON input has an `items` array of complete normalized `SourceItem` records,
optionally `confidence_inputs` using the policy's named feature fields. With those
inputs it reports threshold calculations under the default policy and no
exceptional gate. Supplied features are unverified preview inputs; output always
has `evidence_verified=false` and `entry_authorized=false`. Without them the
command reports the assessment and `thresholds: null`. It writes no database.

## Invariants and side effects

`[../../../crates/trady-application/src/sentiment/mod.rs:assess_sentiment]` enforces rights,
batch/lineage correlation and source freshness before calling the provider.
`[../../../crates/trady-policy/src/sentiment/mod.rs:sentiment_thresholds]` is pure and
rechecks source/assessment freshness; it never mutates a plan or policy.
`[../../../services/finbert/serve.py:main]` alone loads the model and opens the listener;
module imports, configuration validation and offline tests do neither.

Explicit scoring sends permitted text to the configured service. Later service
startup can download pinned model artifacts into its cache; inference consumes
CPU/RAM and returns probabilities without persisting source text or logging request
content. Future durable assessment storage must follow the source's derivative
retention/deletion rules, as specified in [memory and data](../memory-and-data.md).

## Verification and remaining integration

```sh
cargo test -p trady-policy -p trady-application --test sentiment --locked
cargo test -p trady-adapters --test finbert --locked
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s services/finbert/tests -v
python3 scripts/verify-assets.py
```

Offline tests use deterministic predictions, in-memory handler streams and
one-shot mock HTTP fixtures. They do not establish container startup, actual
FinBERT quality, latency or resource capacity. Model/image smoke testing is deferred
because the service must remain unlaunched. Before automated integration, add
durable assessment/dependency storage, approved revision binding, instrument and
rights invalidation, required-assessment readiness gates, watchdog scheduling and
atomic executor plan checks. Do not advertise a new LLM tool until its exact
schema, persistence and scope validation are implemented.

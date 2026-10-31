# FinBERT inference service

## Overview

An optional CPU inference sidecar for Trady's permitted English research text.
The [v1 sentiment design](../../docs/v1/finbert/README.md) defines source permissions,
freshness, the HTTP contract, threshold effects and later activation. Files are
prepared; this change does not build the image, start the service or load weights.

## Interface

- `[serve.py:Handler.do_POST]`: `POST /v1/sentiment`, contract version 1,
  item IDs/text SHA-256, positive/negative/neutral probability strings and truncation flags.
- `[serve.py:Handler.do_GET]`: `GET /health`, readiness and loaded model identity.
- `[serve.py:validate_documents]`, `probability_strings`: request validation and
  finite, bounded, normalized decimal-string output.
- `[serve.py:FinBert]`, `FinBert.predict`, `main`: explicit model loading,
  CPU classification and listener startup.
- `[serve.py:MODEL_ID]`, `MODEL_REVISION`, `MAX_ITEMS`, `MAX_CHARS`,
  `MAX_BODY_BYTES`, `MAX_TOKENS`, `LABELS`: model identity and wire/token limits.
- [Dockerfile](./Dockerfile): dependency installation, UID/GID 10001 and startup command.
- [requirements.txt](./requirements.txt): CPU PyTorch and Transformers versions.
- [Compose service](./compose.yaml) and
  [offline override](./compose.offline.yaml): opt-in deployment/cache configuration.
- [Disabled client configuration](../../config/finbert/config.example.json) and
  [Rust adapter](../../crates/trady-adapters/src/finbert/mod.rs): explicit caller wiring.

## Invariants

- `[serve.py:__main__]` guards startup. Importing the module does not import heavy
  dependencies, download weights or bind sockets. `[serve.py:main]` loads the
  model before creating the HTTP listener.
- `[serve.py:FinBert.__init__]` loads `ProsusAI/finbert` tokenizer and safetensors
  at the same 40-character revision with remote code disabled; verifies the
  positive/negative/neutral label mapping; runs on CPU in evaluation mode.
- `[serve.py:validate_documents]` requires 1–16 unique item IDs, exact request keys,
  nonempty texts of at most 4,096 characters and matching hashes. IDs are at most
  256 UTF-8 bytes. `[serve.py:Handler.do_POST]` bounds bodies to 512 KiB and rejects
  transfer encoding or a non-JSON content type.
- `[serve.py:FinBert.predict]` applies inference mode, softmax and 512-token
  truncation, counting special tokens; it reports truncation instead of hiding it.
  `[serve.py:probability_strings]` rejects invalid distributions.
- `[serve.py:Handler.inference_lock]` permits one inference at a time; busy or
  failed inference returns 503 rather than neutral sentiment. The server uses
  request threads; deployment CPU/memory/task limits bound its resources.
- `[./compose.yaml:services.finbert]` binds host loopback port 8091,
  uses a read-only root filesystem with writable cache/tmp, drops capabilities,
  limits resources and disables automatic restart. The profile is opt-in.
- `[./compose.offline.yaml:services.finbert.environment]` requires
  cached weights and disables Hub/model downloads. Cache ownership must match
  UID/GID 10001; an incompatible existing volume is not repaired automatically.
- `[serve.py:Handler.log_message]` suppresses access logs. The service stores no
  source text or predictions and possesses no application or broker credentials.

## Side Effects

`[Dockerfile:RUN]` installs dependencies during a later explicit build. The pinned
direct packages/model revision do not fully lock the base image or transitive
dependencies. `[serve.py:main]` later loads/downloads weights into `HF_HOME`, sets
PyTorch thread count and listens on port 8000. Explicit inference consumes CPU/RAM
and returns JSON; healthchecks make local HTTP reads. Compose validation and the
offline unit tests do not start this service or download model artifacts.

## Verification

Run from the repository root:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s services/finbert/tests -v
docker compose -f services/finbert/compose.yaml --profile finbert config --quiet
docker compose -f services/finbert/compose.yaml -f services/finbert/compose.offline.yaml --profile finbert config --quiet
cargo test -p trady-adapters --test finbert --locked
python3 scripts/verify-assets.py
```

The tests use in-memory handlers and deterministic predictions; Rust tests use a
one-shot mock HTTP server. Actual model/image loading, latency and capacity remain
unverified because launching the service is deferred.

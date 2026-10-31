"""Bounded CPU FinBERT sidecar. Importing this module never loads a model or listens."""

import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import math
import os
import re
import threading

MODEL_ID = "ProsusAI/finbert"
MODEL_REVISION = "0bc2ac10dda6929bcb1f77ccc43c7ec5c424f031"
MAX_ITEMS = 16
MAX_CHARS = 4096
MAX_BODY_BYTES = 524288
MAX_TOKENS = 512
LABELS = {"positive", "negative", "neutral"}


def validate_documents(payload):
    if (not isinstance(payload, dict) or set(payload) != {"schema_version", "documents"}
            or type(payload["schema_version"]) is not int or payload["schema_version"] != 1):
        raise ValueError("invalid_schema")
    documents = payload["documents"]
    if not isinstance(documents, list) or not 1 <= len(documents) <= MAX_ITEMS:
        raise ValueError("invalid_batch_size")
    seen = set()
    for document in documents:
        if not isinstance(document, dict) or set(document) != {"item_id", "text_sha256", "text"}:
            raise ValueError("invalid_document")
        item_id, text, digest = document["item_id"], document["text"], document["text_sha256"]
        if (not isinstance(item_id, str) or not 1 <= len(item_id.encode("utf-8")) <= 256
                or item_id in seen or not isinstance(text, str) or not text.strip()
                or len(text) > MAX_CHARS or not isinstance(digest, str)
                or hashlib.sha256(text.encode("utf-8")).hexdigest() != digest):
            raise ValueError("invalid_document")
        seen.add(item_id)
    return documents


def probability_strings(values):
    if (set(values) != LABELS or any(not math.isfinite(v) or not 0 <= v <= 1 for v in values.values())
            or abs(sum(values.values()) - 1) > 0.000001):
        raise ValueError("invalid_model_probabilities")
    return {label: f"{values[label]:.9f}" for label in sorted(LABELS)}


class FinBert:
    def __init__(self, revision, local_files_only=False, threads=2):
        # Heavy dependencies and weights are loaded only by explicit service startup.
        import torch
        from transformers import AutoModelForSequenceClassification, AutoTokenizer

        if not re.fullmatch(r"[0-9a-f]{40}", revision) or not 1 <= threads <= 16:
            raise ValueError("invalid_model_configuration")
        self.revision = revision
        self.torch = torch
        torch.set_num_threads(threads)
        options = {"revision": revision, "local_files_only": local_files_only, "trust_remote_code": False}
        self.tokenizer = AutoTokenizer.from_pretrained(MODEL_ID, **options)
        self.model = AutoModelForSequenceClassification.from_pretrained(
            MODEL_ID, use_safetensors=True, **options
        ).to("cpu").eval()
        self.labels = {int(i): label.lower() for i, label in self.model.config.id2label.items()}
        if set(self.labels) != {0, 1, 2} or set(self.labels.values()) != LABELS:
            raise ValueError("unexpected_model_labels")

    def predict(self, documents):
        texts = [document["text"] for document in documents]
        original = self.tokenizer(texts, truncation=False, add_special_tokens=True)["input_ids"]
        inputs = self.tokenizer(texts, padding=True, truncation=True, max_length=MAX_TOKENS, return_tensors="pt")
        with self.torch.inference_mode():
            probabilities = self.model(**inputs).logits.softmax(dim=-1).tolist()
        predictions = []
        for document, tokens, row in zip(documents, original, probabilities, strict=True):
            predictions.append({
                "item_id": document["item_id"], "text_sha256": document["text_sha256"],
                "probabilities": probability_strings({self.labels[i]: value for i, value in enumerate(row)}),
                "truncated": len(tokens) > MAX_TOKENS,
            })
        return {"schema_version": 1, "model_id": MODEL_ID, "model_revision": self.revision,
                "predictions": predictions}


class Handler(BaseHTTPRequestHandler):
    predictor = None
    inference_lock = threading.Lock()
    protocol_version = "HTTP/1.0"

    def setup(self):
        super().setup()
        self.connection.settimeout(10)

    def log_message(self, *_args):
        # Do not log source content, URLs or headers.
        pass

    def respond(self, status, payload):
        body = json.dumps(payload, ensure_ascii=True, allow_nan=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/health" and self.predictor is not None:
            self.respond(200, {"status": "ready", "model_id": MODEL_ID,
                               "model_revision": self.predictor.revision})
        else:
            self.respond(404, {"error": "not_found"})

    def do_POST(self):
        if self.path != "/v1/sentiment":
            return self.respond(404, {"error": "not_found"})
        if self.headers.get_content_type() != "application/json" or self.headers.get("Transfer-Encoding"):
            return self.respond(400, {"error": "invalid_content_type_or_encoding"})
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if not 1 <= length <= MAX_BODY_BYTES:
                return self.respond(413, {"error": "body_size_limit"})
            body = self.rfile.read(length)
            if len(body) != length:
                raise ValueError("incomplete_body")
            documents = validate_documents(json.loads(body))
        except (ValueError, UnicodeError, OSError):
            return self.respond(400, {"error": "invalid_request"})
        if self.predictor is None:
            return self.respond(503, {"error": "model_unavailable"})
        if not self.inference_lock.acquire(blocking=False):
            return self.respond(503, {"error": "inference_busy"})
        try:
            result = self.predictor.predict(documents)
        except Exception:
            return self.respond(503, {"error": "inference_failed"})
        finally:
            self.inference_lock.release()
        self.respond(200, result)


def main():
    revision = os.environ.get("FINBERT_MODEL_REVISION", MODEL_REVISION)
    offline = os.environ.get("FINBERT_LOCAL_FILES_ONLY", "false")
    if offline not in {"true", "false"}:
        raise ValueError("invalid_offline_mode")
    Handler.predictor = FinBert(revision, offline == "true", int(os.environ.get("FINBERT_THREADS", "2")))
    with ThreadingHTTPServer(("0.0.0.0", 8000), Handler) as server:
        server.serve_forever()


if __name__ == "__main__":
    main()

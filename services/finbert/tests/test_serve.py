"""Offline contract/handler tests: no listener, weights, torch or Docker required."""

from email.message import Message
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import threading
import unittest

SPEC = importlib.util.spec_from_file_location("finbert_serve", Path(__file__).parents[1] / "serve.py")
serve = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(serve)


def payload(text="The company raised its earnings outlook."):
    return {"schema_version": 1, "documents": [{"item_id": "article-1", "text": text,
            "text_sha256": hashlib.sha256(text.encode()).hexdigest()}]}


class StubPredictor:
    revision = serve.MODEL_REVISION

    def predict(self, documents):
        return {"schema_version": 1, "model_id": serve.MODEL_ID, "model_revision": self.revision,
                "predictions": [{"item_id": d["item_id"], "text_sha256": d["text_sha256"],
                                 "probabilities": serve.probability_strings(
                                     {"positive": 0.8, "negative": 0.1, "neutral": 0.1}),
                                 "truncated": False} for d in documents]}


def handler(body, predictor=None, length=None):
    # Exercise the real handlers using streams, without binding a port.
    instance = object.__new__(serve.Handler)
    instance.path = "/v1/sentiment"
    instance.headers = Message()
    instance.headers["Content-Type"] = "application/json"
    instance.headers["Content-Length"] = str(len(body) if length is None else length)
    instance.rfile = io.BytesIO(body)
    instance.predictor = predictor
    instance.inference_lock = threading.Lock()
    instance.responses = []
    instance.respond = lambda status, result: instance.responses.append((status, result))
    return instance


class ContractTests(unittest.TestCase):
    def test_import_is_dormant_and_valid_hash_is_preserved(self):
        self.assertIsNone(serve.Handler.predictor)
        data = payload("Revenue increased €20 million.")
        self.assertEqual(serve.validate_documents(data), data["documents"])

    def test_rejects_empty_oversized_duplicate_or_changed_text(self):
        bad = [payload(" "), payload("x" * 4097), payload()]
        bad[2]["documents"][0]["text"] = "changed"
        duplicate = payload()
        duplicate["documents"] *= 2
        bad.extend([duplicate, {"schema_version": 1, "documents": []},
                    {"schema_version": True, "documents": payload()["documents"]}])
        for data in bad:
            with self.subTest(data=str(data)[:80]), self.assertRaises(ValueError):
                serve.validate_documents(data)

    def test_decimal_wire_probabilities_and_invalid_values(self):
        self.assertEqual(serve.probability_strings({"positive": 0.8, "negative": 0.1, "neutral": 0.1})["positive"], "0.800000000")
        for value in [float("nan"), float("inf"), -0.1, 1.1, 0.4]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                serve.probability_strings({"positive": value, "negative": 0.1, "neutral": 0.1})

    def test_post_correlates_item_hash_and_model_identity(self):
        data = payload()
        instance = handler(json.dumps(data).encode(), StubPredictor())
        instance.do_POST()
        status, result = instance.responses[0]
        self.assertEqual(status, 200)
        self.assertEqual(result["model_revision"], serve.MODEL_REVISION)
        prediction = result["predictions"][0]
        self.assertEqual(prediction["text_sha256"], data["documents"][0]["text_sha256"])
        self.assertEqual(prediction["item_id"], "article-1")

    def test_http_body_limits_errors_and_unavailable_model(self):
        for body, length, expected in [(b"{}", 524289, 413), (b"{", 1, 400),
                                        (b"{}", 4, 400), (json.dumps(payload()).encode(), None, 503)]:
            instance = handler(body, length=length)
            instance.do_POST()
            self.assertEqual(instance.responses[0][0], expected)

    def test_busy_and_failed_inference_do_not_return_neutral(self):
        instance = handler(json.dumps(payload()).encode(), StubPredictor())
        instance.inference_lock.acquire()
        instance.do_POST()
        self.assertEqual(instance.responses, [(503, {"error": "inference_busy"})])
        instance.inference_lock.release()
        instance.rfile.seek(0)
        instance.predictor.predict = lambda _docs: (_ for _ in ()).throw(RuntimeError("private text"))
        instance.do_POST()
        self.assertEqual(instance.responses[-1], (503, {"error": "inference_failed"}))
        self.assertFalse(instance.inference_lock.locked())


if __name__ == "__main__":
    unittest.main()

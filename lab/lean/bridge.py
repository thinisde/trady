"""QuantConnect-side bridge to the authoritative Rust calculator, without a shell."""
import hashlib
import json
import pathlib
import subprocess
import argparse
import sys

MAX_BYTES = 4 * 1024 * 1024


class RustFormulaBridge:
    def __init__(self, binary, expected_sha256):
        self.binary = pathlib.Path(binary)
        if not self.binary.is_absolute() or self.binary.is_symlink():
            raise ValueError("absolute pinned Rust binary required")
        if hashlib.sha256(self.binary.read_bytes()).hexdigest() != expected_sha256:
            raise ValueError("Rust bridge binary changed")

    def calculate(self, request):
        response = self.fixture({"kind": "FORMULA", **request})
        if response.get("status") != "ACCEPTED" or not isinstance(response.get("limits"), dict):
            raise ValueError("Rust formula rejected the scenario; abstain")
        return response["limits"]

    def fixture(self, request):
        """Exact wire entrypoint also used by the executable parity corpus."""
        data = json.dumps(request, separators=(",", ":"))
        if len(data.encode()) > MAX_BYTES:
            raise ValueError("formula input too large")
        result = subprocess.run([str(self.binary), "weekly", "formula-bridge", "--file", "-"],
                                input=data, text=True, capture_output=True, timeout=10,
                                env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"}, check=False)
        if result.returncode or len(result.stdout) > MAX_BYTES:
            raise RuntimeError("Rust formula unavailable; abstain")
        response = json.loads(result.stdout)
        if response.get("status") not in ("ACCEPTED", "DECIMAL", "REJECTED"):
            raise ValueError("invalid Rust formula response")
        return response


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", action="store_true", required=True)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--sha256", required=True)
    args = parser.parse_args()
    data = sys.stdin.buffer.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError("formula input too large")
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("duplicate fixture key")
            result[key] = value
        return result
    def reject(_):
        raise ValueError("exact decimal strings required")
    request = json.loads(data, object_pairs_hook=pairs, parse_float=reject, parse_constant=reject)
    reply = RustFormulaBridge(args.binary, args.sha256).fixture(request)
    print(json.dumps(reply, separators=(",", ":"), allow_nan=False))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print("LEAN formula fixture BLOCKED: " + type(error).__name__, file=sys.stderr)
        sys.exit(1)

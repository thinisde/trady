"""QuantConnect-side bridge to the authoritative Rust calculator, without a shell."""
import hashlib
import json
import pathlib
import os
import stat
import subprocess
import argparse
import sys
import fcntl
from contextlib import contextmanager

MAX_BYTES = 4 * 1024 * 1024
PARSE_REJECTED_PREFIX = "LEAN formula fixture PARSE_REJECTED: "
RUST_INPUT_DIAGNOSTICS = frozenset({
    "trady: invalid JSON or duplicate object key\n",
    "trady: trailing JSON data\n",
    "trady: input schema mismatch\n",
})


class FormulaInputRejected(ValueError):
    """A checked input parse/schema failure, never an execution failure."""


class RustFormulaBridge:
    def __init__(self, binary, expected_sha256):
        self.binary = pathlib.Path(binary)
        if not self.binary.is_absolute() or ".." in self.binary.parts:
            raise ValueError("absolute pinned Rust binary required")
        if not isinstance(expected_sha256, str) or len(expected_sha256) != 64 or any(c not in "0123456789abcdef" for c in expected_sha256):
            raise ValueError("exact Rust binary SHA256 required")
        self.expected_sha256 = expected_sha256
        self._verified_binary()

    def _verified_binary(self, destination=None):
        """Hash the opened regular file, optionally copying those exact bytes."""
        descriptor = os.open(self.binary, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
        meta = os.fstat(descriptor)
        if not stat.S_ISREG(meta.st_mode) or not meta.st_mode & 0o111:
            os.close(descriptor)
            raise ValueError("regular executable Rust binary required")
        with os.fdopen(descriptor, "rb") as source:
            before = os.fstat(source.fileno())
            digest = hashlib.sha256()
            while block := source.read(1024 * 1024):
                digest.update(block)
                if destination is not None:
                    destination.write(block)
            after = os.fstat(source.fileno())
            if (before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns) \
                    or digest.hexdigest() != self.expected_sha256:
                raise ValueError("Rust bridge binary changed")

    @contextmanager
    def _executable_snapshot(self):
        """Execute only verified, kernel-sealed bytes; never reopen the source."""
        descriptor = os.memfd_create("trady-formula", os.MFD_CLOEXEC | os.MFD_ALLOW_SEALING | getattr(os, "MFD_EXEC", 0))
        try:
            with os.fdopen(os.dup(descriptor), "wb") as snapshot:
                self._verified_binary(snapshot)
            os.fchmod(descriptor, 0o500)
            seals = fcntl.F_SEAL_WRITE | fcntl.F_SEAL_GROW | fcntl.F_SEAL_SHRINK | fcntl.F_SEAL_SEAL
            fcntl.fcntl(descriptor, fcntl.F_ADD_SEALS, seals)
            if fcntl.fcntl(descriptor, fcntl.F_GET_SEALS) & seals != seals:
                raise ValueError("Rust bridge executable sealing unavailable")
            yield descriptor
        finally:
            os.close(descriptor)

    def calculate(self, request):
        response = self.fixture({"kind": "FORMULA", **request})
        if response.get("status") != "ACCEPTED" or not isinstance(response.get("limits"), dict):
            raise ValueError("Rust formula rejected the scenario; abstain")
        return response["limits"]

    def fixture(self, request):
        """Exact wire entrypoint also used by the executable parity corpus."""
        data = json.dumps(request, separators=(",", ":"))
        return self.fixture_wire(data)

    def fixture_wire(self, data):
        """Forward original JSON lexemes to Rust without decoding/reserializing."""
        if len(data.encode()) > MAX_BYTES:
            raise FormulaInputRejected("formula input too large")
        with self._executable_snapshot() as descriptor:
            result = subprocess.run([str(self.binary), "weekly", "formula-bridge", "--file", "-"],
                                    executable=f"/proc/self/fd/{descriptor}", pass_fds=(descriptor,),
                                    input=data, text=True, capture_output=True, timeout=10,
                                    env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"}, check=False)
        # A mutation during execution invalidates the result, even though only
        # the trusted sealed snapshot could have executed.
        self._verified_binary()
        if result.returncode == 1 and not result.stdout and result.stderr in RUST_INPUT_DIAGNOSTICS:
            raise FormulaInputRejected(result.stderr.removeprefix("trady: ").rstrip("\n"))
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
        raise FormulaInputRejected("formula input too large")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as error:
        raise FormulaInputRejected("invalid input encoding") from error
    reply = RustFormulaBridge(args.binary, args.sha256).fixture_wire(text)
    print(json.dumps(reply, separators=(",", ":"), allow_nan=False))


def cli():
    try:
        main()
        return 0
    except FormulaInputRejected as error:
        print(PARSE_REJECTED_PREFIX + str(error), file=sys.stderr)
        return 2
    except Exception as error:
        print("LEAN formula fixture BLOCKED: " + type(error).__name__, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(cli())

"""Bounded local protocol for a separately isolated, one-candidate evaluator."""
import os
import pathlib
import socket
import stat
import time
from .common import canonical, pinned_json, require, sha, strict_json, valid_hash
from .run import read_lock, reply

LIMIT = 4 * 1024 * 1024


def receive(stream):
    def exact(length):
        data = bytearray()
        while len(data) < length:
            part = stream.recv(min(65536, length - len(data)))
            require(part, "truncated evaluator message")
            data.extend(part)
        return bytes(data)
    length = int.from_bytes(exact(4), "big")
    require(0 < length <= LIMIT, "evaluator message limit")
    return exact(length)


def send(stream, data):
    require(0 < len(data) <= LIMIT, "evaluator message limit")
    stream.sendall(len(data).to_bytes(4, "big") + data)


def private_directory(path):
    require(path.is_absolute() and not path.is_symlink(), "absolute private evaluator directory required")
    meta = path.stat()
    require(stat.S_ISDIR(meta.st_mode) and meta.st_uid == os.getuid() and meta.st_mode & 0o022 == 0,
            "evaluator directory ownership or write access invalid")


def request_evaluation(path, data, worker_uid):
    require(path.is_absolute() and not path.is_symlink(), "absolute evaluator socket required")
    meta = path.stat()
    require(stat.S_ISSOCK(meta.st_mode) and meta.st_mode & 0o007 == 0 and meta.st_uid == worker_uid,
            "private evaluator socket identity required")
    require(path.parent.stat().st_uid == worker_uid and path.parent.stat().st_mode & 0o022 == 0,
            "evaluator socket directory identity/write access invalid")
    request = strict_json(data)
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
        stream.settimeout(120)
        stream.connect(str(path))
        send(stream, data)
        response = strict_json(receive(stream))
    require(response["schema_version"] == 1 and response["input_hash"] == request["input_hash"], "evaluator reply binding failed")
    require("payload" in response and "error" not in response, "independent evaluator blocked")
    return canonical(response)


def consume(directory, lock, request):
    """Persist exposure before computation; another candidate can never reuse it."""
    require(sha(request) == lock["request_sha256"], "request differs from independently frozen candidate")
    private_directory(directory)
    binding = {"dataset_hash": lock["bindings"]["dataset_hash"], "input_hash": request["input_hash"],
               "request_sha256": lock["request_sha256"]}
    require(all(valid_hash(value) for value in binding.values()), "invalid exposure binding")
    target = directory / ("consumed_" + binding["dataset_hash"])
    content = canonical(binding)
    try:
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    except FileExistsError:
        require(not target.is_symlink() and target.read_bytes() == content, "sealed dataset was exposed to a different candidate")
    else:
        with os.fdopen(fd, "wb") as destination:
            destination.write(content); destination.flush(); os.fsync(destination.fileno())
        fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)


def serve(lock_path, socket_path, state_directory, stop_after=None):
    """Fixed deployment paths only. stop_after is a test-only Python parameter."""
    private_directory(socket_path.parent)
    private_directory(state_directory)
    require(not socket_path.exists() and not socket_path.is_symlink(), "evaluator socket already exists")
    lock = read_lock(lock_path)
    artifacts = [pinned_json(lock["artifacts"][key]) for key in ("dataset", "search", "shadow")]
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
        listener.bind(str(socket_path))
        socket_path.chmod(0o660)
        listener.listen(8)
        handled = 0
        try:
            while stop_after is None or handled < stop_after:
                stream, _ = listener.accept()
                with stream:
                    stream.settimeout(120)
                    started = time.monotonic()
                    request = None
                    try:
                        request = strict_json(receive(stream))
                        consume(state_directory, lock, request)
                        response = reply(request, lock, *artifacts, started)
                    except Exception:
                        # Never echo sealed records or exception bodies to the lab.
                        response = {"schema_version": 1, "input_hash": request.get("input_hash") if isinstance(request, dict) else None,
                                    "error": "sealed evaluation BLOCKED"}
                    try:
                        send(stream, canonical(response))
                    except (OSError, ValueError):
                        pass
                handled += 1
        finally:
            socket_path.unlink(missing_ok=True)

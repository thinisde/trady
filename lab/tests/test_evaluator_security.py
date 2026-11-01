"""Local filesystem and kernel boundary regressions, without a database."""
import hashlib
import os
import pathlib
import socket
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

LAB = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(LAB))
from evaluator.common import canonical
from evaluator.run import write_artifact

EVIDENCE = pathlib.Path("/tmp/trady-remediation-20261031/evaluator")


class DiagnosticsBoundary(unittest.TestCase):
    def setUp(self):
        EVIDENCE.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=EVIDENCE)
        self.addCleanup(self.temporary.cleanup)
        self.directory = pathlib.Path(self.temporary.name) / "diagnostics"
        self.content = {"private": "TEST_ONLY sealed diagnostic"}

    def test_creates_private_directories_and_reuses_identical_private_file(self):
        target = self.directory / "nested"
        digest = write_artifact(target, self.content)
        self.assertEqual(write_artifact(target, self.content), digest)
        self.assertEqual(target.stat().st_uid, os.geteuid())
        self.assertEqual(target.stat().st_mode & 0o7777, 0o700)
        self.assertEqual(self.directory.stat().st_mode & 0o7777, 0o700)
        self.assertEqual((target / digest).stat().st_mode & 0o7777, 0o600)
        self.assertEqual((target / digest).read_bytes(), canonical(self.content))

    def test_existing_output_requires_exact_0700_without_chmod_repair(self):
        self.directory.mkdir(mode=0o700)
        for mode in (0o000, 0o500, 0o600, 0o750, 0o755, 0o770, 0o777, 0o1700, 0o2700):
            with self.subTest(mode=oct(mode)):
                self.directory.chmod(mode)
                with self.assertRaises((ValueError, PermissionError)):
                    write_artifact(self.directory, self.content)
                self.assertEqual(self.directory.stat().st_mode & 0o7777, mode)
                self.directory.chmod(0o700)
                self.assertEqual(list(self.directory.iterdir()), [])

    @unittest.skipUnless(os.geteuid() == 0, "foreign-owner fixture requires root")
    def test_foreign_owned_0700_directory_is_rejected(self):
        self.directory.mkdir(mode=0o700)
        os.chown(self.directory, 1, -1)
        try:
            with self.assertRaisesRegex(ValueError, "ownership"):
                write_artifact(self.directory, self.content)
            self.assertEqual(list(self.directory.iterdir()), [])
            self.assertEqual(self.directory.stat().st_uid, 1)
        finally:
            os.chown(self.directory, os.geteuid(), -1)

    def test_relative_parent_traversal_and_symlinked_components_are_rejected(self):
        with self.assertRaises(ValueError):
            write_artifact("relative", self.content)
        with self.assertRaises(ValueError):
            write_artifact(self.directory / ".." / "elsewhere", self.content)
        actual = pathlib.Path(self.temporary.name) / "actual"
        actual.mkdir(mode=0o700)
        self.directory.symlink_to(actual, target_is_directory=True)
        for target in (self.directory, self.directory / "child"):
            with self.subTest(target=target), self.assertRaises((ValueError, OSError)):
                write_artifact(target, self.content)
        self.assertEqual(list(actual.iterdir()), [])

    def test_reused_artifact_rejects_public_modes_symlinks_hardlinks_and_fifo(self):
        digest = write_artifact(self.directory, self.content)
        target = self.directory / digest
        target.chmod(0o640)
        with self.assertRaisesRegex(ValueError, "identity or mode"):
            write_artifact(self.directory, self.content)
        target.chmod(0o600)
        alias = self.directory / "alias"
        os.link(target, alias)
        with self.assertRaisesRegex(ValueError, "identity or mode"):
            write_artifact(self.directory, self.content)
        target.unlink()
        target.symlink_to(alias)
        with self.assertRaises(OSError):
            write_artifact(self.directory, self.content)
        target.unlink()
        os.mkfifo(target, 0o600)
        with self.assertRaisesRegex(ValueError, "identity or mode"):
            write_artifact(self.directory, self.content)

    def test_directory_replacement_cannot_redirect_secret_write(self):
        self.directory.mkdir(mode=0o700)
        original = self.directory.with_name("original")
        attacker = self.directory.with_name("attacker")
        attacker.mkdir(mode=0o700)
        digest = hashlib.sha256(canonical(self.content)).hexdigest()
        real_open = os.open

        def replace_at_create(path, flags, *args, **kwargs):
            if path == digest and flags & os.O_CREAT:
                self.directory.rename(original)
                self.directory.symlink_to(attacker, target_is_directory=True)
            return real_open(path, flags, *args, **kwargs)

        with patch("evaluator.run.os.open", side_effect=replace_at_create):
            with self.assertRaisesRegex(ValueError, "changed during use"):
                write_artifact(self.directory, self.content)
        self.assertEqual(list(attacker.iterdir()), [])
        self.assertEqual((original / digest).read_bytes(), canonical(self.content))


class WorkerIsolation(unittest.TestCase):
    def run_child(self, code, *arguments, **kwargs):
        EVIDENCE.mkdir(parents=True, exist_ok=True)
        result = subprocess.run([sys.executable, "-B", "-c", code, *map(str, arguments)], cwd=LAB,
                                capture_output=True, timeout=10, **kwargs)
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        return result

    def test_kernel_blocks_reachable_unix_database_socket_but_accepts_ipc(self):
        EVIDENCE.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=EVIDENCE) as temporary:
            database = pathlib.Path(temporary) / ".s.PGSQL.5432"
            ipc = pathlib.Path(temporary) / "evaluator.sock"
            # Synthetic endpoint only: no database or external service is used.
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as fake_database:
                fake_database.bind(str(database))
                fake_database.listen(8)
                code = '''
import ctypes, errno, pathlib, socket, sys, threading
from evaluator.isolation import isolate_worker, worker_process
worker_process()
start = threading.Event()
failures = []
def existing_thread():
    start.wait()
    try: escaped = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    except OSError as error:
        if error.errno != errno.EPERM: failures.append(error)
    else: escaped.close(); failures.append("thread escaped isolation")
thread = threading.Thread(target=existing_thread); thread.start()
with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener, socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as outgoing:
    listener.bind(sys.argv[2]); listener.listen(1)
    isolate_worker()
    start.set(); thread.join(timeout=2)
    assert not thread.is_alive() and not failures, failures
    for endpoint in (sys.argv[1], sys.argv[2], "\\0trady-synthetic-database"):
        try: outgoing.connect(endpoint)
        except OSError as error: assert error.errno == errno.EPERM, error
        else: raise AssertionError("outbound Unix connect escaped")
    for family in (socket.AF_UNIX, socket.AF_INET, socket.AF_INET6):
        try: escaped = socket.socket(family, socket.SOCK_STREAM)
        except OSError as error: assert error.errno == errno.EPERM, error
        else: escaped.close(); raise AssertionError("new socket escaped")
    api = ctypes.CDLL("libseccomp.so.2")
    api.seccomp_syscall_resolve_name.argtypes = [ctypes.c_char_p]
    libc = ctypes.CDLL(None, use_errno=True)
    for name in (b"io_uring_setup", b"io_uring_enter", b"io_uring_register"):
        number = api.seccomp_syscall_resolve_name(name)
        assert number >= 0
        assert libc.syscall(number, -1, 0, 0, 0, 0, 0) == -1
        assert ctypes.get_errno() == errno.EPERM, name
    pathlib.Path(sys.argv[3]).touch()
    connection, _ = listener.accept()
    with connection:
        assert connection.recv(4) == b"ping"
        connection.sendall(b"pong")
'''
                ready = pathlib.Path(temporary) / "ready"
                with subprocess.Popen([sys.executable, "-B", "-c", code, str(database), str(ipc), str(ready)],
                                      cwd=LAB, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
                    try:
                        import time
                        deadline = time.monotonic() + 5
                        while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                            time.sleep(0.01)
                        self.assertTrue(ready.exists(), child.communicate() if child.poll() is not None else None)
                        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                            client.settimeout(3); client.connect(str(ipc)); client.sendall(b"ping")
                            self.assertEqual(client.recv(4), b"pong")
                        _, errors = child.communicate(timeout=5)
                        self.assertEqual(child.returncode, 0, errors.decode())
                        fake_database.settimeout(0.05)
                        with self.assertRaises(TimeoutError):
                            fake_database.accept()
                    finally:
                        if child.poll() is None:
                            child.kill(); child.communicate(timeout=5)

    def test_inherited_connected_socket_is_rejected(self):
        code = '''
from evaluator.isolation import worker_process
try: worker_process()
except ValueError as error: assert "inherit open descriptors" in str(error), error
else: raise AssertionError("inherited socket accepted")
'''
        incoming, outgoing = socket.socketpair()
        with incoming, outgoing:
            self.run_child(code, pass_fds=(incoming.fileno(),))

    def test_connected_database_socket_cannot_be_hidden_on_standard_input(self):
        code = '''
from evaluator.isolation import worker_process
try: worker_process()
except ValueError as error: assert "connected stdio sockets" in str(error), error
else: raise AssertionError("inherited stdin socket accepted")
'''
        incoming, outgoing = socket.socketpair()
        with incoming, outgoing:
            self.run_child(code, stdin=incoming)

    def test_worker_rejects_existing_threads_before_isolation_or_input_access(self):
        code = '''
import pathlib, threading
from unittest.mock import patch
from evaluator.transport import serve
stop = threading.Event()
thread = threading.Thread(target=lambda: stop.wait(3)); thread.start()
try:
    with patch("evaluator.transport.isolate_worker") as isolate, patch("evaluator.transport.read_lock") as read:
        try: serve(pathlib.Path("/missing"), pathlib.Path("/missing/socket"), pathlib.Path("/missing/state"))
        except ValueError as error: assert "single-threaded" in str(error), error
        else: raise AssertionError("multithreaded worker accepted")
        isolate.assert_not_called(); read.assert_not_called()
finally: stop.set(); thread.join()
'''
        self.run_child(code)

    def test_unavailable_isolation_blocks_before_sealed_lock_access_and_cleans_socket(self):
        code = '''
import pathlib, tempfile
from unittest.mock import patch
from evaluator.transport import serve
with tempfile.TemporaryDirectory(dir="/tmp/trady-remediation-20261031/evaluator") as temporary:
    directory = pathlib.Path(temporary); state = directory / "state"; state.mkdir(mode=0o700)
    with patch("evaluator.isolation.ctypes.CDLL", side_effect=OSError("unavailable")), patch("evaluator.transport.read_lock") as read:
        try: serve(directory / "lock", directory / "worker.sock", state)
        except OSError: pass
        else: raise AssertionError("isolation failure accepted")
        read.assert_not_called()
        assert not (directory / "worker.sock").exists()
'''
        self.run_child(code)

    def test_failed_kernel_self_test_blocks_before_sealed_lock_access(self):
        code = '''
import pathlib, tempfile
from unittest.mock import patch
from evaluator.transport import serve
with tempfile.TemporaryDirectory(dir="/tmp/trady-remediation-20261031/evaluator") as temporary:
    directory = pathlib.Path(temporary); state = directory / "state"; state.mkdir(mode=0o700)
    with patch("evaluator.isolation.socket.socket.connect", return_value=None), patch("evaluator.transport.read_lock") as read:
        try: serve(directory / "lock", directory / "worker.sock", state)
        except ValueError as error: assert "not isolated" in str(error), error
        else: raise AssertionError("failed self-test accepted")
        read.assert_not_called()
        assert not (directory / "worker.sock").exists()
'''
        self.run_child(code)

    def test_unsafe_diagnostics_configuration_blocks_before_sealed_artifact_access(self):
        code = '''
import pathlib, tempfile
from unittest.mock import patch
from evaluator.transport import serve
with tempfile.TemporaryDirectory(dir="/tmp/trady-remediation-20261031/evaluator") as temporary:
    directory = pathlib.Path(temporary); state = directory / "state"; state.mkdir(mode=0o700)
    diagnostics = directory / "diagnostics"; diagnostics.mkdir(); diagnostics.chmod(0o755)
    with patch("evaluator.transport.read_lock", return_value={"diagnostics_directory": str(diagnostics)}), patch("evaluator.transport.pinned_json") as read:
        try: serve(directory / "lock", directory / "worker.sock", state)
        except ValueError as error: assert "exact mode 0700" in str(error), error
        else: raise AssertionError("unsafe diagnostics accepted")
        read.assert_not_called()
        assert not (directory / "worker.sock").exists()
        assert diagnostics.stat().st_mode & 0o777 == 0o755
'''
        self.run_child(code)


if __name__ == "__main__":
    unittest.main()

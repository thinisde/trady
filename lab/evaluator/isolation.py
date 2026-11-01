"""Irreversible Linux worker isolation; clients run in a separate process."""
import ctypes
import errno
import fcntl
import os
import socket
import stat
import sys

from .common import require

# Stable libseccomp ABI values from seccomp.h. No architecture-specific syscall
# numbers are embedded: libseccomp resolves and rewrites native/multiplexed calls.
ALLOW = 0x7FFF0000
ERRNO = 0x00050000
KILL_PROCESS = 0x80000000
ACT_BADARCH = 2
CTL_NNP = 3
CTL_TSYNC = 4
DENIED_SYSCALLS = ("socket", "socketpair", "connect", "io_uring_setup", "io_uring_enter", "io_uring_register")


def worker_process():
    """Refuse inherited IPC/rings or existing threads before opening any data."""
    require(sys.platform == "linux", "Linux evaluator isolation required")
    require(len(os.listdir("/proc/self/task")) == 1, "evaluator requires a dedicated single-threaded process")
    for name in os.listdir("/proc/self/fd"):
        descriptor = int(name)
        try:
            meta = os.fstat(descriptor)
            target = os.readlink("/proc/self/fd/" + name)
        except OSError as error:
            if error.errno == errno.EBADF:  # /proc's transient directory fd
                continue
            raise
        if descriptor <= 2:
            # systemd journal streams are the only connected stdio sockets.
            # In particular, an inherited DB socket cannot be hidden on stdin.
            if stat.S_ISSOCK(meta.st_mode):
                with socket.socket(fileno=os.dup(descriptor)) as stream:
                    require(descriptor in (1, 2) and stream.family == socket.AF_UNIX
                            and stream.getpeername() == "/run/systemd/journal/stdout",
                            "evaluator cannot inherit connected stdio sockets")
            else:
                require(not target.startswith("anon_inode:"), "evaluator cannot inherit anonymous descriptors")
            continue
        # Some Python/libffi builds retain a read-only library descriptor.
        if stat.S_ISREG(meta.st_mode) and meta.st_uid == 0 and meta.st_mode & 0o022 == 0 \
                and os.path.isabs(target) and fcntl.fcntl(descriptor, fcntl.F_GETFL) & os.O_ACCMODE == os.O_RDONLY:
            continue
        raise ValueError("evaluator cannot inherit open descriptors")


def isolate_worker():
    """Deny new sockets and all outbound connects while retaining accept/reply.

    The actual IPC listener must already be bound. The filter applies to every
    thread and descendants, and cannot be removed. Missing support is fatal.
    """
    require(sys.platform == "linux", "Linux evaluator isolation required")
    api = ctypes.CDLL("libseccomp.so.2", use_errno=True)
    for name, arguments, result in (
        ("seccomp_init", [ctypes.c_uint32], ctypes.c_void_p),
        ("seccomp_attr_set", [ctypes.c_void_p, ctypes.c_uint, ctypes.c_uint32], ctypes.c_int),
        ("seccomp_syscall_resolve_name", [ctypes.c_char_p], ctypes.c_int),
        ("seccomp_rule_add", [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_int, ctypes.c_uint], ctypes.c_int),
        ("seccomp_load", [ctypes.c_void_p], ctypes.c_int),
        ("seccomp_release", [ctypes.c_void_p], None),
    ):
        function = getattr(api, name)
        function.argtypes, function.restype = arguments, result
    context = api.seccomp_init(ALLOW)
    require(context, "evaluator isolation context unavailable")
    try:
        for attribute, value in ((ACT_BADARCH, KILL_PROCESS), (CTL_NNP, 1), (CTL_TSYNC, 1)):
            require(api.seccomp_attr_set(context, attribute, value) == 0, "evaluator isolation attribute unavailable")
        for name in DENIED_SYSCALLS:
            number = api.seccomp_syscall_resolve_name(name.encode())
            require(number != -1 and api.seccomp_rule_add(context, ERRNO | errno.EPERM, number, 0) == 0,
                    "evaluator isolation rule unavailable")
        # These are local test sockets, never a configured service/database.
        # Precreate the connector because socket() is also denied after load.
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as probe_listener, \
                socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as probe:
            address = b"\0trady-evaluator-isolation-" + os.urandom(16).hex().encode()
            probe_listener.bind(address)
            probe_listener.listen(1)
            require(api.seccomp_load(context) == 0, "evaluator isolation could not be installed")
            try:
                probe.connect(address)
            except OSError as error:
                require(error.errno == errno.EPERM, "evaluator outbound isolation self-test failed")
            else:
                raise ValueError("evaluator outbound connections are not isolated")
            try:
                unexpected = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            except OSError as error:
                require(error.errno == errno.EPERM, "evaluator socket isolation self-test failed")
            else:
                unexpected.close()
                raise ValueError("evaluator socket creation is not isolated")
    finally:
        api.seccomp_release(context)

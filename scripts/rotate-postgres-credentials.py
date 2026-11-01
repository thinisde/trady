#!/usr/bin/env python3
"""Rotate Trady passwords or synchronize local DSN ports in an explicitly named container.

Passwords travel through psql's private prompt on stdin, never SQL text or argv.
Saved loopback endpoints must authenticate to the selected local container using
libpq 18 or later. File and server locks serialize cooperating rotations.
Endpoint synchronization preserves passwords and requires explicit --apply.
"""
import argparse
from contextlib import contextmanager
import ctypes
import fcntl
import ipaddress
import json
import os
from pathlib import Path
import re
import secrets
import selectors
import stat
import subprocess
import tempfile
import time

ROLES = {
    "TRADY_ANALYST_DSN": "trady_analyst",
    "TRADY_EXECUTOR_DSN": "trady_executor",
    "TRADY_GATEWAY_DSN": "trady_gateway",
    "TRADY_MIGRATOR_DSN": "trady_migrator",
    "TRADY_LAB_DSN": "trady_lab",
    "TRADY_VERIFIER_DSN": "trady_verifier",
    "TRADY_SHADOW_DSN": "trady_shadow",
    "TRADY_RELEASE_OWNER_DSN": "trady_release_owner",
    "TRADY_RELEASE_DSN": "trady_releaser",
}


def parse_dsn(dsn):
    """Parse libpq keyword syntax, retaining exact value spans for private updates."""
    if not isinstance(dsn, str) or any(
            ord(c) < 32 or ord(c) == 127 or (c.isspace() and c != " ") for c in dsn):
        raise RuntimeError("a single-line keyword DSN is required")
    fields, spans = {}, {}
    index = 0
    while index < len(dsn):
        while index < len(dsn) and dsn[index].isspace():
            index += 1
        if index == len(dsn):
            break
        key = re.match(r"[a-z_]+", dsn[index:])
        if key is None:
            raise RuntimeError("invalid keyword DSN")
        name = key.group()
        index += len(name)
        while index < len(dsn) and dsn[index].isspace():
            index += 1
        if index == len(dsn) or dsn[index] != "=" or name in fields:
            raise RuntimeError("invalid or duplicate keyword DSN field")
        index += 1
        while index < len(dsn) and dsn[index].isspace():
            index += 1
        start = index
        quoted = index < len(dsn) and dsn[index] == "'"
        if quoted:
            index += 1
        value = []
        closed = not quoted
        while index < len(dsn):
            char = dsn[index]
            if char == "\\":
                index += 1
                if index == len(dsn):
                    raise RuntimeError("incomplete keyword DSN escape")
                value.append(dsn[index])
                index += 1
            elif quoted and char == "'":
                index += 1
                closed = True
                break
            elif not quoted and char.isspace():
                break
            else:
                value.append(char)
                index += 1
        if not closed or (index < len(dsn) and not dsn[index].isspace()):
            raise RuntimeError("invalid keyword DSN value")
        fields[name], spans[name] = "".join(value), (start, index)
    return fields, spans


def container_target(container):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}", container):
        raise RuntimeError("an explicit local container name or ID is required")
    daemon = os.environ.get("DOCKER_HOST")
    if not daemon or os.environ.get("DOCKER_CONTEXT"):
        result = subprocess.run(
            ["docker", "context", "inspect", "--format", "{{json .Endpoints.docker.Host}}"],
            text=True, capture_output=True, timeout=30,
        )
        if result.returncode:
            raise RuntimeError("cannot verify the local Docker endpoint")
        daemon = json.loads(result.stdout)
    if not isinstance(daemon, str) or not daemon.startswith("unix:///"):
        raise RuntimeError("rotation requires a local Docker Unix-socket daemon")
    result = subprocess.run(
        ["docker", "inspect", "--type", "container", "--format",
         '{"id":{{json .Id}},"ports":{{json .NetworkSettings.Ports}}}', container],
        text=True, capture_output=True, timeout=30,
    )
    if result.returncode:
        raise RuntimeError("cannot inspect the selected local container")
    target = json.loads(result.stdout)
    if not re.fullmatch(r"[a-f0-9]{64}", target.get("id", "")):
        raise RuntimeError("cannot pin the selected container identity")
    return target["id"], target["ports"]


def validate_endpoints(fields_by_role, ports, server_port):
    bindings = ports.get(str(server_port) + "/tcp") or []
    published = set()
    for binding in bindings:
        address = ipaddress.ip_address(binding["HostIp"])
        if not address.is_loopback:
            raise RuntimeError("the selected PostgreSQL port must be loopback-only")
        published.add((address, int(binding["HostPort"])))
    for fields in fields_by_role.values():
        address = ipaddress.ip_address(fields.get("host", ""))
        port = fields.get("port", "")
        if (not address.is_loopback or not port.isascii() or not port.isdecimal()
                or not 1 <= int(port) <= 65535 or (address, int(port)) not in published
                or "service" in fields
                or ("hostaddr" in fields and ipaddress.ip_address(fields["hostaddr"]) != address)):
            raise RuntimeError("every saved DSN must identify the selected container's published loopback endpoint")


def validate_hba(rules):
    """Conservatively require covering SCRAM rules for all nine TCP identities."""
    if not isinstance(rules, list) or any(row.get("error") for row in rules):
        raise RuntimeError("the selected container has invalid authentication rules")
    for role in ROLES.values():
        covered = set()
        for row in rules:
            if not str(row.get("type", "")).startswith("host"):
                continue
            databases, users = row["database"], row["user_name"]
            # The view loses HBA quoting: "all" might be a literal identifier.
            # Only the explicit database/role can prove covering permission.
            definite_database = "trady" in databases
            definite_user = role in users
            possible_database = definite_database or "all" in databases or any(
                value in {"samerole", "samegroup"} or value.startswith("/") for value in databases)
            possible_user = definite_user or "all" in users or any(value.startswith(("+", "/")) for value in users)
            if not possible_database or not possible_user:
                continue
            if row["auth_method"] not in {"scram-sha-256", "reject"}:
                raise RuntimeError("a potentially applicable Trady authentication rule does not require SCRAM")
            if (row["auth_method"] == "scram-sha-256" and row["type"] == "host"
                    and definite_database and definite_user):
                if row["address"] == "all":
                    covered.update({4, 6})
                else:
                    try:
                        address = ipaddress.ip_address(row["address"])
                        mask = ipaddress.ip_address(row["netmask"])
                        if address.version == mask.version and int(address) == int(mask) == 0:
                            covered.add(address.version)
                    except ValueError:
                        pass
                if covered == {4, 6}:
                    break
        if covered != {4, 6}:
            raise RuntimeError("each Trady role needs a covering SCRAM host rule for IPv4 and IPv6")


def invoke(container, command, payload=None):
    return subprocess.run(
        ["docker", "exec", "-i", container, *command], input=payload,
        text=True, capture_output=True, timeout=30,
    )


def admin(container, command, payload=None, *, private_sql=False):
    # The scoped helper supports the standard local PostgreSQL container socket.
    # Clear libpq/service overrides inside the container on every admin call.
    arguments = ["env", "-i", "PATH=/usr/local/bin:/usr/bin:/bin:/usr/lib/postgresql/18/bin",
                 "psql", "-X", "-w", "-v", "ON_ERROR_STOP=1", "-h", "/var/run/postgresql",
                 "-p", "5432", "-U", "postgres", "-d", "postgres"]
    if private_sql:
        arguments.append("-At")
        payload = command + "\n"
    else:
        arguments.extend(["-Atc", command])
    result = invoke(container, arguments, payload)
    if result.returncode:
        raise RuntimeError("Trady database administrative command failed; details suppressed")
    return result.stdout.strip()


@contextmanager
def saved_connection(fields, password):
    library = ctypes.CDLL("libpq.so.5")
    for name, arguments, result in (
        ("PQlibVersion", [], ctypes.c_int),
        ("PQconnectdb", [ctypes.c_char_p], ctypes.c_void_p),
        ("PQstatus", [ctypes.c_void_p], ctypes.c_int),
        ("PQconnectionUsedPassword", [ctypes.c_void_p], ctypes.c_int),
        ("PQbackendPID", [ctypes.c_void_p], ctypes.c_int),
        ("PQfinish", [ctypes.c_void_p], None),
    ):
        function = getattr(library, name)
        function.argtypes, function.restype = arguments, result
    if library.PQlibVersion() < 180000:
        raise RuntimeError("local credential rotation requires libpq 18 or later")
    marker = "trady_rotation_" + secrets.token_hex(16)
    parameters = {**fields, "password": password, "hostaddr": fields["host"],
                  "application_name": marker, "require_auth": "scram-sha-256", "connect_timeout": "5"}
    text = " ".join(name + "='" + value.replace("\\", "\\\\").replace("'", "\\'") + "'"
                    for name, value in parameters.items())
    connection = library.PQconnectdb(text.encode())
    if not connection:
        raise RuntimeError("credential verification connection unavailable")
    try:
        yield library, connection, marker
    finally:
        library.PQfinish(connection)


def same_server(container, role, library, connection, marker):
    if library.PQstatus(connection) != 0 or not library.PQconnectionUsedPassword(connection):
        return False
    backend = library.PQbackendPID(connection)
    if backend <= 0:
        return False
    # The random live session must be visible on the selected server, not merely
    # on a different server with matching roles/passwords or a different localhost.
    return admin(container, "select exists(select 1 from pg_stat_activity where "
                 f"pid={backend} and datname='trady' and usename='{role}' "
                 f"and application_name='{marker}')") == "t"


def authenticate(container, role, password, fields):
    with saved_connection(fields, password) as (library, connection, marker):
        return same_server(container, role, library, connection, marker)


class AdministrativeSession:
    """The lock holder also executes every protected SQL/password operation."""

    def __init__(self, container):
        self.container = container
        self.failed = False
        self.pending = b""
        self.process = subprocess.Popen(
            ["docker", "exec", "-i", container, "env", "-i",
             "PATH=/usr/local/bin:/usr/bin:/bin:/usr/lib/postgresql/18/bin",
             "PGOPTIONS=-c idle_session_timeout=0", "psql", "-X", "-w", "-qAt",
             "-v", "ON_ERROR_STOP=1", "-h", "/var/run/postgresql", "-p", "5432",
             "-U", "postgres", "-d", "postgres"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self.backend_pid = None

    def admin(self, container, command, payload=None, *, private_sql=False):
        # Every command uses stdin, including password prompts and verifiers.
        # A dead lock connection can never execute a later independent rollback.
        if container != self.container or self.failed or self.process.poll() is not None:
            raise RuntimeError("credential rotation administrative session unavailable")
        marker = "trady_command_" + secrets.token_hex(16)
        separator = "\n" if command.lstrip().startswith("\\") else ";\n"
        request = command + separator + (payload or "") + f"SELECT '{marker}';\n"
        deadline = time.monotonic() + 30
        try:
            self.process.stdin.write(request.encode())
            self.process.stdin.flush()
            while True:
                if len(self.pending) > 262144:
                    raise RuntimeError("credential rotation administrative output exceeded limit")
                lines = self.pending.split(b"\n")
                if marker.encode() in lines:
                    index = lines.index(marker.encode())
                    result = b"\n".join(lines[:index]).decode().strip()
                    self.pending = b"\n".join(lines[index + 1:])
                    return result
                remaining = deadline - time.monotonic()
                if remaining <= 0 or not self.selector.select(remaining):
                    raise RuntimeError("credential rotation administrative session timed out")
                chunk = os.read(self.process.stdout.fileno(), 65536)
                if not chunk:
                    raise RuntimeError("credential rotation administrative session ended")
                self.pending += chunk
        except Exception:
            self.failed = True
            raise RuntimeError("credential rotation administrative command failed; details suppressed") from None

    def close(self):
        self.selector.close()
        self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)
        finally:
            self.process.stdout.close()


@contextmanager
def database_lock(container, fields):
    if not authenticate(container, fields["user"], fields["password"], fields):
        raise RuntimeError("saved credentials do not identify the selected PostgreSQL server")
    session = AdministrativeSession(container)
    try:
        if session.admin(container, "SELECT pg_try_advisory_lock(8153212011::bigint)") != "t":
            raise RuntimeError("another credential rotation holds the server lock")
        session.backend_pid = int(session.admin(container, "SELECT pg_backend_pid()"))
        yield session
    finally:
        session.close()


@contextmanager
def rotation_lock(path):
    lock_path = path.with_name(path.name + ".rotation.lock")
    descriptor = os.open(lock_path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
    try:
        metadata = os.fstat(descriptor)
        if (not stat.S_ISREG(metadata.st_mode) or stat.S_IMODE(metadata.st_mode) != 0o600
                or metadata.st_uid != os.geteuid() or metadata.st_nlink != 1):
            raise RuntimeError("credential rotation lock must be an owned private regular file")
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError("another credential rotation holds the file lock") from None
        current = lock_path.lstat()
        if (current.st_dev, current.st_ino) != (metadata.st_dev, metadata.st_ino):
            raise RuntimeError("credential rotation lock was replaced")
        yield
    finally:
        os.close(descriptor)


def read_private(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor) as stream:
        metadata = os.fstat(stream.fileno())
        if (not stat.S_ISREG(metadata.st_mode) or stat.S_IMODE(metadata.st_mode) != 0o600
                or metadata.st_uid != os.geteuid()):
            raise RuntimeError("credentials must be an existing regular private file")
        return stream.read(), (metadata.st_dev, metadata.st_ino)


def _rotate_locked(container, path, apply):
    original_text, original_identity = read_private(path)
    data = json.loads(original_text)
    credentials = data["credentials"]
    previous, fields_by_role, password_spans = {}, {}, {}
    for name, role in ROLES.items():
        dsn = credentials.get(name)
        if not isinstance(dsn, str):
            raise RuntimeError("all nine Trady keyword DSNs must be configured")
        fields, spans = parse_dsn(dsn)
        if fields.get("user") != role or fields.get("dbname") != "trady" or not fields.get("password"):
            raise RuntimeError("credential role/database/password shape differs from Trady scope")
        previous[role] = fields["password"]
        fields_by_role[role], password_spans[name] = fields, spans["password"]
    container, ports = container_target(container)
    server_port = int(admin(container, "show port"))
    validate_endpoints(fields_by_role, ports, server_port)
    with database_lock(container, fields_by_role["trady_analyst"]) as session:
        current_text, current_identity = read_private(path)
        if current_text != original_text or current_identity != original_identity:
            raise RuntimeError("credentials changed before rotation; preserving the concurrent update")
        return _rotate_database_locked(container, path, apply, data, previous,
                                       fields_by_role, password_spans, original_text,
                                       original_identity, session)


def _rotate_database_locked(container, path, apply, data, previous, fields_by_role,
                            password_spans, original_text, original_identity, session):
    admin = session.admin
    credentials = data["credentials"]
    names = ",".join("'" + role + "'" for role in ROLES.values())
    rows = admin(container, "select rolname from pg_roles where rolcanlogin and not rolsuper "
                 "and not rolcreaterole and not rolcreatedb and not rolreplication and not rolbypassrls "
                 f"and rolname in ({names}) order by rolname").splitlines()
    if sorted(rows) != sorted(ROLES.values()):
        raise RuntimeError("selected container does not contain the expected restricted Trady roles")
    rules = json.loads(admin(container, "select coalesce(json_agg(r order by rule_number), '[]'::json) "
                             "from pg_hba_file_rules r"))
    validate_hba(rules)
    if admin(container, "show password_encryption") != "scram-sha-256":
        raise RuntimeError("the selected container must generate SCRAM password verifiers")
    verifiers = admin(container, "select rolname||'|'||coalesce(rolpassword,'') from pg_authid "
                      f"where rolname in ({names})").splitlines()
    if (len(verifiers) != len(ROLES)
            or any(not row.split("|", 1)[1].startswith("SCRAM-SHA-256$") for row in verifiers)):
        raise RuntimeError("all Trady roles must already have SCRAM password verifiers")
    for role, password in previous.items():
        if (not authenticate(container, role, password, fields_by_role[role])
                or authenticate(container, role, secrets.token_urlsafe(48), fields_by_role[role])):
            raise RuntimeError("current credentials must authenticate and invalid passwords must fail for every role")
    if not apply:
        return {"status": "rotation_plan_checked", "roles": len(ROLES),
                "dsn_endpoints_verified": True, "scram_roles_verified": len(ROLES),
                "secrets_displayed": False}
    passwords = {role: secrets.token_urlsafe(48) for role in ROLES.values()}
    for name, role in ROLES.items():
        start, end = password_spans[name]
        dsn = credentials[name]
        credentials[name] = dsn[:start] + "'" + passwords[role] + "'" + dsn[end:]
    staged = None
    committed = False
    try:
        with tempfile.NamedTemporaryFile(mode="w", dir=path.parent,
                prefix=path.name + ".rotation-", delete=False) as stream:
            staged = Path(stream.name)
            os.fchmod(stream.fileno(), 0o600)
            json.dump(data, stream, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        for role, password in passwords.items():
            admin(container, "\\password " + role, password + "\n" + password + "\n")
        # Check the same lock-owning session before committing the saved store.
        admin(container, "SELECT 1")
        current_text, current_identity = read_private(path)
        if current_text != original_text or current_identity != original_identity:
            raise RuntimeError("credentials changed during rotation; preserving the concurrent update")
        os.replace(staged, path)
        committed = True
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
        admin(container, "select pg_reload_conf()")
        time.sleep(0.25)
        for role, password in passwords.items():
            if (not authenticate(container, role, password, fields_by_role[role])
                    or authenticate(container, role, previous[role], fields_by_role[role])):
                raise RuntimeError("passwords stored, but new/old authentication verification failed")
        admin(container, "select pg_terminate_backend(pid) from pg_stat_activity "
              f"where usename in ({names}) and pid<>pg_backend_pid()")
    except Exception:
        if not committed:
            restoration_failed = False
            for row in verifiers:
                role, verifier = row.split("|", 1)
                literal = "'" + verifier.replace("'", "''") + "'" if verifier else "NULL"
                try:
                    admin(container, f'ALTER ROLE "{role}" PASSWORD {literal}', private_sql=True)
                except Exception:
                    restoration_failed = True
            if restoration_failed:
                raise RuntimeError("rotation failed and password restoration was incomplete; details suppressed") from None
        raise
    finally:
        if staged is not None:
            staged.unlink(missing_ok=True)
    return {"status": "rotated_and_verified", "roles": len(ROLES),
            "new_passwords_accepted": True, "old_passwords_rejected": True,
            "secret_file_mode": "0600", "dsn_endpoints_preserved": True,
            "dsn_endpoints_verified": True, "scram_roles_verified": len(ROLES),
            "secrets_displayed": False}


def rotate(container, path, apply):
    with rotation_lock(path):
        return _rotate_locked(container, path, apply)


def sync_endpoints(container, path, apply, roles=None):
    """Bind existing local Trady DSNs to a verified container without changing passwords."""
    selected = ROLES if roles is None else {name: role for name, role in ROLES.items() if role in roles}
    if not selected or "trady_analyst" not in selected.values():
        raise RuntimeError("endpoint synchronization must include the analyst server-lock identity")
    with rotation_lock(path):
        original, identity = read_private(path)
        data = json.loads(original)
        credentials = data["credentials"]
        container, ports = container_target(container)
        server_port = int(admin(container, "show port"))
        bindings = ports.get(str(server_port) + "/tcp") or []
        fields_by_role, replacements = {}, {}
        for name, role in selected.items():
            dsn = credentials.get(name)
            fields, spans = parse_dsn(dsn)
            if (fields.get("user") != role or fields.get("dbname") != "trady"
                    or not fields.get("password") or "port" not in spans
                    or not ipaddress.ip_address(fields.get("host", "")).is_loopback):
                raise RuntimeError("saved credential scope differs from the local Trady database")
            matches = [b for b in bindings if b["HostIp"] == fields["host"]]
            if len(matches) != 1:
                raise RuntimeError("the saved loopback host must have one published database port")
            target_port = matches[0]["HostPort"]
            fields_by_role[role] = {**fields, "port": target_port}
            start, end = spans["port"]
            replacements[name] = dsn[:start] + "'" + target_port + "'" + dsn[end:]
        validate_endpoints(fields_by_role, ports, server_port)
        with database_lock(container, fields_by_role["trady_analyst"]) as session:
            rules = json.loads(session.admin(container,
                "select coalesce(json_agg(r order by rule_number), '[]'::json) from pg_hba_file_rules r"))
            validate_hba(rules)
            for role, fields in fields_by_role.items():
                if (not authenticate(container, role, fields["password"], fields)
                        or authenticate(container, role, secrets.token_urlsafe(48), fields)):
                    raise RuntimeError("every saved role must authenticate exclusively to the selected server")
            current, current_identity = read_private(path)
            if current != original or current_identity != identity:
                raise RuntimeError("credentials changed before endpoint synchronization")
            if apply:
                credentials.update(replacements)
                staged = None
                try:
                    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent,
                            prefix=path.name + ".rotation-", delete=False) as stream:
                        staged = Path(stream.name)
                        os.fchmod(stream.fileno(), 0o600)
                        json.dump(data, stream, indent=2)
                        stream.write("\n")
                        stream.flush()
                        os.fsync(stream.fileno())
                    current, current_identity = read_private(path)
                    if current != original or current_identity != identity:
                        raise RuntimeError("credentials changed during endpoint synchronization")
                    os.replace(staged, path)
                    directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
                    try:
                        os.fsync(directory)
                    finally:
                        os.close(directory)
                finally:
                    if staged is not None:
                        staged.unlink(missing_ok=True)
        return {
            "status": "endpoints_synchronized" if apply else "endpoint_plan_checked",
            "roles": len(selected), "dsn_endpoints_verified": True,
            "unchanged_roles": sorted(set(ROLES.values()) - set(selected.values())),
            "published_ports": sorted({fields["port"] for fields in fields_by_role.values()}),
            "passwords_preserved": True, "secret_file_mode": "0600", "secrets_displayed": False,
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--container", required=True)
    parser.add_argument("--credentials", type=Path,
                        default=Path(__file__).resolve().parent.parent / "config/credentials.local.json")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--sync-endpoints", action="store_true",
                        help="verify and synchronize existing DSNs to this container's published loopback port")
    parser.add_argument("--roles", nargs="+", choices=sorted(ROLES.values()),
                        help="scope endpoint synchronization; include trady_analyst for the server lock")
    args = parser.parse_args()
    if args.roles and not args.sync_endpoints:
        parser.error("--roles requires --sync-endpoints")
    try:
        result = (sync_endpoints(args.container, args.credentials, args.apply, args.roles)
                  if args.sync_endpoints else rotate(args.container, args.credentials, args.apply))
        print(json.dumps(result, sort_keys=True))
    except Exception:
        # Subprocess and parser exceptions may contain credential material.
        status = "endpoint_sync_failed" if args.sync_endpoints else "rotation_failed"
        print(json.dumps({"status": status, "details_suppressed": True}))
        raise SystemExit(1)


if __name__ == "__main__":
    main()

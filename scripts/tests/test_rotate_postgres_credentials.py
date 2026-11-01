import importlib.util
from contextlib import contextmanager, nullcontext
import io
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import MagicMock, patch

SPEC = importlib.util.spec_from_file_location(
    "rotate_credentials", Path(__file__).resolve().parents[1] / "rotate-postgres-credentials.py"
)
ROTATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ROTATE)


class RotationTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "credentials.local.json"
        self.original = {
            "config_version": 1,
            "credentials": {name: f"host=127.0.0.1 port=5432 dbname=trady user={role} password='old-{role}'"
                            for name, role in ROTATE.ROLES.items()},
        }
        self.path.write_text(json.dumps(self.original))
        self.path.chmod(0o600)
        self.commands = []
        self.passwords = {}
        self.failure = None
        self.concurrent_update = False
        self.auth_method = "scram-sha-256"
        self.hba_prefix = []
        self.bindings = {"5432/tcp": [{"HostIp": "127.0.0.1", "HostPort": "5432"}]}
        self.private_commands = []
        self.session = SimpleNamespace(backend_pid=424242, admin=self.admin)
        lock_patch = patch.object(ROTATE, "database_lock", side_effect=lambda *_: nullcontext(self.session))
        self.database_lock = lock_patch.start()
        self.addCleanup(lock_patch.stop)

    def admin(self, container, command, payload=None, *, private_sql=False):
        self.assertEqual(container, "isolated-trady")
        self.commands.append(command)
        if private_sql:
            self.private_commands.append(command)
        if command == "show port":
            return "5432"
        if command.startswith("select rolname from pg_roles"):
            return "\n".join(sorted(ROTATE.ROLES.values()))
        if command.startswith("select coalesce(json_agg"):
            return json.dumps(self.hba_prefix + [{
                "type": "host", "database": ["trady"], "user_name": list(ROTATE.ROLES.values()),
                "address": "all", "netmask": None, "auth_method": self.auth_method, "error": None,
            }, {"type": "host", "database": ["all"], "user_name": ["all"],
                "address": "all", "netmask": None, "auth_method": "trust", "error": None}])
        if command == "show password_encryption":
            return "scram-sha-256"
        if command.startswith("select rolname||"):
            return "\n".join(role + "|SCRAM-SHA-256$verifier" for role in ROTATE.ROLES.values())
        if command.startswith("\\password "):
            role = command.split()[1]
            if role == self.failure:
                raise RuntimeError("simulated private prompt failure")
            self.passwords[role] = payload.splitlines()[0]
            if self.concurrent_update:
                changed = json.loads(self.path.read_text())
                changed["operator_note"] = "keep this concurrent update"
                self.path.write_text(json.dumps(changed))
        return "t"

    def run_rotation(self):
        with patch.object(ROTATE, "admin", side_effect=self.admin), \
             patch.object(ROTATE, "container_target", return_value=("isolated-trady", self.bindings)), \
             patch.object(ROTATE, "authenticate", side_effect=lambda _, role, password, port:
                          self.passwords.get(role, "old-" + role) == password), \
             patch.object(ROTATE.time, "sleep"):
            return ROTATE.rotate("isolated-trady", self.path, True)

    def run_endpoint_sync(self, apply=True):
        def authenticate(_, role, password, fields):
            self.assertEqual(fields["port"], self.bindings["5432/tcp"][0]["HostPort"])
            return password == "old-" + role
        with patch.object(ROTATE, "admin", side_effect=self.admin), \
             patch.object(ROTATE, "container_target", return_value=("isolated-trady", self.bindings)), \
             patch.object(ROTATE, "authenticate", side_effect=authenticate):
            return ROTATE.sync_endpoints("isolated-trady", self.path, apply)

    def test_endpoint_sync_checks_server_and_preserves_passwords_and_other_credentials(self):
        self.bindings["5432/tcp"][0]["HostPort"] = "65432"
        self.original["credentials"]["UNRELATED_API_KEY"] = "unrelated-fixture-secret"
        self.path.write_text(json.dumps(self.original))
        original_text = self.path.read_text()
        result = self.run_endpoint_sync(apply=False)
        self.assertEqual(result["status"], "endpoint_plan_checked")
        self.assertEqual(self.path.read_text(), original_text)
        result = self.run_endpoint_sync()
        self.assertEqual(result["status"], "endpoints_synchronized")
        self.assertEqual(result["published_ports"], ["65432"])
        self.assertTrue(result["passwords_preserved"])
        self.assertEqual(self.path.stat().st_mode & 0o777, 0o600)
        saved = json.loads(self.path.read_text())["credentials"]
        self.assertEqual(saved["UNRELATED_API_KEY"], "unrelated-fixture-secret")
        for name, role in ROTATE.ROLES.items():
            fields, _ = ROTATE.parse_dsn(saved[name])
            self.assertEqual(fields["port"], "65432")
            self.assertEqual(fields["password"], "old-" + role)
        self.assertFalse(self.passwords)
        self.assertFalse(any(command.startswith("\\password ") for command in self.commands))

    def test_endpoint_sync_rejects_public_bindings_and_wrong_credentials_without_writes(self):
        self.bindings["5432/tcp"].append({"HostIp": "0.0.0.0", "HostPort": "5432"})
        with self.assertRaises(RuntimeError):
            self.run_endpoint_sync()
        self.assertEqual(json.loads(self.path.read_text()), self.original)
        self.bindings["5432/tcp"].pop()
        with patch.object(ROTATE, "admin", side_effect=self.admin), \
             patch.object(ROTATE, "container_target", return_value=("isolated-trady", self.bindings)), \
             patch.object(ROTATE, "authenticate", return_value=False):
            with self.assertRaises(RuntimeError):
                ROTATE.sync_endpoints("isolated-trady", self.path, True)
        self.assertEqual(json.loads(self.path.read_text()), self.original)

    def test_endpoint_sync_can_preserve_a_separately_configured_lab_database(self):
        name = "TRADY_LAB_DSN"
        self.original["credentials"][name] = self.original["credentials"][name].replace(
            "host=127.0.0.1", "host=separate-lab-database")
        self.path.write_text(json.dumps(self.original))
        roles = [role for role in ROTATE.ROLES.values() if role != "trady_lab"]
        with patch.object(ROTATE, "admin", side_effect=self.admin), \
             patch.object(ROTATE, "container_target", return_value=("isolated-trady", self.bindings)), \
             patch.object(ROTATE, "authenticate", side_effect=lambda _, role, password, fields:
                          password == "old-" + role):
            result = ROTATE.sync_endpoints("isolated-trady", self.path, True, roles)
        self.assertEqual(result["roles"], 8)
        self.assertEqual(result["unchanged_roles"], ["trady_lab"])
        self.assertEqual(json.loads(self.path.read_text())["credentials"][name], self.original["credentials"][name])

    def test_endpoint_sync_rejects_remote_or_overridden_hosts(self):
        name = next(iter(ROTATE.ROLES))
        for replacement in ["host=192.0.2.1", "host=127.0.0.2", "host=127.0.0.1 hostaddr=192.0.2.1",
                            "host=127.0.0.1 service=elsewhere"]:
            with self.subTest(replacement=replacement):
                changed = json.loads(json.dumps(self.original))
                changed["credentials"][name] = changed["credentials"][name].replace("host=127.0.0.1", replacement)
                self.path.write_text(json.dumps(changed))
                with self.assertRaises((RuntimeError, ValueError)):
                    self.run_endpoint_sync()
                self.assertEqual(json.loads(self.path.read_text()), changed)

    def test_endpoint_sync_preserves_concurrent_operator_updates(self):
        @contextmanager
        def changing_lock(*_):
            changed = {**self.original, "operator_note": "concurrent endpoint edit"}
            self.path.write_text(json.dumps(changed))
            yield self.session
        self.database_lock.side_effect = changing_lock
        with self.assertRaisesRegex(RuntimeError, "credentials changed"):
            self.run_endpoint_sync()
        saved = json.loads(self.path.read_text())
        self.assertEqual(saved["credentials"], self.original["credentials"])
        self.assertEqual(saved["operator_note"], "concurrent endpoint edit")

    def test_endpoint_sync_cli_suppresses_exception_details(self):
        with patch("sys.argv", ["rotate", "--container", "isolated-trady", "--sync-endpoints"]), \
             patch.object(ROTATE, "sync_endpoints", side_effect=RuntimeError("fixture-private-password")), \
             patch("sys.stdout", new_callable=io.StringIO) as output:
            with self.assertRaises(SystemExit):
                ROTATE.main()
        result = json.loads(output.getvalue())
        self.assertEqual(result, {"status": "endpoint_sync_failed", "details_suppressed": True})
        self.assertNotIn("fixture-private-password", output.getvalue())

    def test_success_uses_private_prompts_atomic_private_store_and_old_password_rejection(self):
        result = self.run_rotation()
        self.assertEqual(result["status"], "rotated_and_verified")
        self.assertEqual(self.path.stat().st_mode & 0o777, 0o600)
        self.assertEqual(len(set(self.passwords.values())), 9)
        self.assertTrue(all(secret not in "\n".join(self.commands) for secret in self.passwords.values()))
        self.assertEqual(len(list(self.path.parent.iterdir())), 2)
        self.assertEqual(self.path.with_name(self.path.name + ".rotation.lock").stat().st_mode & 0o777, 0o600)
        for name, role in ROTATE.ROLES.items():
            updated = json.loads(self.path.read_text())["credentials"][name]
            self.assertIn("host=127.0.0.1 port=5432", updated)
            self.assertIn(self.passwords[role], updated)

    def test_partial_password_failure_restores_verifiers_and_keeps_original_store(self):
        self.failure = "trady_gateway"
        with self.assertRaises(RuntimeError):
            self.run_rotation()
        self.assertEqual(json.loads(self.path.read_text()), self.original)
        self.assertEqual(sum(command.startswith("ALTER ROLE") for command in self.commands), 9)
        self.assertEqual(len(self.private_commands), 9)
        self.assertEqual(len(list(self.path.parent.iterdir())), 2)

    def test_saved_endpoint_mismatch_stops_before_changing_any_password(self):
        for host, port in [("127.0.0.1", "6543"), ("127.0.0.2", "5432"), ("192.0.2.1", "5432")]:
            with self.subTest(host=host, port=port):
                changed = json.loads(json.dumps(self.original))
                name = next(iter(ROTATE.ROLES))
                changed["credentials"][name] = changed["credentials"][name].replace(
                    "host=127.0.0.1 port=5432", f"host={host} port={port}")
                self.path.write_text(json.dumps(changed))
                with self.assertRaises(RuntimeError):
                    self.run_rotation()
                self.assertFalse(self.passwords)
                self.assertEqual(json.loads(self.path.read_text()), changed)

    def test_hostaddr_override_and_duplicate_endpoint_fields_are_rejected(self):
        name = next(iter(ROTATE.ROLES))
        for extra in [" hostaddr=192.0.2.1", " host=127.0.0.2", " service=elsewhere"]:
            with self.subTest(extra=extra):
                changed = json.loads(json.dumps(self.original))
                changed["credentials"][name] += extra
                self.path.write_text(json.dumps(changed))
                with self.assertRaises(RuntimeError):
                    self.run_rotation()
                self.assertFalse(self.passwords)

    def test_public_port_binding_is_rejected_even_if_loopback_binding_exists(self):
        self.bindings["5432/tcp"].append({"HostIp": "0.0.0.0", "HostPort": "5432"})
        with self.assertRaises(RuntimeError):
            self.run_rotation()
        self.assertFalse(self.passwords)

    def test_earlier_permissive_rules_for_any_role_prevent_rotation(self):
        for users in [["all"], ["trady_analyst"], ["+operator_group"], ["/trady_.*"]]:
            with self.subTest(users=users):
                self.hba_prefix = [{"type": "hostssl", "database": ["all"], "user_name": users,
                                    "address": "all", "netmask": None, "auth_method": "trust", "error": None}]
                with self.assertRaises(RuntimeError):
                    self.run_rotation()
                self.assertFalse(self.passwords)

    def test_every_role_is_checked_before_and_after_rotation(self):
        with patch.object(ROTATE, "admin", side_effect=self.admin), \
             patch.object(ROTATE, "container_target", return_value=("isolated-trady", self.bindings)), \
             patch.object(ROTATE, "authenticate", side_effect=lambda _, role, password, port:
                          self.passwords.get(role, "old-" + role) == password) as authenticate, \
             patch.object(ROTATE.time, "sleep"):
            result = ROTATE.rotate("isolated-trady", self.path, True)
        self.assertEqual(authenticate.call_count, 4 * len(ROTATE.ROLES))
        self.assertEqual(result["scram_roles_verified"], 9)
        self.assertTrue(result["dsn_endpoints_verified"])
        self.assertTrue(all(call.args[3]["user"] == call.args[1] for call in authenticate.call_args_list))

    def test_wrong_current_password_prevents_mutation(self):
        with patch.object(ROTATE, "admin", side_effect=self.admin), \
             patch.object(ROTATE, "container_target", return_value=("isolated-trady", self.bindings)), \
             patch.object(ROTATE, "authenticate", return_value=False):
            with self.assertRaises(RuntimeError):
                ROTATE.rotate("isolated-trady", self.path, True)
        self.assertFalse(self.passwords)
        self.assertFalse(any(command.startswith("\\password ") for command in self.commands))

    def test_nonprivate_file_is_rejected(self):
        self.path.chmod(0o640)
        with self.assertRaises(RuntimeError):
            self.run_rotation()
        self.assertFalse(self.commands)

    def test_keyword_parser_preserves_escaped_password_and_rejects_malformed_values(self):
        fields, spans = ROTATE.parse_dsn("host = '127.0.0.1' password='old\\'pass\\\\word' application_name='keep password=literal'")
        self.assertEqual(fields["password"], "old'pass\\word")
        self.assertEqual(fields["application_name"], "keep password=literal")
        self.assertEqual("'old\\'pass\\\\word'", "host = '127.0.0.1' password='old\\'pass\\\\word' application_name='keep password=literal'"[slice(*spans["password"])])
        for malformed in ["password='unterminated", "password=trailing\\", "host=a host=b", "password='a'junk",
                          "application_name=x\u00a0host=127.0.0.1 password=old"]:
            with self.subTest(dsn=malformed), self.assertRaises(RuntimeError):
                ROTATE.parse_dsn(malformed)

    def test_remote_docker_daemon_is_rejected_before_container_access(self):
        with patch.dict(ROTATE.os.environ, {"DOCKER_HOST": "tcp://remote.example:2376", "DOCKER_CONTEXT": ""}), \
             patch.object(ROTATE.subprocess, "run") as run:
            with self.assertRaises(RuntimeError):
                ROTATE.container_target("isolated-trady")
        run.assert_not_called()

    def test_plan_checks_endpoints_and_current_authentication_without_rotation(self):
        with patch.object(ROTATE, "admin", side_effect=self.admin), \
             patch.object(ROTATE, "container_target", return_value=("isolated-trady", self.bindings)), \
             patch.object(ROTATE, "authenticate", side_effect=lambda _, role, password, port:
                          password == "old-" + role):
            result = ROTATE.rotate("isolated-trady", self.path, False)
        self.assertEqual(result["status"], "rotation_plan_checked")
        self.assertTrue(result["dsn_endpoints_verified"])
        self.assertFalse(self.passwords)

    def test_concurrent_secret_update_is_preserved_and_password_changes_rolled_back(self):
        self.concurrent_update = True
        with self.assertRaises(RuntimeError):
            self.run_rotation()
        saved = json.loads(self.path.read_text())
        self.assertEqual(saved["credentials"], self.original["credentials"])
        self.assertIn("operator_note", saved)
        self.assertEqual(sum(command.startswith("ALTER ROLE") for command in self.commands), 9)

    def test_trust_authentication_prevents_any_secret_mutation(self):
        self.auth_method = "trust"
        with self.assertRaises(RuntimeError):
            self.run_rotation()
        self.assertFalse(self.passwords)
        self.assertEqual(json.loads(self.path.read_text()), self.original)

    def test_overlapping_file_rotations_are_rejected_before_database_access(self):
        with ROTATE.rotation_lock(self.path):
            with self.assertRaisesRegex(RuntimeError, "file lock"):
                self.run_rotation()
        self.assertFalse(self.commands)
        self.assertFalse(self.passwords)
        self.database_lock.assert_not_called()
        self.assertEqual(self.run_rotation()["status"], "rotated_and_verified")

    def test_server_lock_rejection_cannot_mutate_or_restore_verifiers(self):
        self.database_lock.side_effect = RuntimeError("another credential rotation holds the server lock")
        with self.assertRaisesRegex(RuntimeError, "server lock"):
            self.run_rotation()
        self.assertEqual(self.commands, ["show port"])
        self.assertFalse(self.passwords)

    def test_file_is_rechecked_after_acquiring_server_lock(self):
        @contextmanager
        def changing_lock(*_):
            changed = {**self.original, "operator_note": "updated before server lock"}
            self.path.write_text(json.dumps(changed))
            yield self.session
        self.database_lock.side_effect = changing_lock
        with self.assertRaisesRegex(RuntimeError, "credentials changed before rotation"):
            self.run_rotation()
        self.assertFalse(self.passwords)
        self.assertIn("operator_note", json.loads(self.path.read_text()))
        self.assertEqual(self.commands, ["show port"])

    def test_ambiguous_quoted_all_cannot_prove_covering_scram(self):
        rules = [
            {"type": "host", "database": ["trady"], "user_name": ["all"],
             "address": "127.0.0.1", "netmask": "255.255.255.255", "auth_method": "scram-sha-256"},
            {"type": "host", "database": ["trady"], "user_name": ["all"],
             "address": "all", "netmask": None, "auth_method": "scram-sha-256"},
            {"type": "host", "database": ["trady"], "user_name": ["all"],
             "address": "all", "netmask": None, "auth_method": "trust"},
        ]
        with self.assertRaisesRegex(RuntimeError, "does not require SCRAM"):
            ROTATE.validate_hba(rules)
        rules[1]["user_name"] = list(ROTATE.ROLES.values())
        rules[1]["database"] = ["all"]
        with self.assertRaises(RuntimeError):
            ROTATE.validate_hba(rules)

    def test_admin_clears_endpoint_environment_and_pins_standard_local_socket(self):
        with patch.object(ROTATE, "invoke", return_value=SimpleNamespace(returncode=0, stdout="t\n")) as invoke:
            self.assertEqual(ROTATE.admin("isolated-trady", "select true"), "t")
        arguments = invoke.call_args.args[1]
        self.assertEqual(arguments[:2], ["env", "-i"])
        self.assertEqual(arguments[arguments.index("-h") + 1], "/var/run/postgresql")
        self.assertEqual(arguments[arguments.index("-p") + 1], "5432")
        self.assertIn("-w", arguments)

    def test_rollback_sql_verifiers_travel_on_stdin_and_never_argv(self):
        verifier = "SCRAM-SHA-256$private-verifier"
        sql = f"ALTER ROLE trady_analyst PASSWORD '{verifier}'"
        with patch.object(ROTATE, "invoke", return_value=SimpleNamespace(returncode=0, stdout="")) as invoke:
            ROTATE.admin("isolated-trady", sql, private_sql=True)
        self.assertNotIn(verifier, " ".join(invoke.call_args.args[1]))
        self.assertEqual(invoke.call_args.args[2], sql + "\n")

    def test_successful_authentication_to_another_server_is_rejected_and_closed(self):
        library = MagicMock()
        library.PQlibVersion.return_value = 180006
        library.PQconnectdb.return_value = 123
        library.PQstatus.return_value = 0
        library.PQconnectionUsedPassword.return_value = 1
        library.PQbackendPID.return_value = 456
        fields, _ = ROTATE.parse_dsn(self.original["credentials"]["TRADY_ANALYST_DSN"])
        with patch.object(ROTATE.ctypes, "CDLL", return_value=library), \
             patch.object(ROTATE, "admin", return_value="f") as admin:
            self.assertFalse(ROTATE.authenticate("isolated-trady", "trady_analyst", fields["password"], fields))
        library.PQfinish.assert_called_once_with(123)
        self.assertIn("application_name='trady_rotation_", admin.call_args.args[1])
        parameters = library.PQconnectdb.call_args.args[0]
        self.assertIn(b"hostaddr='127.0.0.1'", parameters)
        self.assertIn(b"require_auth='scram-sha-256'", parameters)

    def test_directory_fsync_failure_retains_committed_passwords_without_rollback(self):
        with patch.object(ROTATE.os, "fsync", side_effect=[None, OSError("directory fsync failed")]):
            with self.assertRaises(OSError):
                self.run_rotation()
        self.assertFalse(self.private_commands)
        saved = json.loads(self.path.read_text())["credentials"]
        for name, role in ROTATE.ROLES.items():
            self.assertEqual(ROTATE.parse_dsn(saved[name])[0]["password"], self.passwords[role])

    def test_lock_session_loss_cannot_open_an_independent_password_or_rollback_connection(self):
        process = MagicMock()
        process.poll.return_value = None
        process.stdin = io.BytesIO()
        process.stdout = self.enterContext(tempfile.TemporaryFile())
        selector = MagicMock()
        selector.select.return_value = [True]
        with patch.object(ROTATE.subprocess, "Popen", return_value=process) as spawn, \
             patch.object(ROTATE.selectors, "DefaultSelector", return_value=selector), \
             patch.object(ROTATE.os, "read", return_value=b""):
            session = ROTATE.AdministrativeSession("isolated-trady")
            with self.assertRaisesRegex(RuntimeError, "details suppressed"):
                session.admin("isolated-trady", "SELECT 1")
            original_input = process.stdin.getvalue()
            with self.assertRaisesRegex(RuntimeError, "unavailable"):
                session.admin("isolated-trady", "ALTER ROLE trady_gateway NOLOGIN", private_sql=True)
            self.assertEqual(process.stdin.getvalue(), original_input)
            self.assertIn(b"SELECT 1;\n", original_input)
            self.assertIn("PGOPTIONS=-c idle_session_timeout=0", spawn.call_args.args[0])
            spawn.assert_called_once()
            session.close()


if __name__ == "__main__":
    unittest.main()

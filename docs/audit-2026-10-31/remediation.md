# Audit remediation, verified 2026-11-01

This follow-up fixes implementation findings from the [full audit](./report.md).
The original audit and its verification records describe the earlier source
revision. Trading212 API requests remain excluded.

## Final execution results

**776 automated tests passed**, plus the offline requests, proof/parity and
runtime probes below. Commands, source pins, timings, SQL samples and response
hashes are recorded in [remediation verification](./remediation-verification.json).

| Check | Result |
| --- | --- |
| Rust workspace | 525 passed; 34 database cases deferred to the disposable run |
| Disposable PostgreSQL | 34 passed, including explicit multi-connection schedules |
| Python lab | 178 passed |
| Proof input regressions | 16 passed |
| Credential unit tests | 23 passed |
| Offline CLI examples | 18 passed; all ten feature assessments kept `entry_authorized: false` |
| Clean Lean build and axiom audit | 26 theorems; `Classical.choice`, `Quot.sound`, `propext` |
| Copied-snapshot proof/parity helper | 937 cases: 672 exact-output cases and 265 parser rejections |
| Helper binding and tamper checks | 10 passed |
| Formatting, Clippy and assets | Passed |

The final helper regenerated 20 proof-source pins, 211 Rust/build/migration pins
including all 13 root SQL files, 26 theorem bindings and the 3,019-file Lean
toolchain digest. Every recorded source pin matched the resulting checkout.
The complete helper self-test passed in 198.88 seconds. Finite parity and proofs
of Lean definitions do not establish a complete refinement of Rust execution.

The Rust and database checks removed broker environment variables and used an
empty private credential store, mock broker transports and disposable loopback
PostgreSQL. Credential rotation used only generated disposable passwords.
No service was deployed and no real credential store was rotated.

## Evaluator and formula execution

The dedicated evaluator worker installs a mandatory, synchronized Linux seccomp
filter after binding its IPC listener. It denies new sockets, outbound connects
and io_uring, including PostgreSQL Unix sockets, while preserving accepted IPC.
Startup rejects inherited unsafe descriptors, preexisting threads, unavailable
isolation support and failed local self-tests. The shipped systemd unit also
denies outbound connects and alternate syscall ABIs. No service was deployed.

Diagnostic traversal uses directory descriptors, rejects symlinks and unsafe
ancestors, and requires evaluator ownership with exact mode `0700`. Reused
artifacts must be singly linked regular private files with matching bytes;
directory replacement or permission changes fail closed.

The formula bridge revalidates its executable on every invocation, copies
verified bytes into an executable memfd, checks its write/grow/shrink seals and
executes through that descriptor. It checks the original source again before
accepting output. This closes the executable hash/exec race for that binary.
Engine, data and shared-library deployment integrity remain separate requirements.

All 178 Python lab tests passed, including 13 new evaluator security tests and
four new executable integrity tests. The actual Rust adapter wire and IPC/kernel
restrictions were exercised. Linux/procfs, libseccomp, TSYNC/no-new-privileges and
executable sealed memfds are mandatory prerequisites; unsupported environments
reject operation.

## Credential rotation

Rotation now pins a local Docker container ID and checks that all nine saved
keyword DSNs reach its published loopback PostgreSQL listener from the caller's
network namespace. Local libpq 18 or later enforces SCRAM, and each connection's
random session marker is checked on the selected server. Administrative commands
clear inherited settings and select the standard container socket and port.
Remote Docker daemons, public bindings, endpoint overrides, duplicate DSN keys
and non-ASCII DSN whitespace are rejected before password changes. The credential
file must be owned by the invoking user with exact mode `0600`; descriptor reads
reject final symlinks.

HBA inspection covers every role and conservatively rejects potentially matching
permissive rules before covering SCRAM rules. Only explicit database/role names
prove coverage; quoted `all` cannot conceal a later permissive rule. Every current password must work
and a random password must fail. Verification explicitly requires SCRAM. The
private prompts and verifier restoration both use stdin. File replacement is
atomic and both the staged file and parent directory are fsynced. Precommit
failures attempt restoration of all nine original verifiers; later failures
retain the committed passwords and store.

A stable sibling file lock and a server advisory lock serialize cooperating
rotations, including different credential paths. Locks cover preflight through
commit, verification and rollback. The lock-owning administrative connection
disables idle-session timeout and executes protected commands itself. Connection
loss prevents later password changes or restoration on a new connection.
External editors ignoring these locks remain
subject to a best-effort text/inode check with a final check/replace race.

The 23 credential tests pass. A real disposable PostgreSQL check rejected
overlapping same-file and cross-file rotations and restored all nine original
passwords after an injected partial failure. Plan/apply then succeeded, accepted
every new password, rejected every old password and removed its container. The
regular disposable runner also repeats rotation and saved-endpoint runtime probes.
The extended smoke check kept competing paths excluded beyond configured idle
timeouts, then killed the lock connection and verified the losing invocation
could not undo a successful competing rotation.

## Registry propagation

Every non-tax ingest/read registers a validated reviewed config before input or
query checks and before disabled-feature or live-mode early returns. This lets
the persistent repository reconcile rights changes even when no insertion
succeeds. Non-tax reads also purge before unavailable-mode returns. Tax requests
remain outside registry registration and purge.

## Storage gates and erasure

Append-only migration 11 adds null-rejecting weekly/release guards and removes
runtime access to renamed private implementations. Active-release lookup now
rechecks the candidate window, scoped approval and the latest readiness record;
renewal can restore readiness, while newer negative evidence blocks access.
Credit windows enforce their durable ceiling, preserve already charged usage,
and reject refunds or identity changes.

Intelligence SQL validates content hashes, canonical serialization, scope,
chronology, feature bindings and source/lineage dependencies. Current rights
control visibility and retention; repository operations physically erase expired
or withdrawn content and remove dependent assessments/rejection markers. Direct
SQL invalidation and erasure follow the same rules.

Replacing a reviewed scope registry permanently retires the old hash. Cached
callers cannot reactivate it. Erased lineage remains blocked under another
observation ID or a fresh registry; fresh reviewed rights can accept new lineage.
Protected publication, registry activation, erasure, invalidation, cleanup and
current-release authority checks require exactly `READ COMMITTED`. Other
isolation settings fail before guarded reads/writes with SQLSTATE `0A000` and
`STORAGE_REQUIRES_READ_COMMITTED`; callers must roll back and start a supported
transaction. This closes empty-history, newly published dependent-content and
stale readiness/pointer snapshot gaps. Runtime authority applies to the state
checked during lookup; a later concurrent commit requires a new check. Physical
expiry cleanup requires a repository operation or explicit sweep. Row policies
hide expired or unlicensed content meanwhile. Legacy duplicate audit markers
remain supported. Prior migrations were preserved.

All 34 disposable PostgreSQL tests and runtime probes passed, including all nine
persistable feature envelopes, direct SQL rejection/erasure checks, retired
registries, stale snapshots, readiness renewal and credit bounds. The first run
exposed two PL/pgSQL variable/table alias collisions; both were fixed before
the passing run. The final snapshot regressions also exposed interference from
global cleanup in parallel tests. The runner now serializes database test cases;
each concurrency regression still interleaves its own PostgreSQL connections.
Read-only SQL samples confirmed all nine persistable feature families, zero tax
rows and the expected role privileges. The container was removed after verification.

## Helper and reference input validation

Every public weekly helper boundary validates the complete manifest before
execution or artifacts, and direct verification uses checked component lookups.
Missing or malformed pins now return validation errors.

The Lean reference validates account IDs, mode, currencies, order sides and
statuses. Its parser rejects unknown nested fields, duplicate decoded keys,
invalid types/integer widths/timestamps, excess nesting and trailing documents.
It accepts complete multiline input within the existing 4 MiB bound. Unsigned
fields retain their raw numeric-token checks, including exponent forms and
signed zero. One checked string decoder handles keys and values, combines valid
UTF-16 surrogate pairs and rejects malformed pairs. Timestamp input validation
includes Chrono's final represented leap second; duration overflow is checked
separately. Two new account-input theorems bring the inventory to 26.

The adapter forwards the original wire document to the sealed calculator. Only
an exact Rust parse/schema error with the expected exit and empty stdout becomes
`FormulaInputRejected`. Its CLI returns a separate exit and diagnostic for that
type. Missing files, timeouts, crashes and other execution failures cannot count
as parser-rejection evidence. All 16 proof input tests passed.

The offline self-test allows 300 seconds per proof/parity stage. Per-request,
subprocess and deployed helper bounds remain unchanged. The final fixture uses
`objcopy --strip-all` and is 61,055,632 bytes, within the existing 64 MiB helper
pin limit.

## GDELT responses

GDELT payloads without an article array still fail validation. Collection now
returns `gdelt_articles_missing` with a 60-second retry timestamp; diagnostics
expose the sanitized `gdelt_response_invalid` code and suggest a 60-second
backoff when no numeric Retry-After was supplied. GDELT rate-limit responses also
receive that default backoff. An explicit empty article array remains valid empty
data. No automatic retry or source enablement was added.

The rebuilt CLI made one read-only GDELT GET and returned `rate_limited`, HTTP
429 and a 60-second suggested backoff. Restoring live article data depends on the
upstream service; these fixes do not claim live GDELT coverage. The timestamped
response is retained in [remediation verification](./remediation-verification.json).

## Independent review and preservation

Fresh source reviews resolved the prior objections and found no further
actionable findings within their scopes:

| Area | Final review |
| --- | --- |
| Evaluator isolation, diagnostics and executable integrity | [Round 1](./reviews/evaluator-r1.md) |
| Credential endpoint validation, serialization and rollback | [Round 3](./reviews/credential-rotation-r3.md) |
| Storage gates, current rights, erasure and transaction isolation | [Round 2](./reviews/storage-r2.md) |
| Helper validation, parser equivalence cases and rejection evidence | [Round 2](./reviews/proof-r2.md) |

The reviews inspect source and contract consistency; the execution results above
supply runtime evidence. The final proof review also covers the bridge changes
made after the evaluator review. After the credential review, the disposable
runner and its README gained only the test-serialization setting described above;
credential rotation code stayed unchanged and the full disposable run passed.
Original review digests and retained-artifact digests are recorded in the evidence.

The final structural pass read 52 contracts comprising 7,643 lines, checked 413
local links, 1,598 source-file citations and 107 inline Cargo commands, and parsed
all 46 audit JSON files without errors. Fresh source reviews establish the
semantic correspondence for the remediated boundaries.

All 20 original migration/build-manifest files remain unchanged. No baseline
files were deleted or staged. The original 611-line drift report remains intact
with an appended remediation outcome, and the original audit visualization now
links to this follow-up. Disposable containers were removed.

## Documentation references

Connection parsing and authentication checks were verified against PostgreSQL
18's [libpq connection parameters](https://www.postgresql.org/docs/18/libpq-connect.html),
[HBA matching rules](https://www.postgresql.org/docs/18/auth-pg-hba-conf.html) and
[HBA inspection view](https://www.postgresql.org/docs/18/view-pg-hba-file-rules.html).
Rotation serialization follows the documented
[session advisory-lock lifetime](https://www.postgresql.org/docs/18/explicit-locking.html#ADVISORY-LOCKS)
and [nonblocking lock functions](https://www.postgresql.org/docs/18/functions-admin.html#FUNCTIONS-ADVISORY-LOCKS).
The GDELT route follows the provider's
[DOC 2.0 API documentation](https://blog.gdeltproject.org/gdelt-doc-2-0-api-debuts/).
The guarded transaction contract follows PostgreSQL's documented
[transaction isolation behavior](https://www.postgresql.org/docs/18/transaction-iso.html).

# Trady full audit, 2026-10-31

Subsequent implementation fixes are recorded in the [remediation report](./remediation.md).
The audit results below describe the source revision tested before those fixes.

The runtime checks completed with **698 automated tests passing**. Formatting,
Clippy, Lean build, the 24-theorem axiom audit, 647 parity cases and ten proof-helper
tamper checks also passed. **Trading212 API calls were excluded.**

The documentation scope is the 51 source-bearing directories selected from 100
discovered subdirectories after ignoring generated, hidden, test-only and non-code
trees. Existing code and documentation changes were preserved. Contract-review
results are recorded in the final documentation section below.

The [interactive results](./summary.html) compare the feature responses and
summarize verification outcomes.

## Automated verification

| Check | Result |
| --- | --- |
| Rust workspace tests | 519 passed; 21 database tests deferred to the isolated run |
| Disposable PostgreSQL integration | 21 passed |
| Python lab tests | 154 passed |
| Python credential-rotation tests | 4 passed |
| Rust formatting | Passed |
| Clippy, all targets, warnings denied | Passed |
| Lean 4.19.0 build | Passed |
| Lean axiom audit | 24 theorems; recorded foundational axioms |
| Rust/Lean parity | 647 cases passed in the pinned helper self-test |
| Proof-helper negative checks | 10 passed |

Commands, durations, source pins and proof results are in
[verification.json](./verification.json). The helper pinned 210 Rust source files,
12 migration files, 18 proof-source files and a 3,019-file Lean toolchain.
Finite parity checks reduce translation risk; they do not establish a complete
formal refinement of Rust execution.

The Rust and PostgreSQL runs used an empty private credential store and removed
broker environment variables. Automated broker coverage used fixture/mock
transports. PostgreSQL checks ran in disposable loopback containers. Production
database credentials and broker endpoints were not used by these checks.

The first attempted Rust run stopped because the audit's explicit credential path
did not exist. Replacing that temporary path with a valid empty store resolved the
setup failure; the complete rerun passed without product-code changes.

## Requests for all ten intelligence features

Each request uses the repository's existing synthetic fixture and typed test
envelope, including its actual payload digest. The source grants are explicitly
synthetic examples. They do not assert a vendor entitlement.

The fixture column uses the cutoff saved in [requests.json](./requests.json).
The database column records a current-time read before the revocation sample.
Every successful response retained `entry_authorized: false`.

| Feature | Synthetic input | Fixture response | Database response |
| --- | --- | --- | --- |
| Options | [Request](./samples/options.observation.json) / [config](./samples/options.config.json) | [PARTIAL](./samples/options.response.json) | UNAVAILABLE |
| Coordination | [Request](./samples/coordination.observation.json) / [config](./samples/coordination.config.json) | [PARTIAL](./samples/coordination.response.json) | UNAVAILABLE |
| Event expectations | [Request](./samples/event.observation.json) / [config](./samples/event.config.json) | [AVAILABLE](./samples/event.response.json) | AVAILABLE |
| Short interest | [Request](./samples/short_interest.observation.json) / [config](./samples/short_interest.config.json) | [PARTIAL](./samples/short_interest.response.json) | PARTIAL |
| Insider filings | [Request](./samples/insider.observation.json) / [config](./samples/insider.config.json) | [PARTIAL](./samples/insider.response.json) | PARTIAL |
| Cross asset | [Request](./samples/cross_asset.observation.json) / [config](./samples/cross_asset.config.json) | [PARTIAL](./samples/cross_asset.response.json) | PARTIAL |
| Attention | [Request](./samples/attention.observation.json) / [config](./samples/attention.config.json) | [AVAILABLE](./samples/attention.response.json) | ABSTAINED |
| Accounting | [Request](./samples/accounting.observation.json) / [config](./samples/accounting.config.json) | [PARTIAL](./samples/accounting.response.json) | PARTIAL |
| Order book | [Request](./samples/microstructure.observation.json) / [config](./samples/microstructure.config.json) | [PARTIAL](./samples/microstructure.response.json) | INVALIDATED |
| Tax scenarios | [Request](./samples/tax.observation.json) / [config](./samples/tax.config.json) | [PARTIAL](./samples/tax.response.json) | Import rejected |

All 18 offline CLI examples succeeded. They include the ten assessments, an
accounting replay, the paper-memory demonstration, a stock research report and
five status commands. The [accounting replay](./samples/accounting_replay.response.json)
accepted one observation: the query before ingestion returned `UNAVAILABLE`
with no result; the query at ingestion returned a `PARTIAL` assessment.

## Sample database queries and mutations

[Sample SQL](./sample-queries.sql) and [recorded database results](./database-samples.json)
cover the following behavior:

- All nine persistable research features imported and were readable through the
  analyst CLI. Each received a distinct synthetic observation ID.
- Tax import failed with the explicit instruction to use the ephemeral owner
  comparator. The research database contained zero tax observations.
- Only the analyst among the six inspected roles had SELECT permission on the
  intelligence observation table. Actual executor and gateway SELECT attempts
  both failed with PostgreSQL permission errors.
- Revoking rights for `audit_accounting` erased its content and made its result
  `INVALIDATED` with no assessment, while permitted audit markers remained.
- Analyst/executor/gateway probes, the durable paper demonstration and the
  disabled-source news collect/read commands completed in the sample database.
- The sample container was removed after execution.

Privilege and read-only transaction syntax was checked against the
[PostgreSQL 18 privilege functions](https://www.postgresql.org/docs/18/functions-info.html#FUNCTIONS-INFO-ACCESS),
[BEGIN](https://www.postgresql.org/docs/18/sql-begin.html) and
[SET ROLE](https://www.postgresql.org/docs/18/sql-set-role.html) documentation.

## Live service samples

The [online health run](./api-health-online.json) issued 13 selected non-broker
GET requests. Twelve checks were healthy: SEC, FRED, Twelve Data, Finnhub,
Alpha Vantage, FMP, CoinGecko, Tiingo, Massive, Google News, publisher RSS and
Telegram's read-only bot identity endpoint. No Telegram messages were sent.

GDELT returned HTTP 200 but failed response validation. One follow-up request
returned an empty JSON object, `{}`, without the expected article collection.
X remained behind its existing cost gate, and yfinance remained import-only.

The already running local [FinBERT sample](./finbert-sample.json) returned HTTP 200
at the configured model revision. It classified the positive, negative and
neutral synthetic texts with leading probabilities of approximately 0.957,
0.971 and 0.940. A corrupted text hash returned HTTP 400. These were three local
requests; the shipped sentiment configuration remained disabled.

These checks exercise transport and software behavior. The synthetic intelligence
grants and fixtures do not establish live source coverage, forecast calibration
or deployment approval.

## Boundaries exposed by the static review

The corrected contracts describe these existing implementation and deployment
limits; passing tests do not remove them:

- [Evaluator deployment](../../services/runtime/README.md) permits Unix sockets.
  Its service settings alone do not deny access to PostgreSQL Unix sockets;
  deployment must enforce that separation.
- [Evaluator diagnostics](../../lab/evaluator/README.md) reject symlink paths and
  group/other write permissions, but an existing output directory does not get
  a complete owner or exact-0700 check. The
  [formula bridge](../../lab/lean/README.md) hashes its executable at construction,
  so reuse depends on the executable remaining immutable.
- [Release and weekly SQL](../../migrations/README.md) require complete caller
  validation around nullable comparisons. The active-release lookup does not
  refresh readiness expiry; callers retain that responsibility. Financial credit
  limits and intelligence content validation also rely on the documented
  application checks. Content erasure requires the appropriate explicit path.
- [Credential rotation](../../scripts/README.md) authenticates against the
  selected container's loopback endpoint. It does not establish that the saved
  DSN host and port identify that container, and its SCRAM-rule inspection is
  not exhaustive.
- [Direct helper verification](../../crates/trady-adapters/src/weekly_lab/README.md)
  expects `lean_toolchain` and `rust_build` component-map entries before unchecked
  indexing. The normal application weekly setup validates its manifest first.
- [Lean reference validation](../../lab/proofs/README.md) covers a narrower
  account model than production parsing. It does not validate every account ID,
  mode, order side or status field.

These are static findings. Exploitability and deployed enforcement were not
tested, and functional source was preserved.

## Documentation verification

All 51 source-directory contracts were audited, and every contract was explicitly
read during final verification: 7,079 lines in total. The four workers completed
69 fresh-context scouts. Fifteen component READMEs received narrow corrections;
the two parent blueprints were also updated. No package README was missing.

Final checks resolved 398 local contract links and 1,567 cited source references,
verified 107 inline Cargo test-command occurrences, and found no missing contract
sections, unbalanced fences, missing child links or invalid test targets. The
source-reference check establishes file/simple-symbol existence; the scouts
performed the semantic source/contract comparisons.

[Documentation evidence](./documentation.json) lists every audited directory,
the changed READMEs, completed scout counts and the final contract-set digest.
The existing [drift report](../../2026-10-31_DRIFT_REPORT.md) was preserved and
extended with entries for all 51 contracts. The corrections cover provider time
and saturation rules, helper hashes and map prerequisites, local application
persistence, worker timing, storage invalidation/retention, evaluator output
behavior, proof scope, SQL grants and deployment/credential boundaries.

Repository asset verification passed for 11 canonical JSON assets, 52 skill
records, 190 Markdown files and 1,442 local links. All 45 audit JSON files parsed
successfully, and `git diff --check` passed.

No unresolved source/contract mismatch was identified after those corrections.
The caller/deployment limits above remain explicit. Hash comparisons found no
changes to existing functional source, tests, manifests or credentials, and no
deletions. Existing user changes remained in place; nothing was staged or committed.

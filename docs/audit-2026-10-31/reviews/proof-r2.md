# Independent EGM Codebase Details Goldfish review, round 2

Review date: 2026-11-01 (Europe/Berlin). Workspace: `/thinisde/trady`. Mode: REVIEW.

## Verdict and scope

**F1, F2, F3 and F4 are addressed in the inspected source. No new actionable correctness, security or documentation finding was identified in this scope.** The weekly helper remediation remains present. This is a static source-review disposition, not a passing integration, parser-parity, kernel-audit or release result.

The requested `crates/trady-domain/src/contracts.rs` **does not exist**. It was not silently treated as read or clean. The actual production strict JSON parser is in `crates/trady-application/src/contracts.rs`; the account/newtype definitions are in `crates/trady-domain/src/lib.rs`. Both actual boundaries were inspected, with exact coverage below. All 19 explicitly scoped current implementation/test/contract targets were read completely, as were the two existing requested production files and all three supplied responses/reports. Additional production and pinned-library sources were inspected over the stated ranges.

This was an independent fresh-context review. No parent transcript, thread history, memory retrieval, child agent, network call, Trading212 request, test execution, build, project-module import, Lean elaboration, kernel audit, backtest, or deployment was used. Read-only analysis scripts inspected source text, ASTs, hashes and links. Test bodies and worker-reported outcomes were inspected; they were not executed or independently reproduced. The only intentional persistent artifact write is this report through `apply_patch`.

## Dispositions of the prior objections

### F1 — Major — Addressed in source: unsigned JSON lexemes

[Parsing.lean:unsignedFields/scalar](../../../lab/proofs/TradyProof/Parsing.lean#L49), lines 49–70, checks the token before Lean's numeric normalization. A leading minus immediately fails. A digit token is parsed with the pinned `Json.Parser.nat`; an immediately following decimal point, `e` or `E` fails. The existing field-width guards remain at lines 128–129 and 179–215. Object keys are decoded before the unsigned-field lookup (lines 93–97), so an escaped ASCII spelling such as `"\u0076ersion"` receives the same guard.

The unsigned-name inventory covers all twelve actual production fields: three policy fields, four parameter fields and five evidence fields. Compare `Policy` at `crates/trady-policy/src/lib.rs:35–46` and `FormulaParameters`/`FormulaEvidence` at `crates/trady-policy/src/limits/mod.rs:26–35,88–106`. There is no unchecked conversion that substitutes a normalized numeric value for the rejected token.

**Challenges traced statically:** baseline requests with raw `parameters.version:1e0`, `1E+0`, `10e-1`, `1.0e1`, `0e0`, `-0`, `-0.0` or `-0e0` now fail before `fromJson?`/calculation. The same decision applies at the other eleven unsigned fields. Plus-prefixed and leading-zero documents remain invalid JSON: the former cannot enter the number branch; the latter leaves an extra digit where the container requires a delimiter.

[parity.py:malformed_wire_fixtures](../../../lab/proofs/parity.py#L134), lines 137–147, substitutes each of these eight original lexemes into both serialized documents for all twelve fields: **96 raw-wire cases**. It sends strings through `invoke` without JSON reserialization. The adapter's `fixture_wire` preserves those same bytes' JSON lexemes. The separate plus/leading-zero cases are at lines 170–173. [test_inputs.py](../../../lab/proofs/test_inputs.py#L112), lines 112–123, inspects both original/reference token spellings.

The pinned serde_json 1.0.154 source confirms the production distinction: `src/de.rs:509–527` selects floating point for decimal/exponent tokens and negative zero; `src/value/de.rs:169–192,303–314` and `src/number.rs:535–615` retain numeric-category dispatch into typed fields. The pinned Lean parser's normalization behavior remains at `Lean/Data/Json/Parser.lean:129–169`; the new guard avoids depending on it for unsigned syntax. This closes the prior source counterexample. These fixture outcomes were not executed in this review.

### F2 — Major — Addressed in source: Unicode values, keys and duplicates

[Parsing.lean:codeUnit/escapedScalar/string](../../../lab/proofs/TradyProof/Parsing.lean#L8), lines 8–47, supplies a common decoder for object keys and string values. A high surrogate must be followed immediately by a second `\u` escape with a low surrogate; an isolated low surrogate fails. The scalar is constructed as `0x10000 + (high - 0xd800) * 1024 + low - 0xdc00`, which lies in U+10000..U+10FFFF for the checked pair. Non-surrogate code units retain their scalar value. Ordinary direct UTF-8 supplementary characters pass through the same string accumulator without conversion to UTF-16 units.

The old problematic standard string decoder is no longer used for either key or value strings. Although `scalar` retains `Json.Parser.anyCore` for other scalar kinds, its quote branch intercepts every string first (lines 55–60). At exhausted nesting fuel, `value` also rejects containers before delegating (lines 84–88). Decoded-key comparison runs before object-map construction (lines 98–103).

**Challenges traced statically:**

- `"snapshot_id":"\ud800"`, `"\udfff"`, high/ordinary, high/high, reversed low/high, high/literal, malformed-hex and truncated pairs fail; no invalid surrogate reaches `Char.ofNat` as an accepted scalar.
- `\ud800\udc00`, `\ud83d\ude00` and `\udbff\udfff` produce U+10000, U+1F600 and U+10FFFF respectively. Uppercase hex uses the pinned case-insensitive `hexChar`.
- Direct supplementary keys and their escaped spelling compare equal after decoding, so `"😀":1,"\ud83d\ude00":2` reaches duplicate-key rejection before schema rejection. ASCII escaped schema keys decode normally.

The **16 malformed key/value cases** retain raw escapes in `parity.py:148–160`; the supplementary duplicate is at lines 168–169. The accepted metadata fixtures in `fixtures.py:112–120` compare complete results using escaped and direct UTF-8 forms, including all three boundary scalars and escaped ASCII schema keys. Transport never normalizes their wire replacements through a Python decoder before dispatch. `test_inputs.py:131–152` and `test_lean_bridge_errors.py:72–88` inspect this preservation.

Pinned serde_json's pairing/combination rules were checked at `src/read.rs:951–1022`; Lean's old decoder and invalid-scalar-to-NUL behavior were checked at `Lean/Data/Json/Parser.lean:28–59` and `Init/Prelude.lean:2453–2461`. Pinned Parsec consumption/failure behavior at `Std/Internal/Parsec/Basic.lean:58–100` confirms that malformed consumed escapes propagate through `many` rather than ending a valid string. The earlier NUL corruption objection is resolved in source; no executable Unicode parity result is asserted here.

### F3 — Major — Addressed in source: parser evidence versus infrastructure failure

[bridge.py:RustFormulaBridge.fixture_wire](../../../lab/lean/bridge.py#L82), lines 82–101, forwards the original JSON document to the sealed executable snapshot. After execution, the source is revalidated **before** failure classification. Only inner exit **1**, literally empty stdout, and one exact newline-terminated member of `RUST_INPUT_DIAGNOSTICS` raises `FormulaInputRejected`. The allowlist contains only the three production JSON/schema diagnostics from the strict parse/deserialization boundary.

[bridge.py:cli](../../../lab/lean/bridge.py#L121), lines 121–130, maps that dedicated type to exit **2** and the `PARSE_REJECTED` line. Generic exceptions remain exit **1** with `BLOCKED: <exception type>`. Explicit local size/UTF-8 input rejections have distinct reasons; they cannot match one of the corpus's expected Rust parse/schema lines.

[parity.py:execute_parse_rejection](../../../lab/proofs/parity.py#L95), lines 95–100, requires literally empty stdout, a positive expected exit and the selected diagnostic rule. The actual adapter call at lines 196–198 additionally requires exit **2** and exact equality to the complete expected diagnostic plus one newline. Direct Rust requires exit 1; Lean requires exit 2 (lines 193–195). Exceptions escaping `invoke` do not create an evidence record. Records are appended only after all three checks (lines 191–204), following an actual successful-witness call in the harness (lines 184–187).

**Failure schedules traced statically:**

| Event after/before a prior successful witness | Adapter path | Can become this case's parser evidence? |
| --- | --- | --- |
| Binary missing, symlink/nonregular, or digest changed before snapshot creation | Constructor/snapshot verification raises an ordinary exception | No |
| Subprocess timeout, launch failure, or unrelated RuntimeError | Exception escapes `fixture_wire`; CLI emits generic exit 1 | No |
| Inner process returns signal `-11`, even with parser-looking stderr | Exact inner-exit predicate fails; RuntimeError becomes generic exit 1 | No |
| Pinned source changes while sealed bytes execute, followed by an otherwise valid parse rejection | Post-execution `_verified_binary` fails before classification | No |
| Inner exit 1 with parser diagnostic but nonempty/whitespace stdout, or extra stderr text | Exact clean-result predicate fails | No |
| Inner exit 0 with malformed response JSON | JSON decoding fails as a generic exception | No |
| Clean inner exit 1, empty stdout, exactly one allowlisted diagnostic; integrity checks succeed | Dedicated type; adapter exit 2 with exact typed line | Yes |

The production source for those errors is `weekly.rs:203–217`, `contracts.rs:146–157` and the CLI's error formatter at `main.rs:48–55`. The sealed-copy/open/hash/post-execution ordering remains at `bridge.py:36–69,86–95`. The new focused bridge tests (lines 32–93) and proof tests (lines 73–110) explicitly inspect these distinct outcomes, integrity precedence, wrapped signals and literal-empty-stdout behavior. They were read, not run. A successful earlier witness alone is no longer used to excuse a later generic failure.

### F4 — Nit — Addressed in source: final represented leap second and overflow

[parity.py:timestamp](../../../lab/proofs/parity.py#L19), lines 19–54, validates Gregorian dates, clock/offset components and the UTC epoch-second range **after** offset conversion, retaining the fraction separately. [Formula.lean:validTimestamp](../../../lab/proofs/TradyProof/Formula.lean#L14), lines 14–22, checks the encoded second/fraction domain. `maxTimestamp` now includes `+262142-12-31T23:59:60.999999999Z`, encoded as **16420533753599999999999**. The old ordinary maximum remains separately named `maxOrdinaryTimestamp = 16420533753598999999999`.

Positive duration addition uses `addSeconds` and still checks the ordinary-result bound at `Formula.lean:548–558`. Compare the production `checked_add_signed` calls and distinct ordinary/FX error paths at `limits/mod.rs:384–412`. The holding deadline is compared/copied, not duration-extended by this calculation.

**Boundary witnesses inspected:** `fixtures.py:106–111` changes only the baseline holding deadline to the ordinary maximum, final leap start and final leap maximum. Lines 276–294 retain independent ordinary-expiry, final-leap-expiry and FX-expiry overflow cases. `fixtures.py:422–426` includes lower/upper UTC offset overflow, including both final leap fractions. Invalid calendar/range strings reach the actual Lean decoder in negative cases through `transport(..., malformed_timestamp=True)`; Python conversion exceptions never count as observed parser rejection. `test_inputs.py:41–57` declares the exact endpoint encodings.

Pinned Chrono 0.4.45 confirms second 60 becomes underlying second 59 plus fraction 1e9..1999999999 (`format/parsed.rs:768–781`), offset conversion preserves that fraction (`offset/mod.rs:587–592`, `naive/datetime/mod.rs:593–600`, `naive/time/mod.rs:787–791`), and positive whole-second duration addition removes the extra leap fraction and rejects a date overflow (`naive/time/mod.rs:605–640`, `naive/datetime/mod.rs:529–533`). Date endpoints and expanded-year limits were checked in `naive/date/mod.rs:1505–1509,2443–2451`. Fractional truncation was checked at `format/scan.rs:49–65`.

The previous endpoint objection is resolved, not waived. These are static boundary traces and inspected fixtures, not executed timestamp results.

## Weekly helper boundary and certificate checks

Every execution-capable public helper operation validates the complete manifest first: `verify_candidate` at `weekly_lab/mod.rs:75`, reconciliation at line 139, development at line 159, and evaluation at line 189. Trait verification immediately delegates at lines 173–179. The verification component accesses use `.get(...).ok_or_else(...)` at lines 76–83. An invalid manifest therefore returns before request hashing, `HelperCommand::run`, subprocess execution or `persist_artifact`.

`RunManifest::validate` (`optimization/weekly.rs:51–120`) checks identity, account/mode consistency, complete/ordered/distinct sessions, cutoff/deadline relationships, and all **18** required component hashes. This is the full validator rather than a two-pin substitute.

The inspected regression at `weekly_helpers.rs:184–250` constructs **41** invalid manifests (18 missing pins, 18 malformed pins, five other invalid relationships), exercises all five direct/trait call paths and compares the validator's exact error. Its witness uses cutoff `now−1h`, completed session `now−1d..now−2h`, activation deadline `now+1d`, and next session `now+2d..now+2d+6h` (lines 110–181). The invalid future cutoff is `now+1h`; the invalid activation deadline is the next session close. Marker/artifact-absence assertions are at lines 246–247. This matrix was inspected, not executed.

Account parsing matches the inspected production newtype/enum definitions: ASCII identifiers of length 1–96, three uppercase currency letters, exact account modes, both sides and all six statuses, including UNKNOWN. Optional intent/FX fields remain optional. Full nested allowlists and width checks remain before derived decoding. `validateAccount` additionally retains nonempty snapshot identity, nonnegative cash, positive unique positions and broker-order identity/quantity/status consistency. Its executed guard supplies `AcceptedChecks.account_input`; `accepted_account_input` and `invalid_account_input_cannot_accept` use that certificate (`Formula.lean:251–289,414–452,454–462,612–620`).

The 26 registry entries match the 26 distinct `Audit.lean` requests and their source declaration names by static inspection. `Parsing.lean` remains in `SAFETY_FILES` (`bindings.py:23`). No forbidden declaration token was found by the existing stripped-string/line-comment scan over proof sources. None of this claims successful elaboration, an axiom closure result or a Rust parser/refinement theorem.

## Runtime bounds and source preservation

The authorized self-test adjustment is isolated: reversing only `timeout=300` to `timeout=180` in the current `self_test.py:90` reproduces the prior review's complete-file SHA256:

`ff5cdf5d218209d2afe2c79ba5128d1e5744e6fa80a46894fe0e2d999fecb0d6`.

The source therefore changes only that timeout relative to the first reviewed self-test snapshot. It is an **outer bound per offline proof/parity stage**, not a deployed helper or per-case setting.

| Boundary | Inspected current bound | Evidence |
| --- | --- | --- |
| Offline self-test proof/parity stage | 300 seconds each | `self_test.py:87–94`; reverse-only hash above |
| Adapter calculator subprocess | 10 seconds | `bridge.py:87–90`; unchanged timeout in scoped diff |
| Parity subprocess | 30 seconds | `parity.py:78–85` |
| Build/audit subprocess inside helper | 120 seconds | `check.py:22–27`; entire file matches prior review hash |
| Self-test tamper subprocesses | 30 seconds | `self_test.py:130–146`; covered by reverse-only hash |
| Reference/helper/adapter request bounds | 4 MiB | `FormulaMain.lean:35–44`, `check.py:19,81–83`, `bridge.py:13,84,110–112`, `runner.rs:20,116–120` |
| Reference nesting, quantity/target correction | 128 levels; two quantity corrections; one target tick | `Parsing.lean:84–110`, `Formula.lean:393–395,403–410,502` |
| Production helper wall/CPU/resource validation and cleanup | Existing configured limits, 1–1800-second wall/CPU ranges, memory/output bounds and 250 ms pipe cleanup retained | `runner.rs:77–91,132–221`; runner has no current diff against HEAD |

The scoped changes do not raise runtime/per-case/deployed-helper budgets. The only changed tracked runtime configuration path reported by the read-only worktree diff was `services/runtime/trady-evaluator.service`; its diff adds syscall restrictions only, retaining its existing resource/time limits at lines 36–43. This is source configuration inspection, not a deployed-host inspection.

An independent text-only reversal of the new timestamp definitions and the two `expiry ≤ maxOrdinaryTimestamp` substitutions reproduces the prior full `Formula.lean` hash:

`8ee8c4e4ea6636784f8c7804f78c43f3dea23f78e73c5b9b6193adb6d2786aa3`.

The current `AcceptedChecks` slice (from its structure declaration up to `def calculate`) has SHA256 `25fa184887059364e84d31d331f64aa0be5a44659abb9629442cb57112064fef`, matching the supplied preservation script's prior-slice value. That script was read but not run. The full-file reverse hash independently supports preservation of every other Formula byte, including theorem statements/proofs, certificate construction, corrections and heartbeat/depth settings. The unchanged primary/supporting hashes are recorded below.

## EGM contract correspondence and references

All four scoped component contracts have Overview, Interface, Invariants, Side Effects and Verification sections required by the supplied template. Their changed descriptions correspond to the inspected public entrypoints, validation/error paths, source pinning, bounds and parent-owned verification. Their finite-parity versus universal-refinement boundary is explicit.

| Contract | Current correspondence checked | Local Markdown links | Local symbol citations |
| --- | --- | ---: | ---: |
| `lab/proofs/README.md` | Parser/wire/error protocol, 300-second offline stage, complete corpus inventory, source/theorem binding and execution trust limits | 4 | 23 |
| `lab/proofs/TradyProof/README.md` | Model/parser interfaces, account certificate, timestamp versus duration bounds, 26 declarations and pure/build effects | 6 | 23 |
| `lab/lean/README.md` | Sealed execution, raw-wire path, dedicated versus generic failures, dependencies, runtime bounds and inspected tests | 1 | 21 |
| `crates/trady-adapters/src/weekly_lab/README.md` | Full manifest validation at every operation, checked component access, helper/artifact behavior and durable evaluation separation | 2 | 19 |

All **13** local Markdown targets and **86** bracketed file/symbol citation occurrences resolve. Qualified method references were also checked against the inspected containing definitions. The complete Markdown-link inventory is:

| Source location | Relative target | Result |
| --- | --- | --- |
| `lab/proofs/README.md:10` | `IMPLEMENTATION.md` | Exists; explicitly historical at its lines 3–6 |
| `lab/proofs/README.md:18` | `TradyProof/README.md` | Exists |
| `lab/proofs/README.md:53` | `./TradyProof/Parsing.lean` | Exists |
| `lab/proofs/README.md:81` | `./test_inputs.py` | Exists |
| `lab/proofs/TradyProof/README.md:13` | `./CheckedDecimal.lean` | Exists |
| `lab/proofs/TradyProof/README.md:16` | `./Formula.lean` | Exists |
| `lab/proofs/TradyProof/README.md:26` | `./Parsing.lean` | Exists |
| `lab/proofs/TradyProof/README.md:32` | `./Witness.lean` | Exists |
| `lab/proofs/TradyProof/README.md:35` | `./Holding.lean` | Exists |
| `lab/proofs/TradyProof/README.md:37` | `./Revision.lean` | Exists |
| `lab/lean/README.md:108` | `../tests/test_lean_bridge_errors.py` | Exists |
| `crates/trady-adapters/src/weekly_lab/README.md:55` | `../../../../lab/proofs/bindings.py` | Exists |
| `crates/trady-adapters/src/weekly_lab/README.md:58` | `../../../../lab/proofs/README.md` | Exists |

The linked historical implementation handoff was read completely. Its further local links to audit remediation, the weekly-design proof section, and bindings resolve; the design anchor is present at `docs/v1/weekly-optimization.md:425`. Its old counts/digests are historical evidence, not current verification pins. The inspected Lake 4.19 source also confirms that `-d` selects the package while `env` forwards the explicit file argument in the caller's working directory, consistent with the documented root-level audit command.

Independent static AST counting, without importing or invoking project code, gives **116 formula + 27 decimal + 528 seeded = 671 exact-output cases**. The malformed inventory is **144 structured + 96 unsigned lexemes + 16 surrogate + 9 raw document cases = 265**. The helper adds its one revision witness, giving **937**. These are inventory counts, not test outcomes.

Finite parity still reduces translation risk without proving production refinement, parser equivalence, compiler/Python/host correctness, observation truth, durable CAS/storage or broker behavior. No unresolved prior objection was waived to reach this verdict.

## Complete read and SHA256 evidence

Every requested existing primary target and supplied response/report was read over its complete physical line interval. Long targets were read in contiguous declaration/logical sections, with no body omitted or byte cap used as a stopping condition. Supporting/dependency ranges below are exact focused reads; a whole-file digest does not imply a whole-file semantic audit. Existence-only checks of other mentioned paths are not presented as full reads.

`R` = `/thinisde/trady/` for relative repository paths. Plugin and supplied-response tables use absolute paths. Dependency prefixes are defined below. Hashes identify exact reviewed bytes and are **not release pins**.

The primary targets' opening-review hashes and the recorded supporting hashes were compared again with no mismatch. All **77** listed files were rehashed against the recorded review snapshot immediately before report creation at **2026-10-31T23:31:15.805Z**; **0 changed or inaccessible files** were found. The absent requested `R/crates/trady-domain/src/contracts.rs` is excluded from that count and explicitly reported above.

### Instructions and supplied response/report inputs

| File | Physical lines / bytes | Read ranges | SHA256 |
| --- | ---: | --- | --- |
| `/root/.codex/plugins/cache/egm/egm/0.1.0/agents/codebase_details.md` | 69 / 4683 | 1–69 | `d80db07e449b57b3b3776d1401cca72a54b6b65a0ab218b86f9064ee4b8167bc` |
| `/root/.codex/plugins/cache/egm/egm/0.1.0/docs/codex.md` | 117 / 6104 | 1–117 | `a7390390c233ece29a2ee7508a8d328b6b4ee200f649417c097687c76f014ad0` |
| `/root/.codex/plugins/cache/egm/egm/0.1.0/skills/egm-bootstrap/references/templates.md` | 68 / 2064 | 1–68 | `f49793727cccbd72afdc4a0569a1d8e8138e943d2fc9b0df772b09deaca80437` |
| `/root/.codex/plugins/cache/context-mode/context-mode/1.0.169/skills/context-mode/SKILL.md` | 300 / 16683 | 1–300 | `ce26cb01de7a116eab256f916620935f029e850771f3fb6cb7d40a3ce8d1f7ff` |
| `/tmp/trady-remediation-20261031/weekly-proofs/handoff.md` | 107 / 8957 | 1–107 | `aa288cc5a0d92cd0e6d65af3d7a1033a8f424d6ff5946cf9a20a5026b4aca28e` |
| `/tmp/trady-remediation-20261031/proof-review/goldfish-verification.md` | 150 / 20718 | 1–150 | `08d037d146621a6e3fdd8b5ed22808f1564c64bc705c12e8944a4c9bfe609a03` |
| `/tmp/trady-remediation-20261031/proof-fixes-r2/handoff.md` | 141 / 9511 | 1–141 | `145c8852cdcff2a6a4e1afdcba2b18d5c069f9315f9194fa7bec3dcb0ca348e6` |
| `/tmp/trady-remediation-20261031/proof-fixes-r2/verify_preservation.py` | 88 / 5223 | 1–88 | `0f703e086ceb82f77e5c8752db72f36f1ad0d786366cfa0bb2a1fe53a4c673aa` |

### Required current targets (complete coverage)

| File | Physical lines / bytes | Read ranges | SHA256 |
| --- | ---: | --- | --- |
| `lab/proofs/TradyProof/Parsing.lean` | 224 / 10265 | 1–224 | `ccab3a5e4eaaf63c45e1738c720103233fd45fbe4e5a81e778f33fa98711b704` |
| `lab/proofs/TradyProof/Formula.lean` | 644 / 34895 | 1–250; 251–453; 454–644 | `ee9d491f3e380198b451da2c51ff450d014818b86d35eab1becfd22c2033531c` |
| `lab/proofs/FormulaMain.lean` | 48 / 1832 | 1–48 | `e84c62e426ca18cefefee25b2313bcd9346d21f98a411e2d29d56f10947878a3` |
| `lab/proofs/fixtures.py` | 432 / 31345 | 1–83; 84–297; 298–432 | `c182caad35fd51c68ca7dce84755689de64334d0558c9640672a9d120a76763a` |
| `lab/proofs/parity.py` | 224 / 12739 | 1–134; 134–224 | `943f9136c9894daf7c886b0e3ce9ad568f8359af74350972104c3cb9fc874437` |
| `lab/proofs/check.py` | 118 / 6030 | 1–118 | `2b6dc96360339db328aacaaeb4fe128be7a076f6b319d34279dca0f8f16c7400` |
| `lab/proofs/self_test.py` | 169 / 9661 | 1–169 | `47a3d45f889b9b9c50fbd06a751011b23fc01363f6e90a1671ff22fda31b203a` |
| `lab/proofs/test_inputs.py` | 165 / 9500 | 1–165 | `f7f3d91c670ff45e745ec7f457966e3a0a2f7eb79eddaaa23e98a7a4ec64738e` |
| `lab/proofs/bindings.py` | 136 / 6793 | 1–136 | `3f902d736219393aa54f2a321f120cc5f08e65b1adbc73da139c2edbb647c358` |
| `lab/proofs/README.md` | 187 / 12007 | 1–187 | `e28da3a5a56c945e38ff566c8f4ee281794ea5c92b4fe7f00dd6666deae41a9c` |
| `lab/proofs/TradyProof/README.md` | 105 / 6251 | 1–105 | `b19c8e33937b8e22299e03ef444d64d3bf73c86a9df526ad05eb6f1da95d6882` |
| `lab/lean/bridge.py` | 134 / 6023 | 1–134 | `7e6f6e144589667aa1fe1ad24ffa2dbd98d307a00924d1274594d12a1d5ece21` |
| `lab/lean/README.md` | 112 / 6930 | 1–112 | `49332be5cbc572a403f3a3121e4f6c767eafed2fbf529c7f30bec40981ccff03` |
| `lab/tests/test_lean_wire_adapter.py` | 39 / 2201 | 1–39 | `f53a5994b00a7c924977280e3c97ec4920dcbad3fbf0dcac819cc8391e9cc4b8` |
| `lab/tests/test_lean_integrity.py` | 101 / 4609 | 1–101 | `149bd2a7810c18babc01ff15cf0bed2fa6cc17af543b608a6be516dd79b164e9` |
| `lab/tests/test_lean_bridge_errors.py` | 97 / 5718 | 1–97 | `cdfde8701d9741a71cd32f2866e9539a74d9af60fc085e42d03b7c4e80faa0a9` |
| `crates/trady-adapters/src/weekly_lab/mod.rs` | 237 / 8180 | 1–237 | `4d17c1e37fcbba9137d2340e88b2a4d7684a07a4796857ff0fd846543905da79` |
| `crates/trady-adapters/tests/weekly_helpers.rs` | 250 / 9351 | 1–250 | `c5a67cb0df8d6bddd92fcde5ee03c0bf70c906c5953f8911435995b931830032` |
| `crates/trady-adapters/src/weekly_lab/README.md` | 93 / 5687 | 1–93 | `6326f5375df04559f3a843671d7a3a04aa4174a4f5547be1405b7d3491129fdd` |
| `crates/trady-cli/src/weekly.rs` | 262 / 10422 | 1–262 | `d25c1ce20c356808fe38d16b7317060d245aee04486119abfc745ebad3ed3279` |
| `crates/trady-domain/src/lib.rs` | 384 / 11093 | 1–384 | `321ea2c1312451c3b67d115d461aaefd341e43aec858735783f28837c8fbd8b6` |

### Supporting production, proof, contract and configuration evidence

| File | Physical lines / bytes | Read ranges | SHA256 |
| --- | ---: | --- | --- |
| `crates/trady-application/src/contracts.rs` | 419 / 16508 | 130–179; 238–316 | `6f6322b000d3d53286146c0d086f415df2e70b3322558cc5ec9051182fefdbeb` |
| `crates/trady-application/src/optimization/weekly.rs` | 799 / 28917 | 1–123; 415–505 | `69f285ed1399a706bb1e762bd0cef4588b7351636e1ef2537caaa8fefad63340` |
| `crates/trady-policy/src/lib.rs` | 760 / 24699 | 1–104; 236–316; 358–485 | `b5bf061feaa599d8391e6e5151c09050fe971ae327f9063c903f2a2f524ad395` |
| `crates/trady-policy/src/limits/mod.rs` | 447 / 16725 | 1–151; 152–447 | `84c7a196dba8f8d8a782cc466ea046890fcbba4abb8d6b435c80642c9a59565f` |
| `crates/trady-policy/src/limits/proof_bridge.rs` | 122 / 3693 | 1–122 | `fc4b199d377908924c7efc955938b218241b1d6a2ba14dd21a3222fdc80509cc` |
| `crates/trady-policy/src/economics.rs` | 334 / 11291 | 1–140 | `8eb2dd69ca40eeb74e301aa9cc3eb6865074473ae48cae9121bff71c950d9f31` |
| `crates/trady-cli/src/main.rs` | 296 / 12503 | 48–57 | `f0d48147254f5cd3e0dee775f3d3afc1800aa89e628fe13ce2454af16bcf14b3` |
| `crates/trady-adapters/src/weekly_lab/runner.rs` | 394 / 14421 | 1–246; 247–394 | `f990b75978b6cb92350155012ff054c68f7702f14826785d64ddac9a11ad1f42` |
| `lab/proofs/Audit.lean` | 27 / 1388 | 1–27 | `45158a6589fa73475199c323291d9c958a0366050193bcbf31c98f209e46d0e1` |
| `lab/proofs/Main.lean` | 22 / 923 | 1–22 | `a9a03676bf1443071ebe77beb78b2865344b23336745edb6c9a85abfa6a000b7` |
| `lab/proofs/lakefile.lean` | 11 / 197 | 1–11 | `1b513f6549158d69f9fcc80fb5f2b86a6e8a3232d4cae34f2ffc64ab39bce034` |
| `lab/proofs/lean-toolchain` | 1 / 25 | 1–1 | `55e97be96000b5e9e290c9e74482e5e317861499a5540353ce845471bded8cea` |
| `lab/proofs/pin.py` | 36 / 1724 | 1–36 | `994ce31bdbb32936d8c1c853b6aaaca7b56fbe4352809b4f47f54fbfe81fbc35` |
| `lab/proofs/TradyProof.lean` | 6 / 163 | 1–6 | `dd78dc205f39ac5441c708a0d15eeea4285aa9f99750cba65db740c24c07bc51` |
| `lab/proofs/TradyProof/Safety.lean` | 34 / 1171 | 1–34 | `f5b6b14024d959b04a2da0d72055b02f6e936c5d529b49f2503750e717eec260` |
| `lab/proofs/TradyProof/CheckedDecimal.lean` | 187 / 8129 | 1–187 | `fec6edd218426dd4a034882f27f6b0259b97e9429617c720d41474fa09a3e4fa` |
| `lab/proofs/TradyProof/Holding.lean` | 182 / 8320 | 1–182 | `23cfe8515069084323dff9a08a13b1152338cad03f7dba57b18a14969d4561cb` |
| `lab/proofs/TradyProof/Revision.lean` | 67 / 2862 | 1–67 | `ae39b8a24ae03abe82128f23f85100a563f117a8b79f2f39cbb06916397d9999` |
| `lab/proofs/TradyProof/Witness.lean` | 62 / 2704 | 1–62 | `81a68134844887209c33ba6f807589f8ac1b3c2075b93c80279898abca652012` |
| `lab/lean/run.py` | 93 / 4423 | 1–93 | `9893f7c5509b54a034dfe9b14af6037493f25ef75a064302c8fb8257f535b295` |
| `lab/lean/BridgeAlgorithm.py` | 97 / 5477 | 1–97 | `e02987aeceec6868d924c3fc880863f963b828662c721a79063ae4b52450a925` |
| `lab/proofs/IMPLEMENTATION.md` | 242 / 14843 | 1–242 | `74a665c581ba7e7807b132e351495c3b956636c280fe444f09f8fa1ae1cbfd69` |
| `services/runtime/trady-evaluator.service` | 43 / 1364 | 1–43 | `a8935543585c367432f291bd6acfb91c23257af54f63dee9a8901a486fb3efc5` |
| `Cargo.toml` | 39 / 1416 | 12–15; 17; 22 | `494b04f6661d262f8ec99e29e6a69d2f55be72e082b821a0a280c1b18e9fb237` |
| `Cargo.lock` | 2954 / 73149 | 220–223; 1710–1713; 1861–1864 | `d1d5b5ba963551ea5e5d3b5e085a2100ffe1ee838b3b3b39ef6516600458e447` |
| `docs/v1/weekly-optimization.md` | 742 / 50723 | 425 (heading/anchor lookup only) | `e49eb338b96b8ed65737c06244ea4222ad51a6d9864c98595f0823da4bbf520c` |

### Pinned Lean 4.19 source evidence

Prefix L = `/root/.cache/trady-tools/lean-4.19.0-linux/src/lean/`.

| File | Physical lines / bytes | Read ranges | SHA256 |
| --- | ---: | --- | --- |
| `Lean/Data/Json/Parser.lean` | 259 / 6134 | 1–259 | `f9f691a046004be29208e429018b0bf5387961ec9c9c3658d3c41c4c582e681b` |
| `Lean/Data/Json/Basic.lean` | 307 / 9617 | 14–270 | `796e7f1ef70b5679095eaf261d572086a3688b60cac5d342f17ba3c95f96f577` |
| `Std/Internal/Parsec/Basic.lean` | 167 / 4482 | 1–167 | `4cdbd13bab65fbb6a027dd72b7bc52487453ce9f3556933583d1e158df554212` |
| `Std/Internal/Parsec/String.lean` | 115 / 3117 | 1–115 | `38c441b0201c111b2df8e8ad4e29331b157868c9c4548fcbf1f2afa0e782f773` |
| `Init/Prelude.lean` | 5300 / 206229 | 2436–2471 | `8a8aa790a4be65e5c8987b7d87eab3102af62ad812cc341c1b452341f740cd5c` |
| `lake/Lake/CLI/Main.lean` | 694 / 24575 | 185–225; 507–516 | `93f9d50b2c6ca64eb63f53baa65a8505664e3f6866549749e5a774ba663967aa` |
| `lake/Lake/CLI/Actions.lean` | 91 / 3677 | 1–32 | `f2316ccbba3adfd05d014016075fd9ec6d2d25d51e16d23d3f02ea684e3c05e1` |

### Pinned serde_json 1.0.154 source evidence

Prefix J = `/root/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde_json-1.0.154/`.

| File | Physical lines / bytes | Read ranges | SHA256 |
| --- | ---: | --- | --- |
| `Cargo.toml` | 165 / 3142 | 1–30 | `548aafeb7c702b69b9de950033f10b59f70baf38592d275d8ca8cff19f7750c3` |
| `src/de.rs` | 2714 / 86855 | 430–550 | `cb55666913c47d86130e7bd48a9c35081ea9a4bb0e2c45ab6429912d905629c5` |
| `src/read.rs` | 1140 / 35091 | 890–1026 | `523eec008a92c1be499978ee96bce7a8dc7d6ddff9a63b5ed404c19d01fb953a` |
| `src/number.rs` | 814 / 25319 | 525–615 | `1730b1c1a9b2a75f9e52b165191ca77cb3dbd82fec3b23f41f097f4f07f3410e` |
| `src/value/de.rs` | 1507 / 41251 | 169–205; 219–240; 303–340 | `ceda1bf415aafde89caa83bf275811b70d393717d4e1038e0c61822d2da02993` |

### Pinned Chrono 0.4.45 source evidence

Prefix C = `/root/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/`.

| File | Physical lines / bytes | Read ranges | SHA256 |
| --- | ---: | --- | --- |
| `Cargo.toml` | 181 / 3509 | 12–24 | `89b7bdca1f8745c0894ff039971c04fc51e444becbc9eb87598509a32955b1b5` |
| `src/datetime/serde.rs` | 1351 / 44230 | 53–105 | `71f3ff0b9f484efa52c5abf811cbcd0da5d9f01b6bb0c9cc577886831fd6231c` |
| `src/datetime/mod.rs` | 2016 / 74320 | 94–105; 404–418; 435–477; 705–715; 1888–1903 | `1ccc3656a30e1c7115b3db850c121e4b148d6c9ace74fd5d840532ddb4a36f5f` |
| `src/format/parse.rs` | 1941 / 99452 | 580–612 | `0114cc8f7cee11af7fd091f874c609c71e2ba094e2d793459f440150bb3cb1ed` |
| `src/format/parsed.rs` | 1913 / 79444 | 750–785; 803–855; 903–940 | `369844ce5079271e526b8420b70d25a69dbfc8dc6ad2a534ea36fd9e80edd093` |
| `src/format/scan.rs` | 432 / 14663 | 49–68 | `100da6368c449f45b26c44ffe29892d91f814a3bc98ed0cd1c3679e2852b7b56` |
| `src/offset/mod.rs` | 715 / 29413 | 570–621 | `641a632cdec0c966d558231f1e94cdddb1a447f51410b7d56445e65bb6878c25` |
| `src/naive/date/mod.rs` | 2634 / 95929 | 1498–1517; 2440–2452 | `e2ffd9af71e593b2475ce9f3fd3d37fe861e762f1e920e92a35c13a2b0cd882b` |
| `src/naive/datetime/mod.rs` | 2171 / 80441 | 525–543; 585–615; 935–947 | `6239fbfc3fb872a5ee0f24643c95af2e266f70a9ab0812628e0349a694976b22` |
| `src/naive/time/mod.rs` | 1668 / 62651 | 430–468; 600–640; 735–758; 780–811; 1290–1350 | `dad88f7e02c3a552e135a2c7a843d71a86696d29e48808988b2df6c8210d2672` |

## Parent-owned completion boundary

Run the final integrated Rust/Python/Lean checks, complete standalone three-way corpus, copied-snapshot self-test with the authorized 300-second stage limit, and full axiom audit using the final exact binaries. Then regenerate source/binary/dependency/safety/theorem locks from the stable integrated snapshot. This reviewer supplies static dispositions and read/hash evidence only; no full-run success, runtime guarantee or deployment approval is implied.


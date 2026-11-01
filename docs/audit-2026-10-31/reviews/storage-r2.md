**Storage remediation R2 — independent EGM Codebase Details Goldfish review**

Mode: REVIEW. Disposition: **all three prior findings are addressed in the reviewed source by the accepted READ COMMITTED restriction; no additional demonstrated correctness, security or scoped documentation finding was identified.** This is a static disposition, with no deployment approval or claim that the regression tests passed.

The persona, Codex integration, template standard, original handoff, previous independent report and R2 handoff were read before evaluating the implementation. Every supplied review target was then read completely, with the supplemental reads and hashes recorded below. No parent transcript, thread history or memory was retrieved. No product source was changed; no tests, builds, SQL, database connections, network calls, Trading212 requests or child agents were run. The only authored file is this report, written with apply_patch. Compilation and earlier passing-run statements in the supplied handoffs remain reported information, not independently obtained runtime evidence.

**Prior finding 1 — stale writers can miss absent invalidations/tombstones: addressed statically**

Affected boundaries: `migrations/0011_fail_closed_storage.sql`, `research.require_read_committed` L8–17, `research.guard_intelligence_observation` L576–642, and `research.guard_intelligence_rejection` L701–739. Their calls are at L580 and L705, before the advisory lock and all registry/history/invalidation reads.

Independent schedule analysis:

- A pins a REPEATABLE READ snapshot after scope registration but before any observations or invalidations exist. B commits an erasing SOURCE invalidation without updating the scope. A's otherwise valid new observation now fails with SQLSTATE `0A000` / `STORAGE_REQUIRES_READ_COMMITTED` before it can use that empty snapshot. An independently pinned rejection-marker writer fails at the corresponding guard.
- With the same empty-history snapshot, B instead inserts an observation with lineage L and erases it in one READ COMMITTED transaction. A's new-ID observation and analogous marker are again rejected before the snapshot-visible history scan. This does not depend on finding or versioning a row that was absent from A's snapshot.
- After rollback, a READ COMMITTED retry must pass the isolation guard and encounter the actual content rule: observation reimport checks at L629–636 and rejection checks at L712–737. The current source still rejects erased lineage and applicable invalidations. The guard also rejects SERIALIZABLE and every other isolation spelling except exactly `read committed`.

`crates/trady-storage/tests/intelligence.rs`, `sql_intelligence_empty_history_snapshots_cannot_miss_invalidations_or_new_tombstones` L840–928, implements both witnesses for REPEATABLE READ and SERIALIZABLE, separately pins observation and marker writers before publication, confirms their zero-row snapshots, asserts the exact isolation error, and asserts the specific content-rule errors after rollback. Administrative reads check that no content or children were reintroduced. These are inspected test definitions, not executed reproductions.

The prior objection is satisfied by rejecting the unsupported transactions, not by claiming that the unchanged matching-history locks make fixed snapshots current.

**Prior finding 2 — stale erasers can miss newer children: addressed statically**

Affected boundaries: `research.guard_intelligence_erasure` L428–439 (guard L431), `research.guard_intelligence_invalidation` L455–474 (guard L458), and `research.refresh_intelligence` L494–524 (guard L498), all in migration 0011. Publication is guarded at `research.guard_intelligence_assessment` L662–697 and `research.guard_intelligence_rejection` L701–739.

Independent witness: A pins a fixed snapshot while observation O exists and has no children. B directly publishes an assessment and rejection for O and commits, leaving the scope and O versions unchanged. A can no longer perform direct document erasure, insert the erasing invalidation, or run refresh using its stale snapshot: each applicable entry point raises the isolation error before its cleanup. The BEFORE erasure trigger is invoker-rights, and the analyst's explicit EXECUTE grant on the pure isolation helper at L17 is present; its call does not fail merely because PUBLIC execution was revoked. Empty refresh is also guarded before its scope loop.

For supported READ COMMITTED operations, the existing shared scope/input locks and fresh PL/pgSQL statement reads remain in place. Direct erasure still clears both observation JSON and canonical text and deletes children through `research.erase_intelligence_dependents` L440–453. Erasing invalidation still deletes matching children and erases matching observations in the insertion transaction through `research.apply_intelligence_invalidation` L477–492. Refresh retains the same cleanup statements at L505–522. Thus the proposed isolation rejection does not remove the supported cleanup path.

`sql_intelligence_stale_erasers_cannot_miss_later_committed_dependents`, `tests/intelligence.rs` L932–998, covers both fixed-snapshot levels and all three erasing operations. Its later child publication uses the analyst boundary; administrative counts verify physical row presence after rejection and absence after the READ COMMITTED retry. The refresh variant expires only the newer children, preserving the unchanged parent/scope witness. The parent JSON/canonical assertions distinguish refresh from parent erasure. `sql_intelligence_registry_assessment_and_empty_refresh_require_read_committed` L1002–1032 additionally covers registry insertion, explicit activation, assessment insertion and a nonexistent-scope refresh.

**Prior finding 3 — fixed-snapshot runtime authority can reuse old readiness/pointer: addressed statically**

Affected boundaries: migration 0011, `release.current_active` L210–244 (guard L214), and `release.advance` L197–207 (guard L200). The runtime lookup checks isolation before querying `release.active` or resolving candidate/evidence state. The prior schedule's first successful fixed-snapshot authority lookup is itself now unsupported; an executor cannot obtain the two successful lookups required by the old counterexample.

Independent witnesses: A pins a snapshot while R0 is positive and unexpired; B commits either newer negative readiness R1 or a replacement active pointer. A's subsequent authority call raises the fixed isolation error before it can inspect R0/the old pointer. Normal READ COMMITTED lookup retains the newest verifier readiness selection at L220–223, including a negative record, and the scope/owner/candidate/readiness validity checks at L224–242. The renewable evidence projection is returned without rewriting activation audit references.

`release_fixed_snapshots_cannot_reuse_readiness_or_replaced_active_pointer`, `crates/trady-storage/tests/weekly_durable.rs` L687–769, represents both witnesses at both unsupported levels. A harmless SELECT pins each old snapshot because the authority API now rejects its first fixed-snapshot invocation. The test first obtains ordinary positive authority, commits the negative attestation through the verifier API or replaces the pointer through the disposable administrator, checks exact isolation errors, then checks denial/replacement resolution after rollback. It also checks release advancement's isolation error. The administrator-built fixture at L614–671 deliberately tests runtime lookup, rather than purporting to prove the production promotion sequence.

`release.resolve` remains a historical projection (`migrations/0006_weekly_lab.sql` L404–418). It is not granted to the executor and is not a substitute for runtime authority. Both Rust authority readers use `release.current_active`: `PgActiveReleaseReader::resolve_active`, `src/weekly.rs` L442–451, and `PgReleaseStore::active_formula` L524–543, which also checks the requested release ID. Later concurrent commits require another lookup, as the revised contracts state.

**Coverage of the ten guarded boundaries and role isolation**

The isolation helper compares the actual transaction setting, has no data access or writes, fixes its search path, revokes PUBLIC execution, and grants only the needed analyst execution. Exactly ten explicit callers were inspected:

```text
migrations/0011_fail_closed_storage.sql
release.advance                          L200
release.current_active                   L214
research.guard_intelligence_erasure       L431
research.guard_intelligence_invalidation  L458
research.refresh_intelligence             L498
research.activate_intelligence_registry  L530
research.guard_intelligence_registry      L548
research.guard_intelligence_observation   L580
research.guard_intelligence_assessment    L666
research.guard_intelligence_rejection     L705
```

The publication/erasure triggers bind direct SQL callers as well as repository calls. The AFTER invalidation/correction/registry triggers are reached through their guarded BEFORE trigger or guarded activation function; they do not provide an alternate service-callable cleanup/publication API. The research helper/function revokes and explicit analyst grants at L812–823 were checked against migration 0010's original table grants at L70–78. Scope selection and retirement tables give the analyst SELECT, without direct mutation grants.

Every renamed weekly/release `_v6` implementation revokes PUBLIC and its original service grantees: freeze L31–32, reserve_trial L53–54, checkpoint L66–67, claim_holdout L76–77, record_ledger L104–105, candidate L134–135, record_evidence L152–153, advance L195–196. The final weekly/release PUBLIC revoke and fixed owner-controlled definer search paths remain at L247–255. The complete legacy bodies and grants in migration 0006, role-provisioning files, and foundation schema/default-privilege grants were checked. This establishes the fresh-install source grants, not the state of a deployed catalog or arbitrary pre-existing memberships/grants.

**Other original remediation invariants and contract correspondence**

- Forward migration and typed APIs: `src/lib.rs` L19–35 and L50–95 embed/register migration 11 after 10 under the existing transaction, advisory lock and exact applied-source comparison. Read-only staged and unstaged diff checks for every existing root SQL file numbered 0000–0010 both returned no differences. The storage implementations still satisfy the existing `IntelligenceRepository` method signatures (`trady-application/src/intelligence.rs` L264–298); the reviewed production Rust hashes also match the previous independent report's pins.
- Nullable weekly/release guards: migration 0011 L19–207 was checked against the entire legacy migration. Mandatory control arguments and compared JSON fields have rejecting paths, the shared fence rejects null generation, holdout completion binds report/run/dataset/input, compared ledger/evidence timestamps require finite values, and release transition references have bounded shapes. Existing SQL tests retain both invalid-input cases and supported calls (`tests/weekly_durable.rs` L393–612). These envelope checks do not replace all application validation or establish evidence provenance.
- Readiness expiry: the lookup checks candidate and scoped owner deadlines and the latest verifier readiness's boolean, identity, snapshot/capability hashes, decision clock and five-minute bound. The earlier expiry/missing-expiry/renewal/new-negative regression remains at `tests/weekly_durable.rs` L501–612. Its execution remains parent-owned.
- Durable credit ceilings: migration 0009's nonnegative usage/positive limit checks combine with migration 0011 L257–272 to enforce `0 <= used_credits <= credit_limit`, immutable window identity, nondecreasing usage and nonincreasing ceilings. The pre-constraint backfill preserves legacy over-limit charges. `src/financial.rs::reserve_credits` L25–84 serializes the credential key, checks all minute/day/month windows before charging, floors a tightened stored limit at existing usage, and independently admits against the caller's lower limit. The status check exactly matches the domain's six `DataStatus` variants. The complete financial tests include raw SQL bounds/refund/rebinding/status cases and the valid lower-cap denial case.
- Intelligence content/rights: migration 0011 L295–420 and L576–739 retain envelope/schema/scope/feature/entity checks, exact canonical content/payload hashes, chronology, exact source/lineage projections, source/reference bindings, current licensed rights and retention bounds. The nine SQL input field sets were independently compared with all nine current non-tax input declarations; every set matches. `src/intelligence.rs` L157–173 supplies typed serialization and returns the SQL-clamped observation deadline. SQL field/envelope/hash checks do not certify nested feature calculations or source truth.
- Current registry and irreversible erasure: activation L526–543 serializes the scope, rejects a retired hash before changing the pointer, retires the predecessor and refreshes in the same transaction. Current-hash registration remains idempotent. New observations require the active hash at L621–625; erased observation identities remain protected by the original primary key and lack of analyst deletion grants, while L629–636 and L712–737 preserve erased-lineage and invalidation checks. A fresh reviewed hash with fresh rights and newly sourced lineage remains valid; it cannot restore an erased identity/lineage. The explicit fresh-lineage/retired-hash test at `tests/intelligence.rs` L646–756 and direct-erasure/new-ID/marker cases L760–802 remain present.
- Correction and cleanup: an existing same-feature/entity predecessor is required at L637–640, and its non-erasing correction marker is inserted atomically at L646–660. Direct invalidations cover OBSERVATION, SOURCE and LINEAGE and require erasure for DELETED/RIGHTS_REVOKED. Already-stored correction vintages retain their stated historical semantics. There is no newly imposed semantic unique index invalidating legacy duplicate audit rows.
- Alias fixes: `refresh_intelligence` uses `stored` in its DELETE USING query at L517–520, distinct from its local `o` record. Observation dependency projections use `evidence` at L617–619, distinct from local `e`. Both parent fixes are preserved; neither collision remains in those statements.
- Application sequencing: the relevant application definitions were independently read. Non-tax `ingest` validates/registers configuration before observation checks and rejection handling (L522–542); non-tax `read` registers before query validation and disabled/live early returns (L799–841). Tax bypasses registration/purge. Subject/alias/audience policy, deterministic assessment validation and source provenance remain application/policy responsibilities rather than implied SQL guarantees.

The three scoped READMEs contain the template's Overview, Interface, Invariants, Side Effects and Verification sections. The package contract also links its source boundary and the migration contract. Their isolation/error/rollback, lookup-time authority, retirement, permanent tombstone, cleanup-side-effect and parent-verification descriptions correspond to the reviewed code (`trady-storage/README.md` L66–103, L128–136; `src/README.md` L112–164, L303–352, L429–439; `migrations/README.md` L145–207, L283–301, L339–344, L373–383).

All 27 local Markdown link occurrences resolve. All 206 explicit inline file/symbol citation occurrences resolve to existing files and local declarations/statements; qualified Rust methods were checked against their cited implementation context. The additional path spellings in commands/prose resolve, including the crate test targets and the root disposable runner. The 109 qualified SQL object/function names found in these contracts resolve to local migration DDL or, for `public.trady_migrations`, the runner's DDL. Reference-only checks of unrelated modules establish reference existence, not a complete semantic audit of those modules.

**Supported limitations and remaining parent work**

- Exactly READ COMMITTED is supported at the ten guarded boundaries. READ UNCOMMITTED is rejected by the exact setting comparison too; the new regression loops directly exercise REPEATABLE READ and SERIALIZABLE. Unsupported transactions require rollback and a supported retry. The implementation does not claim an MVCC publication-fence redesign or continuing authority after later commits.
- Wall time cannot execute a trigger. RLS denies expired/unlicensed payloads while idle, and a repository operation or explicit refresh performs physical cleanup. Erased observation metadata and invalidations remain. Historical caller timestamps do not extend current SQL retention.
- The analyst is a shared trusted service role, not end-user authorization. The database cannot authenticate the external rights review or source truth from hashes, and does not replace typed feature calculations or consumer-specific financial policy.
- The four new regressions are ignored database tests, with explicit isolated-fixture guards. The inspected runner, `scripts/test-postgres.py` L71–95, serializes the suite and invokes all ignored storage tests, so it includes these new cases without a target-list edit. No execution result is asserted here. The parent owns final SQL/Cargo/format/Clippy verification, binaries, proof/source-pin regeneration and any operational decision.

**Complete reads and SHA-256 evidence**

Repository-relative paths below resolve under `/thinisde/trady`. Each full range denotes complete cumulative coverage, read in contiguous numbered sections around functions/tests/contract paragraphs without omitted or byte-capped source. Any initially shortened tool presentation was reread in smaller complete sections. The hashes pin whole-file bytes even when only a supplemental range was semantically inspected. All 54 source/response/standard pins were rechecked against their first inspection values at `2026-10-31T23:29:27.898178+00:00`; none changed.

All supplied repository targets, including the full migration-registration file:

```text
migrations/0011_fail_closed_storage.sql       L1–823   5d78a0d5c64620271752079db2498bf44b33e4503ad4f70fdd11826368dea39a
migrations/0010_optional_intelligence.sql     L1–78    9339bfd4361431c33fce0e7bd612b61aa09cf150f5876e98bb971ad2215e8496
migrations/0006_weekly_lab.sql                L1–481   ec54eb1d32f77b452c0cdbd54f3a3126cf5424069a57fdb5e5b08228f6f367a9
migrations/0009_financial_data.sql            L1–90    7afabbba23848165ce5fbb39aec8dbf9ef94e033177f8bd211f330567c4a5162
crates/trady-storage/src/intelligence.rs      L1–314   3f51ef4c5e217227e8daef8c8453f18d1b9e0f0fcc0fdf6a9993146914e1968c
crates/trady-storage/src/financial.rs         L1–413   88c6462c80712d19fda4823ec174d20c7cca8adac9d8c49085d0e1e39482ab1a
crates/trady-storage/src/weekly.rs            L1–593   f03ec9fa711cb3943de61e43cb47deb872b5e373a690807c4f255fe8c83407cf
crates/trady-storage/tests/intelligence.rs    L1–1032  56b00e021e265e9a0db28e663e2c6e518d537b3c3c1d3a9cb1489c7f3eab0f59
crates/trady-storage/tests/financial.rs       L1–352   36ee5a2d0eeb9efdcd0c42c4a3a9514fd65ba24d7067b741cee87139d2c2158a
crates/trady-storage/tests/weekly_durable.rs  L1–769   e7fc41d4c798cf09672deadcf160ddc7886c6b5a80db80d8e9cdadd05faa423d
crates/trady-storage/README.md                L1–150   d9a72558ac9c2f634e3ec61c5d3ee2ea2616a78b15ddcbfef0cfd6469ce11305
crates/trady-storage/src/README.md            L1–444   727daf761ab1cb87609c8c6f2fb68aace1a7e46a6d19fbbf2693bc93c1048d32
migrations/README.md                         L1–383   f15798b903869ed2e15f6b5759f22e23631ad2c4bf9248097986201c5958a380
crates/trady-storage/src/lib.rs               L1–242   37252540ad29317dafeb22122855bb29df945fa95c607790639b830187e4cbe5
```

Supplied responses, persona, integration, standard and applied context-mode guidance:

```text
/tmp/trady-remediation-20261031/storage/handoff.md
  L1–123  188b9b7adf431b36f58252d12daca7bd4ceab4c22658ef25ca895cab4ddaa062
/tmp/trady-remediation-20261031/storage-review/goldfish-verification.md
  L1–121  dc3b85e238151ceeb55bc4ffb52f985f5d3a1cc587a819627f040ab028a2473c
/tmp/trady-remediation-20261031/storage-fixes-r2/handoff.md
  L1–164  84ca134cb6755ba92fdb0a72656978095d6a72e9c8616ae1b6f52992e51667e0
/root/.codex/plugins/cache/egm/egm/0.1.0/agents/codebase_details.md
  L1–69   d80db07e449b57b3b3776d1401cca72a54b6b65a0ab218b86f9064ee4b8167bc
/root/.codex/plugins/cache/egm/egm/0.1.0/docs/codex.md
  L1–117  a7390390c233ece29a2ee7508a8d328b6b4ee200f649417c097687c76f014ad0
/root/.codex/plugins/cache/egm/egm/0.1.0/skills/egm-bootstrap/references/templates.md
  L1–68   f49793727cccbd72afdc4a0569a1d8e8138e943d2fc9b0df772b09deaca80437
/root/.codex/plugins/cache/context-mode/context-mode/1.0.169/skills/context-mode/SKILL.md
  L1–300  ce26cb01de7a116eab256f916620935f029e850771f3fb6cb7d40a3ce8d1f7ff
```

Supplemental semantic reads; focused ranges carry no whole-file semantic-review claim:

```text
migrations/0000_roles.sql
  L1–8  813dd40f552eb2b65875c80d5e75b313edb2f4ec927715f6099217945adfb4e4
migrations/0000_lab_roles.sql
  L1–7  964c8ba7a54476051b5803232b9991123f780a3385db685f899c3920b24f2583
migrations/0001_foundation.sql
  L1–15,236–253  a1d61df5e1436c7dbe51edec4fad8bc8d891d4c557443d47c280666abbd98093
crates/trady-domain/src/intelligence/mod.rs
  L1–304  1c4bc9f9c3881a03053cc3a57eeea69ff11dc6ba30c95a86be136afce5fe8b9d
crates/trady-application/src/intelligence.rs
  L1–599,635–657,790–899  704e63170c5d501fafc8a80fd7abea4201d351ae6f00f213be1b7a235b491934
crates/trady-adapters/tests/intelligence_support/mod.rs
  L1–190  ac01ce7640ba134ee767d1d3c7a77a1f2e0b4af3211b909054c7e4b7a0cc3c09
scripts/test-postgres.py
  L1–112  75e654db9a2ebae574a43c547fdef84cdaaf16d18c5be61231b0edb8249a4edc
crates/trady-application/src/research_jobs.rs
  L1–24 (digest)  87538dafcf52d25b5e1cb23ab2804dc3794cdceb20db05cff0010e4163e7898a
crates/trady-application/src/harness.rs
  L20–25 (content_hash)  e6d9b3c541457380ed7b678ced43a7007de1f7b05b58e1a71dbe9f85ba4d39e7
crates/trady-domain/src/financial.rs
  L94–123,301–312  bc1021bf443b56cf7ca891a0a827bd62f9be1d354bedb677482f111204717c46
crates/trady-domain/src/intelligence/options.rs
  L103–123 (OptionsInput)  afb10e36a709c66db0b1b5f3068bf4f9e5995a3fdc444c1b2fa785aa5433e623
crates/trady-domain/src/intelligence/coordination.rs
  L96–111 (CoordinationInput)  ac5128f416790b9fe01fca39888d6b07aacaeeb9dba06c796f35e471365012c4
crates/trady-domain/src/intelligence/event.rs
  L121–140 (EventInput)  34510ccc6d336f5819c84f8fbc9694f4fa5a456c1097176d841ef2b9b20839a8
crates/trady-domain/src/intelligence/short_interest.rs
  L107–120 (ShortInterestInput)  0fe8f6262e0d089ca8d55f4c5a1de842448fe85f210be00952c5dfa11f441b3b
crates/trady-domain/src/intelligence/insider.rs
  L188–199 (InsiderInput)  c1ad573e73d795a399c337af6858fcdb5b9daf6a6abbe3e8f3d7bf5716f6a410
crates/trady-domain/src/intelligence/cross_asset.rs
  L137–145 (CrossAssetInput)  b0c6c47645af947e9aaae50ef83ec1ddf8833feceb0fe3f71deee9c6655b477b
crates/trady-domain/src/intelligence/attention.rs
  L121–127 (AttentionInput)  431cdfe4b6bad51733744c8faa499c1760a5536f1139899331ef5f7b687d0e9a
crates/trady-domain/src/intelligence/accounting.rs
  L137–151 (AccountingInput)  728069a4fb2ccec032bdc13e01fbc6337892ee0c06dc46d7fbbedf8b1ae34e99
crates/trady-domain/src/intelligence/microstructure.rs
  L102–118 (MicrostructureInput)  33baf40768cc14d1b7e59db4522983e66fb0de0d3f1f75d990181d2166692ea8
```

Additional reference-only evidence: complete file bytes were searched for citation declarations/statements, with the matching line ranges listed here. These modules/migrations were not otherwise reviewed in full. The same declaration checks also covered already-listed files without adding a semantic-review claim for their unread ranges.

```text
crates/trady-storage/src/autonomous.rs
  declaration L10,15,37,225,260
  7a16ff3e83c13bbc27a509680e13ab501c62d4318f19f2dff7059f2a69b38967
crates/trady-storage/src/holding_research.rs
  declaration L10,169
  4334e7b174be2fdee20f17158c5099daddd52f21535375d1ed286d025ee73cde
crates/trady-storage/src/research.rs
  declaration L11,17,64,182,205,237,284,301,482
  0b748e24aac0e3cd3c28ab7e09be8d5de8733b2173ae511801b840f875c6728a
crates/trady-storage/src/research_jobs.rs
  declaration L22,26,111,370,394,495,548,860,1096,1142
  3b244eba649f75d23fb75aaef9719dfee079e0a712817f39178025d8a3cef6f1
crates/trady-storage/src/sessions.rs
  declaration L8,11,14,24,34,59,82,102
  7f34855bd48e28c23c7f5f74251c2caadf17f626c2c5e934bdef5976865e0e7f
crates/trady-storage/src/trading_research.rs
  declaration L70,76
  0161658f318c4f9b9715667f2e4d305814d6aebc48e7a90c25cb810b3bfbab12
crates/trady-storage/src/transport.rs
  declaration L8,40
  8e470fdc5baead7eb2d0977c7da7701f0dd13d079e85fa80b74bfc5ce90baebd
crates/trady-storage/src/x_monitoring.rs
  declaration L10,16,59,96,129,197
  ff91b8f746d3ba00d451c9e30494671a325cc7551494f341715b4a627d140d4e
migrations/0002_model_sessions.sql
  declaration L2,23
  78e35d0641567bc031841450325fb260fd7c78b72944d502427855e758c5f738
migrations/0003_research_sources.sql
  declaration/statement L2,20–22,48,60–65
  07a33e3693c2a400d873995a5dcf3434016ad76ec54891250bf7c9f5c6dda44c
migrations/0004_autonomous_execution.sql
  declaration L12,68,133–134,145,172–173
  befeab90b72d1246b83f7deadcadcb05fb1882507f8d40793f1a16b1add16d0a
migrations/0005_x_api_monitoring.sql
  declaration/statement L46,56,59–62
  d2e2abd39c951331e32be3bd3b7684c84fab7c6f5980bae282f9bd8561d0ce9b
migrations/0007_research_jobs.sql
  declaration/statement L2–3,16,120,202,252–253,257,304–305
  f8572389ae7d0653ec03e31ad0f62e4c050c17d001d8e32af855995ae6dcb441
migrations/0008_verified_holdings.sql
  declaration/statement L190,206,288–289
  e32b5f92129609f8782ad5843237034ebd33598e136b10aac6b629e59fd677b2
```

No supplied mandatory input remains inaccessible or unread. Runtime behavior and final release evidence remain for the parent to establish on the final integrated bytes.

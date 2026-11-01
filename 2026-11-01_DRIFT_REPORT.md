# Cold-start account contract drift report — 2026-11-01

Scope: the 53 EGM contract READMEs under the repository root, `crates/`, `lab/`, and `services/`, checked against the current working tree after the cold-start contract update. Only implementation differences are listed.

## ./crates/README.md

No drift detected.

## ./crates/trady-adapters/README.md

No drift detected.

## ./crates/trady-adapters/examples/README.md

No drift detected.

## ./crates/trady-adapters/src/README.md

No drift detected.

## ./crates/trady-adapters/src/autonomous/README.md

No drift detected.

## ./crates/trady-adapters/src/financial/README.md

No drift detected.

## ./crates/trady-adapters/src/finbert/README.md

No drift detected.

## ./crates/trady-adapters/src/inputs_service/README.md

No drift detected.

## ./crates/trady-adapters/src/intelligence/README.md

No drift detected.

## ./crates/trady-adapters/src/news/README.md

No drift detected.

## ./crates/trady-adapters/src/opencode/README.md

No drift detected.

## ./crates/trady-adapters/src/perpetual/README.md

No drift detected.

## ./crates/trady-adapters/src/research_jobs/README.md

No drift detected.

## ./crates/trady-adapters/src/screening/README.md

No drift detected.

## ./crates/trady-adapters/src/telegram/README.md

No drift detected.

## ./crates/trady-adapters/src/telegram/spool/README.md

No drift detected.

## ./crates/trady-adapters/src/trading212/README.md

No drift detected.

## ./crates/trady-adapters/src/weekly_lab/README.md

No drift detected.

## ./crates/trady-application/README.md

No drift detected.

## ./crates/trady-application/src/README.md

No drift detected.

## ./crates/trady-application/src/autonomous/README.md

No drift detected.

## ./crates/trady-application/src/model_lab/README.md

No drift detected.

## ./crates/trady-application/src/optimization/README.md

No drift detected.

## ./crates/trady-application/src/sentiment/README.md

No drift detected.

## ./crates/trady-application/src/telegram/README.md

No drift detected.

## ./crates/trady-cli/README.md

No drift detected.

## ./crates/trady-cli/src/README.md

No drift detected.

## ./crates/trady-cli/src/finbert/README.md

No drift detected.

## ./crates/trady-cli/src/opencode/README.md

No drift detected.

## ./crates/trady-cli/src/telegram/README.md

No drift detected.

## ./crates/trady-domain/README.md

No drift detected.

## ./crates/trady-domain/src/README.md

No drift detected.

## ./crates/trady-domain/src/intelligence/README.md

No drift detected.

## ./crates/trady-domain/src/sentiment/README.md

No drift detected.

## ./crates/trady-policy/README.md

No drift detected.

## ./crates/trady-policy/src/README.md

No drift detected.

## ./crates/trady-policy/src/intelligence/README.md

No drift detected.

## ./crates/trady-policy/src/limits/README.md

No drift detected.

## ./crates/trady-policy/src/prediction/README.md

No drift detected.

## ./crates/trady-policy/src/sentiment/README.md

No drift detected.

## ./crates/trady-storage/README.md

No drift detected.

## ./crates/trady-storage/src/README.md

No drift detected.

## ./lab/README.md

No drift detected.

## ./lab/evaluator/README.md

No drift detected.

## ./lab/lean/README.md

No drift detected.

## ./lab/proofs/README.md

No drift detected.

## ./lab/proofs/TradyProof/README.md

No drift detected.

## ./lab/training/README.md

No drift detected.

## ./services/README.md

No drift detected.

## ./services/finbert/README.md

No drift detected.

## ./services/inputs/README.md

No drift detected.

## ./services/runtime/README.md

No drift detected.

## README.md

- The cold-start command is implemented at [`crates/trady-cli/src/account.rs:run`](./crates/trady-cli/src/account.rs), but the current demo run cannot resolve a broker instrument universe because Trading 212's metadata response is invalid. The initializer correctly records an empty bounded candidate pool and refuses to invent identifiers.
- The first-order command records an immutable intent but does not yet submit or reconcile a genuine order. The current Trading 212 transport has no verified attached-protection mutation, so `--approve` remains fail-closed until protection and the reviewed input service are independently available.
- The runtime review request is bound to the artifact hashes and payloads. OpenCode Go produced a successful `REQUEST_CHANGES` review in the first run; later retries can be unavailable or timed out and are retained as unavailable rather than treated as approval.
- The five deployment inputs remain pending by design: the seed market model, execution-cost model, trading-decision model and formula are shadow-only; the dynamic universe has no broker candidates; quote/order/protection/cancellation/partial-fill/fee/FX capabilities are unknown; and no prospective execution sample exists.

## docs/cold-start.md

No drift detected.

## docs/cold-start-run-2026-11-01.md

No drift detected.

## config/cold-start.demo.json

No drift detected.

# EGM contract bootstrap drift recheck — 2026-11-01

Scope: the README contract changed by the approved full-tree bootstrap, checked
against the current working tree. Only implementation differences are listed.

## crates/trady-cli/README.md

- The Invariants section says database-backed autonomous paths require migrations
  **1–9** (line 76 in [the CLI contract](./crates/trady-cli/README.md)). The current storage
  migration runner applies versions **1–11**, including optional intelligence
  and fail-closed storage ([`trady-storage/src/lib.rs:migrate`](./crates/trady-storage/src/lib.rs)); the CLI intelligence help also states
  that database commands use migrations through 11
  ([`intelligence.rs:HELP`](./crates/trady-cli/src/intelligence.rs)). The
  package contract should describe the current migration range so operators do
  not stop at an incomplete schema.

# EGM contract correction recheck — 2026-11-01

Scope: the migration-range corrections made during the follow-up bootstrap.

## crates/trady-cli/README.md

No drift detected.

## crates/trady-cli/src/README.md

No drift detected.



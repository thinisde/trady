# Trady tracked-file decision matrix

Generated from `git ls-files` at commit `64fe00afad88628ac63ef719ea841f9917e4104f`; rows: **567**. The audit agent read every tracked file. Status is a per-file decision; rationale is inherited from the package audit and is intentionally conservative.

| Path | Type | Lines | Status | Rationale / next action |
|---|---|---:|---|---|
| `.agents/skills/trady-weekly-optimization/SKILL.md` | `md` | 126 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `.gitignore` | `no extension` | 41 | **REFACTOR** | Update repository scope, links, and generated-state ignores. |
| `Cargo.lock` | `lock` | 3439 | **RETAIN / REVIEW** | Review against target authority before changing or deleting. |
| `Cargo.toml` | `toml` | 43 | **RETAIN / REVIEW** | Review against target authority before changing or deleting. |
| `LICENSE` | `no extension` | 675 | **RETAIN / REVIEW** | Review against target authority before changing or deleting. |
| `README.md` | `md` | 71 | **REFACTOR** | Update repository scope, links, and generated-state ignores. |
| `comprehension_report.md` | `md` | 116 | **DEPRECATE / ARCHIVE** | Historical review artifact; not a runtime input. |
| `config/README.md` | `md` | 113 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/alpaca-iex30.paper.example.json` | `json` | 12 | **DEPRECATE / ARCHIVE** | IEX30/Alpaca authority conflicts with EUR/yfinance target; keep fixtures until migration. |
| `config/autonomous.demo-no-submit.json` | `json` | 155 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/autonomous.example.json` | `json` | 91 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/autonomous.iex30.demo.json` | `json` | 374 | **DEPRECATE / ARCHIVE** | Conflicts with target EUR/yfinance authority; preserve migration fixtures temporarily. |
| `config/cold-start.demo.json` | `json` | 88 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/cold-start.iex30.demo.json` | `json` | 143 | **DEPRECATE / ARCHIVE** | Conflicts with target EUR/yfinance authority; preserve migration fixtures temporarily. |
| `config/credentials.example.json` | `json` | 41 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/credentials.local.json.rotation.lock` | `lock` | 1 | **RETAIN / CHECK** | Generated credential state; remove if unused and fix ignore pattern. |
| `config/credentials/README.md` | `md` | 88 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/credentials/alpha-vantage/README.md` | `md` | 20 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/credentials/financial-data/README.md` | `md` | 56 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/credentials/local-secrets/README.md` | `md` | 38 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/credentials/opencode/README.md` | `md` | 27 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/credentials/postgresql/README.md` | `md` | 171 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/credentials/telegram/README.md` | `md` | 50 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/credentials/trading-inputs/README.md` | `md` | 40 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/credentials/trading212/README.md` | `md` | 55 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/credentials/x/README.md` | `md` | 56 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/dev-pg_hba.conf` | `conf` | 9 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/financial-data.json` | `json` | 84 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/finbert/README.md` | `md` | 34 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `config/finbert/config.example.json` | `json` | 8 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `config/formulas/long-donchian-atr-v1.authority.json` | `json` | 49 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/formulas/long-donchian-atr-v1.integrity.json` | `json` | 21 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/formulas/long-donchian-atr-v1.json` | `json` | 33 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/iex30.universe.json` | `json` | 14 | **DEPRECATE / ARCHIVE** | Conflicts with target EUR/yfinance authority; preserve migration fixtures temporarily. |
| `config/intelligence.example.json` | `json` | 20 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/opencode/README.md` | `md` | 38 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/opencode/llm.json` | `json` | 22 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/paper.json` | `json` | 27 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/research.json` | `json` | 283 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/skills.demo.approved.json` | `json` | 60 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/skills.demo.candidate.json` | `json` | 60 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/telegram.example.json` | `json` | 15 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/trading212.demo.example.json` | `json` | 9 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/trading212.iex30.demo.example.json` | `json` | 9 | **DEPRECATE / ARCHIVE** | Conflicts with target EUR/yfinance authority; preserve migration fixtures temporarily. |
| `config/trading212.live.example.json` | `json` | 9 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/watchdog.iex30.json` | `json` | 16 | **DEPRECATE / ARCHIVE** | Conflicts with target EUR/yfinance authority; preserve migration fixtures temporarily. |
| `config/x-pilot.example.json` | `json` | 133 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `config/yfinance.euronext.example.json` | `json` | 14 | **RETAIN / REFACTOR** | Configuration example or policy; bind it to EUR/Trading212 target and keep secrets out. |
| `consolidated_review_report.md` | `md` | 152 | **DEPRECATE / ARCHIVE** | Historical review artifact; not a runtime input. |
| `crates/README.md` | `md` | 100 | **RETAIN / REVIEW** | Review against target authority before changing or deleting. |
| `crates/trady-adapters/Cargo.toml` | `toml` | 27 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/README.md` | `md` | 173 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/examples/README.md` | `md` | 108 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/examples/train_candidate.rs` | `rs` | 692 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/README.md` | `md` | 235 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/alpaca.rs` | `rs` | 984 | **DEPRECATE / ARCHIVE** | IEX30/Alpaca authority conflicts with EUR/yfinance target; keep fixtures until migration. |
| `crates/trady-adapters/src/api_health.rs` | `rs` | 662 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/autonomous/README.md` | `md` | 129 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/autonomous/market.rs` | `rs` | 359 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/autonomous/mod.rs` | `rs` | 676 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/credentials.rs` | `rs` | 287 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/financial/README.md` | `md` | 80 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `crates/trady-adapters/src/financial/mod.rs` | `rs` | 277 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `crates/trady-adapters/src/financial/parse.rs` | `rs` | 589 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `crates/trady-adapters/src/financial/tests.rs` | `rs` | 396 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `crates/trady-adapters/src/finbert/README.md` | `md` | 51 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `crates/trady-adapters/src/finbert/mod.rs` | `rs` | 131 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `crates/trady-adapters/src/inputs_service/README.md` | `md` | 140 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/inputs_service/calendar.rs` | `rs` | 108 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/inputs_service/http.rs` | `rs` | 361 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/inputs_service/mod.rs` | `rs` | 505 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/inputs_service/store.rs` | `rs` | 441 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/inputs_service/tests.rs` | `rs` | 1234 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/inputs_service/validate.rs` | `rs` | 566 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/intelligence/README.md` | `md` | 39 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/intelligence/book.rs` | `rs` | 333 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/intelligence/mod.rs` | `rs` | 290 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/intelligence/ownership.rs` | `rs` | 1165 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/intelligence/tools.rs` | `rs` | 185 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/lib.rs` | `rs` | 510 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/news/README.md` | `md` | 170 | **RETAIN / REWIRE** | Reuse provenance/budget collectors; add yfinance/RSS/browser/user URL handoff. |
| `crates/trady-adapters/src/news/alpha.rs` | `rs` | 175 | **RETAIN / REWIRE** | Reuse provenance/budget collectors; add yfinance/RSS/browser/user URL handoff. |
| `crates/trady-adapters/src/news/financial_news.rs` | `rs` | 199 | **RETAIN / REWIRE** | Reuse provenance/budget collectors; add yfinance/RSS/browser/user URL handoff. |
| `crates/trady-adapters/src/news/http.rs` | `rs` | 307 | **RETAIN / REWIRE** | Reuse provenance/budget collectors; add yfinance/RSS/browser/user URL handoff. |
| `crates/trady-adapters/src/news/mod.rs` | `rs` | 409 | **RETAIN / REWIRE** | Reuse provenance/budget collectors; add yfinance/RSS/browser/user URL handoff. |
| `crates/trady-adapters/src/news/rss.rs` | `rs` | 491 | **RETAIN / REWIRE** | Reuse provenance/budget collectors; add yfinance/RSS/browser/user URL handoff. |
| `crates/trady-adapters/src/news/tests.rs` | `rs` | 907 | **RETAIN / REWIRE** | Reuse provenance/budget collectors; add yfinance/RSS/browser/user URL handoff. |
| `crates/trady-adapters/src/news/x.rs` | `rs` | 653 | **RETAIN / REWIRE** | Reuse provenance/budget collectors; add yfinance/RSS/browser/user URL handoff. |
| `crates/trady-adapters/src/news/x_meter.rs` | `rs` | 429 | **RETAIN / REWIRE** | Reuse provenance/budget collectors; add yfinance/RSS/browser/user URL handoff. |
| `crates/trady-adapters/src/opencode/README.md` | `md` | 176 | **RETAIN / MORE WORK** | Reuse pinned OpenCode transport; wire autonomous selection/holding contracts. |
| `crates/trady-adapters/src/opencode/llm.rs` | `rs` | 513 | **RETAIN / MORE WORK** | Reuse pinned OpenCode transport; wire autonomous selection/holding contracts. |
| `crates/trady-adapters/src/opencode/mod.rs` | `rs` | 5 | **RETAIN / MORE WORK** | Reuse pinned OpenCode transport; wire autonomous selection/holding contracts. |
| `crates/trady-adapters/src/opencode/runtime_tools.rs` | `rs` | 448 | **RETAIN / MORE WORK** | Reuse pinned OpenCode transport; wire autonomous selection/holding contracts. |
| `crates/trady-adapters/src/perpetual/README.md` | `md` | 58 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/perpetual/holding_economics_tests.rs` | `rs` | 589 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/perpetual/mod.rs` | `rs` | 323 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/perpetual/tests.rs` | `rs` | 637 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/research_jobs/README.md` | `md` | 144 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/research_jobs/mod.rs` | `rs` | 309 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/screening/README.md` | `md` | 127 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/screening/mod.rs` | `rs` | 362 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/screening/tests.rs` | `rs` | 320 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/screening/universe.example.json` | `json` | 18 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/sessions.rs` | `rs` | 154 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/shadow.rs` | `rs` | 233 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/telegram/README.md` | `md` | 91 | **RETAIN / REWIRE** | Keep authenticated outbox; connect rich lifecycle notifications. |
| `crates/trady-adapters/src/telegram/mod.rs` | `rs` | 333 | **RETAIN / REWIRE** | Keep authenticated outbox; connect rich lifecycle notifications. |
| `crates/trady-adapters/src/telegram/spool.rs` | `rs` | 243 | **RETAIN / REWIRE** | Keep authenticated outbox; connect rich lifecycle notifications. |
| `crates/trady-adapters/src/telegram/spool/README.md` | `md` | 44 | **RETAIN / REWIRE** | Keep authenticated outbox; connect rich lifecycle notifications. |
| `crates/trady-adapters/src/telegram/spool/tests.rs` | `rs` | 117 | **RETAIN / REWIRE** | Keep authenticated outbox; connect rich lifecycle notifications. |
| `crates/trady-adapters/src/telegram/tests.rs` | `rs` | 214 | **RETAIN / REWIRE** | Keep authenticated outbox; connect rich lifecycle notifications. |
| `crates/trady-adapters/src/trading212/README.md` | `md` | 180 | **RETAIN / MORE WORK** | Only broker path; remove hard-coded IEX guard and add EUR crosswalk/protection recovery. |
| `crates/trady-adapters/src/trading212/autonomous.rs` | `rs` | 263 | **RETAIN / MORE WORK** | Only broker path; remove hard-coded IEX guard and add EUR crosswalk/protection recovery. |
| `crates/trady-adapters/src/trading212/autonomous_tests.rs` | `rs` | 406 | **RETAIN / MORE WORK** | Only broker path; remove hard-coded IEX guard and add EUR crosswalk/protection recovery. |
| `crates/trady-adapters/src/trading212/dto.rs` | `rs` | 472 | **RETAIN / MORE WORK** | Only broker path; remove hard-coded IEX guard and add EUR crosswalk/protection recovery. |
| `crates/trady-adapters/src/trading212/http.rs` | `rs` | 227 | **RETAIN / MORE WORK** | Only broker path; remove hard-coded IEX guard and add EUR crosswalk/protection recovery. |
| `crates/trady-adapters/src/trading212/mod.rs` | `rs` | 712 | **RETAIN / MORE WORK** | Only broker path; remove hard-coded IEX guard and add EUR crosswalk/protection recovery. |
| `crates/trady-adapters/src/trading212/tests.rs` | `rs` | 830 | **RETAIN / MORE WORK** | Only broker path; remove hard-coded IEX guard and add EUR crosswalk/protection recovery. |
| `crates/trady-adapters/src/weekly_lab/README.md` | `md` | 94 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/weekly_lab/mod.rs` | `rs` | 238 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/weekly_lab/runner.rs` | `rs` | 395 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/src/yfinance.rs` | `rs` | 467 | **RETAIN / MORE WORK** | Reusable EUR quote adapter; wire polling/history/venue evidence. |
| `crates/trady-adapters/tests/credentials.rs` | `rs` | 134 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/tests/finbert/main.rs` | `rs` | 190 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `crates/trady-adapters/tests/harness.rs` | `rs` | 784 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/tests/intelligence.rs` | `rs` | 1126 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/tests/intelligence_support/mod.rs` | `rs` | 191 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/tests/research_jobs.rs` | `rs` | 144 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/tests/shadow_recording.rs` | `rs` | 100 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-adapters/tests/weekly_helpers.rs` | `rs` | 251 | **RETAIN / REFACTOR** | Adapter foundation; verify active authority and simplify optional health checks. |
| `crates/trady-application/Cargo.toml` | `toml` | 21 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/README.md` | `md` | 139 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/README.md` | `md` | 433 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/autonomous/README.md` | `md` | 202 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/autonomous/mod.rs` | `rs` | 1412 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/autonomous/readiness.rs` | `rs` | 553 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/autonomous/updates.rs` | `rs` | 234 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/broker.rs` | `rs` | 316 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/contracts.rs` | `rs` | 420 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/financial.rs` | `rs` | 664 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/harness.rs` | `rs` | 1357 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/intelligence.rs` | `rs` | 900 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/lib.rs` | `rs` | 413 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/model_lab/README.md` | `md` | 120 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/model_lab/fit.rs` | `rs` | 551 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/model_lab/inference.rs` | `rs` | 351 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/model_lab/journal.rs` | `rs` | 201 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/model_lab/mod.rs` | `rs` | 496 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/model_lab/tests.rs` | `rs` | 548 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/optimization/README.md` | `md` | 146 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/optimization/integrity.rs` | `rs` | 226 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/optimization/mod.rs` | `rs` | 303 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/optimization/weekly.rs` | `rs` | 800 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/perpetual.rs` | `rs` | 232 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/research.rs` | `rs` | 620 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/research_jobs.rs` | `rs` | 663 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/screening.rs` | `rs` | 159 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/sentiment/README.md` | `md` | 55 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/sentiment/mod.rs` | `rs` | 134 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/stock_report.rs` | `rs` | 359 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/telegram/README.md` | `md` | 103 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/telegram/gateway.rs` | `rs` | 214 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/telegram/mod.rs` | `rs` | 391 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/telegram/tests.rs` | `rs` | 277 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/watchdog.rs` | `rs` | 188 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/src/x_monitoring.rs` | `rs` | 433 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/tests/autonomous.rs` | `rs` | 1922 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/tests/formula_authority.rs` | `rs` | 229 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/tests/formula_integrity.rs` | `rs` | 123 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/tests/optimization.rs` | `rs` | 493 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/tests/research.rs` | `rs` | 306 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/tests/research_jobs.rs` | `rs` | 497 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/tests/screening.rs` | `rs` | 279 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/tests/sentiment/main.rs` | `rs` | 191 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-application/tests/x_monitoring.rs` | `rs` | 347 | **REFACTOR / MORE WORK** | Rewire EUR authority, LLM selection, coordinator, dynamic exits, reports. |
| `crates/trady-cli/Cargo.toml` | `toml` | 32 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/README.md` | `md` | 129 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/README.md` | `md` | 387 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/account.rs` | `rs` | 2821 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/alpaca.rs` | `rs` | 82 | **DEPRECATE / ARCHIVE** | IEX30/Alpaca authority conflicts with EUR/yfinance target; keep fixtures until migration. |
| `crates/trady-cli/src/api_health.rs` | `rs` | 33 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/autonomous.rs` | `rs` | 947 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/autonomous_shadow.rs` | `rs` | 268 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/autonomous_worker.rs` | `rs` | 993 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/broker_read.rs` | `rs` | 181 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/entry_demo.rs` | `rs` | 129 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/financial.rs` | `rs` | 245 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/finbert/README.md` | `md` | 49 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `crates/trady-cli/src/finbert/mod.rs` | `rs` | 71 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `crates/trady-cli/src/inputs_service.rs` | `rs` | 151 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/intelligence.rs` | `rs` | 269 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/main.rs` | `rs` | 315 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/model_lab.rs` | `rs` | 136 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/news.rs` | `rs` | 408 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/opencode/README.md` | `md` | 132 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/opencode/mod.rs` | `rs` | 339 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/release.rs` | `rs` | 182 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/stock_report.rs` | `rs` | 42 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/telegram/README.md` | `md` | 148 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/telegram/gateway.example.json` | `json` | 28 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/telegram/gateway.rs` | `rs` | 737 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/telegram/mod.rs` | `rs` | 422 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/telegram/tests.rs` | `rs` | 138 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/watchdog.rs` | `rs` | 31 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/weekly.rs` | `rs` | 263 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/src/yfinance.rs` | `rs` | 41 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/autonomous_cli.rs` | `rs` | 251 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/cli.rs` | `rs` | 70 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/credentials_cli.rs` | `rs` | 393 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/entry_demo_cli.rs` | `rs` | 146 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/financial_cli.rs` | `rs` | 84 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/fixtures/news.rss` | `rss` | 8 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/fixtures/research.redacted.json` | `json` | 28 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/inputs_model_pipeline.rs` | `rs` | 653 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/intelligence.rs` | `rs` | 201 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/news.rs` | `rs` | 143 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/opencode.rs` | `rs` | 177 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/telegram_cli.rs` | `rs` | 105 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/telegram_jobs_cli.rs` | `rs` | 208 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/trading212_cli.rs` | `rs` | 71 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-cli/tests/weekly_cli.rs` | `rs` | 82 | **REFACTOR** | Replace IEX-focused commands; centralize order gate; add selection/report commands. |
| `crates/trady-domain/Cargo.toml` | `toml` | 18 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/README.md` | `md` | 79 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/README.md` | `md` | 317 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/financial.rs` | `rs` | 347 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/iex.rs` | `rs` | 88 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/README.md` | `md` | 153 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/accounting.rs` | `rs` | 193 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/attention.rs` | `rs` | 219 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/coordination.rs` | `rs` | 165 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/cross_asset.rs` | `rs` | 250 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/event.rs` | `rs` | 183 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/insider.rs` | `rs` | 252 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/microstructure.rs` | `rs` | 197 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/mod.rs` | `rs` | 305 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/options.rs` | `rs` | 216 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/short_interest.rs` | `rs` | 193 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/intelligence/tax.rs` | `rs` | 356 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/lib.rs` | `rs` | 432 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/perpetual.rs` | `rs` | 782 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/prediction.rs` | `rs` | 374 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/research.rs` | `rs` | 202 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/screening.rs` | `rs` | 848 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/sentiment/README.md` | `md` | 43 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/src/sentiment/mod.rs` | `rs` | 67 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/tests/research.rs` | `rs` | 59 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-domain/tests/screening.rs` | `rs` | 598 | **RETAIN / REFACTOR** | Typed domain; extend EUR identity, quote, session, selection types. |
| `crates/trady-policy/Cargo.toml` | `toml` | 19 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/README.md` | `md` | 84 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/examples/donchian_bridge.rs` | `rs` | 82 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/README.md` | `md` | 259 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/capital.rs` | `rs` | 59 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/economics.rs` | `rs` | 335 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/holding.rs` | `rs` | 573 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/README.md` | `md` | 135 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/accounting.rs` | `rs` | 953 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/attention.rs` | `rs` | 691 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/coordination.rs` | `rs` | 766 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/cross_asset.rs` | `rs` | 1056 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/event.rs` | `rs` | 812 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/insider.rs` | `rs` | 613 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/microstructure.rs` | `rs` | 668 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/mod.rs` | `rs` | 40 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/options.rs` | `rs` | 1135 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/short_interest.rs` | `rs` | 858 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/intelligence/tax.rs` | `rs` | 1583 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/lib.rs` | `rs` | 794 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/limits/README.md` | `md` | 113 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/limits/donchian.rs` | `rs` | 497 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/limits/mod.rs` | `rs` | 532 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/limits/proof_bridge.rs` | `rs` | 123 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/prediction/README.md` | `md` | 110 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/prediction/history.rs` | `rs` | 145 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/prediction/mod.rs` | `rs` | 561 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/sentiment/README.md` | `md` | 69 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/src/sentiment/mod.rs` | `rs` | 104 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/tests/donchian.rs` | `rs` | 164 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/tests/financial.rs` | `rs` | 1381 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/tests/limits.rs` | `rs` | 307 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/tests/prediction.rs` | `rs` | 434 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-policy/tests/sentiment/main.rs` | `rs` | 169 | **RETAIN / REFACTOR** | Keep deterministic policy; replace IEX30 and multi-session defaults. |
| `crates/trady-storage/Cargo.toml` | `toml` | 25 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/README.md` | `md` | 157 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/README.md` | `md` | 511 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/autonomous.rs` | `rs` | 539 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/financial.rs` | `rs` | 414 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/holding_research.rs` | `rs` | 216 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/intelligence.rs` | `rs` | 315 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/lib.rs` | `rs` | 251 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/research.rs` | `rs` | 505 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/research_jobs.rs` | `rs` | 1448 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/sessions.rs` | `rs` | 107 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/trading_research.rs` | `rs` | 321 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/transport.rs` | `rs` | 93 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/weekly.rs` | `rs` | 603 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/src/x_monitoring.rs` | `rs` | 260 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/autonomous.rs` | `rs` | 151 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/financial.rs` | `rs` | 353 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/first_order.rs` | `rs` | 156 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/holding_economics.rs` | `rs` | 325 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/intelligence.rs` | `rs` | 1033 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/postgres.rs` | `rs` | 444 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/research.rs` | `rs` | 469 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/research_jobs.rs` | `rs` | 425 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/sessions.rs` | `rs` | 155 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/trading_research.rs` | `rs` | 223 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/verified_holdings.rs` | `rs` | 927 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/weekly_durable.rs` | `rs` | 773 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `crates/trady-storage/tests/x_monitoring.rs` | `rs` | 297 | **RETAIN / MORE WORK** | Keep persistence; add quotes, crosswalk, selection, exits, Telegram, daily reports. |
| `critic_report.md` | `md` | 97 | **DEPRECATE / ARCHIVE** | Historical review artifact; not a runtime input. |
| `design_draft.md` | `md` | 479 | **DEPRECATE / ARCHIVE** | Historical review artifact; not a runtime input. |
| `docs/README.md` | `md` | 64 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/api-health.md` | `md` | 52 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/autonomous.md` | `md` | 274 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/cold-start.md` | `md` | 120 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/credentials.md` | `md` | 157 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/designs/trading212-same-day-sequential-protection.md` | `md` | 323 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/evidence/trading212-cost-policy-2026-11-03.json` | `json` | 59 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/evidence/trading212-demo-capability-2026-11-03.json` | `json` | 165 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/evidence/trading212-quote-compatibility-2026-11-03.json` | `json` | 48 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/financial-data.md` | `md` | 227 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/formulas/long-donchian-atr-v1.md` | `md` | 55 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/foundation.md` | `md` | 158 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/iex30-audit.md` | `md` | 70 | **DEPRECATE / ARCHIVE** | Historical IEX30 operator authority; replace active claims with EUR target docs. |
| `docs/iex30.md` | `md` | 111 | **DEPRECATE / ARCHIVE** | Historical IEX30 operator authority; replace active claims with EUR target docs. |
| `docs/independent-review-policy.md` | `md` | 28 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/news.md` | `md` | 261 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/opencode/README.md` | `md` | 170 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/stock-reports.md` | `md` | 51 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/telegram.md` | `md` | 216 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/trading212.md` | `md` | 166 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/README.md` | `md` | 33 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/architecture.md` | `md` | 245 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/company-intelligence.md` | `md` | 53 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/finbert/README.md` | `md` | 246 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `docs/v1/free-financial-api-catalog.md` | `md` | 394 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/free-financial-data.md` | `md` | 562 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/implementation.md` | `md` | 221 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/llm-and-tools.md` | `md` | 264 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/llm-contracts.md` | `md` | 493 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/memory-and-data.md` | `md` | 226 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/operations.md` | `md` | 238 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/perpetual.md` | `md` | 454 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/prediction-fusion.md` | `md` | 365 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/prediction-implementation.md` | `md` | 105 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/references.md` | `md` | 65 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/research-contracts.md` | `md` | 1432 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/research-sources.md` | `md` | 42 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/research.md` | `md` | 114 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/runtime-skills.md` | `md` | 77 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/sector-workflow.md` | `md` | 53 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/strategy.md` | `md` | 251 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/telegram.md` | `md` | 146 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/watchdogs-and-x.md` | `md` | 86 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/weekly-optimization.md` | `md` | 743 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `docs/v1/x-api-monitoring.md` | `md` | 345 | **RETAIN / RECONCILE** | Useful protocol/design/evidence docs; mark conflicts and link target authority. |
| `goldfish_review_report.md` | `md` | 181 | **DEPRECATE / ARCHIVE** | Historical review artifact; not a runtime input. |
| `implementation_readiness_report.md` | `md` | 97 | **DEPRECATE / ARCHIVE** | Historical review artifact; not a runtime input. |
| `index.md` | `md` | 99 | **REFACTOR** | Update repository scope, links, and generated-state ignores. |
| `intermediate_zoom_out_report.md` | `md` | 94 | **DEPRECATE / ARCHIVE** | Historical review artifact; not a runtime input. |
| `lab/README.md` | `md` | 104 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/evaluate.py` | `py` | 14 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/evaluator/README.md` | `md` | 483 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/evaluator/__init__.py` | `py` | 2 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/evaluator/common.py` | `py` | 160 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/evaluator/isolation.py` | `py` | 104 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/evaluator/observations.py` | `py` | 339 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/evaluator/predictions.py` | `py` | 359 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/evaluator/run.py` | `py` | 308 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/evaluator/statistics.py` | `py` | 129 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/evaluator/transport.py` | `py` | 119 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/evaluator_worker.py` | `py` | 24 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/lean/BridgeAlgorithm.py` | `py` | 98 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/lean/README.md` | `md` | 126 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/lean/bridge.py` | `py` | 135 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/lean/run.py` | `py` | 94 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/Audit.lean` | `lean` | 28 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/FormulaMain.lean` | `lean` | 49 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/IMPLEMENTATION.md` | `md` | 242 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/Main.lean` | `lean` | 23 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/README.md` | `md` | 207 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/TradyProof.lean` | `lean` | 7 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/TradyProof/CheckedDecimal.lean` | `lean` | 188 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/TradyProof/Formula.lean` | `lean` | 645 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/TradyProof/Holding.lean` | `lean` | 183 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/TradyProof/Parsing.lean` | `lean` | 225 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/TradyProof/README.md` | `md` | 106 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/TradyProof/Revision.lean` | `lean` | 68 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/TradyProof/Safety.lean` | `lean` | 35 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/TradyProof/Witness.lean` | `lean` | 63 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/bindings.py` | `py` | 137 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/check.py` | `py` | 119 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/fixtures.py` | `py` | 433 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/lakefile.lean` | `lean` | 12 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/lean-toolchain` | `no extension` | 2 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/parity.py` | `py` | 225 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/pin.py` | `py` | 37 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/self_test.py` | `py` | 170 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/proofs/test_inputs.py` | `py` | 166 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/reconcile.py` | `py` | 55 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/README.md` | `md` | 86 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/sealed_fixture.py` | `py` | 168 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/test_acceptance_boundaries.py` | `py` | 347 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/test_evaluator_counterexamples.py` | `py` | 284 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/test_evaluator_identity_scope.py` | `py` | 318 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/test_evaluator_lifecycle.py` | `py` | 540 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/test_evaluator_outcome_identity.py` | `py` | 460 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/test_evaluator_security.py` | `py` | 285 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/test_helpers.py` | `py` | 51 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/test_lean_bridge_errors.py` | `py` | 98 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/test_lean_integrity.py` | `py` | 102 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/test_lean_wire_adapter.py` | `py` | 40 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/tests/test_sealed_evaluator.py` | `py` | 384 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/training/README.md` | `md` | 100 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/training/pipeline.py` | `py` | 267 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `lab/training/test_pipeline.py` | `py` | 75 | **RETAIN / EXTEND** | Replay, proof, parity, and evaluation support; add EUR/intraday/dynamic-exit fixtures. |
| `long_donchian_atr_v1_coding_agent_plan.md` | `md` | 413 | **RETAIN / MERGE** | Formula evidence remains useful while Donchian/ATR stays approved. |
| `migrations/0000_lab_roles.sql` | `sql` | 8 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0000_roles.sql` | `sql` | 9 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0001_foundation.sql` | `sql` | 254 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0002_model_sessions.sql` | `sql` | 27 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0003_research_sources.sql` | `sql` | 66 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0004_autonomous_execution.sql` | `sql` | 174 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0005_x_api_monitoring.sql` | `sql` | 63 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0006_weekly_lab.sql` | `sql` | 482 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0007_research_jobs.sql` | `sql` | 324 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0008_verified_holdings.sql` | `sql` | 325 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0009_financial_data.sql` | `sql` | 91 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0010_optional_intelligence.sql` | `sql` | 79 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0011_fail_closed_storage.sql` | `sql` | 824 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0012_first_order_dispatch.sql` | `sql` | 343 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0013_daily_order_limits.sql` | `sql` | 51 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/0014_iex_watchdog.sql` | `sql` | 44 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `migrations/README.md` | `md` | 456 | **RETAIN / EXTEND** | Add target event/report tables; retire IEX-specific tables after dependency proof. |
| `models/perpetual/native-perpetual-v1/README.md` | `md` | 20 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `models/perpetual/native-perpetual-v1/approval-bundle.json` | `json` | 14 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `models/perpetual/native-perpetual-v1/artifact-integrity.json` | `json` | 29 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `models/perpetual/native-perpetual-v1/chronological-evaluation.json` | `json` | 53 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `models/perpetual/native-perpetual-v1/dataset-approval.json` | `json` | 74 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `models/perpetual/native-perpetual-v1/holdout-evaluation.json` | `json` | 41 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `models/perpetual/native-perpetual-v1/independent-approval.json` | `json` | 23 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `models/perpetual/native-perpetual-v1/model-manifest.json` | `json` | 118 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `models/perpetual/native-perpetual-v1/native-model.json` | `json` | 1 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `models/perpetual/native-perpetual-v1/owner-approval.json` | `json` | 18 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `models/perpetual/native-perpetual-v1/prospective-shadow-evidence.json` | `json` | 56 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `models/perpetual/native-perpetual-v1/prospective-shadow-journal.jsonl` | `jsonl` | 3 | **RETAIN / REVIEW** | Keep only if model policy supports target EUR/intraday horizon; otherwise archive. |
| `schemas/README.md` | `md` | 40 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/conversation-1.json` | `json` | 12 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/conversation-2.json` | `json` | 22 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/conversation-3.json` | `json` | 12 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/conversation-4.json` | `json` | 22 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/conversation-5.json` | `json` | 14 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/intelligence/README.md` | `md` | 33 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/intelligence/accounting.json` | `json` | 156 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/intelligence/attention.json` | `json` | 18 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/intelligence/coordination.json` | `json` | 24 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/intelligence/cross_asset.json` | `json` | 226 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/intelligence/event.json` | `json` | 109 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/intelligence/insider.json` | `json` | 81 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/intelligence/microstructure.json` | `json` | 49 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/intelligence/options.json` | `json` | 78 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/intelligence/short_interest.json` | `json` | 54 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/intelligence/tax.json` | `json` | 17 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/llm-contracts-1.json` | `json` | 48 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/llm-contracts-2.json` | `json` | 84 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/research-contracts-1.json` | `json` | 49 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/fixtures/research-contracts-2.json` | `json` | 8 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/llm-contracts.v2.json` | `json` | 293 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `schemas/research-contracts.v1.json` | `json` | 1319 | **RETAIN / EXTEND** | Add selection, quote, exit, URL, report, and capability contracts. |
| `scripts/README.md` | `md` | 216 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/build-long-donchian-authority.py` | `py` | 142 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/build-long-donchian-release-evidence.py` | `py` | 224 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/create-long-donchian-baseline.py` | `py` | 85 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/evaluate-long-donchian-atr.py` | `py` | 228 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/replay-long-donchian-ledger.py` | `py` | 598 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/requirements-dev.txt` | `txt` | 2 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/rotate-postgres-credentials.py` | `py` | 572 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/test-postgres.py` | `py` | 113 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/tests/test_rotate_postgres_credentials.py` | `py` | 419 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/validate-long-donchian-parity.py` | `py` | 217 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/validate-long-donchian-risk.py` | `py` | 192 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/verify-assets.py` | `py` | 111 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/verify-long-donchian-integrity.py` | `py` | 199 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/verify-long-donchian-ledger.py` | `py` | 226 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/verify-long-donchian-paper.py` | `py` | 188 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/verify-long-donchian-shadow.py` | `py` | 129 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `scripts/verify-news-packet.py` | `py` | 237 | **RETAIN SELECTIVELY** | Keep verification/replay/security scripts; remove obsolete authority builders after migration. |
| `services/README.md` | `md` | 81 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/finbert/.dockerignore` | `no extension` | 5 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `services/finbert/Dockerfile` | `no extension` | 15 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `services/finbert/README.md` | `md` | 105 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `services/finbert/compose.offline.yaml` | `yaml` | 8 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `services/finbert/compose.yaml` | `yaml` | 41 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `services/finbert/requirements.txt` | `txt` | 4 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `services/finbert/serve.py` | `py` | 153 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `services/finbert/tests/test_serve.py` | `py` | 104 | **DEPRECATE / CONFIGURE OFF** | Optional/legacy provider; keep only with an explicitly approved policy or fixture dependency. |
| `services/inputs/HANDOFF.md` | `md` | 182 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/inputs/HOLDING_ECONOMICS_HANDOFF.md` | `md` | 95 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/inputs/README.md` | `md` | 247 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/inputs/config.example.json` | `json` | 14 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/runtime/README.md` | `md` | 174 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/runtime/firewall.nft` | `nft` | 46 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/runtime/prepare.py` | `py` | 46 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/runtime/sysusers.conf` | `conf` | 13 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/runtime/trady-analyst.service` | `service` | 41 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/runtime/trady-evaluator.service` | `service` | 44 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/runtime/trady-executor.service` | `service` | 40 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/runtime/trady-gateway.service` | `service` | 38 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/runtime/trady-weekly.service` | `service` | 36 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `services/runtime/trady-weekly.timer` | `timer` | 12 | **RETAIN / REFACTOR** | Keep bounded service pattern; add daily/session monitor and verified collectors. |
| `skills/README.md` | `md` | 48 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/catalog.yaml` | `yaml` | 282 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/company-profile.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/conversation-and-tools/SKILL.md` | `md` | 44 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/entry-readiness/SKILL.md` | `md` | 43 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/holding-watchdog.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/influential-people.SKILL.md` | `md` | 21 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/market-research/SKILL.md` | `md` | 35 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/memory-and-review/SKILL.md` | `md` | 42 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/news-analysis/SKILL.md` | `md` | 37 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/opportunity-comparison.SKILL.md` | `md` | 21 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/perpetual-review.SKILL.md` | `md` | 28 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/position-review/SKILL.md` | `md` | 37 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/relationship-analysis.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/search-planning/SKILL.md` | `md` | 38 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sector-selection.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/10-energy.SKILL.md` | `md` | 25 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/15-materials.SKILL.md` | `md` | 25 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/20-industrials.SKILL.md` | `md` | 25 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/25-consumer-discretionary.SKILL.md` | `md` | 25 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/30-consumer-staples.SKILL.md` | `md` | 25 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/35-health-care.SKILL.md` | `md` | 25 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/40-financials.SKILL.md` | `md` | 25 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/45-information-technology.SKILL.md` | `md` | 25 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/50-communication-services.SKILL.md` | `md` | 25 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/55-utilities.SKILL.md` | `md` | 25 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/60-real-estate.SKILL.md` | `md` | 25 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/sectors/README.md` | `md` | 21 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/source-comparison/SKILL.md` | `md` | 35 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/1010-energy.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/1510-materials.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/2010-capital-goods.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/2020-commercial-professional-services.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/2030-transportation.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/2510-automobiles-components.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/2520-consumer-durables-apparel.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/2530-consumer-services.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/2550-discretionary-distribution-retail.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/3010-staples-distribution-retail.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/3020-food-beverage-tobacco.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/3030-household-personal-products.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/3510-health-care-equipment-services.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/3520-pharma-biotech-life-sciences.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/4010-banks.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/4020-financial-services.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/4030-insurance.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/4510-software-services.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/4520-technology-hardware-equipment.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/4530-semiconductors-equipment.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/5010-telecommunication-services.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/5020-media-entertainment.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/5510-utilities.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/6010-equity-reits.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/6020-real-estate-management-development.SKILL.md` | `md` | 23 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/subsectors/README.md` | `md` | 35 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `skills/uncertainty-and-incidents/SKILL.md` | `md` | 36 | **RETAIN / CONSOLIDATE** | Reuse pinned research and weekly skills; avoid duplicate skill sets. |
| `zoom_out_report.md` | `md` | 98 | **DEPRECATE / ARCHIVE** | Historical review artifact; not a runtime input. |

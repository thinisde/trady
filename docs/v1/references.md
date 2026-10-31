# Trady v1 reference checks and open capabilities

The expanded [research-source catalog](./research-sources.md) supports the 11 sector/25 subsector guides. [X listeners](./watchdogs-and-x.md) records the owner's five-account selection, identity evidence and capability references. [Perpetual](./perpetual.md) records the inspected upstream revision, Rust API checks and advisory contract. These are documentation inputs, not live provider entitlements or deployed integrations.

Documentation check date: **2026-10-31**. External pages describe available interfaces, not verified permissions on the owner's accounts. Recheck them at implementation and before live deployment. Numeric policy defaults elsewhere are Trady design choices, not recommendations asserted by these sources.

## Primary references

| Source | Checked information | Limits and follow-up |
|---|---|---|
| [Trading 212 API overview](https://docs.trading212.com/api) | Official API returned successfully; identifies beta status and primary-account-currency constraints | Confirm actual account entity, currency, permissions and demo/live behavior |
| [Trading 212 orders](https://docs.trading212.com/api/orders) | Official order reference lists market, limit, stop and stop-limit operations | Listing order types does not prove native linked/OCO, reduce-only, fractional compatibility or safe replacement behavior; test the exact profile |
| [Trading 212 accounts](https://docs.trading212.com/api/accounts), [positions](https://docs.trading212.com/api/positions), [historical events](https://docs.trading212.com/api/historical-events) | Official information families cover account summary, open positions, order/cash/dividend history | Trady exposes normalized read tools only; account `currentPrice` is not a replacement for a validated executable bid/ask |
| [OpenAI function calling](https://developers.openai.com/api/docs/guides/function-calling) and [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs) | Application-run multi-step tool loop, correlated call IDs/results, and structured final output | Preserve actual provider continuation records and verify exact adapter/schema support; Trady's canonical JSON is its own contract |
| [Telegram Bot API](https://core.telegram.org/bots/api) | Official update identifiers, polling/webhook alternatives, webhook secret header and bot-command registration | Trady commands and confirmation semantics are application design; numeric owner authorization remains Trady's responsibility |
| [X recent search](https://docs.x.com/x-api/posts/recent-search) | Official recent-search documentation describes a seven-day search window and cursor/time parameters | Confirm plan entitlements, individual-post lookup, deletion/retention duties and permitted external-model processing separately |
| [Alpha Vantage documentation](https://www.alphavantage.co/documentation/) | Documents `NEWS_SENTIMENT` and ticker/topic filtering as a non-X news candidate | Verify current price/plan, result coverage, latency and rights; provider sentiment is not Trady's confidence score |
| [Finnhub company-news documentation](https://finnhub.io/docs/api/company-news) | URL returned successfully, but the fetched page did not expose enough endpoint documentation for validation | Candidate only; verify the rendered documentation and actual subscription before implementing/enabling |
| [Google News RSS](https://news.google.com/rss?hl=en-US&gl=US&ceid=US%3Aen) and [issuer search example](https://news.google.com/rss/search?q=Microsoft&hl=en-US&gl=US&ceid=US%3Aen) | Both returned RSS/XML with item/title/link content during checking | Observed public feed behavior, not a documented paid API/SLA or permission to reuse publishers' full articles |
| [NYSE hours and calendars](https://www.nyse.com/markets/hours-calendars) | Primary regular-session and early-close reference | Use maintained instrument-specific exchange calendars; do not hardcode a German clock schedule |
| [PostgreSQL database creation](https://www.postgresql.org/docs/current/sql-createdatabase.html) | Database ownership/template syntax and database-creation transaction restriction | The installed server version is unknown; validate against that version before provisioning |
| [PostgreSQL schemas](https://www.postgresql.org/docs/current/ddl-schemas.html) and [privileges](https://www.postgresql.org/docs/current/ddl-priv.html) | Schema ownership/access and explicit privilege model used for isolation design | Database separation alone does not deny every other database login; inspect authentication rules and grants |
| [PostgreSQL role attributes](https://www.postgresql.org/docs/current/role-attributes.html) | LOGIN and administrative role attributes for dedicated runtime identities | No runtime superuser, owner membership, role creation or database creation |
| [PostgreSQL pg_dump](https://www.postgresql.org/docs/current/app-pgdump.html) | Single-database logical backup; server-level roles require separate treatment | Validate restore, compatible tooling and live recovery-point requirements on the shared server |

## Documentation lookup record

FinBERT preparation checked the official [model metadata](https://huggingface.co/ProsusAI/finbert)
and [pinned configuration](https://huggingface.co/ProsusAI/finbert/blob/0bc2ac10dda6929bcb1f77ccc43c7ec5c424f031/config.json):
revision `0bc2ac10dda6929bcb1f77ccc43c7ec5c424f031`, English classification,
positive/negative/neutral labels, a 512-token position limit and safetensors present
at that revision. Context7 resolved `Transformers` to `/huggingface/transformers`
and checked v4.57.3 classification/tokenization/model-loading documentation;
`Docker Compose` resolved to `/docker/compose` for service configuration options.
The prepared CPU packages are PyTorch `2.8.0+cpu` and Transformers `4.57.3`.
Metadata/docs and configuration validation establish these declarations, not
successful inference, image build, measured latency or trading performance. See
the [FinBERT implementation and open integration work](./finbert/README.md).

Following the repository's Context7 instructions, PostgreSQL was first resolved with `npx ctx7@latest library PostgreSQL` using a specific isolation/provisioning query. The selected result was `/websites/postgresql`, the high-reputation official documentation collection. A subsequent `docs` lookup covered database ownership and restricted runtime access; returned primary sources included the official schema, database-creation, role and default-privilege documentation.

Other primary pages were checked using bounded HTTP reads without account credentials. An HTTP 200 check establishes page availability and the cited visible material, not successful end-to-end provider integration. OpenAI-compatible behavior remains a provider-specific implementation check; no particular wire-protocol feature is assumed in these contracts.

For the multi-round extension, Context7 resolved `OpenAI API` to `/websites/developers_openai_api` and returned current tool-loop guidance. The ordinary function-calling and structured-output pages above were fetched directly as well, to distinguish Trady's application-managed tools from provider-specific programmatic execution features. No provider code-execution tool is required or enabled by this design.

No news/quote subscription, Rust dependency version, LLM provider/model, broker API rate/pricing assumption, current tax treatment or installed PostgreSQL version has been selected or verified for this deployment. These remain explicit capability/configuration work, rather than inherited claims from the historical v1.1 document.

## Decisions still needed at deployment

| Decision | Evidence to record | Behavior until resolved |
|---|---|---|
| Broker and account | Cash/fractional permissions, exact order/protection behavior and reconciliation coverage | Fake/demo adapters; no live trading |
| Quote/FX feed | Licensed venue coverage, timestamps, latency, quotas and halt handling | Paper/research only where real-time suitability is unproven |
| Non-X news provider and primary sources | Access plan, instrument coverage, source rights and budgets | Use permitted discovery data; no entry without verified evidence |
| X processing arrangement | Authorized access and source/LLM-provider terms | X disabled or metadata/discovery-only as permitted |
| LLM provider/model | Context capacity, structured-output/error behavior, allowed data processing and token pricing | Fake responses or bounded read-only evaluation |
| Existing PostgreSQL service | Server version, private endpoint, administrator provisioning path, capacity and restore | Documentation/fake-storage work; no assumptions about Paperless credentials |
| Owner/Telegram configuration | Numeric IDs, bot secret reference and tested notification delivery | No command acceptance with an empty allowlist |
| Paid operating budget | Daily/monthly caps, outside-account funding and price version | No uncapped paid collectors/generation |

The [implementation plan](./implementation.md) can start with fake adapters while these choices are resolved. They are live-deployment dependencies, not reasons to omit the requested v1 design.

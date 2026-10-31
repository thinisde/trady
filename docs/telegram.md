# Telegram monitoring

Trady has an owner-authorized, read-only Telegram monitor. Use the same Telegram
account on your phone, tablet or desktop to check the deployment and optional
Trading 212 account observations. Long polling needs outbound HTTPS access and
does not require a public server or webhook.

The monitor reports the configured paper foundation separately from a monitored
demo or live broker account. It does not supervise the analyst/executor processes
or expose trading controls, LLM conversations or research jobs through Telegram.
The broader [v1 interface](./v1/telegram.md) remains the product specification.

## Set up the bot

1. Open [@BotFather](https://t.me/BotFather), send `/newbot` and follow its prompts.
   Telegram's [official setup guide](https://core.telegram.org/bots/tutorial#obtain-your-bot-token)
   explains obtaining and revoking the token.
2. Store the token as `TRADY_TELEGRAM_BOT_TOKEN` in the
   [central credentials file](./credentials.md) or the monitor's environment.
   For an interactive Bash session, enter it without adding it to shell history:

   ```bash
   read -r -s -p 'Telegram bot token: ' TRADY_TELEGRAM_BOT_TOKEN
   printf '\n'
   export TRADY_TELEGRAM_BOT_TOKEN
   ```

3. Open your new bot in a private Telegram chat and send `/start`. Before starting
   the monitor, run this command from the repository root:

   ```sh
   cargo run --locked --bin trady -- telegram discover
   ```

   Discovery prints the bot identity and numeric user/chat IDs and timestamps of
   pending private text messages. It prints no message text, replies to nobody,
   grants no access, and does not acknowledge updates. Verify the IDs and message
   time belong to your own chat; do not authorize another pending sender.
4. Copy the non-secret example and edit the copy:

   ```sh
   cp config/telegram.example.json config/telegram.local.json
   ```

   For a single owner, store your verified numeric ID as a string under
   `credentials` in the central credentials file:

   ```json
   "TRADY_TELEGRAM_WHITELIST_USER_ID": "123456789"
   ```

   Leave both `access` arrays empty. The central setting replaces both user/chat
   allowlists with that single private ID before configuration validation. An
   environment value takes precedence over the file entry. A malformed configured
   ID prevents startup.

   If the central ID is empty or absent, configure explicit lists instead:

   ```json
   "access": {
     "allowed_user_ids": [123456789],
     "allowed_chat_ids": [123456789]
   }
   ```

   The number above is illustrative. Use the number from your verified private
   chat. Private user/chat IDs must match. Empty effective lists prevent startup.
   To allow multiple owners, leave the central ID empty and explicitly add each
   verified ID to both configuration lists;
   enabling notifications sends account updates to every configured private chat.

The bot token is resolved through the central loader. Telegram configuration rejects token,
custom endpoint and trading-authority fields. The local configuration copy and
default `var/` state directory are ignored by Git.

## Check and run

Validate configuration and resolved access lists without requiring bot/broker
credentials, provider requests or state writes:

```sh
cargo run --locked --bin trady -- telegram check --config config/telegram.local.json
```

Start the monitor, then send `/status` to your bot:

```sh
cargo run --locked --bin trady -- telegram run --config config/telegram.local.json
```

Keep this process running on the machine hosting Trady. A service manager can
restart it with the same environment, configuration and persistent state directory.
Ctrl+C ends the monitor after its bounded current request. The existing
`gateway --once` command remains a separate foundation startup probe.

For a bounded operational check, add `--once`. This performs one nonblocking poll,
persists handled commands and sends queued replies/notifications before exiting.
It can contact real providers and send real Telegram messages. JSON output reports
recorded commands, delivered messages and pending deliveries.

All paths inside the Telegram configuration resolve relative to that file, even
when the command is launched from another working directory. Defaults are:

- `foundation_config`: `paper.json`, validated as a `FoundationConfig`.
- `trading212_config`: `null`, so no account observations are available initially.
- `state_directory`: `../var/telegram`, relative to the configuration directory.
- `poll_timeout_seconds`: 20; allowed range 1–25.
- `refresh_interval_seconds`: 60; allowed range 60–3600.
- `stale_after_seconds`: 120; allowed range 60–3600 and at least the refresh interval.
- `notifications`: `true`, enabling startup and material broker monitoring alerts.

## Optional broker account observations

Prepare an account-bound configuration using the [Trading 212 guide](./trading212.md).
Set `trading212_config` to its path, for example `trading212.demo.example.json`
after replacing that file's illustrative account values. Its `environment`
selects `TRADY_T212_DEMO_API_KEY` / `TRADY_T212_DEMO_API_SECRET` for demo
or `TRADY_T212_LIVE_API_KEY` / `TRADY_T212_LIVE_API_SECRET` for live.
Supply the selected pair through the [central store](./credentials.md) or service
environment using an account-scoped API key. The monitor makes information requests only. Monitoring
requires both values from the selected mode and never falls back to the other pair.

The monitor constructs only `Trading212Reader`. A live broker binding remains
explicitly labelled as a live account observation; it does not change the paper
deployment mode or enable execution. Broker snapshots check the actual broker
account number and currency before reporting cash, positions and pending orders.

## Phone commands

| Command | Implemented response |
|---|---|
| `/start`, `/help` | Available monitoring commands |
| `/status`, `/health` | Configured mode/account/policy, monitor uptime, entry blockers, broker snapshot age and coverage limits |
| `/mode` | Deployment mode and separate broker observation scope |
| `/balance`, `/t212 account` | Observed spendable cash and timestamp |
| `/position`, `/positions`, `/t212 positions` | Instrument IDs and actual quantities, up to 20 holdings |
| `/orders`, `/t212 orders` | Pending order IDs, instrument, side, status and filled/total quantities, up to 20 orders |

Commands addressed to this bot, such as `/status@your_bot_name`, are supported.
Commands addressed to a different bot are ignored. Arguments to `/mode`, trading
controls and unsupported commands produce monitoring help and never mutate mode,
policy, plans or orders.

Every command, including `/help`, requires an allowed numeric sender and chat,
a private chat and a human sender. Group/channel messages, unauthorized senders,
edits, callbacks and non-text updates receive no account information. Command age
is checked in whole seconds within 0–300; oversized text is ignored.

Without a broker configuration or successful read, the bot reports data as
unavailable. During an outage it keeps the last in-memory successful snapshot,
marks it stale/degraded and preserves its observation timestamp. Cache entries
also become stale after the configured age. Cache contents do not survive a
restart; the monitor must obtain another successful broker snapshot.

Equity, cost basis, P&L, reserved/unsettled cash, active plans and protection state
are not supplied by this monitor. Absence from the pending-order endpoint does
not prove an order filled or was cancelled. The status response explicitly says
that other services are not supervised; it is not an execution heartbeat.

## Alerts and recovery

With notifications enabled, startup sends the current monitoring status. Scheduled
broker reads alert on cash, holding-quantity or pending-order changes, the first
failed read, and recovery. A new snapshot ID or reordered result alone produces
no alert. No-broker deployments provide commands and startup notification only.

Reads occur at the refresh interval plus polling/request time. Alerts describe
observations, rather than confirmed fills or trading outcomes. Telegram downtime
can delay monitoring; the monitor does not own order submission or protection.

`state_directory` holds a bot-bound JSON inbox/offset/outbox and an OS lock. Each
accepted typed command, its pending response and the next polling offset are
written atomically and synced before a later poll acknowledges the update.
The state retains the last 100 command records and up to 512 pending message
parts, with a 4 MiB file limit. Disk failures stop processing before advancing the
in-memory offset. New state files use mode `0600` and new directories use `0700`
on Unix. Preserve the private directory across process restarts.

Pending deliveries retry after transient failures with exponential backoff of
1–60 seconds, extended by Telegram's `retry_after`. Permanent API errors stop the
process with the outbox intact. Fix credentials, unblock the bot or resolve a
webhook/competing-poller conflict before restarting. A pending recipient removed
from the allowlists receives no further queued messages.

A crash between remote delivery and local acknowledgement can duplicate a message.
Stable `[event tg:…]` identifiers distinguish retries. Replies remain the recorded,
timestamped response when retried. Outgoing messages use plain text, bounded Unicode
parts, disabled link previews and content protection. Provider descriptions and
token-bearing request URLs are omitted from operational errors.

Run exactly one monitor per bot, using the same state directory on restart.
The OS lock coordinates processes sharing that directory; separate hosts or state
directories are not coordinated. Discovery also requires the normal poller to be
stopped. This monitor's local outbox is separate from the future PostgreSQL command
and execution notification workflow; it requires no schema migration or database role.

The implementation uses Telegram's documented
[`getMe`](https://core.telegram.org/bots/api#getme),
[`getUpdates`](https://core.telegram.org/bots/api#getupdates) and
[`sendMessage`](https://core.telegram.org/bots/api#sendmessage) methods. No webhook
is created or removed automatically.

## Verification

```sh
cargo test --workspace --locked telegram
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Tests cover owner spoofing/group rejection, command authority, stale/account-bound
snapshots, alerts, HTTP payloads and sanitized failures, rate limits, Unicode
splitting, durable replay, exclusive locking, failed persistence, queue bounds,
relative configuration paths and credential requirements. HTTP tests use loopback
fixtures without real tokens or external Telegram/broker requests.

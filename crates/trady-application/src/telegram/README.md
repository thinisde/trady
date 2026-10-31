# Owner monitoring application boundary

## Overview

[mod.rs](./mod.rs) authorizes owner messages, parses monitoring commands and renders
read-only account observations. It returns replies and change alerts as text;
Telegram polling/sending and broker snapshot acquisition belong to adapters.

## Interface

- [gateway.rs](./gateway.rs) defines authenticated `GatewayRequest`, bounded
  `GatewayCommand` parsing, `OwnerControlContext`, `OwnerControlRequest` and
  `OwnerControlKind` and `GATEWAY_HELP`. `[gateway.rs:authenticate]` validates the
  supplied scope, positive bot ID and nonnegative update ID, then applies the
  numeric/private-chat allowlists and age/text bounds before returning a command.
  Callers validate allowlist configuration and bind the sender to the supplied
  owner scope. Ordinary nonslash text becomes `Ask`; leading X/Twitter URLs
  become `Research`. Neither form becomes a close/policy action.
- `[gateway.rs:GatewayCommand]` uses `command`/`argument` serialization tags and
  `SCREAMING_SNAKE_CASE` variants: `Monitor`, `Ask`, `Research`, `Jobs`, `Report`,
  `Session`, `PauseEntries`, `ResumeEntries`, `ClosePosition`, `Confirm`,
  `CancelConfirmation` and `Unsupported`. Its public `parse` has no independent
  overall length bound; authentication applies the 4096-byte bound.
- `[gateway.rs:GatewayRequest]` carries bot/update/sender/chat IDs, scope and command.
  `[gateway.rs:OwnerControlRequest]` carries request/kind/scope, bot/owner/chat IDs,
  context/hash and request time; it has no local validator.
- `[gateway.rs:OwnerControlContext::binding_hash]` includes owner/account/policy, deployment
  mode, plan revision, holding ID/revision, instrument and exact quantity.
  Refresh timestamps are excluded; `validate` separately requires freshness.
- `[mod.rs:Monitor::new_for_gateway]` validates information-only metadata for explicitly
  namespaced paper/shadow/live deployments. `Monitor::new` retains its legacy
  foundation-only behavior. Neither constructor creates execution authority.
  The gateway constructor sets broker scope to `None`, so that instance cannot
  accept snapshots through `observe`.

- `[mod.rs:OwnerAccess]`: `validate` and `authorizes` for owner-user/private-chat
  access. `IncomingMessage` and `IncomingUpdate` carry incoming message data.
- `[mod.rs:MonitorCommand]`: `Help`, `Status`, `Mode`, `Balance`, `Position`,
  `Orders`, `Unsupported`; its parser is private. `CommandReply` carries reply data.
- `[mod.rs:BrokerScope]`: account ID, mode and currency, configured independently
  of the foundation configuration.
- `[mod.rs:Monitor]`: `new`, `observe`, `reply`, `render`. Construction takes
  `(access, foundation, scope, stale_after_seconds, now)`. `render` returns text
  directly; authorization is enforced by `reply`.

## Invariants

- `[gateway.rs:OwnerControlContext::validate]` is an explicit check: context age
  is 0–60 seconds, revisions are 64 hexadecimal characters, IDs pass syntax checks
  and quantity is a positive decimal string of at most 64 bytes. Caller workflows
  must obtain fresh executor-published control previews. Pause
  requests may have no holding fields; the optional holding ID, revision,
  instrument and quantity must be consistently present or absent. Mode and account
  namespace must agree. `[gateway.rs:OwnerControlContext::binding_hash]` changes
  with financial bindings, but does not call `validate` or invalidate previews.
- `[gateway.rs:GatewayCommand::parse]` recognizes supported aliases and rejects arguments that would change a
  mode or turn a command into arbitrary network/shell authority. Unimplemented
  commands return explicit non-authoritative explanations. Research targets have
  no registration, approval or URL validation here; downstream workflows check
  them. Session and job persistence lives in separately credentialed
  storage/worker components as an integration requirement.

- `[mod.rs:OwnerAccess::validate]` requires 1–32 unique IDs per list, each between
  1 and `2^52 - 1`; allowed chat IDs must also be allowed user IDs.
- `[mod.rs:OwnerAccess::authorizes]` accepts private, nonbot messages only when
  sender and chat IDs are equal and appear in both allowlists. Message age in
  whole seconds must be 0–300 and text is bounded to 4096 bytes.
- `[mod.rs:Monitor::reply]` rejects negative update IDs and absent messages.
  This module keeps no durable replay state. `[mod.rs:MonitorCommand::parse]`
  ignores blank messages and commands addressed to a different recipient;
  recipient comparison ignores ASCII case. Unmatched commands are unsupported.
- `[mod.rs:Monitor::new]` validates foundation/access configuration and requires
  a staleness threshold of 60–3600 seconds. `[mod.rs:Monitor::observe]` checks
  account, mode and spendable-cash currency against broker scope, validates the
  snapshot and rejects future, stale or regressing observations.
  Construction permits an absent broker scope, but observations then cannot be
  accepted.
- `[mod.rs:Monitor::render]` marks degraded/future/expired cached observations and
  bounds displayed lists to 20 entries. The orders view labels all snapshot orders
  pending without filtering their status.
- `[mod.rs:Monitor::render]` exposes no trading, research or mode-changing action.
  Execution, service supervision, plans/protection, equity/reserved funds,
  performance, costs/returns/targets/stops remain unavailable. Mode changes require
  deployment configuration.

## Side Effects

`[mod.rs:Monitor::observe]` sorts/replaces its in-memory snapshot cache. Changes to
cash, positions or orders, and recovery conditions, produce alert text. A first
success is silent unless recovering from an earlier failure. Failure retains the
cache, marks it degraded and alerts once. `[mod.rs:Monitor::reply]` and `render`
return unsent text.

There are no polling, sending, database, execution or model calls in this module.

## Verification

Run `cargo test -p trady-application --locked` from the repository root, or
`cargo test --workspace --locked` for the workspace. Adapter/database integration
uses `python3 scripts/test-postgres.py` against a disposable PostgreSQL container.

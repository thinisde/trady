# Owner monitoring application boundary

## Overview

[mod.rs](./mod.rs) authorizes owner messages, parses monitoring commands and renders
read-only account observations. It returns replies and change alerts as text;
Telegram polling/sending and broker snapshot acquisition belong to adapters.

## Interface

- `[mod.rs:OwnerAccess]`: `validate` and `authorizes` for owner-user/private-chat
  access. `IncomingMessage` and `IncomingUpdate` carry incoming message data.
- `[mod.rs:MonitorCommand]`: `Help`, `Status`, `Mode`, `Balance`, `Position`,
  `Orders`, `Unsupported` and command parsing. `CommandReply` carries reply data.
- `[mod.rs:BrokerScope]`: account ID, mode and currency, configured independently
  of the foundation configuration.
- `[mod.rs:Monitor]`: `new`, `observe`, `reply`, `render`. Construction takes
  foundation/access configuration, a staleness threshold and optional broker scope.

## Invariants

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
- `[mod.rs:Monitor::render]` marks degraded/future/expired cached observations and
  bounds displayed lists to 20 entries. The orders view labels all snapshot orders
  pending without filtering their status.
- `[mod.rs:Monitor::render]` exposes no trading, research or mode-changing action.
  Execution, service supervision, plans/protection, equity/reserved funds,
  performance, costs/returns/targets/stops remain unavailable. Mode changes require
  deployment configuration.

## Side Effects

`[mod.rs:Monitor::observe]` sorts/replaces its in-memory snapshot cache. Changes to
cash, positions or orders, and recovery conditions, produce alert text; the first
successful observation does not. Failure retains the cache, marks it degraded and
alerts once. `[mod.rs:Monitor::reply]` and `render` return unsent text.

There are no polling, sending, database, execution or model calls in this module.

## Verification

Run `cargo test -p trady-application --locked` from the repository root, or
`cargo test --workspace --locked` for the workspace. Adapter/database integration
uses `python3 scripts/test-postgres.py` against a disposable PostgreSQL container.

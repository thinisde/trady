# Telegram transport and durable replies

## Overview

The Telegram adapter implements fixed-origin Bot API requests and a bot-bound
filesystem inbox/offset/outbox. Authentication and monitoring text belong to the
[application module](../../../trady-application/src/telegram/mod.rs); polling and
retry scheduling belong to the [CLI](../../../trady-cli/src/telegram/mod.rs).

## Interface

- `[mod.rs:BotToken]`: validates token syntax, exposes the numeric bot ID and
  redacts the credential in `Debug` output.
- `[mod.rs:TelegramClient]`: `new`, `identity`, `updates` and `send_message`
  implement `getMe`, `getUpdates` and `sendMessage`.
- `[mod.rs:TelegramFailure]` / `[mod.rs:TelegramResult]`: sanitized numeric status,
  retry delay and permanence without provider descriptions or request URLs.
- `[mod.rs:BotIdentity]`: public `id: i64` and `username: String` returned by
  `identity`. `[mod.rs:MESSAGE_UNITS]` is `3500`;
  `[mod.rs:split_message]` divides text into parts of at most that many UTF-16 units.
- `[spool.rs:StateStore]`: `open`, `state`, `accept_update`, `enqueue_alert` and
  `acknowledge_front` provide local durable command and delivery state.
- `[spool.rs:MonitorState]`: public `version`, `bot_id`, `next_offset`, `next_event`,
  `recent_commands` and `outbox` fields. `[spool.rs:RecordedCommand]` exposes
  `update_id`, `sender_id`, `chat_id`, `command` and `received_at`;
  `[spool.rs:Delivery]` exposes `event_id`, `chat_id` and `text`.

## Invariants

- `[mod.rs:TelegramClient::new]` fixes production to `https://api.telegram.org`.
  `[mod.rs:TelegramClient::build]` disables redirects, proxies and automatic
  transport retries; poll timeout is 1–25 seconds and request timeout is poll + 10.
- `[mod.rs:TelegramClient::identity]` checks returned bot identity against the
  token's numeric prefix and validates the username.
- `[mod.rs:TelegramClient::updates]` omits offset for discovery; otherwise it
  accepts nonnegative offsets and returns at most 100 sorted, unique update IDs
  below `i64::MAX`. Missing sender/text or an unrepresentable timestamp retains
  the update ID with `message: None`; malformed required DTO fields/types fail
  the response. Private-chat/bot-sender flags do not enforce caller authorization.
- `[mod.rs:TelegramClient::send_message]` bounds messages to 4096 UTF-16 units;
  success requires a positive message ID and the expected private chat.
- `[mod.rs:TelegramClient::call]` classifies transport failures as nonpermanent;
  parsed API errors prefer body status/retry delay over HTTP headers.
  `[mod.rs:TelegramFailure::invalid]` marks local validation failures permanent.
- `[spool.rs:StateStore::open]` checks version/bot identity and bounds state to
  4 MiB, 100 recent command records and 512 pending parts. One directory-local OS
  lock remains held for the store lifetime. Other hosts/directories are not fenced.
  `[spool.rs:MonitorState]`, `RecordedCommand` and `Delivery` reject unknown fields;
  loading does not validate recent-command contents or event-ID uniqueness.
- `[spool.rs:StateStore::accept_update]` ignores already-accepted IDs and commits
  a new typed command, reply parts and next offset together. Ignored traffic may
  advance the offset without an accepted command or reply.
- `[spool.rs:append]` adds stable bot/event/part markers and rejects a staged
  append when capacity or message bounds would be exceeded.
- `[spool.rs:StateStore::acknowledge_front]` is the explicit delivery-removal
  operation. A remote-send/local-ack crash can duplicate delivery with the same ID.
- `[mod.rs:TelegramClient::send_message]` and `[spool.rs:append]` validate positive
  chat IDs; caller authorization is required before invoking these boundaries.

## Side Effects

`[mod.rs:TelegramClient::call]` makes authenticated HTTPS POSTs and accepts at most
2 MiB of response data, reading up to one additional byte to detect overflow.
Tokens occur in the provider URL path; returned failures discard underlying
errors, provider text and URLs. Outgoing messages request
content protection and disabled previews, with no formatting parse mode.

`[spool.rs:StateStore::open]` creates the state directory and lock file.
`[spool.rs:StateStore::commit]` writes plaintext private JSON through
`state.json.tmp`, syncs, renames and syncs the directory on Unix. Newly created Unix
directories/files use `0700`/`0600`. A missing state file initializes only memory
until a mutator commits. `[spool.rs:StateStore::enqueue_alert]` consumes and
persists an event sequence even with no recipients; duplicate recipients remain.
Existing paths are trusted deployment input
and their ownership, permissions and symlink targets are not validated or repaired.
A post-rename directory-sync failure can leave disk ahead of memory; callers
must stop further mutations of that store after a persistence error. This is a
caller obligation; the store does not enforce a poisoned state.

## Verification

Run `cargo test -p trady-adapters --locked telegram` from the repository root.
[HTTP tests](./tests.rs) use loopback fixtures, fixed non-secret test tokens and
bounded request waits. They exercise payloads, discovery, redaction, rate limits,
redirect rejection, malformed responses and Unicode splitting.

The existing `spool/README.md` preserves focused durability/recovery test notes;
that directory contains test code only and is excluded from production boundaries.

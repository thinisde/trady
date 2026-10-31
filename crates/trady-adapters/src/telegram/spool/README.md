# Telegram spool verification

## Overview

This directory contains the private Rust test module for
[the durable Telegram state store](../spool.rs). It is documented explicitly for
the scoped Telegram bootstrap; it contains no production implementation.

## Interface

- No public production symbols. `[tests.rs:Temp]` supplies isolated temporary
  paths for tests and removes fixture directories on drop.
- `[tests.rs:inbox_offset_and_outbox_survive_restart_and_replay_is_idempotent]`
  checks persisted replies, offsets, event IDs, acknowledgement and replay.
- `[tests.rs:one_active_consumer_and_bot_binding_are_enforced_without_stale_pid_files]`
  checks exclusive locking, automatic unlock and bot binding.
- `[tests.rs:failed_persistence_never_advances_the_in_memory_polling_offset]`
  checks a pre-write failure with no in-memory offset/outbox mutation.
- `[tests.rs:ignored_updates_can_advance_but_invalid_or_unbound_replies_cannot]`
  checks ignored updates, reply/update binding and invalid IDs.
- `[tests.rs:queue_bounds_are_transactional_and_long_messages_keep_stable_part_ids]`
  checks staged capacity rejection, Unicode message parts, event IDs and new Unix
  state-file permissions.

## Invariants

`[tests.rs:Temp::new]` uses the OS temporary directory, process ID and an atomic
counter. Tests create no project-local state and require no database, provider
credentials or external network calls. Assertions concern the specific exercised
failure points; they do not simulate every filesystem crash or directory-sync
failure.

## Side Effects

`[tests.rs:Temp::new]` supplies temporary paths; tests create files/directories,
acquire OS file locks, serialize state and reopen it. `[tests.rs:Temp::drop]`
removes each fixture directory. The queue-bound test creates a large in-memory
message and verifies rejection before persistence.

## Verification

From the repository root, run
`cargo test -p trady-adapters --locked telegram::spool::tests`.

# Telegram credentials

## TRADY_TELEGRAM_BOT_TOKEN

Obtain this token from Telegram's [@BotFather](https://t.me/BotFather), following
the [official bot registration guide](https://core.telegram.org/bots/tutorial#obtain-your-bot-token).

1. Open the verified `@BotFather` account in Telegram.
2. Send `/newbot` and follow the prompts for a bot name and username.
3. Copy the complete token BotFather returns, including its numeric prefix and
   colon.
4. Store it as `TRADY_TELEGRAM_BOT_TOKEN` in
   `config/credentials.local.json`; see the [store setup](../README.md).

For an existing bot, manage its token through BotFather instead of creating a
second bot. BotFather also provides token revocation/replacement.

## TRADY_TELEGRAM_WHITELIST_USER_ID

Use your numeric Telegram user ID. To find it, open your bot's private chat,
send `/start`, then run:

```sh
cargo run --locked --bin trady -- telegram discover
```

Discovery reports `pending_private_senders` with `user_id`, `chat_id` and
`sent_at`. Verify that the IDs and timestamp match your own private message.
Use the matching `user_id` and `chat_id`, then save the ID as a string under
`credentials` in `config/credentials.local.json`:

```json
"TRADY_TELEGRAM_WHITELIST_USER_ID": "123456789"
```

The number above is illustrative. Use the verified ID from discovery. This
setting accepts one positive numeric private user ID; use the original decimal
digits without spaces, an `@` username or a comma-separated list.

When configured, this value replaces both Telegram user/chat allowlists with
that single ID before `telegram check`, `telegram run` or `telegram worker`
validates configuration. You can leave both arrays in the private Telegram
configuration empty. Environment variables override the central file as usual.

If this entry is empty or absent, the configuration's `access.allowed_user_ids`
and `access.allowed_chat_ids` apply. An explicitly empty environment value
disables the file entry. Empty effective lists or an invalid configured ID
prevent startup. Follow the [monitoring guide](../../../docs/telegram.md) for
the remaining configuration and for multiple-owner setup.

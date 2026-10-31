# X credentials

## TRADY_X_CONSUMER_KEY

X calls this the **API Key** or **Consumer Key**. Obtain it from the same app
as the bearer token:

1. Sign in to the [X Developer Console](https://console.x.com) and select your
   Trady app, or create an app if needed.
2. Open the app's **Keys and tokens** section and obtain or generate the
   **API Key and Secret** pair.
3. Copy the **API Key** value into `TRADY_X_CONSUMER_KEY` in
   `config/credentials.local.json`; see the [store setup](../README.md).

The [official API Key and Secret guide](https://docs.x.com/fundamentals/authentication/oauth-1-0a/api-key-and-secret)
explains these app credentials and how to regenerate them.

## TRADY_X_CONSUMER_SECRET

Copy the **API Key Secret** (also called **Consumer Secret**) from the same
app's key/secret pair into `TRADY_X_CONSUMER_SECRET` in
`config/credentials.local.json`. Store the original secret text.

X displays generated credentials only once, so save both values before leaving
the console. Regenerating the pair invalidates the old credentials; update both
central entries and restart any clients using them.

The consumer pair is available in the central store for integrations that need
it. Trady's current X lookup and timeline readers use the bearer token below.

## TRADY_X_BEARER_TOKEN

Trady's X lookup and timeline readers use an app-only OAuth 2.0 bearer token.

1. Sign in to the [X Developer Console](https://console.x.com) with your X account
   and complete developer enrollment.
2. Create an app, or select the app you will use for Trady.
3. Open the app's **Keys and tokens** section and obtain or generate its
   **Bearer Token**, as described in the
   [official bearer-token guide](https://docs.x.com/fundamentals/authentication/oauth-2-0/bearer-tokens).
4. Copy the token value into `TRADY_X_BEARER_TOKEN` in
   `config/credentials.local.json`; see the [store setup](../README.md).
   Store only the token text; Trady adds the `Bearer` header prefix.

Confirm that the app has access to the lookup/timeline endpoints you intend to
use and review current billing/access in the
[Developer Console documentation](https://docs.x.com/fundamentals/developer-portal).

The reviewed source entries in [research.json](../../research.json) reference
this name through `credential_env`. They ship disabled. Use the
[news setup guide](../../../docs/news.md) to review source rights, account IDs
and collection settings before enabling them.

If you regenerate the token in the console, update the central entry and restart
clients that still hold the old value.

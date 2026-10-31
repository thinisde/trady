# Trading 212 credentials

Trady stores exactly four T212 values: one API key and one API secret for demo,
and one API key and one API secret for live. The broker configuration's
`environment` selects the matching pair for all T212 clients.

## Generate a pair

Follow Trading 212's
[official key-generation instructions](https://helpcentre.trading212.com/hc/en-us/articles/14584770928157-Trading-212-API-key).

1. Sign in to Trading 212 and select the account/environment you want to use.
   Start with the practice/demo account when obtaining demo credentials.
2. Open **Menu → Settings → API (Beta) → Generate API key** and review the
   provider's required prompts.
3. Name the key and configure the permissions and any IP restrictions needed by
   your deployment.
4. Generate the pair and copy the **API Key** and **API Secret Key** into the
   corresponding entries below. The secret is displayed only once; losing it
   requires generating another pair.
5. Repeat in the real/live account for live credentials.

Store raw values in `config/credentials.local.json` using the
[central store setup](../README.md). Trady constructs authentication headers;
these entries contain the original key and secret, without Base64 encoding.

## TRADY_T212_DEMO_API_KEY

Copy **API Key** from the pair generated in the practice/demo account.
Use it with `environment: "demo"` in the
[demo broker configuration](../../trading212.demo.example.json).

## TRADY_T212_DEMO_API_SECRET

Copy **API Secret Key** from that same demo pair.

## TRADY_T212_LIVE_API_KEY

Copy **API Key** from the pair generated in the real/live account.
Use it with `environment: "live"` in the
[live broker configuration](../../trading212.live.example.json).

## TRADY_T212_LIVE_API_SECRET

Copy **API Secret Key** from that same live pair.

Both values must come from the same pair and environment. Configure the real
broker account number and currency before requesting a snapshot; see the
[T212 guide](../../../docs/trading212.md). Missing credentials do not trigger a
fallback to the other environment.

If the API menu is unavailable, check account eligibility in the official guide
or contact Trading 212 support. Account availability and provider permissions are
separate from storing the credentials.

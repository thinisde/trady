# Alpha Vantage credentials

## TRADY_ALPHA_VANTAGE_API_KEY

1. Open Alpha Vantage's
   [official API-key request form](https://www.alphavantage.co/support/#api-key).
2. Complete the requested use-case, organization and email fields and submit
   the form.
3. Copy the issued API key into `TRADY_ALPHA_VANTAGE_API_KEY` in
   `config/credentials.local.json`; see the [store setup](../README.md).

If your organization already has an appropriate Alpha Vantage key, obtain it
through its account administrator or secret manager.

Trady's Alpha Vantage source requests `NEWS_SENTIMENT`. Confirm access and
current request limits with the provider for your key before enabling the source.
The source's `credential_env` in [research.json](../../research.json) already
references this name, but the source ships disabled. The
[news guide](../../../docs/news.md) explains source review and collection.

Storing the key does not change your provider entitlement or enable the source.

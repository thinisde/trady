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

The legacy Alpha Vantage adapter requests `NEWS_SENTIMENT`. It is not loaded by
the focused IEX30 registry; keep this credential only for offline compatibility
work and do not enable it in the IEX30 deployment.

Storing the key does not change your provider entitlement or enable the source.

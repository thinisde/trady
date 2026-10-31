# Trading-inputs service credentials

## TRADY_TRADING_INPUTS_TOKEN

This is the bearer token for the approved service configured as `inputs.base_url`
in an autonomous deployment. That service supplies session, quote, cost and
finalized research inputs to
[`AutomaticInputs`](../../../crates/trady-adapters/src/autonomous/mod.rs).

1. Identify the operator of the configured inputs service.
2. Request a token with access to the endpoints required by your deployment.
   Follow that service's own issuance and rotation procedure.
3. Store the raw token as `TRADY_TRADING_INPUTS_TOKEN` in
   `config/credentials.local.json`; see the [store setup](../README.md).
   Trady adds the `Bearer` header prefix.

If you operate the service and it supports a static shared bearer token, generate
a value locally with OpenSSL:

```sh
openssl rand -hex 32
```

Configure that same value in the service's authentication configuration and in
Trady. Generating a value alone does not authorize it on an existing service.
See the [OpenSSL random-generation documentation](https://docs.openssl.org/3.5/man1/openssl-rand/).

## TRADY_INPUTS_SERVICE_TOKEN

The bundled `trady inputs-service serve` reads this server-side credential through
the same central store and requires it. Give it the same raw private value as
the client's `TRADY_TRADING_INPUTS_TOKEN`. Keep server and client credential files
scoped to their respective service identities. The
[input-service guide](../../../services/inputs/README.md) describes its loopback
listener and HTTPS reverse proxy.

The client token is optional in Trady's client: leave it empty only when the approved
service's authentication setup permits unauthenticated requests. The service
operator provides the actual credential; Trady has no public sign-up flow for it.

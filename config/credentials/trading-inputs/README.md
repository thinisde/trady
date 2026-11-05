# Trading-inputs service credentials

## TRADY_TRADING_INPUTS_TOKEN

This is the historical bearer-token name for the
[retired automatic-input client](../../../docs/eur-legacy-retirement-2026-11-05.md).
Its existing private value is preserved, but the EUR runtime does not load an
`inputs.base_url` trading client. Shared research wire documents and the reviewed
input-service server remain available for offline/interoperability tools.

For a separately reviewed interoperability client, obtain its scoped bearer
from the service operator and keep it in the private [store](../README.md).
An old token name grants no EUR quote or execution authority.

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

The retained server requires its own credential. Any new client must follow
the service's authentication policy; the retired client's optional-token behavior
is not an authorization policy for that server.

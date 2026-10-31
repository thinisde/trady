# Locally generated secrets

These values have no external provider. Generate independent random values
locally when a consumer needs them, and store them using the
[central credentials setup](../README.md).

## TRADY_HASHING_SECRET_KEY

This entry is reserved for a future keyed-hashing consumer. With OpenSSL installed,
generate 32 random bytes represented as a 64-character hexadecimal string:

```sh
openssl rand -hex 32
```

Copy the result into `TRADY_HASHING_SECRET_KEY` in
`config/credentials.local.json`. The current SHA-256 content fingerprints are
unkeyed and do not use this value; leaving it empty is supported.

## TRADY_SIGNING_SECRET_KEY

This entry is reserved for a future signing consumer. Run the command again to
obtain an independent value:

```sh
openssl rand -hex 32
```

Copy that result into `TRADY_SIGNING_SECRET_KEY`. It also has no active consumer
today and may remain empty. A future consumer must define its algorithm and key
encoding; this generic store does not create an asymmetric signing key pair or
define a signing protocol.

OpenSSL documents random-byte generation and hexadecimal output in
[`openssl rand`](https://docs.openssl.org/3.5/man1/openssl-rand/).
The generation commands display values in your local terminal; keep the resulting
values in the private store rather than the committed example.

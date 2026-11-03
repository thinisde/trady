# Native Perpetual advisory approval

This bundle registers `native_perpetual_walk_forward_v1_20261103` for bounded
advisory and prospective-shadow use. The immutable native artifact is
`native-model.json` with SHA-256
`cbff93e1a653565662f85cea0e472d1d6e8c62a69adebd1b8e41ba4b22bc1480`.

`model-manifest.json` is the machine-readable `ModelManifest` copied into
`config/autonomous.iex30.demo.json`. The other JSON files bind dataset lineage,
artifact/schema checks, chronological evaluation, the exact frozen holdout,
prospective-shadow start. `prospective-shadow-journal.jsonl` records the two
currently supported IEX30 observations (MSFT and JPM) with their feature values,
source hashes and pending five-session outcomes. It intentionally emits no
forecast until an executable scenario and quote are bound.

The approval scope is advisory/shadow only. The chronological and holdout
checks pass their integrity gates but show no measured predictive advantage over
the prevalence baseline. The prospective shadow record has started with no
matured outcomes, so the demo remains disabled and entry-paused.

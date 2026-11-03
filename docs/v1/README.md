# Shared Trady references

The deployed strategy is defined by [the IEX30 guide](../iex30.md) and its
[focused audit](../iex30-audit.md). This directory contains reusable contracts
that remain in the workspace for offline tests and supervised research. None of
these documents grants broker authority or changes the fixed IEX30 universe.

## References

- [Architecture](./architecture.md): process boundaries and ownership.
- [Research contracts](./research-contracts.md): evidence, provenance and
  source permissions.
- [Research sources](./research-sources.md): provider and attribution rules.
- [X monitoring](./x-api-monitoring.md): cost, identity and rights constraints;
  the active registry is `config/research.json`.
- [Watchdogs and X](./watchdogs-and-x.md): holding-monitoring concepts; the
  executable IEX30 schedule is `config/watchdog.iex30.json`.
- [Trading policy](./strategy.md): sizing, exits and safety invariants used by
  shared policy code.
- [Prediction fusion](./prediction-fusion.md): advisory model semantics and
  abstention rules.
- [Prediction implementation](./prediction-implementation.md): tested offline
  subset and external release inputs.
- [FinBERT](./finbert/README.md): optional sentiment sidecar, disabled in the
  focused runtime.
- [Memory and data](./memory-and-data.md): persistence and retention contracts.
- [Telegram](./telegram.md): optional owner-notification surface.
- [Weekly optimization](./weekly-optimization.md): offline research workflow;
  it does not deploy runtime changes.

The removed feature-design and dated audit directories were not part of the
focused deployment. Current implementation status lives in the IEX30 audit.

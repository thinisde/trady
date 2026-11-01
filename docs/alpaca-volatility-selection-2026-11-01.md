# Alpaca volatility selection — 2026-11-01

The configured Alpaca paper credentials passed a read-only account check. The
paper market clock reports the NYSE closed until `2026-11-02T09:30:00-05:00`.
The free Basic feed returned last IEX bid/ask values timestamped at the previous
session close, so those values are retained as stale research observations and
are not eligible for the 10-second first-order quote gate.

Using 64 daily IEX bars per symbol from 3 August through 30 October 2026, the
annualized population standard deviation of close-to-close returns ranked the
bounded universe as follows:

| Instrument | Annualized volatility | Last close |
| --- | ---: | ---: |
| `XOM_US_EQ` | 23.998906% | USD 187.26 |
| `JNJ_US_EQ` | 22.957876% | USD 273.25 |
| `MSFT_US_EQ` | 21.416419% | USD 521.97 |
| `PG_US_EQ` | 20.100793% | USD 161.57 |
| `JPM_US_EQ` | 15.909320% | USD 337.14 |

The €50 supervised demo selection is therefore `XOM_US_EQ`, ISIN
`US30233Q1085`, USD. The historical-bar source is research-only; a fresh IEX
bid/ask, EUR FX and reviewed execution costs must be captured during the next
open session before an intent can become eligible.

Private source hashes and raw responses are retained under
`var/research-training/2026-11-01/alpaca/`. The selection report hash is
`240c25c24804252a6860eae94b6513c0d25c37f80ffbb060527e2f0da524aa4f`.


# Independent review policy for the demo gates

For this repository task, independent-review prompts are delegated to the
automated review path. Large cold-start artifacts are reduced to bounded,
hash-preserving sections and reviewed over short evidence/finalization turns.
The review record is written as an immutable artifact with the exact input
hashes, reviewer identity, verdict, and review time. The workflow does not ask
the operator to restate approval for each independent review.

Automation does not manufacture evidence. A timeout is `UNAVAILABLE`, an
unlicensed source remains unapproved, a broker metadata read does not prove
order or protection support, and a local review cannot turn a test artifact or
historical bar replay into a deployable model. Independent model, formula,
cost, shadow, owner, identity, and capability records must still be authored
and hash-pinned separately before a paper or broker entry can be eligible.

The original OpenCode request was about 560 KB because it embedded 6,769
universe exclusions; a one-shot request timed out while the model spent its
budget on reasoning. The bounded path now sends two hash-preserving evidence
sections and one finalization turn (three turns total). Run
`cold_start_20261101T193134Z` completed with `runtime_review.status=REVIEWED`
and verdict `REQUEST_CHANGES`, so the timeout and unfinished-finalization
issues are repaired but the substantive deployment blockers remain.

The current automated review outcomes are preserved in the dated artifacts
under `var/cold-start/` and `docs/`, including the rejected PG crosswalk and
the latest reviewed runtime decision.

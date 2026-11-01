# Demo skill review and operator scope

On 2026-11-01, bundle **2.1.1** was installed as operator-approved research
instructions in `config/autonomous.demo-no-submit.json`, under the owner's
delegation to complete the record-only demo setup. This decision is restricted
to **shadow / record_only**. It grants no source verification, financial,
model, formula or live-deployment approval.

## Review evidence

Fresh-context delegated reviewers read the complete 52-entry catalog and all
skill bodies in two scopes: 16 workflows and 36 sector/subsector guides. Both
also read `runtime_tools.rs` and autonomous `readiness.rs`. Their findings are
preserved in the task results in the originating conversation. The task request
identifiers were:

- `demo-egm-skill-doc-workflow-20261101-v1`
- `demo-egm-skill-doc-taxonomy-20261101-v1`
- `demo-egm-workflow-corrections-20261101-v2`

The first workflow review found unconditional execution/protection claims,
assumed tool availability, and contradictory proposal-expiry ownership.
Five workflow files were corrected and versioned. The second workflow review
read all 16 workflows again and found no remaining substantive blocker to
shadow-only instruction approval. The unchanged 36 taxonomy files retained
their previously reviewed hashes. Linked design documents were not included
in the approved skill-body review and are not implicitly approved instructions.

## Corrections

- `conversation-and-tools` 1.0.2 requires the actual deployment mode and tool
  catalog; missing capabilities block the affected workflow. Prose cannot
  establish submissions, fills, protection, exits or scheduled jobs.
- `entry-readiness` 1.1.1 assigns envelope expiry to the application and
  qualifies shadow execution as recording only.
- `position-review` 1.1.1 qualifies executor and exit behavior by actual mode
  and results.
- `uncertainty-and-incidents` 1.0.1 requires actual notification results and
  does not promise shadow protection or exits.
- `holding-watchdog` 1.0.1 requires actual registered tools and compiler
  results; shadow records do not activate watchdogs.

## Content binding and limits

The [approved manifest](../config/skills.demo.approved.json) records all 52 exact
file hashes. The catalog SHA-256 is
`acea7eb10b6670874909e382014fee029f71440231a4fccb6e93a0d67109530f`.
Native `trady skill-manifest skills` output equals this manifest. The
[demo configuration](../config/autonomous.demo-no-submit.json) contains 52
approved name-to-hash entries. Other deployment configurations were not granted
this approval.

Hash checks bind content; they do not verify market sources or establish
operational capabilities. Missing tools and missing trusted evidence still
produce blocked/no-action decisions. No model or formula approval was created.

The native bundle hash-change rejection test and the CLI missing-prerequisite
test both passed after installation. The latter explicitly clears the fixture's
allowlist and skill approvals to continue testing absent prerequisites.

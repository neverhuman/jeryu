# Rollback

Redline consumer rollback is a reviewed source change to a previously verified
immutable tag and commit. Never move or recreate an existing tag, rewrite
history, or edit proof eligibility by hand. If the consumer contract, family
receipt, checksum, or proof lock disagrees, stop and leave cutover ineligible.

Re-run `just release-readiness` after the corrective change, merge it through
the protected Jeryu lifecycle, and generate new consumer evidence from clean,
forge-equal `main`. Jain's runtime rollback target remains `7.0.6`; this source
repository neither applies that rollback nor performs any production mutation.

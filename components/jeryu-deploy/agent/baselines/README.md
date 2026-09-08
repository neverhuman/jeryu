# Jankurai ratchet baselines

`main.repo-score.json` is a full-mode Jankurai 1.6.11 report generated from
hosted protected `main` commit
`b114cbfd2a996b5a6528310954c0a1ab4553414d` in an automatically removed
no-local clone. `main.repo-score.provenance.json` binds its source commit/tree,
governed binary identity, fingerprints, and tracked-file checksum. The report
is proposed by this topic and becomes accepted only through independent
exact-head review and protected merge; merely running the candidate gate does
not accept or refresh it.

The superseded 1.6.10 baseline remains byte-for-byte under `historical/` as
non-authoritative audit history and must never be relabelled as 1.6.11 evidence.
When hosted `main` advances, regenerate both active files from that exact
protected commit and submit the changed bytes to detached review. A topic run
requires that exact current protected base. A protected-main replay accepts the
same baseline only as a strict ancestor, avoiding an impossible self-referential
report; the next topic must refresh it to the then-current protected base.

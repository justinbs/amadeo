# ADR 0001 — Record architecture decisions

**Status:** Accepted · **Date:** 2026-07-30

## Context

Amadeo is expected to span many working sessions, with a human and an AI agent as its two authors.
Neither carries perfect memory of prior reasoning: Justin will not recall why a trade-off was made
six months earlier, and Claude begins each session with no memory of the last one beyond what is
written in this repository.

Undocumented decisions get re-litigated. Worse, they get *silently reversed* by someone who doesn't
know they were decisions at all — which is how invariants erode.

## Decision

Every decision that constrains future work gets an ADR in `docs/adr/`, numbered sequentially.

An ADR is short: context, decision, consequences, and the alternatives that were rejected and why.
The rejected alternatives are often the most valuable part — they're what stops the same debate from
recurring.

**ADRs are immutable once accepted.** To change a decision, write a new ADR that supersedes the old
one and mark the old one `Superseded by NNNN`. Never edit history; the reasoning trail is the point.

Not everything needs an ADR. Implementation details, naming, and reversible choices don't. The test
is: *would someone reasonably do this differently, and would changing it later be expensive?* If yes,
write one.

## Consequences

- A small, recurring documentation cost.
- New sessions can reconstruct intent from the repository alone, which is the difference between this
  project surviving multi-session development and slowly dissolving.
- Invariants gain a paper trail, so violating one becomes visibly wrong instead of merely undiscussed.

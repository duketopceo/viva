# VIVA — product and architecture brief

Codename: **VIVA** — understanding your own code.

Shared design principle across VEIN, SWITCHBOARD, and VIVA: **simple on the
surface, with every claim, result, and explanation clickable down to its
evidence.**

## Existing overlap matters

DeepTutor already documents GitHub-backed knowledge sources, versioned
retrieval, source-linked learning, mastery paths, and live agent consultation
([DeepTutor README](https://github.com/HKUDS/DeepTutor/blob/main/README.md)).
Its repository represents substantial existing overlap — not just visual
inspiration.

VIVA's differentiation is **understanding a particular revision of executable
software**, including the connection between architecture, source code, tests,
observed behavior, and the learner's own explanations.

## A clickable understanding graph

The main screen combines a repository map, source viewer, tutor, and evidence
drawer.

| Click | What opens |
|---|---|
| **Feature** | Entry points, implementation, data access, tests |
| **Function** | Definition, callers, dependencies, relevant questions |
| **Architecture claim** | Supporting code and whether the claim is inferred or observed |
| **Runtime path** | Trace, environment, inputs, and linked source |
| **Learning objective** | Prior attempts, feedback, supporting evidence, next exercise |
| **Commit change** | Affected concepts and explanations that may now be stale |

This supports questions such as:

> "Show me how signing in actually reaches the database. Now quiz me on where
> authorization happens and what breaks if this check moves."

The answer must point to the specific files and revision. A polished
architecture diagram is not enough.

## Three evidence categories

Keep these visibly distinct:

**Static evidence:** definitions, imports, references, configuration, schemas,
and tests.

**Observed evidence:** traces and test results from a specific execution with
recorded inputs and environment.

**Inferred explanation:** the tutor's interpretation of intent or behavior.

A static call graph should not be presented as a complete runtime trace. A
passing test should not establish behavior outside the case it exercised.

Start with syntax-aware indexing and exact search. Add semantic retrieval
where it helps navigation; do not make embeddings the only way to find code.

## Teaching that proves understanding

Use a repeatable learning cycle:

1. Ask the learner to explain or predict before revealing the answer.
2. Retrieve the relevant source and tests.
3. Give targeted hints.
4. Ask a follow-up that changes an assumption.
5. Run a bounded exercise where execution is permitted.
6. Save the demonstrated understanding and unresolved gaps.

For example, a learner who can repeat "this middleware checks authentication"
should still be asked what happens if it is removed, which routes it protects,
and which test would catch a regression.

Support two modes: **Explore**, where explanations are freely available, and
**Defend**, where the learner answers before seeing the solution. Do not turn
every coding session into an exam.

## Meaningful mastery records

Track competencies, not a single opaque "you understand 82% of this repo"
score.

Examples: tracing a request, explaining state transitions, locating
authorization, predicting a failure, interpreting a test, implementing a
bounded change.

Each assessment retains the question, learner response, rubric, source
references, commit, hints used, and verification results. Passing tests
contributes evidence; it does not prove understanding by itself.

When code changes, mark related learning evidence as **needs review** rather
than automatically resetting everything or leaving old mastery labels
untouched.

## Reuse versus fork

DeepTutor's documented CLI already supports agent-driven, machine-readable
event output, while its knowledge tools support synchronized GitHub sources.
A narrow integration is worth evaluating before copying its full application.

Recommended: a standalone repo-analysis core with an optional DeepTutor
connector. A short technical spike should test whether existing
teaching/session components can consume commit-pinned code evidence and emit
the assessment records VIVA needs.

Do not fork the whole product before proving that its source anchors,
assessment extensibility, and update behavior fit.

## First useful release

Start with one language ecosystem and three journeys:

- Explain the application’s main request path.
- Investigate a real failure path.
- Defend and implement a small change.

The release gate is not "generated a nice tutorial." It is that explanations
resolve to the correct revision, unsupported claims are identified, exercises
are permission-controlled, and the learner's answers produce inspectable
assessment evidence.

## Shared engineering, limited coupling

Shared primitives across the three products: evidence reference, version
identity, execution events, inspectable derivation. Domain-specific records
stay separate — no giant shared platform.

A reasonable starting implementation is a local-first service,
SQLite-backed records, a filesystem artifact store, and a web interface —
here realized as a local service plus Rune UI. Keep credentials outside
model-readable workspaces and make external transmission of private
repository content explicit.

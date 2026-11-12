# DESIGN.md — viva

Design contract for the viva interface. The shared principle across VEIN,
SWITCHBOARD, and VIVA: **simple on the surface, with every claim, result, and
explanation clickable down to its evidence.**

## The main screen

One screen combines four surfaces: repository map, source viewer, tutor, and
evidence drawer. Everything cross-links:

| Click | What opens |
|---|---|
| Feature | Entry points, implementation, data access, tests |
| Function | Definition, callers, dependencies, relevant questions |
| Architecture claim | Supporting code and whether the claim is inferred or observed |
| Runtime path | Trace, environment, inputs, linked source |
| Learning objective | Prior attempts, feedback, supporting evidence, next exercise |
| Commit change | Affected concepts and explanations that may now be stale |

Target questions it must answer: "Show me how signing in actually reaches the
database. Now quiz me on where authorization happens and what breaks if this
check moves." — pointed at the specific files and revision.

## Three evidence categories — visibly distinct

- **Static evidence:** definitions, imports, references, configuration,
  schemas, tests. Always shows file:line + commit.
- **Observed evidence:** traces and test results from a recorded execution —
  inputs and environment stored with the trace.
- **Inferred explanation:** the tutor's interpretation of intent or behavior.
  Labeled as interpretation, never dressed as fact.

A static call graph is not a runtime trace. A passing test does not establish
behavior outside the case it exercised. The UI keeps these distinctions
legible at a glance (badge + label, not color alone).

## Learning modes

- **Explore** — explanations freely available; for building the map.
- **Defend** — the learner answers before seeing the solution; for proving
  understanding. Not every session is an exam.

## Learning cycle

1. Ask the learner to explain or predict before revealing the answer.
2. Retrieve the relevant source and tests.
3. Give targeted hints.
4. Ask a follow-up that changes an assumption.
5. Run a bounded exercise where execution is permitted.
6. Save demonstrated understanding and unresolved gaps.

## Mastery records

Competencies, not one opaque score: tracing a request, explaining state
transitions, locating authorization, predicting a failure, interpreting a
test, implementing a bounded change. Each assessment retains question,
response, rubric, source references, commit, hints used, verification
results. Code changes mark related evidence `needs review`.

## UI stack

Rune (`anomalyco/rune`, pinned rev in `Cargo.toml`) + `rune-kit` components.
Headless rendering (PNG + element-tree dump) is the UI verification path.

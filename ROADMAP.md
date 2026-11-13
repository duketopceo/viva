# ROADMAP.md — viva

Phased delivery per `docs/BRIEF.md`. Each phase ends at a verifiable gate;
nothing ships on a build alone.

Canonical plan: `docs/plans/2026-11-12-0000-feat-viva-v1-plan.md`.
Active slice: `docs/plans/2026-11-13-0001-feat-viva-commit-pinned-indexer-plan.md`.
Living roadmap notes: `docs/plans/2026-11-13-0000-feat-viva-full-roadmap-plan.md`.

## Phase 0 — Indexing core (plan v1, units U1–U4)

- [x] Rust workspace, embedded service
- [x] HTML audit dashboard wedge (pin commit/PR/feature → explain with anchors;
  CBM/graphify absence degrades honestly) — ahead of Rune shell
- [ ] Commit-pinned repository indexer: tree-sitter parse → symbol/reference/
  call-edge graph → SQLite records keyed to commit SHA
- [ ] Evidence model: static / observed / inferred categories with source anchors
- [ ] Working-tree fingerprint so explanations bind to the exact revision viewed
- [ ] Exact path + symbol search (text/FTS follows)

**Gate:** index a real TypeScript repo at a pinned commit; every indexed
symbol resolves to file:line + commit.

## Phase 1 — Understanding graph + explorer (units U5–U7)

- Feature/function/architecture-claim nodes with click-through to code
- Source viewer with evidence drawer (three categories, labeled)
- Commit-change impact: which concepts/explanations may be stale
- Rune repository map (upgrade from HTML wedge)

**Gate:** "how does sign-in reach the database" answers with file:line
anchors at the correct revision — not a generated summary.

## Phase 2 — Tutor + assessment (units U8–U9)

- Learning cycle: predict → retrieve → hint → follow-up → bounded exercise →
  record
- Explore / Defend modes
- Mastery records per competency; `needs review` on code change

**Gate:** a learner's answers produce inspectable assessment evidence
(question, response, rubric, refs, commit, hints, verification).

## Phase 3 — Three journeys + DeepTutor evaluation (units U10–U11)

- Journey 1: explain the main request path
- Journey 2: investigate a real failure path
- Journey 3: defend and implement a small change (bounded exercise)
- DeepTutor spike: connector only if it fits — no fork

**Release gate:** explanations resolve to the correct revision, unsupported
claims are identified, exercises are permission-controlled, and learner
answers produce inspectable assessment evidence — on all three journeys.

## Explicitly deferred

- Additional language ecosystems beyond the first (TypeScript)
- DeepTutor fork or deep integration (connector evaluated, not assumed)
- Embedding-only retrieval (exact search + syntax-aware indexing first)
- Multi-repo fleet views

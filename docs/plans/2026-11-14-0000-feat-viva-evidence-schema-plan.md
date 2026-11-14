---
title: "VIVA — Evidence + Assessment Schema Slice"
type: feat
date: 2026-11-14
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: docs/plans/2026-11-12-0000-feat-viva-v1-plan.md
execution: code
origin_plan: docs/plans/2026-11-12-0000-feat-viva-v1-plan.md
---

# VIVA — Evidence + Assessment Schema Slice

## Goal Capsule

- **Objective:** Persist three-category evidence and first-class assessment records so every claim is `static | observed | inferred` with source anchors (and observed rows require an execution ref).
- **Means:** Schema + typed APIs in `viva-db`; wire explain/audit to read/write persisted evidence without blurring categories.
- **Authority:** `docs/BRIEF.md` → origin plan U3 → this slice → `AGENTS.md` / luke-agents.
- **Stop conditions:** schema rejects category-less evidence and observed-without-execution_ref; assessment round-trips all fields; existing indexer + explain tests stay green; `cargo test --workspace` + clippy `-D warnings`.
- **Execution profile:** `ce-work` consumes units in order; test-first on schema constraints.
- **Who finishes:** implementing agent.

---

## Product Contract

Maps to origin **U3** / **R4–R6**, **R14–R16**, **KTD3**, **KTD4**, **KTD6**. Indexer output remains the only source of minting **static** rows from symbols; tutor (later) mints **inferred** only; exercises mint **observed** only.

### Requirements (slice-local)

| Slice ID | Maps to origin | Obligation |
|---|---|---|
| ES1 | R4, KTD3 | Evidence rows carry category enum + file:line + commit. |
| ES2 | R4, KTD4 | Observed evidence requires `execution_ref`; insert without it fails. |
| ES3 | R14–R16, KTD6 | Assessment rows append-only with question, response, rubric, refs, commit, hints, verification. |
| ES4 | RS-dash | Existing HTML explain still labels categories; does not invent graph anchors. |

### Scope Boundaries

**In:** `viva-db` migrations/types/APIs; tests; ROADMAP U3 evidence when done.

**Deferred:** tutor dialogue (U8), exercise sandbox (U9), FTS5 (U4 remainder), Rune UI (U6).

---

## Implementation Units

| Unit | Title | Depends |
|---|---|---|
| ES-U1 | Evidence + assessment tables + CHECK constraints | — |
| ES-U2 | Store APIs + round-trip / rejection tests | ES-U1 |
| ES-U3 | Optional explain persistence hook (static from index) | ES-U2 |

### ES-U1. Evidence + assessment tables

- **Goal:** Schema enforces category and observed execution_ref.
- **Files:** `crates/viva-db/src/lib.rs` (or `schema/`).
- **Approach:** Tables as origin U3; `category` constrained to `static|observed|inferred`; CHECK that `observed` implies non-null `execution_ref`.
- **Verification:** `cargo test -p viva-db`.

### ES-U2. Store APIs + tests

- **Goal:** Insert/list APIs; rejection tests for ES1–ES3.
- **Test scenarios:** no category → fail; observed without execution_ref → fail; assessment round-trip; static row from indexer-shaped payload.
- **Verification:** `cargo test -p viva-db`.

### ES-U3. Explain persistence hook (optional thin)

- **Goal:** When explain attaches indexed symbol anchors, optionally persist matching **static** evidence rows keyed to target + commit (idempotent).
- **Verification:** `cargo test -p viva-core`; degrade honesty unchanged.

---

## Verification Contract

| Gate | Command |
|---|---|
| Test | `cargo test --workspace` |
| Lint | `cargo clippy --workspace -- -D warnings` |
| Schema | `cargo test -p viva-db` |

## Definition of Done

- ES-U1–U2 complete (ES-U3 if cheap); ROADMAP U3 checked with PR evidence.
- Categories never blurred; observed without execution_ref impossible at schema/API layer.

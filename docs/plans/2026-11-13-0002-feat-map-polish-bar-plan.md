---
title: "VIVA — Map polish bar (Floorplan-style audit dashboard)"
type: feat
date: 2026-11-13
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: docs/plans/2026-11-12-0000-feat-viva-v1-plan.md
execution: code
origin_plan: docs/plans/2026-11-12-0000-feat-viva-v1-plan.md
---

# VIVA — Map polish bar (Floorplan-style audit dashboard)

## Goal Capsule

- **Objective:** The HTML audit wedge teaches in plain English via three panes — Ask → How it fits → Proof — with polish-bar tokens, human-readable map labels, and explain copy that never leads with jargon paths or variable names.
- **Means:** Extend `crates/viva/src/main.rs` dashboard (CSS + JS + layout); plain-English `Auditor::explain` strings in `viva-core`; document bar in `DESIGN.md` / `AGENTS.md`.
- **Authority:** `docs/BRIEF.md` → origin plan U6 visualization bar → `DESIGN.md` → this slice.
- **Stop conditions:** Dashboard HTML exposes labeled panes `1 · Ask`, `2 · How it fits`, `3 · Proof`; seeded questions use learner-facing labels; map API returns tree with human titles client-side; explain uses feature_id for index lookup when label differs; `cargo test --workspace` green; `cargo clippy --workspace -- -D warnings` clean.
- **Execution profile:** Single PR on `feat/map-polish-bar`; verify with workspace tests including dashboard string smoke test.
- **Who finishes:** implementing agent.

---

## Product Contract

Partial delivery of origin **U6** (repository map + teaching layout) on the **HTML wedge** ahead of Rune. Does not ship Rune UI or tutor cycle.

### Requirements

| Slice ID | Obligation |
|---|---|
| MAP-R1 | Three-column teaching layout: Ask (questions list + custom ask), How it fits (lesson + collapsible repo map), Proof (evidence drawer). |
| MAP-R2 | Primary copy plain English; paths and revision hashes secondary; evidence category badges human-labeled. |
| MAP-R3 | Theme toggle (light/dark) using CSS variables consistent with polish-bar tokens. |
| MAP-R4 | `explain` resolves symbols by `feature_id` when present; plain-English audit section bodies. |

### Scope Boundaries

**In this PR:** `crates/viva`, `crates/viva-core` explain copy + lookup, `DESIGN.md`, `AGENTS.md`.

**Deferred:** Rune shell (full U6), headless PNG element-tree dump, tutor/mastery (U8).

---

## Planning Contract

### Key Technical Decisions

- KTD1. **HTML wedge stays authoritative** for Phase-0 map UX until Rune lands. (session-settled: user-directed — matches prior indexer slice KTD4)
- KTD2. **Floorplan-style copy** — human names first; `FEATURE` / raw stems not primary labels. (session-settled: user-directed — AGENTS.md)
- KTD3. **Repo map remains filesystem tree** from `/api/map`; humanization is client-side to avoid breaking anchors.

### Units

| Unit | Work | Verification |
|---|---|---|
| MAP-U1 | Dashboard layout + tokens + three panes | HTML contains pane labels; smoke test |
| MAP-U2 | Lesson/map JS (ROOM, LESSON, humanize) | Manual smoke; tests unchanged green |
| MAP-U3 | Core explain plain English + feature_id lookup | Existing `viva-core` tests + workspace green |
| MAP-U4 | Docs (`DESIGN.md`, `AGENTS.md`) | Doc review |

### Verification

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

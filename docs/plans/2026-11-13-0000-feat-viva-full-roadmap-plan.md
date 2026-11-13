---
title: VIVA — Full Roadmap Execution Plan
type: feat
date: 2026-11-13
artifact_contract: ce-unified-plan/v1
product_contract_source: docs/BRIEF.md
execution: code
origin_plan: docs/plans/2026-11-12-0000-feat-viva-v1-plan.md
---

# VIVA — Full Roadmap Execution Plan

## Goal Capsule

- **Objective:** A developer can point VIVA at a repository, ask how a path actually reaches the database, get answers anchored to the exact files and commit, be quizzed until they can defend the behavior, and leave with inspectable mastery records.
- **Means:** Commit-pinned indexer (tree-sitter → SQLite), tutor service, HTML audit wedge then Rune UI — three evidence categories (static / observed / inferred).
- **Authority:** `docs/BRIEF.md` → `docs/plans/2026-11-12-0000-feat-viva-v1-plan.md` → this roadmap → active slice plans under `docs/plans/`.
- **Stop conditions:** origin release gate on three journeys (request path, failure path, defend-and-change).

## Progress baseline (2026-11-13)

| Area | Status |
|---|---|
| Workspace crates `viva-db` / `viva-core` / CLI | Done (scaffold) |
| HTML audit dashboard + pin/explain + CBM/graphify degrade | Done (Phase 0 wedge ahead of Rune) |
| Commit-pinned tree-sitter indexer | In progress — see `docs/plans/2026-11-13-0001-feat-viva-commit-pinned-indexer-plan.md` |
| Evidence/assessment schema, understanding graph, tutor, journeys | Not started |

## Product Contract

Unchanged from origin plan and BRIEF. VIVA understands a particular revision of executable software. DeepTutor is evaluated (connector), not forked. HTML audit dashboard is an early Phase 0 surface; Rune remains the planned explorer UI.

## Sequencing (mirrors origin U1–U11)

```text
U1   Workspace + service skeleton (+ HTML wedge landed)
U2   Commit-pinned indexer (tree-sitter)          ← active slice
U3   Evidence + assessment schema
U4   Exact search + symbol/reference queries
U5   Understanding graph builder
U6   Repository map + source viewer (Rune)
U7   Evidence drawer + staleness
U8   Tutor dialogue + OpenRouter spike
U9   Learning cycle + exercise sandbox + mastery
U10  DeepTutor evaluation spike
U11  Three journeys + release gate
```

## Definition of Done

Origin U11 release gate green. Explanations resolve to the correct revision; unsupported claims identified; exercises permission-controlled; mastery records inspectable.

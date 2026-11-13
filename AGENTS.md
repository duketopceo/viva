# AGENTS.md — viva

> This file is the agent entry point for this repo.
> Full agent context lives at: https://github.com/duketopceo/luke-agents

## What This Repo Does

viva helps a developer understand and defend the software they are building. It indexes a repository at a specific commit, builds a clickable understanding graph (features → functions → architecture claims → runtime paths → learning objectives), and runs a tutor that quizzes the learner — with every explanation resolving to files and revision, and three visibly distinct evidence categories: static, observed, inferred.

## Key Files

- `Cargo.toml` — workspace: `viva-db`, `viva-core`, `viva` (local audit dashboard)
- `docs/BRIEF.md` — product brief: evidence categories, learning cycle, mastery records, release gate
- `docs/plans/` — ce-plan implementation plans (Tallyline-imported)
- `ROADMAP.md` — phased delivery
- `DESIGN.md` — interaction contract

## Current Status

- [x] Repo initialized — scaffold + plan
- [ ] In development
- [ ] Deployed
- [ ] Production traffic

## Active Issues / Known State

- Plan: `docs/plans/2026-11-12-0000-feat-viva-v1-plan.md`
- Rune pinned at `anomalyco/rune@24bd7bbb` (dev rev, pre-release — re-pin deliberately, never float)
- DeepTutor overlap is substantial — plan unit U10 is an evaluation spike, not a fork
- First ecosystem: TypeScript (tree-sitter); other languages come after the slice proves out

## Agent Instructions (repo-specific)

- Every explanation, claim, and assessment must resolve to a file + commit. A polished diagram without source anchors is a defect.
- Three evidence categories stay visibly distinct: static (code/tests/config), observed (traces/test runs with recorded inputs+env), inferred (tutor interpretation). Never blur them.
- Passing tests contribute evidence; they do not prove understanding by themselves.
- On code change, related learning evidence is marked `needs review` — never auto-reset, never leave stale mastery labels untouched.
- Explore mode gives explanations freely; Defend mode requires the learner's answer first. Both exist; neither is mandatory.
- Default: follow duketopceo/luke-agents for all standards.

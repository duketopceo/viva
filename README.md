<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/logo-dark.svg">
    <img src="assets/brand/logo.svg" alt="viva" width="220">
  </picture>
</p>

# viva

> Understand and defend the software you're building — commit-grounded explanations of a specific repository revision, an interactive tutor that quizzes you, and mastery records that prove understanding rather than consumption.

## Stack

- Rust workspace — one local binary: indexer + tutor service + UI
- [Rune](https://github.com/anomalyco/rune) (GPU-rendered UI, pinned dev rev) + `rune-kit` components
- SQLite for index/graph/mastery records; filesystem artifact store for snapshots and traces
- tree-sitter for syntax-aware indexing; OpenRouter for tutor explanations (labeled inferred, never evidence)

## Local Setup

```bash
# clone
git clone https://github.com/duketopceo/viva
cd viva

# toolchain: see rust-toolchain.toml
rustup show

# build + Phase 0 indexer gate
cargo test --workspace
cargo run -p viva -- --help
# index a TypeScript git repo, then exact symbol search:
# cargo run -p viva -- --db viva.db index --repo /path/to/ts-repo --commit HEAD
# cargo run -p viva -- --db viva.db search signIn --repo /path/to/ts-repo --commit HEAD
# HTML audit dashboard (default):
# cargo run -p viva -- serve --repo /path/to/ts-repo --port 8787
```

Tutor / OpenRouter env vars are not required for the indexer gate (Phase 0).

## Deploy

Local-first desktop app. Indexes repositories on your machine; optional DeepTutor connector is evaluated in plan unit U10 — no fork of DeepTutor.

## Docs

- `docs/BRIEF.md` — product and architecture brief (source of truth for scope)
- `docs/plans/` — ce-plan implementation plans
- `ROADMAP.md` — phased delivery and release gates
- `DESIGN.md` — interaction contract (three evidence categories, Explore/Defend modes)
- See `AGENTS.md` for agent context.

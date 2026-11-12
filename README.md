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

# toolchain: Rust 1.99.1
rustup show

# env vars
cp .env.example .env
# fill in .env (OPENROUTER_API_KEY at minimum)

# run — no crates yet; first runnable slice lands with plan unit U1
cargo build --workspace
```

## Deploy

Local-first desktop app. Indexes repositories on your machine; optional DeepTutor connector is evaluated in plan unit U10 — no fork of DeepTutor.

## Docs

- `docs/BRIEF.md` — product and architecture brief (source of truth for scope)
- `docs/plans/` — ce-plan implementation plans
- `ROADMAP.md` — phased delivery and release gates
- `DESIGN.md` — interaction contract (three evidence categories, Explore/Defend modes)
- See `AGENTS.md` for agent context.

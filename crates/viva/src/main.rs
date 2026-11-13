//! Viva CLI — index, search, and local audit dashboard.

use axum::{
    Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Json},
    routing::{get, post},
};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use viva_core::{Auditor, short_revision};
use viva_db::{Store, TargetKind};
use viva_index::{IndexOptions, index_commit};

#[derive(Parser)]
#[command(name = "viva", about = "Understand and defend your own code")]
struct Cli {
    #[arg(long, default_value = "viva.db", global = true)]
    db: String,
    #[command(subcommand)]
    cmd: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the HTML audit dashboard (default when no subcommand).
    Serve {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value_t = 8787)]
        port: u16,
    },
    /// Index a git repository at a pinned commit.
    Index {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = "HEAD")]
        commit: String,
        #[arg(long)]
        fingerprint_dirty: bool,
    },
    /// Exact symbol-name search over indexed commits.
    Search {
        name: String,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        commit: Option<String>,
    },
}

#[derive(Clone)]
struct AppState {
    auditor: Arc<Auditor>,
}

#[derive(Debug, Serialize)]
struct MapNode {
    path: String,
    kind: String,
    children: Vec<MapNode>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    incomplete: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd.unwrap_or(Command::Serve {
        repo: PathBuf::from("."),
        port: 8787,
    }) {
        Command::Serve { repo, port } => serve(&cli.db, repo, port).await,
        Command::Index {
            repo,
            commit,
            fingerprint_dirty,
        } => {
            let store = Store::open_path(&cli.db)?;
            let summary = index_commit(
                &store,
                &IndexOptions {
                    repo_root: repo,
                    commit_sha: commit,
                    fingerprint_dirty,
                },
            )?;
            println!(
                "indexed {} symbols, {} refs, {} edges at {} (run {})",
                summary.symbols,
                summary.references,
                summary.edges,
                summary.commit_sha,
                summary.run_id
            );
            if let Some(fp) = summary.working_tree_fingerprint {
                println!("working_tree_fingerprint={fp}");
            }
            Ok(())
        }
        Command::Search { name, repo, commit } => {
            let store = Store::open_path(&cli.db)?;
            let commit_spec = commit.unwrap_or_else(|| "HEAD".into());
            let commit = resolve_repo_commit(&repo, &commit_spec)?;
            let hits = store.find_symbols(&commit, &name)?;
            if hits.is_empty() {
                println!("no symbols named `{name}` at commit {commit}");
            }
            for h in hits {
                println!(
                    "{}:{}-{}@{} ({})",
                    h.file_path,
                    h.start_line,
                    h.end_line,
                    short_revision(&h.commit_sha),
                    h.kind
                );
            }
            Ok(())
        }
    }
}

fn resolve_repo_commit(repo: &PathBuf, spec: &str) -> anyhow::Result<String> {
    let repo = git2::Repository::open(repo)?;
    if spec.eq_ignore_ascii_case("HEAD") {
        return Ok(repo.head()?.peel_to_commit()?.id().to_string());
    }
    Ok(repo
        .revparse_single(spec)?
        .peel_to_commit()?
        .id()
        .to_string())
}

fn repo_display_name(root: &FsPath) -> String {
    let resolved = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    resolved
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo")
        .to_ascii_lowercase()
}

fn seed_repo_targets(auditor: &Auditor) -> anyhow::Result<()> {
    if !auditor.list_targets()?.is_empty() {
        return Ok(());
    }
    let name = repo_display_name(&auditor.repo_root);
    // Questions the learner can ask; search keys stay path-friendly.
    let seeds: &[(&str, &str)] = if name.contains("omaseal") {
        &[
            ("How are secrets stored and handed out?", "engine"),
            ("How do AI assistants get a secret?", "mcp"),
            ("Where does the password vault live?", "keyring"),
            ("What happens when a secret is copied?", "clipboard"),
            ("How do outside services get credentials?", "providers"),
            ("When does the app need your admin password?", "sudo"),
            ("What does the settings panel show?", "Panel"),
            ("Where do you see and edit secrets?", "SecretsView"),
        ]
    } else {
        &[("What is this repository for?", "")]
    };
    let commit = resolve_repo_commit(&auditor.repo_root, "HEAD").unwrap_or_else(|_| "HEAD".into());
    for (label, hint) in seeds {
        let feature_id = if hint.is_empty() { None } else { Some(*hint) };
        let _ = auditor.register_target(
            TargetKind::Feature,
            label,
            &commit,
            Some("**/*"),
            None,
            feature_id,
            false,
        )?;
    }
    Ok(())
}

fn build_map(root: &FsPath, max_depth: usize) -> MapNode {
    fn walk(dir: &FsPath, root: &FsPath, depth: usize, max_depth: usize) -> (Vec<MapNode>, bool) {
        if depth > max_depth {
            return (Vec::new(), false);
        }
        let read = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return (Vec::new(), true),
        };
        let mut entries = Vec::new();
        let mut incomplete = false;
        for entry in read {
            match entry {
                Ok(entry) => entries.push(entry),
                Err(_) => incomplete = true,
            }
        }
        entries.sort_by_key(|e| e.file_name());
        let mut out = Vec::new();
        for entry in entries {
            let name = entry.file_name();
            let name_s = name.to_string_lossy();
            if name_s.starts_with('.')
                || name_s == "node_modules"
                || name_s == "target"
                || name_s == "vendor"
            {
                continue;
            }
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            if path.is_dir() {
                let (children, child_incomplete) = walk(&path, root, depth + 1, max_depth);
                incomplete |= child_incomplete;
                if !children.is_empty() || depth < 2 || child_incomplete {
                    out.push(MapNode {
                        path: rel,
                        kind: "dir".into(),
                        children,
                        incomplete: child_incomplete,
                    });
                }
            } else {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(
                    ext,
                    "go" | "qml" | "ts" | "tsx" | "rs" | "py" | "md" | "json"
                ) {
                    out.push(MapNode {
                        path: rel,
                        kind: "file".into(),
                        children: Vec::new(),
                        incomplete: false,
                    });
                }
            }
        }
        (out, incomplete)
    }
    let (children, incomplete) = walk(root, root, 0, max_depth);
    MapNode {
        path: root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(".")
            .to_string(),
        kind: "dir".into(),
        children,
        incomplete,
    }
}

async fn serve(db: &str, repo: PathBuf, port: u16) -> anyhow::Result<()> {
    let store = Store::open_path(db)?;
    let auditor = Arc::new(Auditor::new(store, repo));
    seed_repo_targets(&auditor)?;
    let state = AppState { auditor };
    let app = Router::new()
        .route("/", get(index))
        .route("/api/targets", get(list_targets))
        .route("/api/targets", post(add_target))
        .route("/api/targets/{id}/explain", get(explain))
        .route("/api/map", get(repo_map))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("viva dashboard http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn dashboard_html(repo_root: &FsPath) -> String {
    let name = repo_display_name(repo_root);
    let literal = serde_json::to_string(&name).unwrap_or_else(|_| "\"repo\"".to_string());
    DASHBOARD_HTML.replace("\"__VIVA_REPO_NAME__\"", &literal)
}

async fn index(State(state): State<AppState>) -> Html<String> {
    Html(dashboard_html(&state.auditor.repo_root))
}

async fn list_targets(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.auditor.list_targets().unwrap_or_default())
}

async fn repo_map(State(state): State<AppState>) -> impl IntoResponse {
    let map = build_map(&state.auditor.repo_root, 3);
    Json(map)
}

#[derive(Deserialize)]
struct AddBody {
    label: String,
    commit_sha: String,
    #[serde(default)]
    kind: String,
    feature_id: Option<String>,
    pr_number: Option<u64>,
}

async fn add_target(State(state): State<AppState>, Json(body): Json<AddBody>) -> impl IntoResponse {
    let kind = match body.kind.as_str() {
        "pull_request" | "pr" => TargetKind::PullRequest,
        "commit" => TargetKind::Commit,
        _ => TargetKind::Feature,
    };
    match state.auditor.register_target(
        kind,
        &body.label,
        &body.commit_sha,
        None,
        body.pr_number,
        body.feature_id.as_deref(),
        false,
    ) {
        Ok(t) => Json(serde_json::json!({"ok": true, "id": t.id})).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn explain(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match state.auditor.explain(&id) {
        Ok(exp) => Json(exp).into_response(),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

const DASHBOARD_HTML: &str = r###"<!DOCTYPE html>
<html lang="en" data-theme="dark">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>Viva — map</title>
<style>
  /* Tokens mirror anomalyco/rune tok::Theme + accent(blue/green) — views use vars only. */
  :root, [data-theme="dark"] {
    --bg: oklch(0.185 0.004 265);
    --card: oklch(0.232 0.005 265);
    --card-border: oklch(1 0 0 / 0.05);
    --text: oklch(0.955 0.003 265);
    --text-secondary: oklch(0.72 0.008 265);
    --separator: oklch(1 0 0 / 0.055);
    --hover: oklch(1 0 0 / 0.06);
    --selected: oklch(0.66 0.17 256 / 0.16);
    --accent: oklch(0.66 0.17 256);
    --evidence-static: oklch(0.70 0.16 152);
    --evidence-observed: oklch(0.66 0.17 256);
    --evidence-inferred: oklch(0.74 0.14 58);
    --control: oklch(1 0 0 / 0.05);
    --radius-sm: 6px;
    --radius-md: 8px;
    --radius-lg: 12px;
    --space-1: 4px;
    --space-2: 8px;
    --space-3: 12px;
    --space-4: 16px;
    --space-6: 24px;
    --type-body: 13px;
    --type-label: 12px;
    --type-section: 11.5px;
    --type-title: 13px;
    --type-mono: 12px;
    --weight-body: 400;
    --weight-label: 500;
    --weight-title: 600;
    --font-sans: "IBM Plex Sans", "Segoe UI", ui-sans-serif, system-ui, sans-serif;
    --font-mono: "IBM Plex Mono", ui-monospace, "SFMono-Regular", monospace;
    --motion-snappy: 0.25s;
    --board-glow: oklch(0.66 0.17 256 / 0.08);
  }
  [data-theme="light"] {
    --bg: oklch(0.958 0.004 265);
    --card: oklch(1 0 0);
    --card-border: oklch(0.2 0.01 265 / 0.1);
    --text: oklch(0.225 0.006 265);
    --text-secondary: oklch(0.5 0.01 265);
    --separator: oklch(0.2 0.01 265 / 0.13);
    --hover: oklch(0.2 0.01 265 / 0.07);
    --selected: oklch(0.6 0.17 256 / 0.12);
    --accent: oklch(0.6 0.17 256);
    --evidence-static: oklch(0.55 0.14 152);
    --evidence-observed: oklch(0.55 0.16 256);
    --evidence-inferred: oklch(0.62 0.14 58);
    --control: oklch(0.2 0.01 265 / 0.06);
    --board-glow: oklch(0.6 0.17 256 / 0.06);
  }
  * { box-sizing: border-box; }
  html, body { height: 100%; }
  body {
    margin: 0;
    font-family: var(--font-sans);
    font-size: var(--type-body);
    font-weight: var(--weight-body);
    line-height: 1.45;
    color: var(--text);
    background:
      radial-gradient(1200px 600px at 70% -10%, var(--board-glow), transparent 55%),
      var(--bg);
    display: grid;
    grid-template-columns: 300px minmax(0, 1fr) 340px;
    grid-template-rows: auto 1fr;
  }
  .app-bar {
    grid-column: 1 / -1;
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--separator);
    background: var(--card);
  }
  .app-bar .product {
    font-size: var(--type-title);
    font-weight: var(--weight-title);
  }
  .app-bar .flow {
    color: var(--text-secondary);
    font-size: var(--type-label);
  }
  .app-bar .flow strong { color: var(--text); font-weight: var(--weight-label); }
  .pane-job {
    margin: 0 0 var(--space-3);
    color: var(--text-secondary);
    font-size: var(--type-label);
  }
  .lesson {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .lesson h2 {
    margin: 0;
    font-size: 18px;
    font-weight: var(--weight-title);
  }
  .lesson .story {
    margin: 0;
    font-size: 15px;
    line-height: 1.55;
  }
  .lesson .notice {
    margin: 0;
    padding: var(--space-3);
    border-left: 3px solid var(--accent);
    background: var(--control);
    border-radius: 0 var(--radius-md) var(--radius-md) 0;
    color: var(--text);
  }
  .lesson .next {
    margin: 0;
    color: var(--text-secondary);
    font-size: var(--type-label);
  }
  .map-secondary {
    margin-top: var(--space-4);
    padding-top: var(--space-3);
    border-top: 1px solid var(--separator);
  }
  .map-secondary summary {
    cursor: pointer;
    color: var(--text-secondary);
    font-size: var(--type-label);
    font-weight: var(--weight-label);
  }
  @media (prefers-reduced-motion: reduce) {
    * { transition: none !important; animation: none !important; }
  }
  .pane {
    border-right: 1px solid var(--separator);
    overflow: auto;
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
  }
  .pane:last-child { border-right: none; }
  .section-label {
    margin: 0;
    font-size: var(--type-section);
    font-weight: var(--weight-title);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-secondary);
  }
  .chrome {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .btn {
    appearance: none;
    font: inherit;
    font-size: var(--type-label);
    font-weight: var(--weight-label);
    color: var(--text);
    background: var(--control);
    border: 1px solid var(--card-border);
    border-radius: var(--radius-sm);
    padding: var(--space-2) var(--space-3);
    cursor: pointer;
  }
  .btn-primary {
    background: var(--accent);
    color: oklch(0.98 0 0);
    border-color: transparent;
    width: 100%;
  }
  .btn-ghost { background: transparent; }
  .field {
    width: 100%;
    font: inherit;
    font-size: var(--type-label);
    color: var(--text);
    background: var(--control);
    border: 1px solid var(--card-border);
    border-radius: var(--radius-sm);
    padding: var(--space-2) var(--space-3);
  }
  .field::placeholder { color: var(--text-secondary); }
  .stack { display: flex; flex-direction: column; gap: var(--space-2); }
  .list { display: flex; flex-direction: column; gap: 2px; flex: 1; min-height: 0; }
  .row {
    display: block;
    width: 100%;
    text-align: left;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    border: 1px solid transparent;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .row:hover { background: var(--hover); }
  .row[aria-selected="true"] {
    background: var(--selected);
    border-color: var(--card-border);
  }
  .row .meta {
    font-size: var(--type-section);
    font-weight: var(--weight-label);
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .row .title { font-size: var(--type-title); font-weight: var(--weight-title); }
  .hint { margin: 0; color: var(--text-secondary); font-size: var(--type-label); }
  .empty {
    margin: var(--space-6) 0;
    padding: var(--space-4);
    border: 1px dashed var(--separator);
    border-radius: var(--radius-lg);
    color: var(--text-secondary);
    font-size: var(--type-label);
  }
  .board {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(148px, 1fr));
    gap: var(--space-3);
  }
  .node-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    align-items: flex-start;
    text-align: left;
    padding: var(--space-3);
    min-height: 88px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--card-border);
    background: var(--card);
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .node-card:hover { background: var(--hover); }
  .node-card[aria-selected="true"] {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .node-card .name {
    font-size: var(--type-title);
    font-weight: var(--weight-title);
  }
  .node-card .blurb {
    color: var(--text-secondary);
    font-size: var(--type-label);
  }
  .node-card .stat {
    margin-top: auto;
    font-size: var(--type-label);
    color: var(--text-secondary);
  }
  .tree {
    font-family: var(--font-mono);
    font-size: var(--type-mono);
    margin-top: var(--space-2);
  }
  .tree ul {
    list-style: none;
    margin: 0;
    padding-left: var(--space-3);
    border-left: 1px solid var(--separator);
  }
  .tree > ul { padding-left: 0; border: none; }
  .tree-node {
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .tree-node:hover { background: var(--hover); }
  .tree-node[aria-selected="true"] { background: var(--selected); }
  .tree-node .place { color: var(--evidence-static); font-weight: var(--weight-label); }
  .tree-node .path {
    color: var(--text-secondary);
    font-size: var(--type-section);
  }
  .badge {
    display: inline-block;
    font-size: var(--type-section);
    font-weight: var(--weight-label);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    border: 1px solid currentColor;
    border-radius: 3px;
    padding: 1px 6px;
    margin-right: var(--space-2);
  }
  .static { color: var(--evidence-static); }
  .observed { color: var(--evidence-observed); }
  .inferred { color: var(--evidence-inferred); }
  .unsupported { opacity: 0.7; }
  .anchor {
    font-family: var(--font-mono);
    font-size: var(--type-mono);
    color: var(--accent);
  }
  .evidence-block { margin-bottom: var(--space-3); }
  .evidence-title {
    margin: var(--space-3) 0 var(--space-1);
    font-size: var(--type-title);
    font-weight: var(--weight-title);
  }
  .evidence-body { margin: 0 0 var(--space-2); color: var(--text-secondary); }
  .form-gap { margin-top: auto; padding-top: var(--space-3); border-top: 1px solid var(--separator); }
</style>
</head>
<body>
<header class="app-bar">
  <div class="product">Viva · understand this revision</div>
  <div class="flow"><strong>Ask</strong> what you want to know → <strong>see how it fits</strong> → <strong>check the proof</strong> in the code</div>
  <button type="button" class="btn btn-ghost" id="themeToggle" aria-label="Toggle light and dark">Appearance</button>
</header>
<aside class="pane" aria-label="Ask">
  <h1 class="section-label">1 · Ask</h1>
  <p class="pane-job">Left bar: pick a question. This is what you are trying to learn — not a file list.</p>
  <div id="list" class="list" role="listbox" aria-label="Questions"></div>
  <div class="form-gap stack">
    <input class="field" id="label" placeholder="Ask in your own words…" autocomplete="off"/>
    <input class="field" id="sha" placeholder="Revision (optional)" value="HEAD" autocomplete="off"/>
    <button type="button" class="btn btn-primary" id="add">Ask this</button>
  </div>
</aside>
<main class="pane" aria-label="How it fits">
  <h1 class="section-label">2 · How it fits</h1>
  <p class="pane-job">Center: the plain-English story for your question — what the part does and how it connects.</p>
  <div id="lesson" class="lesson">
    <div class="empty" id="lessonEmpty">Choose a question on the left. You will get a short explanation here before any file list.</div>
  </div>
  <details class="map-secondary" id="mapDetails">
    <summary>Where this lives in the repository</summary>
    <p class="hint" id="repoHint" style="margin-top:var(--space-2)">Loading…</p>
    <div id="board" class="board" aria-label="Related rooms"></div>
    <div id="overview" class="tree" aria-label="Related places"></div>
  </details>
</main>
<section class="pane" aria-label="Proof">
  <h1 class="section-label">3 · Proof</h1>
  <p class="pane-job">Right bar: why the story is trustworthy. Every claim should point at real code on this revision — or say it is only an interpretation.</p>
  <div id="drawer"><div class="empty" id="drawerEmpty">Nothing selected yet. After you ask, proof from the code shows up here.</div></div>
</section>
<script>
const THEME_KEY = 'viva.theme';
const REPO_NAME = "__VIVA_REPO_NAME__";
const PREPARED = String(REPO_NAME).toLowerCase().includes('omaseal');
const ROOM = {
  engine: { title: 'Secret engine', blurb: 'Stores secrets, checks who may read them, and hands them out.' },
  docs: { title: 'Guides and plans', blurb: 'Written notes about how the product is meant to work.' },
  packaging: { title: 'Install packages', blurb: 'How the app is packaged so people can install it.' },
  scripts: { title: 'Helper scripts', blurb: 'Small commands used while building and checking.' },
  spike: { title: 'Experiments', blurb: 'Try-outs that are not the main product yet.' },
  assets: { title: 'Brand assets', blurb: 'Images and brand files shown in the product.' },
  contrib: { title: 'Extra checks', blurb: 'Side tests and helpers that sit beside the main code.' }
};
const LESSON = {
  engine: {
    title: 'How secrets are stored and handed out',
    story: 'OmaSeal keeps secrets in a local vault. The secret engine is the heart of that: it decides what can be saved, who may read it, and how a secret is returned without leaking it into logs or casual clipboard use.',
    notice: 'Notice who is allowed to read, and what happens when permission is missing. That is usually more important than the storage format.',
    next: 'Next: ask how an AI assistant is allowed to fetch a secret — that is a different door into the same vault.'
  },
  mcp: {
    title: 'How AI assistants get a secret',
    story: 'Agents do not dig through your files. They ask through a controlled connection. That connection must follow the same permission rules as a person using the panel — or the vault is pointless.',
    notice: 'Ask yourself: if the assistant is blocked, does the refusal leave a clear trail?',
    next: 'Next: look at where the vault itself lives, so you can see what the assistant was talking to.'
  },
  keyring: {
    title: 'Where the password vault lives',
    story: 'The password store is the durable place secrets sit when the machine is locked or unlocked. The engine talks to it; the screens only show what the store will allow.',
    notice: 'If the store is empty or locked, every other feature fails closed. That is intentional.',
    next: 'Next: follow what happens when someone copies a secret — the dangerous path out of the vault.'
  },
  clipboard: {
    title: 'What happens when a secret is copied',
    story: 'Copying is a deliberate exit from the vault. The helper should make that rare, short-lived, and obvious — not a silent always-on paste buffer.',
    notice: 'Check whether the copy clears itself, and whether the UI warns you.',
    next: 'Next: outside services that need credentials — another exit path with different rules.'
  },
  providers: {
    title: 'How outside services get credentials',
    story: 'Providers are named slots for services you use (models, tools, hosts). Resolving a provider should pull the right secret under the same gates as a manual lookup.',
    notice: 'A wrong provider name should fail clearly — never invent a credential.',
    next: 'Next: when the app needs your admin password for a privileged action.'
  },
  sudo: {
    title: 'When the app needs your admin password',
    story: 'Some actions are dangerous enough to require a fresh admin check. The helper is the gate for those actions — not a general free pass.',
    notice: 'Ask what is gated, how often, and what is logged when it fails.',
    next: 'Next: the settings panel — how a person walks these controls without reading the engine.'
  },
  Panel: {
    title: 'What the settings panel shows',
    story: 'The settings panel is the human map of the vault: status, jobs, and the controls you reach without a terminal. It should explain state, not dump internals.',
    notice: 'If a control is visible, you should be able to say what it changes and how to undo it.',
    next: 'Next: the secrets screen — where individual secrets are listed and edited.'
  },
  SecretsView: {
    title: 'Where you see and edit secrets',
    story: 'This screen is the day-to-day list: names, add/edit, and revealing a value only when you ask. It sits on top of the engine and store; it should never be the only security layer.',
    notice: 'Watch what is shown by default versus what requires an extra confirm.',
    next: 'Go back to the secret engine and restate the path from this screen to storage in your own words.'
  }
};
const CAT_LABEL = {
  static: 'From the code',
  observed: 'From a run',
  inferred: 'Interpretation'
};
function esc(s){ const d=document.createElement('div'); d.textContent=s==null?'':String(s); return d.innerHTML; }
function countFiles(n){
  if (!n) return 0;
  if (n.kind === 'file') return 1;
  return (n.children||[]).reduce((a,c)=>a+countFiles(c),0);
}
function leafName(path){
  if (!path) return '';
  const parts = String(path).split('/');
  return parts[parts.length-1] || path;
}
function humanize(id){
  let s = String(id||'')
    .replace(/\.[a-z0-9]+$/i, '')
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')
    .replace(/[_\-.\s]+/g, ' ')
    .trim()
    .toLowerCase();
  s = s.replace(/\b(api|ui|mcp|cli|db|jwt|oauth|sso|http|json|qml)\b/g, (m) => m.toUpperCase());
  return s ? s.charAt(0).toUpperCase() + s.slice(1) : String(id||'');
}
function preparedRoom(leaf){
  return PREPARED && ROOM[leaf] ? ROOM[leaf] : null;
}
function placeTitle(path, kind){
  const leaf = leafName(path);
  const room = kind === 'dir' ? preparedRoom(leaf) : null;
  if (room) return room.title;
  if (kind === 'file') {
    if (/^lib\./i.test(leaf)) return 'Library entry point';
    if (/^main\./i.test(leaf)) return 'Program entry point';
    const isTest = /\.(test|spec)\./i.test(leaf) || /_test\./i.test(leaf) || /^test_/.test(leaf);
    const name = humanize(leaf.replace(/_test(?=\.)/, '').replace(/^test_/, ''));
    return isTest ? name + ' checks' : name;
  }
  return humanize(leaf || path || 'Repository');
}
function placeBlurb(path){
  const room = preparedRoom(leafName(path));
  return room ? room.blurb : '';
}
function topicForPath(path){
  const leaf = leafName(path);
  const room = preparedRoom(leaf);
  if (room) return { label: room.title, feature_id: leaf };
  return { label: placeTitle(path, 'dir'), feature_id: path };
}
function applyTheme(theme){
  document.documentElement.dataset.theme = theme;
  localStorage.setItem(THEME_KEY, theme);
}
function initTheme(){
  const saved = localStorage.getItem(THEME_KEY);
  const pref = window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
  applyTheme(saved === 'light' || saved === 'dark' ? saved : pref);
}
function renderTree(n, depth, focusPath){
  const li = document.createElement('li');
  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className = 'tree-node';
  btn.dataset.path = n.path || '';
  btn.setAttribute('aria-selected', focusPath && n.path === focusPath ? 'true' : 'false');
  const title = placeTitle(n.path, n.kind);
  const label = document.createElement('span');
  if (n.kind === 'dir') label.className = 'place';
  label.textContent = title;
  btn.appendChild(label);
  const leaf = leafName(n.path);
  const secondary = n.path && n.path !== title ? n.path : (leaf && leaf !== title ? leaf : '');
  if (secondary) {
    const note = document.createElement('span');
    note.className = 'path';
    note.textContent = secondary;
    btn.appendChild(note);
  }
  li.appendChild(btn);
  if (n.children && n.children.length && depth < 3){
    const ul = document.createElement('ul');
    n.children.forEach(c => ul.appendChild(renderTree(c, depth + 1, focusPath)));
    li.appendChild(ul);
  }
  return li;
}
let mapRoot = null;
let focusPath = '';
function renderLesson(featureId, question){
  const box = document.getElementById('lesson');
  const L = PREPARED ? LESSON[featureId] : null;
  if (!L) {
    box.innerHTML = `<h2>${esc(question || 'Your question')}</h2>
      <p class="story">We do not have a prepared explanation for this yet. Use the proof on the right — files that match your words — and say the story back in plain language.</p>
      <p class="next">Tip: start from a prepared question on the left until the map grows richer.</p>`;
    return;
  }
  box.innerHTML = `<h2>${esc(L.title)}</h2>
    <p class="story">${esc(L.story)}</p>
    <p class="notice"><strong>What to notice:</strong> ${esc(L.notice)}</p>
    <p class="next">${esc(L.next)}</p>`;
}
async function loadMap(){
  mapRoot = await (await fetch('/api/map')).json();
  document.getElementById('repoHint').textContent = mapRoot.incomplete
    ? 'Some folders could not be read, so this map is incomplete.'
    : 'Rooms related to this repository. Open only after you understand the story above.';
  renderBoard();
  renderOverview();
}
function renderBoard(){
  const board = document.getElementById('board');
  board.innerHTML = '';
  if (!mapRoot) return;
  const dirs = (mapRoot.children||[]).filter(c => c.kind==='dir');
  if (!dirs.length){
    board.innerHTML = '<div class="empty">No rooms found in this repository yet.</div>';
    return;
  }
  dirs.forEach(c => {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'node-card';
    b.dataset.path = c.path;
    b.title = c.path;
    b.setAttribute('aria-selected', focusPath === c.path ? 'true' : 'false');
    const n = countFiles(c);
    const stat = n === 1 ? '1 related file' : `${n} related files`;
    const blurb = placeBlurb(c.path) || 'A part of this repository.';
    b.innerHTML = `<span class="name">${esc(placeTitle(c.path, 'dir'))}</span><span class="blurb">${esc(blurb)}</span><span class="stat">${esc(stat)}</span>`;
    b.onclick = () => {
      focusPath = c.path;
      renderBoard();
      renderOverview();
      const topic = topicForPath(c.path);
      addAndSelect(topic.label, topic.feature_id);
    };
    board.appendChild(b);
  });
}
function renderOverview(){
  const overview = document.getElementById('overview');
  overview.replaceChildren();
  if (!mapRoot) return;
  const root = focusPath
    ? (mapRoot.children||[]).find(c => c.path === focusPath) || mapRoot
    : mapRoot;
  const ul = document.createElement('ul');
  ul.appendChild(renderTree(root, 0, focusPath));
  overview.appendChild(ul);
  overview.querySelectorAll('.tree-node').forEach(el => {
    el.onclick = (e) => {
      e.stopPropagation();
      focusPath = el.dataset.path;
      renderBoard();
      renderOverview();
      const topic = topicForPath(el.dataset.path);
      addAndSelect(topic.label, topic.feature_id);
    };
  });
}
async function addAndSelect(label, feature_id){
  if (!label) return;
  const commit_sha = document.getElementById('sha').value.trim() || 'HEAD';
  const searchKey = feature_id || label;
  await load();
  const existing = [...document.querySelectorAll('.row')].find(i =>
    i.dataset.commitSha === commit_sha &&
    (i.dataset.label === label || i.dataset.featureId === searchKey)
  );
  if (existing) {
    select(existing.dataset.id, existing);
    return;
  }
  const res = await fetch('/api/targets', {
    method:'POST',
    headers:{'content-type':'application/json'},
    body: JSON.stringify({ label, commit_sha, kind: 'feature', feature_id: searchKey })
  });
  const body = await res.json();
  await load();
  if (body.id) {
    const el = [...document.querySelectorAll('.row')].find(i => i.dataset.id === body.id);
    if (el) select(body.id, el);
  }
}
async function load() {
  const targets = await (await fetch('/api/targets')).json();
  const list = document.getElementById('list');
  list.innerHTML = '';
  if (!targets.length){
    list.innerHTML = '<div class="empty">No questions yet. Type one below, or wait for the prepared list.</div>';
    return;
  }
  targets.forEach(t => {
    const el = document.createElement('button');
    el.type = 'button';
    el.className = 'row';
    el.dataset.id = t.id;
    el.dataset.label = t.label;
    el.dataset.featureId = t.feature_id || '';
    el.dataset.commitSha = t.commit_sha || '';
    el.setAttribute('role', 'option');
    el.setAttribute('aria-selected', 'false');
    el.innerHTML = `<div class="title">${esc(t.label)}</div>`;
    el.onclick = () => select(t.id, el);
    list.appendChild(el);
  });
}
let selectToken = 0;
async function select(id, el) {
  const token = ++selectToken;
  document.querySelectorAll('.row').forEach(i => i.setAttribute('aria-selected', 'false'));
  el.setAttribute('aria-selected', 'true');
  const featureId = el.dataset.featureId || '';
  renderLesson(featureId, el.dataset.label);
  if (PREPARED && featureId && ROOM[featureId] && mapRoot) {
    focusPath = featureId;
    renderBoard();
    renderOverview();
    document.getElementById('mapDetails').open = false;
  }
  const drawer = document.getElementById('drawer');
  drawer.innerHTML = '<div class="empty">Checking the code for this question…</div>';
  const exp = await (await fetch('/api/targets/' + encodeURIComponent(id) + '/explain')).json();
  if (token !== selectToken) return;
  drawer.innerHTML = '';
  if (!exp.sections || !exp.sections.length){
    drawer.innerHTML = '<div class="empty">No proof collected for this question yet.</div>';
    return;
  }
  (exp.sections||[]).forEach(s => {
    const h = document.createElement('h2');
    h.className = 'evidence-title';
    h.textContent = s.title;
    drawer.appendChild(h);
    const p = document.createElement('p');
    p.className = 'evidence-body';
    p.textContent = s.body;
    drawer.appendChild(p);
    (s.evidence||[]).forEach(e => {
      const d = document.createElement('div');
      d.className = 'evidence-block';
      const cat = document.createElement('span');
      cat.className = 'badge ' + e.category;
      cat.textContent = CAT_LABEL[e.category] || e.category;
      d.appendChild(cat);
      if (e.unsupported) {
        const u = document.createElement('span');
        u.className = 'badge unsupported';
        u.textContent = 'not proven';
        d.appendChild(u);
      }
      const claim = document.createElement('div');
      claim.textContent = e.claim;
      d.appendChild(claim);
      if (e.anchor) {
        const a = document.createElement('div');
        a.className = 'anchor';
        const sha = String(e.anchor.commit_sha || '');
        const short = [...sha].slice(0, 8).join('');
        a.title = `${e.anchor.path}:${e.anchor.start_line}-${e.anchor.end_line}@${sha}`;
        const name = document.createElement('div');
        name.textContent = placeTitle(e.anchor.path, 'file');
        const meta = document.createElement('div');
        meta.textContent = `${e.anchor.path} · lines ${e.anchor.start_line}–${e.anchor.end_line}${short ? ' · ' + short : ''}`;
        a.appendChild(name);
        a.appendChild(meta);
        d.appendChild(a);
      }
      drawer.appendChild(d);
    });
  });
}
document.getElementById('add').onclick = async () => {
  const label = document.getElementById('label').value.trim();
  if (!label) return;
  await addAndSelect(label, '');
};
document.getElementById('themeToggle').onclick = () => {
  const next = document.documentElement.dataset.theme === 'light' ? 'dark' : 'light';
  applyTheme(next);
};
initTheme();
Promise.all([loadMap(), load()]).then(() => {
  const first = document.querySelector('.row');
  if (first) select(first.dataset.id, first);
});
</script>
</body>
</html>
"###;

#[cfg(test)]
mod dashboard_tests {
    use super::DASHBOARD_HTML;

    #[test]
    fn teaching_panes_labeled_in_plain_english() {
        for needle in [
            "1 · Ask",
            "2 · How it fits",
            "3 · Proof",
            "what you want to know",
        ] {
            assert!(
                DASHBOARD_HTML.contains(needle),
                "dashboard HTML missing {needle:?}"
            );
        }
    }

    #[test]
    fn evidence_categories_use_human_labels() {
        assert!(DASHBOARD_HTML.contains("From the code"));
        assert!(DASHBOARD_HTML.contains("Interpretation"));
    }

    #[test]
    fn entry_points_and_proof_identity_are_in_the_page() {
        assert!(DASHBOARD_HTML.contains("Library entry point"));
        assert!(DASHBOARD_HTML.contains("Program entry point"));
        assert!(DASHBOARD_HTML.contains("selectToken"));
        assert!(DASHBOARD_HTML.contains("dataset.commitSha"));
        assert!(DASHBOARD_HTML.contains("Checking the code for this question"));
        assert!(DASHBOARD_HTML.contains("__VIVA_REPO_NAME__"));
    }
}

#[cfg(test)]
mod map_tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn unreadable_directory_marks_map_incomplete() {
        let root = std::env::temp_dir().join(format!("viva-map-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("locked")).unwrap();
        fs::write(root.join("locked/note.rs"), "fn note() {}\n").unwrap();
        let locked = root.join("locked");
        let mut perms = fs::metadata(&locked).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&locked, perms.clone()).unwrap();
        let restricted = fs::read_dir(&locked).is_err();
        let map = build_map(&root, 3);
        perms.set_mode(0o755);
        let _ = fs::set_permissions(&locked, perms);
        let _ = fs::remove_dir_all(&root);
        if !restricted {
            return;
        }
        assert!(map.incomplete, "root should record the failed scan");
        assert!(
            map.children
                .iter()
                .any(|c| c.path.ends_with("locked") && c.incomplete)
        );
    }

    #[test]
    fn dot_path_uses_the_real_directory_name() {
        let root = std::env::temp_dir().join(format!("viva-name-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let name = repo_display_name(&root.join("."));
        let _ = fs::remove_dir_all(&root);
        assert_eq!(
            name,
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .to_ascii_lowercase()
        );
    }

    #[test]
    fn seeded_targets_pin_resolved_head() {
        let root = std::env::temp_dir().join(format!("viva-seed-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        {
            let mut cfg = repo.config().unwrap();
            cfg.set_str("user.name", "viva-test").unwrap();
            cfg.set_str("user.email", "viva@test.local").unwrap();
        }
        fs::write(root.join("README.md"), "hello\n").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let oid = index.write_tree().unwrap();
        let tree = repo.find_tree(oid).unwrap();
        let sig = repo.signature().unwrap();
        let sha = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap()
            .to_string();
        let store = Store::open_in_memory().unwrap();
        let auditor = Auditor::new(store, root.clone());
        seed_repo_targets(&auditor).unwrap();
        let targets = auditor.list_targets().unwrap();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].commit_sha, sha);
    }

    #[test]
    fn dashboard_injects_repo_name() {
        let html = dashboard_html(FsPath::new("omaseal"));
        assert!(html.contains("\"omaseal\""));
        assert!(!html.contains("__VIVA_REPO_NAME__"));
    }
}

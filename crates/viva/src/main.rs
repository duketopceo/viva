//! Viva CLI — index, search, and local audit dashboard.

use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use viva_core::Auditor;
use viva_db::{Store, TargetKind};
use viva_index::{index_commit, IndexOptions};

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
        Command::Search {
            name,
            repo,
            commit,
        } => {
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
                    h.file_path, h.start_line, h.end_line, &h.commit_sha[..8.min(h.commit_sha.len())], h.kind
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
    Ok(repo.revparse_single(spec)?.peel_to_commit()?.id().to_string())
}

async fn serve(db: &str, repo: PathBuf, port: u16) -> anyhow::Result<()> {
    let store = Store::open_path(db)?;
    let auditor = Arc::new(Auditor::new(store, repo));
    if auditor.list_targets()?.is_empty() {
        let _ = auditor.register_target(
            TargetKind::Feature,
            "signin",
            "HEAD",
            Some("**/*"),
            None,
            Some("demo-signin"),
            false,
        )?;
    }
    let state = AppState { auditor };
    let app = Router::new()
        .route("/", get(index))
        .route("/api/targets", get(list_targets))
        .route("/api/targets", post(add_target))
        .route("/api/targets/{id}/explain", get(explain))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("viva dashboard http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

async fn list_targets(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.auditor.list_targets().unwrap_or_default())
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
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>Viva — audit</title>
<style>
  :root { --ink:#0b0f14; --cloud:#e8eef6; --line:rgba(232,238,246,.12); --muted:rgba(232,238,246,.55); --accent:#6ea8fe; }
  * { box-sizing: border-box; }
  body { margin:0; font:14px/1.45 ui-sans-serif,system-ui; background:var(--ink); color:var(--cloud); height:100vh; display:grid; grid-template-columns:280px 1fr 360px; }
  aside, main, section { border-right:1px solid var(--line); overflow:auto; padding:16px; }
  section { border-right:none; }
  h1 { font-size:14px; letter-spacing:.08em; text-transform:uppercase; color:var(--muted); margin:0 0 12px; }
  .item { padding:10px 12px; border-radius:8px; cursor:pointer; border:1px solid transparent; }
  .item:hover, .item[data-on="1"] { background:rgba(110,168,254,.08); border-color:var(--line); }
  .item .k { font-size:11px; color:var(--muted); text-transform:uppercase; }
  .badge { display:inline-block; font-size:10px; letter-spacing:.06em; text-transform:uppercase; border:1px solid currentColor; padding:1px 6px; border-radius:3px; margin-right:6px; }
  .static { color:#8fd19e; } .observed { color:#6ea8fe; } .inferred { color:#e6c07b; }
  .unsupported { opacity:.7; }
  a.anchor { color:var(--accent); text-decoration:none; font-family:ui-monospace,monospace; font-size:12px; }
  pre { white-space:pre-wrap; background:rgba(0,0,0,.25); padding:10px; border-radius:8px; }
  input, button { background:#121821; color:var(--cloud); border:1px solid var(--line); border-radius:6px; padding:8px 10px; width:100%; margin:4px 0; }
  button { cursor:pointer; }
</style>
</head>
<body>
<aside>
  <h1>Targets</h1>
  <div id="list"></div>
  <hr style="border-color:var(--line);margin:16px 0"/>
  <input id="label" placeholder="feature / commit label"/>
  <input id="sha" placeholder="commit sha" value="HEAD"/>
  <button id="add">Add feature target</button>
</aside>
<main>
  <h1>Overview</h1>
  <p style="color:var(--muted)">Floorplan loop: pick a commit, PR, or feature → inspect evidence. Every claim is anchored or marked unsupported.</p>
  <div id="overview"></div>
</main>
<section>
  <h1>Evidence drawer</h1>
  <div id="drawer"></div>
</section>
<script>
async function load() {
  const targets = await (await fetch('/api/targets')).json();
  const list = document.getElementById('list');
  list.innerHTML = '';
  targets.forEach(t => {
    const el = document.createElement('div');
    el.className = 'item';
    el.innerHTML = `<div class="k">${t.kind}</div><div>${t.label}</div><div class="k">${t.commit_sha.slice(0,10)}</div>`;
    el.onclick = () => select(t.id, el);
    list.appendChild(el);
  });
}
async function select(id, el) {
  document.querySelectorAll('.item').forEach(i => i.dataset.on = '0');
  el.dataset.on = '1';
  const exp = await (await fetch('/api/targets/' + id + '/explain')).json();
  document.getElementById('overview').innerHTML = exp.sections.map(s =>
    `<h2 style="font-size:16px;margin:18px 0 6px">${s.title}</h2><p>${s.body}</p>`
  ).join('');
  const drawer = document.getElementById('drawer');
  drawer.innerHTML = '';
  exp.sections.forEach(s => s.evidence.forEach(e => {
    const d = document.createElement('div');
    d.style.marginBottom = '12px';
    const cat = e.category;
    d.innerHTML = `<span class="badge ${cat}">${cat}</span>` +
      (e.is_interpretation ? '<span class="badge inferred">interpretation</span>' : '') +
      (e.unsupported ? '<span class="badge unsupported">unsupported</span>' : '') +
      `<div>${e.claim}</div>` +
      (e.anchor ? `<a class="anchor" href="">${e.anchor.path}:${e.anchor.start_line}-${e.anchor.end_line}@${e.anchor.commit_sha.slice(0,8)}</a>` : '');
    drawer.appendChild(d);
  }));
}
document.getElementById('add').onclick = async () => {
  const label = document.getElementById('label').value.trim();
  const commit_sha = document.getElementById('sha').value.trim() || 'HEAD';
  if (!label) return;
  await fetch('/api/targets', { method:'POST', headers:{'content-type':'application/json'},
    body: JSON.stringify({ label, commit_sha, kind: 'feature' }) });
  load();
};
load();
</script>
</body>
</html>
"###;

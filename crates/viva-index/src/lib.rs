//! Commit-pinned TypeScript indexer for Viva.

use chrono::Utc;
use git2::{ObjectType, Oid, Repository, TreeWalkMode, TreeWalkResult};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tree_sitter::{Node, Parser, TreeCursor};
use viva_core::fingerprint_tree;
use viva_db::{new_id, IndexRun, ReferenceRow, Store, SymbolEdge, SymbolRow};

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("db: {0}")]
    Db(#[from] viva_db::DbError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, IndexError>;

#[derive(Debug, Clone)]
pub struct IndexOptions {
    pub repo_root: PathBuf,
    pub commit_sha: String,
    pub fingerprint_dirty: bool,
}

#[derive(Debug, Clone)]
pub struct IndexSummary {
    pub run_id: String,
    pub commit_sha: String,
    pub symbols: usize,
    pub references: usize,
    pub edges: usize,
    pub working_tree_fingerprint: Option<String>,
}

pub fn index_commit(store: &Store, opts: &IndexOptions) -> Result<IndexSummary> {
    let repo = Repository::open(&opts.repo_root)?;
    let oid = resolve_commit(&repo, &opts.commit_sha)?;
    let commit = repo.find_commit(oid)?;
    let commit_sha = commit.id().to_string();
    let tree = commit.tree()?;

    let ignore = load_gitignore_at_tree(&repo, &tree)?;

    let fp = if opts.fingerprint_dirty {
        Some(
            fingerprint_tree(&opts.repo_root)
                .map_err(|e| IndexError::Msg(e.to_string()))?,
        )
    } else {
        None
    };

    let run = IndexRun {
        id: new_id(),
        repo_root: opts.repo_root.to_string_lossy().to_string(),
        commit_sha: commit_sha.clone(),
        working_tree_fingerprint: fp.clone(),
        created_at: Utc::now(),
    };
    store.insert_index_run(&run)?;

    let mut ts_parser = Parser::new();
    ts_parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .map_err(|e| IndexError::Msg(format!("language: {e}")))?;
    let mut tsx_parser = Parser::new();
    tsx_parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
        .map_err(|e| IndexError::Msg(format!("tsx language: {e}")))?;

    let mut symbols = 0usize;
    let mut references = 0usize;
    let mut edges = 0usize;

    tree.walk(
        TreeWalkMode::PreOrder,
        |root_path, entry| {
            let name = entry.name().unwrap_or("");
            if entry.kind() != Some(ObjectType::Blob) {
                return TreeWalkResult::Ok;
            }
            if !is_ts_path(name) {
                return TreeWalkResult::Ok;
            }
            let rel = if root_path.is_empty() {
                name.to_string()
            } else {
                format!("{root_path}{name}")
            };
            if ignore.matched(&rel, false).is_ignore() {
                return TreeWalkResult::Ok;
            }
            let Ok(blob) = entry.to_object(&repo).and_then(|o| o.peel_to_blob()) else {
                return TreeWalkResult::Ok;
            };
            let Ok(text) = std::str::from_utf8(blob.content()) else {
                return TreeWalkResult::Ok;
            };
            let parser = if name.ends_with(".tsx") {
                &mut tsx_parser
            } else {
                &mut ts_parser
            };
            if let Ok(parsed) = parse_typescript(parser, text)
                && let Ok(counts) =
                    persist_parse(store, &run, &commit_sha, &rel, text, &parsed)
            {
                symbols += counts.0;
                references += counts.1;
                edges += counts.2;
            }
            TreeWalkResult::Ok
        },
    )?;

    Ok(IndexSummary {
        run_id: run.id,
        commit_sha,
        symbols,
        references,
        edges,
        working_tree_fingerprint: fp,
    })
}

fn resolve_commit(repo: &Repository, spec: &str) -> Result<Oid> {
    if spec.eq_ignore_ascii_case("HEAD") {
        return Ok(repo.head()?.peel_to_commit()?.id());
    }
    Ok(repo.revparse_single(spec)?.peel_to_commit()?.id())
}

fn is_ts_path(path: &str) -> bool {
    path.ends_with(".ts") || path.ends_with(".tsx")
}

struct CommitGitignore {
    patterns: Vec<String>,
}

impl CommitGitignore {
    fn matched(&self, path: &str, _is_dir: bool) -> Match {
        for p in &self.patterns {
            let p = p.trim();
            if p.is_empty() || p.starts_with('#') {
                continue;
            }
            let neg = p.starts_with('!');
            let pat = if neg { &p[1..] } else { p };
            if path_matches(pat, path) {
                return if neg { Match::None } else { Match::Ignore };
            }
        }
        Match::None
    }
}

enum Match {
    None,
    Ignore,
}

impl Match {
    fn is_ignore(&self) -> bool {
        matches!(self, Match::Ignore)
    }
}

fn path_matches(pat: &str, path: &str) -> bool {
    if let Some(rest) = pat.strip_prefix("**/") {
        return path.contains(rest) || path.ends_with(rest) || path_matches(rest, path);
    }
    if let Some(stem) = pat.strip_prefix('*').and_then(|s| s.strip_suffix('*')) {
        return path.contains(stem);
    }
    if let Some(suf) = pat.strip_prefix('*') {
        return path.ends_with(suf);
    }
    if let Some(pre) = pat.strip_suffix('*') {
        return path.starts_with(pre);
    }
    path == pat || path.ends_with(&format!("/{pat}"))
}

fn load_gitignore_at_tree(repo: &Repository, tree: &git2::Tree<'_>) -> Result<CommitGitignore> {
    let mut patterns = Vec::new();
    if let Ok(entry) = tree.get_path(Path::new(".gitignore"))
        && let Ok(blob) = entry.to_object(repo).and_then(|o| o.peel_to_blob())
        && let Ok(text) = std::str::from_utf8(blob.content())
    {
        patterns.extend(text.lines().map(str::to_string));
    }
    Ok(CommitGitignore { patterns })
}

#[derive(Debug, Default)]
struct ParsedFile {
    symbols: Vec<(String, String, usize, usize, usize, usize)>, // name, kind, sb, eb, sl, el
    imports: Vec<(String, usize, usize, usize, usize)>,
    calls: Vec<(String, String)>, // from_symbol_name, to_name
}

fn parse_typescript(parser: &mut Parser, source: &str) -> Result<ParsedFile> {
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| IndexError::Msg("parse failed".into()))?;
    let root = tree.root_node();
    let mut out = ParsedFile::default();
    let mut cursor = root.walk();
    walk_decls(source, &mut cursor, &mut out, None);
    Ok(out)
}

fn walk_decls(
    source: &str,
    cursor: &mut TreeCursor,
    out: &mut ParsedFile,
    enclosing: Option<&str>,
) {
    loop {
        let node = cursor.node();
        let kind = node.kind();
        match kind {
            "function_declaration" | "method_definition" | "generator_function_declaration" => {
                if let Some(name) = child_named(source, node, "name") {
                    let sl = node.start_position().row + 1;
                    let el = node.end_position().row + 1;
                    out.symbols.push((
                        name.clone(),
                        "function".into(),
                        node.start_byte(),
                        node.end_byte(),
                        sl,
                        el,
                    ));
                    if cursor.goto_first_child() {
                        walk_decls(source, cursor, out, Some(&name));
                        cursor.goto_parent();
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                    continue;
                }
            }
            "class_declaration" | "interface_declaration" | "type_alias_declaration" => {
                if let Some(name) = child_named(source, node, "name") {
                    let kind_s = if kind.starts_with("class") {
                        "class"
                    } else if kind.starts_with("interface") {
                        "interface"
                    } else {
                        "type"
                    };
                    let sl = node.start_position().row + 1;
                    let el = node.end_position().row + 1;
                    out.symbols.push((
                        name,
                        kind_s.into(),
                        node.start_byte(),
                        node.end_byte(),
                        sl,
                        el,
                    ));
                }
            }
            "import_statement" => {
                // crude: grab string literal or identifier
                if let Some(n) = first_identifier(source, node) {
                    let sl = node.start_position().row + 1;
                    let el = node.end_position().row + 1;
                    out.imports
                        .push((n, node.start_byte(), node.end_byte(), sl, el));
                }
            }
            "call_expression" => {
                if let Some(en) = enclosing
                    && let Some(callee) = call_name(source, node)
                {
                    out.calls.push((en.to_string(), callee));
                }
            }
            _ => {}
        }
        if cursor.goto_first_child() {
            walk_decls(source, cursor, out, enclosing);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn child_named(source: &str, node: Node, field: &str) -> Option<String> {
    let n = node.child_by_field_name(field)?;
    Some(n.utf8_text(source.as_bytes()).ok()?.to_string())
}

fn first_identifier(source: &str, node: Node) -> Option<String> {
    let mut c = node.walk();
    loop {
        let n = c.node();
        if n.kind() == "identifier" || n.kind() == "string" {
            let t = n.utf8_text(source.as_bytes()).ok()?;
            let cleaned = t.trim_matches('"').trim_matches('\'').to_string();
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
        if c.goto_first_child() {
            if let Some(v) = first_identifier(source, c.node()) {
                return Some(v);
            }
            c.goto_parent();
        }
        if !c.goto_next_sibling() {
            break;
        }
    }
    None
}

fn call_name(source: &str, node: Node) -> Option<String> {
    let func = node.child_by_field_name("function")?;
    match func.kind() {
        "identifier" => Some(func.utf8_text(source.as_bytes()).ok()?.to_string()),
        "member_expression" => {
            let prop = func.child_by_field_name("property")?;
            Some(prop.utf8_text(source.as_bytes()).ok()?.to_string())
        }
        _ => None,
    }
}

fn persist_parse(
    store: &Store,
    run: &IndexRun,
    commit_sha: &str,
    file_path: &str,
    _source: &str,
    parsed: &ParsedFile,
) -> Result<(usize, usize, usize)> {
    let mut symbol_ids = std::collections::HashMap::new();
    let mut sc = 0usize;
    for (name, kind, sb, eb, sl, el) in &parsed.symbols {
        let id = new_id();
        store.insert_symbol(&SymbolRow {
            id: id.clone(),
            run_id: run.id.clone(),
            commit_sha: commit_sha.to_string(),
            file_path: file_path.to_string(),
            name: name.clone(),
            kind: kind.clone(),
            start_byte: *sb as i64,
            end_byte: *eb as i64,
            start_line: *sl as u32,
            end_line: *el as u32,
        })?;
        symbol_ids.insert(name.clone(), id);
        sc += 1;
    }
    let mut rc = 0usize;
    for (name, sb, eb, sl, el) in &parsed.imports {
        store.insert_reference(&ReferenceRow {
            id: new_id(),
            run_id: run.id.clone(),
            commit_sha: commit_sha.to_string(),
            file_path: file_path.to_string(),
            name: name.clone(),
            kind: "import".into(),
            start_byte: *sb as i64,
            end_byte: *eb as i64,
            start_line: *sl as u32,
            end_line: *el as u32,
        })?;
        rc += 1;
    }
    let mut ec = 0usize;
    for (from, to) in &parsed.calls {
        if let Some(from_id) = symbol_ids.get(from) {
            store.insert_edge(&SymbolEdge {
                id: new_id(),
                run_id: run.id.clone(),
                commit_sha: commit_sha.to_string(),
                from_symbol_id: from_id.clone(),
                to_name: to.clone(),
                kind: "call".into(),
            })?;
            ec += 1;
        }
    }
    Ok((sc, rc, ec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn init_repo_with_auth(dir: &Path) -> String {
        let repo = Repository::init(dir).unwrap();
        {
            let mut cfg = repo.config().unwrap();
            cfg.set_str("user.name", "viva-test").unwrap();
            cfg.set_str("user.email", "viva@test.local").unwrap();
        }
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("src/auth.ts"),
            r#"
export function validate(x: string): boolean {
  return x.length > 0;
}

export function signIn(user: string): boolean {
  return validate(user);
}
"#,
        )
        .unwrap();
        fs::write(dir.join(".gitignore"), "skip-me.ts\n").unwrap();
        fs::write(dir.join("skip-me.ts"), "export function hidden() {}\n").unwrap();

        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let oid = index.write_tree().unwrap();
        let tree = repo.find_tree(oid).unwrap();
        let sig = repo.signature().unwrap();
        let commit = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        commit.to_string()
    }

    #[test]
    fn indexes_signin_and_call_edge() {
        let dir = tempdir().unwrap();
        let sha = init_repo_with_auth(dir.path());
        let store = Store::open_in_memory().unwrap();
        let summary = index_commit(
            &store,
            &IndexOptions {
                repo_root: dir.path().to_path_buf(),
                commit_sha: sha.clone(),
                fingerprint_dirty: false,
            },
        )
        .unwrap();
        assert!(summary.symbols >= 2);
        let hits = store.find_symbols(&sha, "signIn").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file_path, "src/auth.ts");
        assert!(hits[0].start_line >= 1);
        let edges = store.list_edges_from(&hits[0].id).unwrap();
        assert!(
            edges.iter().any(|e| e.to_name == "validate"),
            "expected call edge to validate, got {edges:?}"
        );
        // ignored file not indexed
        assert!(store.find_symbols(&sha, "hidden").unwrap().is_empty());
    }

    #[test]
    fn second_commit_preserves_first() {
        let dir = tempdir().unwrap();
        let sha1 = init_repo_with_auth(dir.path());
        let store = Store::open_in_memory().unwrap();
        index_commit(
            &store,
            &IndexOptions {
                repo_root: dir.path().to_path_buf(),
                commit_sha: sha1.clone(),
                fingerprint_dirty: false,
            },
        )
        .unwrap();

        let repo = Repository::open(dir.path()).unwrap();
        fs::write(
            dir.path().join("src/auth.ts"),
            r#"
export function validate(x: string): boolean { return true; }
export function signIn(user: string): boolean { return validate(user); }
export function signOut(): void {}
"#,
        )
        .unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let oid = index.write_tree().unwrap();
        let tree = repo.find_tree(oid).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = repo.signature().unwrap();
        let sha2 = repo
            .commit(Some("HEAD"), &sig, &sig, "add signOut", &tree, &[&parent])
            .unwrap()
            .to_string();

        index_commit(
            &store,
            &IndexOptions {
                repo_root: dir.path().to_path_buf(),
                commit_sha: sha2.clone(),
                fingerprint_dirty: false,
            },
        )
        .unwrap();

        assert_eq!(store.find_symbols(&sha1, "signIn").unwrap().len(), 1);
        assert!(store.find_symbols(&sha1, "signOut").unwrap().is_empty());
        assert_eq!(store.find_symbols(&sha2, "signOut").unwrap().len(), 1);
    }

    #[test]
    fn dirty_fingerprint_matches_core() {
        let dir = tempdir().unwrap();
        let _sha = init_repo_with_auth(dir.path());
        fs::write(dir.path().join("extra.ts"), "export const x = 1;\n").unwrap();
        let from_core = fingerprint_tree(dir.path()).unwrap();
        let store = Store::open_in_memory().unwrap();
        let summary = index_commit(
            &store,
            &IndexOptions {
                repo_root: dir.path().to_path_buf(),
                commit_sha: "HEAD".into(),
                fingerprint_dirty: true,
            },
        )
        .unwrap();
        assert_eq!(summary.working_tree_fingerprint.as_deref(), Some(from_core.as_str()));
    }
}

//! Commit-pinned explain targets and index tables for Viva.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, DbError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Commit,
    PullRequest,
    Feature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainTarget {
    pub id: String,
    pub kind: TargetKind,
    pub label: String,
    pub commit_sha: String,
    pub working_tree_fingerprint: Option<String>,
    pub path_glob: Option<String>,
    pub pr_number: Option<u64>,
    pub feature_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexRun {
    pub id: String,
    pub repo_root: String,
    pub commit_sha: String,
    pub working_tree_fingerprint: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolRow {
    pub id: String,
    pub run_id: String,
    pub commit_sha: String,
    pub file_path: String,
    pub name: String,
    pub kind: String,
    pub start_byte: i64,
    pub end_byte: i64,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReferenceRow {
    pub id: String,
    pub run_id: String,
    pub commit_sha: String,
    pub file_path: String,
    pub name: String,
    pub kind: String,
    pub start_byte: i64,
    pub end_byte: i64,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolEdge {
    pub id: String,
    pub run_id: String,
    pub commit_sha: String,
    pub from_symbol_id: String,
    pub to_name: String,
    pub kind: String,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let s = Self { conn };
        s.migrate()?;
        Ok(s)
    }

    pub fn open_path(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let s = Self { conn };
        s.migrate()?;
        Ok(s)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS explain_targets (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                label TEXT NOT NULL,
                commit_sha TEXT NOT NULL,
                working_tree_fingerprint TEXT,
                path_glob TEXT,
                pr_number INTEGER,
                feature_id TEXT,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS index_runs (
                id TEXT PRIMARY KEY,
                repo_root TEXT NOT NULL,
                commit_sha TEXT NOT NULL,
                working_tree_fingerprint TEXT,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS symbols (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL REFERENCES index_runs(id),
                commit_sha TEXT NOT NULL,
                file_path TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                start_byte INTEGER NOT NULL,
                end_byte INTEGER NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS references_tbl (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL REFERENCES index_runs(id),
                commit_sha TEXT NOT NULL,
                file_path TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                start_byte INTEGER NOT NULL,
                end_byte INTEGER NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS symbol_edges (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL REFERENCES index_runs(id),
                commit_sha TEXT NOT NULL,
                from_symbol_id TEXT NOT NULL REFERENCES symbols(id),
                to_name TEXT NOT NULL,
                kind TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_symbols_commit_name ON symbols(commit_sha, name);
            CREATE INDEX IF NOT EXISTS idx_symbols_path ON symbols(commit_sha, file_path);
            "#,
        )?;
        Ok(())
    }

    pub fn insert_target(&self, t: &ExplainTarget) -> Result<()> {
        self.conn.execute(
            "INSERT INTO explain_targets (id, kind, label, commit_sha, working_tree_fingerprint, path_glob, pr_number, feature_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                t.id,
                serde_json::to_string(&t.kind)?,
                t.label,
                t.commit_sha,
                t.working_tree_fingerprint,
                t.path_glob,
                t.pr_number.map(|n| n as i64),
                t.feature_id,
                t.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_targets(&self) -> Result<Vec<ExplainTarget>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, label, commit_sha, working_tree_fingerprint, path_glob, pr_number, feature_id, created_at FROM explain_targets ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], map_target)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get_target(&self, id: &str) -> Result<ExplainTarget> {
        self.conn
            .query_row(
                "SELECT id, kind, label, commit_sha, working_tree_fingerprint, path_glob, pr_number, feature_id, created_at FROM explain_targets WHERE id = ?1",
                params![id],
                map_target,
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    pub fn insert_index_run(&self, run: &IndexRun) -> Result<()> {
        self.conn.execute(
            "INSERT INTO index_runs (id, repo_root, commit_sha, working_tree_fingerprint, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                run.id,
                run.repo_root,
                run.commit_sha,
                run.working_tree_fingerprint,
                run.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn insert_symbol(&self, s: &SymbolRow) -> Result<()> {
        self.assert_run_commit(&s.run_id, &s.commit_sha)?;
        self.conn.execute(
            "INSERT INTO symbols (id, run_id, commit_sha, file_path, name, kind, start_byte, end_byte, start_line, end_line)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                s.id,
                s.run_id,
                s.commit_sha,
                s.file_path,
                s.name,
                s.kind,
                s.start_byte,
                s.end_byte,
                s.start_line as i64,
                s.end_line as i64,
            ],
        )?;
        Ok(())
    }

    pub fn insert_reference(&self, r: &ReferenceRow) -> Result<()> {
        self.assert_run_commit(&r.run_id, &r.commit_sha)?;
        self.conn.execute(
            "INSERT INTO references_tbl (id, run_id, commit_sha, file_path, name, kind, start_byte, end_byte, start_line, end_line)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                r.id,
                r.run_id,
                r.commit_sha,
                r.file_path,
                r.name,
                r.kind,
                r.start_byte,
                r.end_byte,
                r.start_line as i64,
                r.end_line as i64,
            ],
        )?;
        Ok(())
    }

    pub fn insert_edge(&self, e: &SymbolEdge) -> Result<()> {
        self.assert_run_commit(&e.run_id, &e.commit_sha)?;
        self.conn.execute(
            "INSERT INTO symbol_edges (id, run_id, commit_sha, from_symbol_id, to_name, kind)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                e.id,
                e.run_id,
                e.commit_sha,
                e.from_symbol_id,
                e.to_name,
                e.kind
            ],
        )?;
        Ok(())
    }

    fn assert_run_commit(&self, run_id: &str, commit_sha: &str) -> Result<()> {
        let run_sha: String = self
            .conn
            .query_row(
                "SELECT commit_sha FROM index_runs WHERE id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .map_err(|_| DbError::NotFound(run_id.to_string()))?;
        if run_sha != commit_sha {
            return Err(DbError::Msg(format!(
                "commit_sha mismatch: row={commit_sha} run={run_sha}"
            )));
        }
        Ok(())
    }

    pub fn find_symbols(&self, commit_sha: &str, name: &str) -> Result<Vec<SymbolRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, run_id, commit_sha, file_path, name, kind, start_byte, end_byte, start_line, end_line
             FROM symbols WHERE commit_sha = ?1 AND name = ?2",
        )?;
        let rows = stmt.query_map(params![commit_sha, name], map_symbol)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// All symbols recorded for a commit — Phase 0 gate: every row resolves to file:line@commit.
    pub fn list_symbols_at_commit(&self, commit_sha: &str) -> Result<Vec<SymbolRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, run_id, commit_sha, file_path, name, kind, start_byte, end_byte, start_line, end_line
             FROM symbols WHERE commit_sha = ?1 ORDER BY file_path, start_line, name",
        )?;
        let rows = stmt.query_map(params![commit_sha], map_symbol)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn find_symbols_by_path_prefix(
        &self,
        commit_sha: &str,
        path_prefix: &str,
    ) -> Result<Vec<SymbolRow>> {
        let like = format!("{path_prefix}%");
        let mut stmt = self.conn.prepare(
            "SELECT id, run_id, commit_sha, file_path, name, kind, start_byte, end_byte, start_line, end_line
             FROM symbols WHERE commit_sha = ?1 AND file_path LIKE ?2",
        )?;
        let rows = stmt.query_map(params![commit_sha, like], map_symbol)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn list_edges_from(&self, from_symbol_id: &str) -> Result<Vec<SymbolEdge>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, run_id, commit_sha, from_symbol_id, to_name, kind FROM symbol_edges WHERE from_symbol_id = ?1",
        )?;
        let rows = stmt.query_map(params![from_symbol_id], map_edge)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }
}

fn map_target(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExplainTarget> {
    let kind_s: String = row.get(1)?;
    let created_s: String = row.get(8)?;
    let pr: Option<i64> = row.get(6)?;
    Ok(ExplainTarget {
        id: row.get(0)?,
        kind: serde_json::from_str(&kind_s).unwrap_or(TargetKind::Commit),
        label: row.get(2)?,
        commit_sha: row.get(3)?,
        working_tree_fingerprint: row.get(4)?,
        path_glob: row.get(5)?,
        pr_number: pr.map(|n| n as u64),
        feature_id: row.get(7)?,
        created_at: DateTime::parse_from_rfc3339(&created_s)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn map_symbol(row: &rusqlite::Row<'_>) -> rusqlite::Result<SymbolRow> {
    Ok(SymbolRow {
        id: row.get(0)?,
        run_id: row.get(1)?,
        commit_sha: row.get(2)?,
        file_path: row.get(3)?,
        name: row.get(4)?,
        kind: row.get(5)?,
        start_byte: row.get(6)?,
        end_byte: row.get(7)?,
        start_line: row.get::<_, i64>(8)? as u32,
        end_line: row.get::<_, i64>(9)? as u32,
    })
}

fn map_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<SymbolEdge> {
    Ok(SymbolEdge {
        id: row.get(0)?,
        run_id: row.get(1)?,
        commit_sha: row.get(2)?,
        from_symbol_id: row.get(3)?,
        to_name: row.get(4)?,
        kind: row.get(5)?,
    })
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_list_target() {
        let store = Store::open_in_memory().unwrap();
        let t = ExplainTarget {
            id: new_id(),
            kind: TargetKind::Feature,
            label: "sign-in path".into(),
            commit_sha: "abc123".into(),
            working_tree_fingerprint: None,
            path_glob: Some("src/auth/**".into()),
            pr_number: None,
            feature_id: Some("auth-signin".into()),
            created_at: Utc::now(),
        };
        store.insert_target(&t).unwrap();
        assert_eq!(store.list_targets().unwrap().len(), 1);
        assert_eq!(store.get_target(&t.id).unwrap().commit_sha, "abc123");
    }

    #[test]
    fn symbols_keyed_by_commit_coexist() {
        let store = Store::open_in_memory().unwrap();
        let run_a = IndexRun {
            id: new_id(),
            repo_root: "/tmp/r".into(),
            commit_sha: "aaa".into(),
            working_tree_fingerprint: None,
            created_at: Utc::now(),
        };
        let run_b = IndexRun {
            id: new_id(),
            repo_root: "/tmp/r".into(),
            commit_sha: "bbb".into(),
            working_tree_fingerprint: None,
            created_at: Utc::now(),
        };
        store.insert_index_run(&run_a).unwrap();
        store.insert_index_run(&run_b).unwrap();
        let sa = SymbolRow {
            id: new_id(),
            run_id: run_a.id.clone(),
            commit_sha: "aaa".into(),
            file_path: "src/auth.ts".into(),
            name: "signIn".into(),
            kind: "function".into(),
            start_byte: 0,
            end_byte: 10,
            start_line: 1,
            end_line: 3,
        };
        let sb = SymbolRow {
            id: new_id(),
            run_id: run_b.id.clone(),
            commit_sha: "bbb".into(),
            file_path: "src/auth.ts".into(),
            name: "signIn".into(),
            kind: "function".into(),
            start_byte: 0,
            end_byte: 12,
            start_line: 1,
            end_line: 4,
        };
        store.insert_symbol(&sa).unwrap();
        store.insert_symbol(&sb).unwrap();
        assert_eq!(store.find_symbols("aaa", "signIn").unwrap().len(), 1);
        assert_eq!(store.find_symbols("bbb", "signIn").unwrap().len(), 1);
    }

    #[test]
    fn call_edge_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let run = IndexRun {
            id: new_id(),
            repo_root: "/tmp/r".into(),
            commit_sha: "ccc".into(),
            working_tree_fingerprint: None,
            created_at: Utc::now(),
        };
        store.insert_index_run(&run).unwrap();
        let sym = SymbolRow {
            id: new_id(),
            run_id: run.id.clone(),
            commit_sha: "ccc".into(),
            file_path: "src/auth.ts".into(),
            name: "signIn".into(),
            kind: "function".into(),
            start_byte: 0,
            end_byte: 40,
            start_line: 1,
            end_line: 5,
        };
        store.insert_symbol(&sym).unwrap();
        let edge = SymbolEdge {
            id: new_id(),
            run_id: run.id.clone(),
            commit_sha: "ccc".into(),
            from_symbol_id: sym.id.clone(),
            to_name: "validate".into(),
            kind: "call".into(),
        };
        store.insert_edge(&edge).unwrap();
        let edges = store.list_edges_from(&sym.id).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to_name, "validate");
    }

    #[test]
    fn list_symbols_at_commit_returns_all_for_sha() {
        let store = Store::open_in_memory().unwrap();
        let run = IndexRun {
            id: new_id(),
            repo_root: "/tmp/r".into(),
            commit_sha: "ddd".into(),
            working_tree_fingerprint: None,
            created_at: Utc::now(),
        };
        store.insert_index_run(&run).unwrap();
        for name in ["a", "b"] {
            store
                .insert_symbol(&SymbolRow {
                    id: new_id(),
                    run_id: run.id.clone(),
                    commit_sha: "ddd".into(),
                    file_path: "src/x.ts".into(),
                    name: name.into(),
                    kind: "function".into(),
                    start_byte: 0,
                    end_byte: 1,
                    start_line: 1,
                    end_line: 1,
                })
                .unwrap();
        }
        let all = store.list_symbols_at_commit("ddd").unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|s| s.commit_sha == "ddd" && s.start_line >= 1));
        assert!(store.list_symbols_at_commit("missing").unwrap().is_empty());
    }

    #[test]
    fn mismatched_commit_sha_rejected() {
        let store = Store::open_in_memory().unwrap();
        let run = IndexRun {
            id: new_id(),
            repo_root: "/tmp/r".into(),
            commit_sha: "good".into(),
            working_tree_fingerprint: None,
            created_at: Utc::now(),
        };
        store.insert_index_run(&run).unwrap();
        let bad = SymbolRow {
            id: new_id(),
            run_id: run.id,
            commit_sha: "bad".into(),
            file_path: "a.ts".into(),
            name: "x".into(),
            kind: "function".into(),
            start_byte: 0,
            end_byte: 1,
            start_line: 1,
            end_line: 1,
        };
        assert!(store.insert_symbol(&bad).is_err());
    }
}

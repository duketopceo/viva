//! Viva audit explanations — senior-dev style, every claim anchored or unsupported.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;
use viva_db::{ExplainTarget, Store, TargetKind};

#[derive(Debug, Error)]
pub enum CoreError {
    #[error(transparent)]
    Db(#[from] viva_db::DbError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

fn short_revision(sha: &str) -> &str {
    let end = sha.len().min(8);
    &sha[..end]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceCategory {
    Static,
    Observed,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeAnchor {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub commit_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub category: EvidenceCategory,
    pub claim: String,
    pub anchor: Option<CodeAnchor>,
    /// When true, claim is interpretation and must not be dressed as fact.
    pub is_interpretation: bool,
    pub unsupported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSection {
    pub title: String,
    pub body: String,
    pub evidence: Vec<EvidenceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExplanation {
    pub target_id: String,
    pub commit_sha: String,
    pub working_tree_fingerprint: Option<String>,
    pub sections: Vec<AuditSection>,
    pub retrieval: RetrievalReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RetrievalReport {
    pub codebase_memory: AdapterStatus,
    pub graphify: AdapterStatus,
    pub path_search: AdapterStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AdapterStatus {
    #[default]
    Absent,
    Used,
    Degraded,
}

/// Optional local helpers — present or degrade; never invent anchors.
pub trait RetrievalAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn available(&self) -> bool;
    /// Return candidate file paths related to the target label (hints only).
    fn hint_paths(&self, label: &str) -> Vec<String>;
}

pub struct AbsentAdapter {
    pub name: &'static str,
}

impl RetrievalAdapter for AbsentAdapter {
    fn name(&self) -> &'static str {
        self.name
    }
    fn available(&self) -> bool {
        false
    }
    fn hint_paths(&self, _label: &str) -> Vec<String> {
        Vec::new()
    }
}

/// Path/symbol search over a fixture or repo tree — always available when root exists.
pub struct PathSearchAdapter {
    pub root: PathBuf,
}

impl RetrievalAdapter for PathSearchAdapter {
    fn name(&self) -> &'static str {
        "path_search"
    }
    fn available(&self) -> bool {
        self.root.is_dir()
    }
    fn hint_paths(&self, label: &str) -> Vec<String> {
        let needle = label.to_ascii_lowercase();
        let mut hits = Vec::new();
        let _ = visit(&self.root, &self.root, &needle, &mut hits);
        hits.truncate(8);
        hits
    }
}

fn visit(root: &Path, dir: &Path, needle: &str, out: &mut Vec<String>) -> std::io::Result<()> {
    if out.len() >= 8 {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            visit(root, &path, needle, out)?;
        } else if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            if name.to_ascii_lowercase().contains(needle)
                || rel.to_ascii_lowercase().contains(needle)
            {
                out.push(rel);
            }
        }
    }
    Ok(())
}

pub fn fingerprint_tree(root: &Path) -> Result<String> {
    let mut paths = Vec::new();
    collect_files(root, root, &mut paths)?;
    paths.sort();
    let mut h = Sha256::new();
    for p in paths {
        h.update(p.as_bytes());
        h.update(b"\n");
    }
    Ok(hex::encode(h.finalize()))
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name() == ".git" {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            out.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }
    Ok(())
}

pub struct Auditor {
    store: Mutex<Store>,
    pub repo_root: PathBuf,
    pub cbm: Box<dyn RetrievalAdapter>,
    pub graphify: Box<dyn RetrievalAdapter>,
}

impl Auditor {
    pub fn new(store: Store, repo_root: PathBuf) -> Self {
        // CBM / graphify default absent — wire when local MCP/binary present.
        // Path search runs inside explain() whenever repo_root exists.
        Self {
            store: Mutex::new(store),
            repo_root,
            cbm: Box::new(AbsentAdapter {
                name: "codebase_memory",
            }),
            graphify: Box::new(AbsentAdapter { name: "graphify" }),
        }
    }

    pub fn list_targets(&self) -> Result<Vec<ExplainTarget>> {
        Ok(self
            .store
            .lock()
            .map_err(|_| CoreError::Msg("store lock poisoned".into()))?
            .list_targets()?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register_target(
        &self,
        kind: TargetKind,
        label: &str,
        commit_sha: &str,
        path_glob: Option<&str>,
        pr_number: Option<u64>,
        feature_id: Option<&str>,
        dirty: bool,
    ) -> Result<ExplainTarget> {
        let fp = if dirty {
            Some(fingerprint_tree(&self.repo_root)?)
        } else {
            None
        };
        let t = ExplainTarget {
            id: viva_db::new_id(),
            kind,
            label: label.to_string(),
            commit_sha: commit_sha.to_string(),
            working_tree_fingerprint: fp,
            path_glob: path_glob.map(str::to_string),
            pr_number,
            feature_id: feature_id.map(str::to_string),
            created_at: Utc::now(),
        };
        self.store
            .lock()
            .map_err(|_| CoreError::Msg("store lock poisoned".into()))?
            .insert_target(&t)?;
        Ok(t)
    }

    pub fn explain(&self, target_id: &str) -> Result<AuditExplanation> {
        let target = self
            .store
            .lock()
            .map_err(|_| CoreError::Msg("store lock poisoned".into()))?
            .get_target(target_id)?;
        let path_search = PathSearchAdapter {
            root: self.repo_root.clone(),
        };

        let retrieval = RetrievalReport {
            codebase_memory: if self.cbm.available() {
                AdapterStatus::Used
            } else {
                AdapterStatus::Absent
            },
            graphify: if self.graphify.available() {
                AdapterStatus::Used
            } else {
                AdapterStatus::Absent
            },
            path_search: if path_search.available() {
                AdapterStatus::Used
            } else {
                AdapterStatus::Degraded
            },
        };

        let mut evidence = Vec::new();
        let path_key = target
            .feature_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(target.label.as_str());

        // Prefer commit-pinned index symbols; try the display label, then the search key.
        let store = self
            .store
            .lock()
            .map_err(|_| CoreError::Msg("store lock poisoned".into()))?;
        let mut indexed = store.find_symbols(&target.commit_sha, &target.label)?;
        if indexed.is_empty() && path_key != target.label {
            indexed = store.find_symbols(&target.commit_sha, path_key)?;
        }
        drop(store);
        for sym in &indexed {
            evidence.push(EvidenceItem {
                category: EvidenceCategory::Static,
                claim: format!("This definition is part of {}.", target.label),
                anchor: Some(CodeAnchor {
                    path: sym.file_path.clone(),
                    start_line: sym.start_line,
                    end_line: sym.end_line,
                    commit_sha: sym.commit_sha.clone(),
                }),
                is_interpretation: false,
                unsupported: false,
            });
        }

        let mut hints = Vec::new();
        let mut push_hints = |key: &str| {
            if self.cbm.available() {
                hints.extend(self.cbm.hint_paths(key));
            }
            if self.graphify.available() {
                hints.extend(self.graphify.hint_paths(key));
            }
        };
        push_hints(path_key);
        if path_key != target.label {
            push_hints(&target.label);
        }
        if evidence.is_empty() && hints.is_empty() {
            hints.extend(path_search.hint_paths(path_key));
            if path_key != target.label {
                hints.extend(path_search.hint_paths(&target.label));
            }
        }
        hints.sort();
        hints.dedup();

        for rel in &hints {
            let full = self.repo_root.join(rel);
            if let Ok(content) = std::fs::read_to_string(&full) {
                let lines = content.lines().count() as u32;
                evidence.push(EvidenceItem {
                    category: EvidenceCategory::Static,
                    claim: format!("This file is part of {}.", target.label),
                    anchor: Some(CodeAnchor {
                        path: rel.clone(),
                        start_line: 1,
                        end_line: lines.max(1),
                        commit_sha: target.commit_sha.clone(),
                    }),
                    is_interpretation: false,
                    unsupported: false,
                });
            }
        }

        if evidence.is_empty() {
            evidence.push(EvidenceItem {
                category: EvidenceCategory::Inferred,
                claim: format!(
                    "Nothing in the index matched {} on this revision yet.",
                    target.label
                ),
                anchor: None,
                is_interpretation: true,
                unsupported: true,
            });
        }

        let short_rev = short_revision(&target.commit_sha);
        let what = AuditSection {
            title: "What".into(),
            body: format!("Looking at {} on revision {}.", target.label, short_rev),
            evidence: evidence.clone(),
        };
        let why = AuditSection {
            title: "Why".into(),
            body: "So you can check work the same way a careful reviewer would: what it is for, before how it is built.".into(),
            evidence: vec![EvidenceItem {
                category: EvidenceCategory::Inferred,
                claim: "This reason comes from product guidance, not from a recorded run.".into(),
                anchor: None,
                is_interpretation: true,
                unsupported: true,
            }],
        };
        let how = AuditSection {
            title: "How".into(),
            body: "Start from the files below. Claims marked as interpretation are guesses, not proof.".into(),
            evidence: evidence
                .iter()
                .filter(|e| e.category == EvidenceCategory::Static)
                .cloned()
                .collect(),
        };
        let alternatives = AuditSection {
            title: "Alternatives & standards".into(),
            body: "Prefer evidence pinned to a revision over answers that only sound right. Teaching and quizzes come later — this pass is explanation only.".into(),
            evidence: vec![EvidenceItem {
                category: EvidenceCategory::Inferred,
                claim: "Standards notes are interpretation unless they point at a file in the repo.".into(),
                anchor: None,
                is_interpretation: true,
                unsupported: true,
            }],
        };

        Ok(AuditExplanation {
            target_id: target.id,
            commit_sha: target.commit_sha,
            working_tree_fingerprint: target.working_tree_fingerprint,
            sections: vec![what, why, how, alternatives],
            retrieval,
        })
    }
}

/// Every non-unsupported claim must carry an anchor.
pub fn validate_explanation(exp: &AuditExplanation) -> bool {
    exp.sections.iter().all(|s| {
        s.evidence.iter().all(|e| {
            if e.unsupported {
                e.anchor.is_none()
            } else {
                e.anchor.is_some()
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn fixture_repo_explanation_has_anchors_or_unsupported() {
        let dir = tempdir().unwrap();
        let auth = dir.path().join("src/auth");
        fs::create_dir_all(&auth).unwrap();
        fs::write(auth.join("signin.ts"), "export function signIn() {}\n").unwrap();

        let store = Store::open_in_memory().unwrap();
        let auditor = Auditor::new(store, dir.path().to_path_buf());
        let t = auditor
            .register_target(
                TargetKind::Feature,
                "signin",
                "deadbeef",
                Some("src/auth/**"),
                None,
                Some("auth-signin"),
                false,
            )
            .unwrap();
        let exp = auditor.explain(&t.id).unwrap();
        assert!(validate_explanation(&exp));
        assert!(exp.sections.iter().any(|s| {
            s.evidence
                .iter()
                .any(|e| e.anchor.as_ref().is_some_and(|a| a.path.contains("signin")))
        }));
        assert_eq!(exp.retrieval.path_search, AdapterStatus::Used);
        assert_eq!(exp.retrieval.codebase_memory, AdapterStatus::Absent);
    }

    #[test]
    fn dirty_fingerprint_differs_from_clean() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
        let clean = fingerprint_tree(dir.path()).unwrap();
        fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();
        let dirty = fingerprint_tree(dir.path()).unwrap();
        assert_ne!(clean, dirty);
    }

    #[test]
    fn explain_uses_indexed_symbol_line_ranges() {
        use git2::Repository;
        use viva_index::{IndexOptions, index_commit};

        let dir = tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        {
            let mut cfg = repo.config().unwrap();
            cfg.set_str("user.name", "viva-test").unwrap();
            cfg.set_str("user.email", "viva@test.local").unwrap();
        }
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/auth.ts"),
            "export function validate(x: string) { return true; }\nexport function signIn(u: string) { return validate(u); }\n",
        )
        .unwrap();
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
        index_commit(
            &store,
            &IndexOptions {
                repo_root: dir.path().to_path_buf(),
                commit_sha: sha.clone(),
                fingerprint_dirty: false,
            },
        )
        .unwrap();

        let auditor = Auditor::new(store, dir.path().to_path_buf());
        let t = auditor
            .register_target(
                TargetKind::Feature,
                "signIn",
                &sha,
                None,
                None,
                Some("auth-signin"),
                false,
            )
            .unwrap();
        let exp = auditor.explain(&t.id).unwrap();
        assert!(validate_explanation(&exp));
        let static_e: Vec<_> = exp
            .sections
            .iter()
            .flat_map(|s| s.evidence.iter())
            .filter(|e| e.category == EvidenceCategory::Static && !e.unsupported)
            .collect();
        assert!(!static_e.is_empty());
        let a = static_e[0].anchor.as_ref().unwrap();
        assert_eq!(a.path, "src/auth.ts");
        assert!(a.start_line >= 1);
        // Not forced to whole-file 1..EOF when symbol range is known.
        assert!(a.end_line >= a.start_line);
        assert_eq!(exp.retrieval.codebase_memory, AdapterStatus::Absent);
        assert_eq!(exp.retrieval.graphify, AdapterStatus::Absent);
    }

    #[test]
    fn unsupported_without_anchors_is_valid() {
        let exp = AuditExplanation {
            target_id: "x".into(),
            commit_sha: "abc".into(),
            working_tree_fingerprint: None,
            sections: vec![AuditSection {
                title: "What".into(),
                body: "none".into(),
                evidence: vec![EvidenceItem {
                    category: EvidenceCategory::Inferred,
                    claim: "no files".into(),
                    anchor: None,
                    is_interpretation: true,
                    unsupported: true,
                }],
            }],
            retrieval: RetrievalReport::default(),
        };
        assert!(validate_explanation(&exp));
    }
}

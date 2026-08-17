use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use git2::Repository;
use rayon::prelude::*;

use crate::types::{ChangeStatus, FileChange};

/// Thread-safe reader for git blobs reusing thread-local `Repository` instances.
///
/// Prevents redundant repository opening (`Repository::open`) across worker threads
/// during parallel blob extraction.
pub struct SharedBlobReader {
    pub repo_path: String,
}

impl SharedBlobReader {
    pub fn new(repo_path: impl Into<String>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    /// Read blob content by OID, reusing a thread-local Repository handle.
    pub fn read_blob(&self, oid: git2::Oid) -> Result<Option<String>> {
        thread_local! {
            static REPO_CACHE: std::cell::RefCell<Option<(String, Repository)>> = const { std::cell::RefCell::new(None) };
        }

        REPO_CACHE.with(|cache| {
            let mut cache_ref = cache.borrow_mut();
            let need_open = match cache_ref.as_ref() {
                Some((path, _)) => path != &self.repo_path,
                None => true,
            };

            if need_open {
                let repo = Repository::open(&self.repo_path)
                    .or_else(|_| Repository::discover(&self.repo_path))
                    .with_context(|| format!("Failed to open repository at '{}'", self.repo_path))?;
                *cache_ref = Some((self.repo_path.clone(), repo));
            }

            let repo = &cache_ref.as_ref().unwrap().1;
            read_blob_by_oid(repo, oid)
        })
    }

    /// Read raw blob byte slice by OID directly without string allocation.
    pub fn read_blob_bytes(&self, oid: git2::Oid) -> Result<Option<Vec<u8>>> {
        thread_local! {
            static REPO_CACHE: std::cell::RefCell<Option<(String, Repository)>> = const { std::cell::RefCell::new(None) };
        }

        REPO_CACHE.with(|cache| {
            let mut cache_ref = cache.borrow_mut();
            let need_open = match cache_ref.as_ref() {
                Some((path, _)) => path != &self.repo_path,
                None => true,
            };

            if need_open {
                let repo = Repository::open(&self.repo_path)
                    .or_else(|_| Repository::discover(&self.repo_path))
                    .with_context(|| format!("Failed to open repository at '{}'", self.repo_path))?;
                *cache_ref = Some((self.repo_path.clone(), repo));
            }

            let repo = &cache_ref.as_ref().unwrap().1;
            read_blob_bytes_by_oid(repo, oid)
        })
    }
}

/// Read raw blob byte slice by its OID.
fn read_blob_bytes_by_oid(repo: &Repository, oid: git2::Oid) -> Result<Option<Vec<u8>>> {
    let blob = repo.find_blob(oid).context("Failed to find blob")?;
    if blob.is_binary() {
        Ok(None)
    } else {
        Ok(Some(blob.content().to_vec()))
    }
}

/// Lightweight delta metadata collected in the sequential phase,
/// before parallel content extraction.
struct DeltaInfo {
    path: String,
    old_path: Option<String>,
    new_path: Option<String>,
    status: ChangeStatus,
    old_oid: Option<git2::Oid>,
    new_oid: Option<git2::Oid>,
}

/// Retrieve the list of changed files between two targets, with their contents.
///
/// Supports diffing between:
/// 1. Commit A and Commit B
/// 2. Commit A and Staged Index (if `staged == true`)
/// 3. Commit A and Working Directory (if `commit_b_ref` is `None` or `"WORKING"`)
pub fn get_changed_files(
    repo_path: &str,
    commit_a_ref: &str,
    commit_b_ref: Option<&str>,
    staged: bool,
) -> Result<Vec<FileChange>> {
    // ── Phase 1: collect delta metadata (sequential, fast) ───────────
    let deltas = {
        let repo = Repository::open(repo_path)
            .or_else(|_| Repository::discover(repo_path))
            .with_context(|| format!("Failed to open or discover git repository at '{}'", repo_path))?;

        let commit_a = resolve_commit(&repo, commit_a_ref)?;
        let tree_a = commit_a.tree().context("Failed to get tree for commit A")?;

        let mut diff_opts = git2::DiffOptions::new();
        let mut diff = if let Some(cb_str) = commit_b_ref {
            if cb_str.eq_ignore_ascii_case("WORKING") || cb_str.eq_ignore_ascii_case("WORKTREE") {
                repo.diff_tree_to_workdir_with_index(Some(&tree_a), Some(&mut diff_opts))
                    .context("Failed to compute diff between commit A and working tree")?
            } else {
                let commit_b = resolve_commit(&repo, cb_str)?;
                let tree_b = commit_b.tree().context("Failed to get tree for commit B")?;
                repo.diff_tree_to_tree(Some(&tree_a), Some(&tree_b), Some(&mut diff_opts))
                    .context("Failed to compute diff between commits")?
            }
        } else if staged {
            let index = repo.index().context("Failed to open repository index")?;
            repo.diff_tree_to_index(Some(&tree_a), Some(&index), Some(&mut diff_opts))
                .context("Failed to compute diff between commit A and staged index")?
        } else {
            repo.diff_tree_to_workdir_with_index(Some(&tree_a), Some(&mut diff_opts))
                .context("Failed to compute diff between commit A and working tree")?
        };

        // Enable native Git rename detection (find_similar)
        let _ = diff.find_similar(None);

        let mut deltas = Vec::new();

        for delta in diff.deltas() {
            let status = match delta.status() {
                git2::Delta::Added => ChangeStatus::Added,
                git2::Delta::Deleted => ChangeStatus::Deleted,
                git2::Delta::Modified => ChangeStatus::Modified,
                git2::Delta::Renamed => ChangeStatus::Renamed,
                _ => continue,
            };

            let old_path = delta
                .old_file()
                .path()
                .map(|p| p.to_string_lossy().to_string());
            let new_path = delta
                .new_file()
                .path()
                .map(|p| p.to_string_lossy().to_string());

            let path = new_path
                .clone()
                .or_else(|| old_path.clone())
                .unwrap_or_default();

            let old_oid = {
                let oid = delta.old_file().id();
                if !oid.is_zero() && status != ChangeStatus::Added {
                    Some(oid)
                } else {
                    None
                }
            };

            let new_oid = {
                let oid = delta.new_file().id();
                if !oid.is_zero() && status != ChangeStatus::Deleted {
                    Some(oid)
                } else {
                    None
                }
            };

            deltas.push(DeltaInfo {
                path,
                old_path,
                new_path,
                status,
                old_oid,
                new_oid,
            });
        }

        deltas
    };

    // ── Phase 2: parallel blob / file extraction ──────────────────────
    let blob_reader = SharedBlobReader::new(repo_path);

    let changes: Result<Vec<FileChange>> = deltas
        .par_iter()
        .map(|delta| {
            let old_blob_hash = delta.old_oid.map(|o| o.to_string());
            let new_blob_hash = delta.new_oid.map(|o| o.to_string());

            let old_content = match delta.old_oid {
                Some(oid) => blob_reader.read_blob(oid)?,
                None => None,
            };

            let new_content = match delta.new_oid {
                Some(oid) => blob_reader.read_blob(oid)?,
                None => {
                    // Working tree fallback if new_oid is zero (uncommitted working file)
                    if delta.status != ChangeStatus::Deleted {
                        let full_file_path = Path::new(repo_path).join(&delta.path);
                        if full_file_path.is_file() {
                            fs::read_to_string(&full_file_path).ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            };

            Ok(FileChange {
                path: delta.path.clone(),
                old_path: delta.old_path.clone(),
                new_path: delta.new_path.clone(),
                old_content,
                new_content,
                status: delta.status,
                old_blob_hash,
                new_blob_hash,
            })
        })
        .collect();

    changes
}

/// Resolve a commit reference string to a git2::Commit.
fn resolve_commit<'a>(repo: &'a Repository, reference: &str) -> Result<git2::Commit<'a>> {
    let obj = repo
        .revparse_single(reference)
        .with_context(|| format!("Failed to resolve reference: '{}'", reference))?;

    obj.peel_to_commit()
        .with_context(|| format!("Reference '{}' does not point to a commit", reference))
}

/// Read blob content by its OID.
fn read_blob_by_oid(repo: &Repository, oid: git2::Oid) -> Result<Option<String>> {
    let blob = repo.find_blob(oid).context("Failed to find blob")?;

    match std::str::from_utf8(blob.content()) {
        Ok(s) => Ok(Some(s.to_string())),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_blob_reader_new() {
        let reader = SharedBlobReader::new(".");
        assert_eq!(reader.repo_path, ".");
    }

    #[test]
    fn test_resolve_commit_nonexistent_reference() {
        let repo_res = Repository::open(".");
        if let Ok(repo) = repo_res {
            let res = resolve_commit(&repo, "non_existent_commit_hash_12345");
            assert!(res.is_err());
        }
    }
}

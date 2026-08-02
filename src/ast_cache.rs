use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bincode::Options;
use lru::LruCache;
use std::num::NonZeroUsize;

use crate::types::AstNode;

/// Maximum number of entries in the in-memory LRU cache.
const IN_MEMORY_CACHE_SIZE: usize = 256;

/// Current cache format version. Bumped on any schema change.
const CACHE_FORMAT_VERSION: u8 = 1;

/// Maximum bytes allowed during deserialization (20 MiB).
/// Prevents OOM from poisoned or oversized cache files.
const MAX_DESERIALIZATION_BYTES: u64 = 20_971_520;

/// Cache key: blob hash fully identifies file content in git.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub blob_hash: String,
    pub logic_only: bool,
    pub limits_hash: u64,
}

/// Stored payload for a cached AST entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheEntry {
    pub ast: AstNode,
    pub node_count: u64,
}

/// Versioned envelope wrapping cached AST data.
/// Provides schema evolution and integrity checking against poisoned caches.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CacheEnvelope {
    /// Schema version — deserialization is rejected on mismatch.
    version: u8,
    /// The git blob OID this entry was derived from (integrity check).
    blob_oid: String,
    /// The actual cached AST data.
    payload: CacheEntry,
}

/// A two-tier AST cache: in-memory LRU + on-disk persistence.
///
/// Disk cache is stored in an **external** directory (outside the repo tree)
/// to prevent cache injection and accidental commits. The cache path is
/// derived from `blake3(canonical_repo_path)`.
pub struct AstCache {
    memory: Mutex<LruCache<CacheKey, CacheEntry>>,
    disk_dir: Option<PathBuf>,
}

/// Build the bounded bincode options used for all cache serialization.
#[inline]
fn cache_bincode_options() -> impl Options {
    bincode::options().with_limit(MAX_DESERIALIZATION_BYTES)
}

impl AstCache {
    /// Create a new cache, optionally backed by a disk directory.
    ///
    /// `cache_dir` is the full path to the external cache directory, e.g.
    /// `$XDG_CACHE_HOME/symtrace/<repo_hash>/`.  The directory is created
    /// if it does not exist.
    pub fn new(cache_dir: Option<PathBuf>) -> Self {
        let disk_dir = cache_dir.and_then(|dir| {
            fs::create_dir_all(&dir).ok()?;
            // Restrict directory permissions on Unix to owner-only (0o700)
            // to prevent other users on shared systems from reading cached ASTs.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
            }
            Some(dir)
        });

        AstCache {
            memory: Mutex::new(LruCache::new(
                NonZeroUsize::new(IN_MEMORY_CACHE_SIZE).unwrap(),
            )),
            disk_dir,
        }
    }

    /// Try to get a cached AST entry for the given blob hash.
    ///
    /// On-disk entries are deserialized with a **bounded reader** (20 MiB max)
    /// and verified against the envelope version and blob OID.  Any mismatch
    /// or corruption is treated as a cache miss and the stale file is removed.
    pub fn get(&self, key: &CacheKey) -> Option<CacheEntry> {
        // Check in-memory first
        {
            let mut mem = self.memory.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = mem.get(key) {
                return Some(entry.clone());
            }
        }

        // Check disk with bounded, versioned deserialization
        if let Some(ref dir) = self.disk_dir {
            let path = self.disk_path(dir, key)?;
            if path.exists() {
                if let Ok(data) = fs::read(&path) {
                    match cache_bincode_options().deserialize::<CacheEnvelope>(&data) {
                        Ok(envelope) => {
                            // Version check: reject mismatched schemas
                            if envelope.version != CACHE_FORMAT_VERSION {
                                eprintln!(
                                    "  cache: version mismatch (file v{}, expected v{}), discarding",
                                    envelope.version, CACHE_FORMAT_VERSION
                                );
                                let _ = fs::remove_file(&path);
                                return None;
                            }
                            // Blob OID integrity check
                            if envelope.blob_oid != key.blob_hash {
                                eprintln!("  cache: blob OID mismatch, discarding");
                                let _ = fs::remove_file(&path);
                                return None;
                            }
                            // Promote to in-memory cache
                            let mut mem = self.memory.lock().unwrap_or_else(|e| e.into_inner());
                            mem.put(key.clone(), envelope.payload.clone());
                            return Some(envelope.payload);
                        }
                        Err(_) => {
                            // Corrupted, poisoned, or oversized — remove stale entry
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        None
    }

    /// Store a parsed AST in the cache, wrapping it in a versioned envelope.
    pub fn put(&self, key: CacheKey, entry: CacheEntry) {
        // Write to disk with versioned envelope
        if let Some(ref dir) = self.disk_dir {
            if let Some(path) = self.disk_path(dir, &key) {
                let envelope = CacheEnvelope {
                    version: CACHE_FORMAT_VERSION,
                    blob_oid: key.blob_hash.clone(),
                    payload: entry.clone(),
                };
                if let Ok(data) = cache_bincode_options().serialize(&envelope) {
                    let _ = fs::write(&path, data);
                }
            }
        }

        // Write to in-memory LRU
        let mut mem = self.memory.lock().unwrap_or_else(|e| e.into_inner());
        mem.put(key, entry);
    }

    /// Build a deterministic disk file path for a cache key.
    /// Returns None if the blob hash contains non-hex characters (safety check).
    fn disk_path(&self, dir: &Path, key: &CacheKey) -> Option<PathBuf> {
        // Validate blob hash is hex-only (prevent directory traversal)
        if !key.blob_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let suffix = if key.logic_only { "_logic" } else { "" };
        Some(dir.join(format!("{}{}.bin", key.blob_hash, suffix)))
    }

    /// Return cache statistics: (memory_entries, disk_entries)
    pub fn stats(&self) -> (usize, usize) {
        let mem_count = self.memory.lock().unwrap_or_else(|e| e.into_inner()).len();
        let disk_count = self
            .disk_dir
            .as_ref()
            .and_then(|dir| fs::read_dir(dir).ok())
            .map(|rd| rd.count())
            .unwrap_or(0);
        (mem_count, disk_count)
    }
}

/// Global convenience: check if two blob hashes indicate unchanged content.
pub fn blobs_are_identical(old_hash: Option<&str>, new_hash: Option<&str>) -> bool {
    match (old_hash, new_hash) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

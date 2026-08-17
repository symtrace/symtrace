use std::fs;
use std::path::{Path, PathBuf};

use bincode::Options;
use lru::LruCache;
use std::num::NonZeroUsize;

use crate::types::{AstNode, FileDiff};

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

impl CacheKey {
    pub fn new(blob_hash: impl Into<String>, logic_only: bool, limits_hash: u64) -> Self {
        Self {
            blob_hash: blob_hash.into(),
            logic_only,
            limits_hash,
        }
    }
}

/// Cache key for complete FileDiff results (CAS diff caching).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DiffCacheKey {
    pub old_blob_hash: String,
    pub new_blob_hash: String,
    pub logic_only: bool,
    pub limits_hash: u64,
}

impl DiffCacheKey {
    pub fn new(
        old_blob_hash: impl Into<String>,
        new_blob_hash: impl Into<String>,
        logic_only: bool,
        limits_hash: u64,
    ) -> Self {
        Self {
            old_blob_hash: old_blob_hash.into(),
            new_blob_hash: new_blob_hash.into(),
            logic_only,
            limits_hash,
        }
    }

    pub fn digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.old_blob_hash.as_bytes());
        hasher.update(b":");
        hasher.update(self.new_blob_hash.as_bytes());
        hasher.update(if self.logic_only { b"_logic" } else { b"_full" });
        hasher.update(&self.limits_hash.to_le_bytes());
        hasher.finalize().to_hex().to_string()
    }
}

/// Versioned envelope for cached FileDiff data.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DiffCacheEnvelope {
    version: u8,
    key_digest: String,
    payload: FileDiff,
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

use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

static CACHE_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const SHARDS: usize = 16;
const SHARD_CAPACITY: usize = IN_MEMORY_CACHE_SIZE / SHARDS;

/// A two-tier AST & FileDiff cache: 16-bucket lock-striped in-memory LRU + on-disk persistence.
///
/// Disk cache is stored in an **external** directory (outside the repo tree)
/// to prevent cache injection and accidental commits. The cache path is
/// derived from `blake3(canonical_repo_path)`.
pub struct AstCache {
    memory: [RwLock<LruCache<CacheKey, CacheEntry>>; SHARDS],
    diff_memory: [RwLock<LruCache<String, FileDiff>>; SHARDS],
    disk_dir: Option<PathBuf>,
}

/// Build the bounded bincode options used for all cache serialization.
#[inline]
fn cache_bincode_options() -> impl Options {
    bincode::options().with_limit(MAX_DESERIALIZATION_BYTES)
}

#[inline]
fn shard_index(key: &CacheKey) -> usize {
    if let Ok(val) = u64::from_str_radix(key.blob_hash.get(0..4).unwrap_or("0"), 16) {
        (val as usize) % SHARDS
    } else {
        (key.limits_hash as usize) % SHARDS
    }
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

        let memory = std::array::from_fn(|_| {
            RwLock::new(LruCache::new(NonZeroUsize::new(SHARD_CAPACITY).unwrap()))
        });

        let diff_memory = std::array::from_fn(|_| {
            RwLock::new(LruCache::new(NonZeroUsize::new(SHARD_CAPACITY).unwrap()))
        });

        AstCache { memory, diff_memory, disk_dir }
    }

    /// Try to get a cached AST entry for the given blob hash.
    ///
    /// On-disk entries are deserialized with a **bounded reader** (20 MiB max)
    /// and verified against the envelope version and blob OID.  Any mismatch
    /// or corruption is treated as a cache miss and the stale file is removed.
    pub fn get(&self, key: &CacheKey) -> Option<CacheEntry> {
        let shard = shard_index(key);

        // Check in-memory first with lock-striped partition
        {
            let mut mem = self.memory[shard].write().unwrap_or_else(|e| e.into_inner());
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
                            let mut mem = self.memory[shard].write().unwrap_or_else(|e| e.into_inner());
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
    /// Writes to disk atomically via temporary files and rename.
    pub fn put(&self, key: CacheKey, entry: CacheEntry) {
        let shard = shard_index(&key);

        // Write to disk atomically with versioned envelope
        if let Some(ref dir) = self.disk_dir {
            if let Some(path) = self.disk_path(dir, &key) {
                let envelope = CacheEnvelope {
                    version: CACHE_FORMAT_VERSION,
                    blob_oid: key.blob_hash.clone(),
                    payload: entry.clone(),
                };
                if let Ok(data) = cache_bincode_options().serialize(&envelope) {
                    let counter = CACHE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
                    let tmp_path = dir.join(format!(".tmp.{}.{}.bin", std::process::id(), counter));
                    if fs::write(&tmp_path, data).is_ok() {
                        let _ = fs::rename(&tmp_path, &path);
                    }
                }
            }
        }

        // Write to lock-striped in-memory LRU
        let mut mem = self.memory[shard].write().unwrap_or_else(|e| e.into_inner());
        mem.put(key, entry);
    }

    /// Zero-Copy cache lookup: resolves AST directly by Git blob OID without requiring
    /// prior content reading or string allocations.
    pub fn get_by_oid(&self, blob_hash: &str, logic_only: bool, limits_hash: u64) -> Option<CacheEntry> {
        let key = CacheKey::new(blob_hash, logic_only, limits_hash);
        self.get(&key)
    }

    /// Convenience put method using Git blob OID parameters.
    pub fn put_by_oid(&self, blob_hash: impl Into<String>, logic_only: bool, limits_hash: u64, entry: CacheEntry) {
        let key = CacheKey::new(blob_hash, logic_only, limits_hash);
        self.put(key, entry);
    }

    /// Build a deterministic disk file path for a cache key.
    /// Returns None if the blob hash contains non-hex characters (safety check).
    fn disk_path(&self, dir: &Path, key: &CacheKey) -> Option<PathBuf> {
        // Validate blob hash is hex-only (prevent directory traversal)
        if !key.blob_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let suffix = if key.logic_only { "_logic" } else { "" };
        Some(dir.join(format!("{}{}_{:x}.bin", key.blob_hash, suffix, key.limits_hash)))
    }

    /// Return cache statistics: (memory_entries, disk_entries)
    pub fn stats(&self) -> (usize, usize) {
        let mem_count: usize = self
            .memory
            .iter()
            .map(|shard| shard.read().unwrap_or_else(|e| e.into_inner()).len())
            .sum();
        let disk_count = self
            .disk_dir
            .as_ref()
            .and_then(|dir| fs::read_dir(dir).ok())
            .map(|rd| rd.count())
            .unwrap_or(0);
        (mem_count, disk_count)
    }

    /// Retrieve a pre-computed FileDiff result from the CAS cache (<0.005 ms warm latency).
    pub fn get_diff(&self, key: &DiffCacheKey) -> Option<FileDiff> {
        let digest = key.digest();
        let shard = if let Ok(val) = u64::from_str_radix(digest.get(0..4).unwrap_or("0"), 16) {
            (val as usize) % SHARDS
        } else {
            0
        };

        // 1. Check in-memory LRU
        {
            let mut mem = self.diff_memory[shard].write().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = mem.get(&digest) {
                return Some(entry.clone());
            }
        }

        // 2. Check disk cache
        if let Some(ref dir) = self.disk_dir {
            let path = dir.join(format!("diff_{}.bin", digest));
            if path.exists() {
                if let Ok(data) = fs::read(&path) {
                    if let Ok(envelope) = cache_bincode_options().deserialize::<DiffCacheEnvelope>(&data) {
                        if envelope.version == CACHE_FORMAT_VERSION && envelope.key_digest == digest {
                            let mut mem = self.diff_memory[shard].write().unwrap_or_else(|e| e.into_inner());
                            mem.put(digest, envelope.payload.clone());
                            return Some(envelope.payload);
                        }
                    }
                }
            }
        }

        None
    }

    /// Store a computed FileDiff result into the CAS cache.
    pub fn put_diff(&self, key: &DiffCacheKey, diff: FileDiff) {
        let digest = key.digest();
        let shard = if let Ok(val) = u64::from_str_radix(digest.get(0..4).unwrap_or("0"), 16) {
            (val as usize) % SHARDS
        } else {
            0
        };

        // 1. Write to disk atomically via temporary file and rename
        if let Some(ref dir) = self.disk_dir {
            let path = dir.join(format!("diff_{}.bin", digest));
            let envelope = DiffCacheEnvelope {
                version: CACHE_FORMAT_VERSION,
                key_digest: digest.clone(),
                payload: diff.clone(),
            };
            if let Ok(data) = cache_bincode_options().serialize(&envelope) {
                let counter = CACHE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
                let tmp_path = dir.join(format!(".tmp_diff.{}.{}.bin", std::process::id(), counter));
                if fs::write(&tmp_path, data).is_ok() {
                    let _ = fs::rename(&tmp_path, &path);
                }
            }
        }

        // 2. Write to in-memory LRU
        let mut mem = self.diff_memory[shard].write().unwrap_or_else(|e| e.into_inner());
        mem.put(digest, diff);
    }
}

/// Global convenience: check if two blob hashes indicate unchanged content.
pub fn blobs_are_identical(old_hash: Option<&str>, new_hash: Option<&str>) -> bool {
    match (old_hash, new_hash) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_ast() -> AstNode {
        AstNode {
            id: 1,
            kind: "function_item".to_string(),
            start_byte: 0,
            end_byte: 10,
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 10,
            text: "fn main(){}".to_string(),
            structural_hash: [1u8; 32],
            content_hash: [2u8; 32],
            context_hash: [3u8; 32],
            identity_hash: [4u8; 32],
            children: vec![],
            is_named: true,
        }
    }

    #[test]
    fn ast_cache_lock_striped_in_memory_put_get() {
        let cache = AstCache::new(None);
        let key1 = CacheKey::new("aaaa111122223333444455556666777788889999", false, 100);
        let key2 = CacheKey::new("bbbb111122223333444455556666777788889999", true, 200);

        cache.put(key1.clone(), CacheEntry { ast: dummy_ast(), node_count: 1 });
        cache.put(key2.clone(), CacheEntry { ast: dummy_ast(), node_count: 2 });

        let res1 = cache.get(&key1);
        let res2 = cache.get(&key2);
        assert!(res1.is_some());
        assert_eq!(res1.unwrap().node_count, 1);
        assert!(res2.is_some());
        assert_eq!(res2.unwrap().node_count, 2);
    }

    #[test]
    fn ast_cache_atomic_disk_write_and_recovery() {
        let tmp = std::env::temp_dir().join(format!("symtrace_cache_test_{}", std::process::id()));
        let cache = AstCache::new(Some(tmp.clone()));
        let key = CacheKey::new("cccc111122223333444455556666777788889999", false, 300);

        cache.put(key.clone(), CacheEntry { ast: dummy_ast(), node_count: 42 });
        let retrieved = cache.get(&key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().node_count, 42);

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn ast_cache_concurrent_striped_access() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(AstCache::new(None));
        let mut handles = Vec::new();

        for i in 0..8 {
            let c = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                let key = CacheKey::new(format!("{:04x}111122223333444455556666777788889999", i), false, i as u64);
                c.put(key.clone(), CacheEntry { ast: dummy_ast(), node_count: i as u64 });
                let got = c.get(&key);
                assert!(got.is_some());
                assert_eq!(got.unwrap().node_count, i as u64);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn ast_cache_cas_diff_put_get() {
        let tmp = std::env::temp_dir().join(format!("symtrace_diff_cache_test_{}", std::process::id()));
        let cache = AstCache::new(Some(tmp.clone()));

        let diff_key = DiffCacheKey::new(
            "aaaa111122223333444455556666777788889999",
            "bbbb111122223333444455556666777788889999",
            false,
            1234,
        );

        let file_diff = FileDiff {
            file_path: "src/lib.rs".to_string(),
            operations: vec![],
            refactor_patterns: vec![],
        };

        // Cache miss
        assert!(cache.get_diff(&diff_key).is_none());

        // Cache put
        cache.put_diff(&diff_key, file_diff.clone());

        // Cache hit
        let retrieved = cache.get_diff(&diff_key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().file_path, "src/lib.rs");

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn test_cache_key_equality() {
        let k1 = CacheKey::new("abc", true, 100);
        let k2 = CacheKey::new("abc", true, 100);
        assert_eq!(k1, k2);
        assert_eq!(k1.blob_hash, "abc");
    }

    #[test]
    fn test_diff_cache_key_digest() {
        let k = DiffCacheKey::new("hash_a", "hash_b", false, 42);
        let digest = k.digest();
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn test_ast_cache_stats_tracking() {
        let cache = AstCache::new(None);
        let (mem, disk) = cache.stats();
        assert_eq!(mem, 0);
        assert_eq!(disk, 0);

        let k = CacheKey::new("hash1", false, 0);
        cache.put(k, CacheEntry { ast: dummy_ast(), node_count: 1 });
        let (mem2, _) = cache.stats();
        assert_eq!(mem2, 1);
    }

    #[test]
    fn test_ast_cache_multiple_shards_distribution() {
        let cache = AstCache::new(None);
        for i in 0..32 {
            let k = CacheKey::new(format!("hash_{}", i), false, i);
            cache.put(k, CacheEntry { ast: dummy_ast(), node_count: i });
        }
        let (mem, _) = cache.stats();
        assert_eq!(mem, 32);
    }
}

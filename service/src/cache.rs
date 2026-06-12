//! The answer cache: LRU, 256 entries, storing final canonical envelope
//! bytes. The key is the EXISTING `query_hash` — which already pins the
//! normalized query, the principal, `snapshot_version`, and `index_version`,
//! so scope isolation is inherited and a fixture change invalidates by
//! construction — plus the request's mode flags (hybrid/judge), so a cached
//! lexical envelope is never served for a hybrid ask.
//!
//! Only clean envelopes are cached: transient degradations (embedder/judge/
//! generator failures, citation faults) are recomputed, never pinned.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

pub const CACHE_CAPACITY: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub query_hash: String,
    pub hybrid: bool,
    pub judge: bool,
}

struct Inner {
    map: HashMap<CacheKey, Vec<u8>>,
    /// Least-recently-used at the front.
    order: VecDeque<CacheKey>,
}

pub struct AnswerCache {
    inner: Mutex<Inner>,
}

impl Default for AnswerCache {
    fn default() -> AnswerCache {
        AnswerCache::new()
    }
}

impl AnswerCache {
    pub fn new() -> AnswerCache {
        AnswerCache {
            inner: Mutex::new(Inner {
                map: HashMap::new(),
                order: VecDeque::new(),
            }),
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<Vec<u8>> {
        let mut inner = self.inner.lock().expect("cache mutex");
        let bytes = inner.map.get(key)?.clone();
        // Touch: move to most-recently-used.
        if let Some(position) = inner.order.iter().position(|k| k == key) {
            inner.order.remove(position);
        }
        inner.order.push_back(key.clone());
        Some(bytes)
    }

    pub fn put(&self, key: CacheKey, bytes: Vec<u8>) {
        let mut inner = self.inner.lock().expect("cache mutex");
        if inner.map.insert(key.clone(), bytes).is_none() {
            inner.order.push_back(key);
        } else if let Some(position) = inner.order.iter().position(|k| k == &key) {
            inner.order.remove(position);
            inner.order.push_back(key);
        }
        while inner.map.len() > CACHE_CAPACITY {
            let Some(evicted) = inner.order.pop_front() else {
                break;
            };
            inner.map.remove(&evicted);
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("cache mutex").map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

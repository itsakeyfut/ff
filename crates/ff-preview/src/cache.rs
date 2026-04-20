//! Decoded RGBA frame cache for instant seek responses.

#![allow(clippy::cast_possible_truncation)]

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

/// A single cached RGBA frame.
pub(crate) struct CachedFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// LRU-evicting RGBA frame cache bounded by a byte budget.
///
/// Frames are keyed by their PTS. When the byte budget would be exceeded on
/// insert, the oldest frame (front of the insertion-order deque) is evicted.
pub(crate) struct FrameCache {
    capacity_bytes: usize,
    used_bytes: usize,
    /// Insertion order — front is oldest.
    order: VecDeque<Duration>,
    entries: HashMap<Duration, CachedFrame>,
}

impl FrameCache {
    pub fn new(capacity_bytes: usize) -> Self {
        Self {
            capacity_bytes,
            used_bytes: 0,
            order: VecDeque::new(),
            entries: HashMap::new(),
        }
    }

    /// Look up a frame by PTS.
    pub fn get(&self, pts: Duration) -> Option<&CachedFrame> {
        self.entries.get(&pts)
    }

    /// Insert a decoded RGBA frame.
    ///
    /// If `pts` is already cached, this is a no-op. Otherwise, oldest entries
    /// are evicted until the new frame fits within the budget.
    pub fn insert(&mut self, pts: Duration, rgba: Vec<u8>, w: u32, h: u32) {
        if self.entries.contains_key(&pts) {
            return;
        }

        let frame_bytes = (w as usize) * (h as usize) * 4;

        // Evict oldest until there is room.
        while self.used_bytes + frame_bytes > self.capacity_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&oldest) {
                let evicted_bytes = (evicted.width as usize) * (evicted.height as usize) * 4;
                self.used_bytes = self.used_bytes.saturating_sub(evicted_bytes);
            }
        }

        // Skip insert if a single frame exceeds the entire budget.
        if frame_bytes > self.capacity_bytes {
            return;
        }

        self.order.push_back(pts);
        self.used_bytes += frame_bytes;
        self.entries.insert(
            pts,
            CachedFrame {
                rgba,
                width: w,
                height: h,
            },
        );
    }

    /// Flush all cached entries.
    pub fn invalidate(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.used_bytes = 0;
    }

    /// Current byte usage.
    #[allow(dead_code)]
    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    /// Inclusive PTS range covered by the cache, or `None` if empty.
    pub fn pts_range(&self) -> Option<(Duration, Duration)> {
        let min = self.order.front().copied()?;
        let max = self.order.back().copied()?;
        Some((min, max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_cache_get_should_return_none_when_empty() {
        let cache = FrameCache::new(1024 * 1024);
        assert!(cache.get(Duration::ZERO).is_none());
    }

    #[test]
    fn frame_cache_insert_and_get_should_store_and_retrieve_frame() {
        let mut cache = FrameCache::new(1024 * 1024);
        let rgba = vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        cache.insert(Duration::from_millis(100), rgba.clone(), 2, 2);
        let entry = cache
            .get(Duration::from_millis(100))
            .expect("inserted frame must be present");
        assert_eq!(entry.rgba, rgba);
        assert_eq!(entry.width, 2);
        assert_eq!(entry.height, 2);
    }

    #[test]
    fn frame_cache_insert_should_evict_oldest_when_over_budget() {
        // 2×2 RGBA = 16 bytes per frame; budget = 32 bytes = exactly 2 frames.
        let mut cache = FrameCache::new(32);
        cache.insert(Duration::ZERO, vec![0u8; 16], 2, 2);
        cache.insert(Duration::from_millis(100), vec![1u8; 16], 2, 2);
        assert_eq!(cache.used_bytes(), 32);

        // 3rd frame evicts the oldest (0 ms).
        cache.insert(Duration::from_millis(200), vec![2u8; 16], 2, 2);
        assert!(
            cache.get(Duration::ZERO).is_none(),
            "oldest frame must be evicted"
        );
        assert!(cache.get(Duration::from_millis(100)).is_some());
        assert!(cache.get(Duration::from_millis(200)).is_some());
        assert_eq!(cache.used_bytes(), 32);
    }

    #[test]
    fn frame_cache_invalidate_should_clear_all_entries() {
        let mut cache = FrameCache::new(1024);
        cache.insert(Duration::ZERO, vec![0u8; 16], 2, 2);
        cache.insert(Duration::from_millis(100), vec![0u8; 16], 2, 2);
        cache.invalidate();
        assert_eq!(cache.used_bytes(), 0);
        assert!(cache.get(Duration::ZERO).is_none());
        assert!(cache.get(Duration::from_millis(100)).is_none());
    }

    #[test]
    fn frame_cache_pts_range_should_return_none_when_empty() {
        let cache = FrameCache::new(1024);
        assert!(cache.pts_range().is_none());
    }

    #[test]
    fn frame_cache_pts_range_should_return_min_and_max_pts() {
        let mut cache = FrameCache::new(1024 * 1024);
        cache.insert(Duration::ZERO, vec![0u8; 16], 2, 2);
        cache.insert(Duration::from_millis(100), vec![0u8; 16], 2, 2);
        cache.insert(Duration::from_millis(200), vec![0u8; 16], 2, 2);
        let (min, max) = cache.pts_range().unwrap();
        assert_eq!(min, Duration::ZERO);
        assert_eq!(max, Duration::from_millis(200));
    }

    #[test]
    fn frame_cache_duplicate_pts_insert_should_be_a_no_op() {
        let mut cache = FrameCache::new(1024);
        let first = vec![1u8; 16];
        let second = vec![2u8; 16];
        cache.insert(Duration::ZERO, first.clone(), 2, 2);
        cache.insert(Duration::ZERO, second, 2, 2);
        let entry = cache.get(Duration::ZERO).unwrap();
        assert_eq!(
            entry.rgba, first,
            "duplicate insert must be a no-op; first frame must remain"
        );
        assert_eq!(cache.used_bytes(), 16);
    }

    #[test]
    fn frame_cache_oversized_frame_should_not_be_inserted() {
        let mut cache = FrameCache::new(10); // budget smaller than one frame (16 bytes)
        cache.insert(Duration::ZERO, vec![0u8; 16], 2, 2);
        assert!(
            cache.get(Duration::ZERO).is_none(),
            "oversized frame must not be inserted"
        );
        assert_eq!(cache.used_bytes(), 0);
    }

    #[test]
    #[ignore = "performance thresholds are environment-dependent; run explicitly with -- --include-ignored"]
    fn frame_cache_scrub_latency_with_cache_should_be_faster_than_without() {
        // Placeholder: open a 1080p H.264 source, seek with and without cache,
        // assert cache path is ≥ 10× faster than decode path.
    }
}

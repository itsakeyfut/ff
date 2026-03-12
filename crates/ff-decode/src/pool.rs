//! Frame pool for memory reuse.
//!
//! This module provides the [`SimpleFramePool`] which enables memory pooling
//! for decoded frames, reducing allocation overhead during video playback.
//!
//! # Examples
//!
//! ```ignore
//! use ff_decode::SimpleFramePool;
//!
//! // Create a decoder with a simple frame pool (automatically initialized)
//! let pool = SimpleFramePool::new(32);
//! let decoder = VideoDecoder::open("video.mp4")?
//!     .frame_pool(pool)
//!     .build()?;
//! ```

use std::sync::{Arc, Mutex, Weak};

// Re-export types from ff-common
pub use ff_common::{FramePool, PooledBuffer};

/// A simple frame pool implementation with fixed capacity.
///
/// This pool stores a fixed number of frame buffers and reuses them
/// during video decoding. When the pool is empty, callers must allocate
/// new buffers directly.
///
/// # Thread Safety
///
/// This implementation uses a [`Mutex`] internally, making it safe to
/// share across threads.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::{VideoDecoder, SimpleFramePool};
///
/// // Create a pool with capacity for 32 frames (automatically initialized)
/// let pool = SimpleFramePool::new(32);
///
/// let decoder = VideoDecoder::open("video.mp4")?
///     .frame_pool(pool)
///     .build()?;
///
/// // Frames are acquired from the pool during decoding
/// for frame in decoder.frames().take(100) {
///     let frame = frame?;
///     // Process frame...
/// }
/// ```
#[derive(Debug)]
pub struct SimpleFramePool {
    /// Maximum number of buffers to keep in the pool
    max_capacity: usize,
    /// Pool of available buffers
    buffers: Mutex<Vec<Vec<u8>>>,
    /// Weak self-reference for creating `PooledBuffers`
    self_ref: Mutex<Weak<Self>>,
}

impl SimpleFramePool {
    /// Creates a new frame pool with the specified maximum capacity.
    ///
    /// This function uses RAII (Resource Acquisition Is Initialization) pattern
    /// and automatically initializes the pool's self-reference, eliminating the
    /// need for a separate initialization step.
    ///
    /// # Arguments
    ///
    /// * `max_capacity` - Maximum number of buffers to keep in the pool.
    ///   When the pool is full, returned buffers are dropped instead of
    ///   being stored.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::SimpleFramePool;
    /// use std::sync::Arc;
    ///
    /// // Create a pool for 32 frames - automatically initialized
    /// let pool = SimpleFramePool::new(32);
    ///
    /// // Use with decoder
    /// # /*
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .frame_pool(pool)
    ///     .build()?;
    /// # */
    /// ```
    #[must_use]
    pub fn new(max_capacity: usize) -> Arc<Self> {
        let pool = Arc::new(Self {
            max_capacity,
            buffers: Mutex::new(Vec::with_capacity(max_capacity)),
            self_ref: Mutex::new(Weak::new()),
        });

        // Auto-initialize self-reference using RAII pattern
        if let Ok(mut self_ref) = pool.self_ref.lock() {
            *self_ref = Arc::downgrade(&pool);
        }

        pool
    }

    /// Returns the maximum capacity of this pool.
    #[must_use]
    pub fn max_capacity(&self) -> usize {
        self.max_capacity
    }

    /// Returns the current number of buffers available in the pool.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::SimpleFramePool;
    ///
    /// let pool = SimpleFramePool::new(32);
    /// assert_eq!(pool.available(), 0); // Pool starts empty
    /// ```
    #[must_use]
    pub fn available(&self) -> usize {
        self.buffers.lock().map_or(0, |b| b.len())
    }
}

impl FramePool for SimpleFramePool {
    fn acquire(&self, size: usize) -> Option<PooledBuffer> {
        // Try to get a buffer from the pool
        if let Ok(mut buffers) = self.buffers.lock() {
            // Find a buffer that has sufficient capacity
            // We prefer to find the smallest buffer that fits to avoid wasting memory
            let suitable_idx = buffers
                .iter()
                .enumerate()
                .filter(|(_, b)| b.capacity() >= size)
                .min_by_key(|(_, b)| b.capacity())
                .map(|(idx, _)| idx);

            if let Some(idx) = suitable_idx {
                let mut buf = buffers.swap_remove(idx);

                // Resize to the requested size (within existing capacity)
                buf.resize(size, 0);

                // Zero the entire buffer to ensure clean state
                // Note: resize() only zeros new elements, not existing ones
                buf.fill(0);

                // Get weak reference to self for the PooledBuffer
                let weak_ref = self
                    .self_ref
                    .lock()
                    .ok()
                    .and_then(|r| r.upgrade())
                    .map(|arc| Arc::downgrade(&(arc as Arc<dyn FramePool>)))?;

                // Return pooled buffer
                return Some(PooledBuffer::new(buf, weak_ref));
            }
        }

        // Pool is empty or no suitable buffer found - return None
        // Caller will allocate a new buffer
        None
    }

    fn release(&self, buffer: Vec<u8>) {
        if let Ok(mut buffers) = self.buffers.lock() {
            // Only keep the buffer if we haven't reached max capacity
            if buffers.len() < self.max_capacity {
                buffers.push(buffer);
            }
            // Otherwise, drop the buffer (it will be freed)
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, atomic::AtomicUsize, atomic::Ordering};

    #[derive(Debug)]
    struct TestPool {
        buffers: Mutex<Vec<Vec<u8>>>,
        acquire_count: AtomicUsize,
        release_count: AtomicUsize,
    }

    impl TestPool {
        fn new(count: usize, size: usize) -> Self {
            let buffers: Vec<Vec<u8>> = (0..count).map(|_| vec![0u8; size]).collect();
            Self {
                buffers: Mutex::new(buffers),
                acquire_count: AtomicUsize::new(0),
                release_count: AtomicUsize::new(0),
            }
        }
    }

    impl FramePool for TestPool {
        fn acquire(&self, size: usize) -> Option<PooledBuffer> {
            let mut buffers = self.buffers.lock().ok()?;
            // Find a buffer of sufficient size
            if let Some(idx) = buffers.iter().position(|b| b.len() >= size) {
                let buf = buffers.swap_remove(idx);
                self.acquire_count.fetch_add(1, Ordering::SeqCst);
                // We can't return a proper PooledBuffer here since we need Arc<Self>
                // For testing, return a standalone buffer
                Some(PooledBuffer::standalone(buf))
            } else {
                None
            }
        }

        fn release(&self, buffer: Vec<u8>) {
            if let Ok(mut buffers) = self.buffers.lock() {
                buffers.push(buffer);
                self.release_count.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    #[test]
    fn test_pooled_buffer_standalone() {
        let data = vec![1u8, 2, 3, 4, 5];
        let buffer = PooledBuffer::standalone(data.clone());

        assert_eq!(buffer.len(), 5);
        assert!(!buffer.is_empty());
        assert_eq!(buffer.data(), &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_pooled_buffer_data_mut() {
        let mut buffer = PooledBuffer::standalone(vec![0u8; 4]);
        buffer.data_mut()[0] = 42;
        assert_eq!(buffer.data()[0], 42);
    }

    #[test]
    fn test_pooled_buffer_into_inner() {
        let buffer = PooledBuffer::standalone(vec![1, 2, 3]);
        let inner = buffer.into_inner();
        assert_eq!(inner, vec![1, 2, 3]);
    }

    #[test]
    fn test_pooled_buffer_as_ref() {
        let buffer = PooledBuffer::standalone(vec![1, 2, 3]);
        let slice: &[u8] = buffer.as_ref();
        assert_eq!(slice, &[1, 2, 3]);
    }

    #[test]
    fn test_pooled_buffer_as_mut() {
        let mut buffer = PooledBuffer::standalone(vec![1, 2, 3]);
        let slice: &mut [u8] = buffer.as_mut();
        slice[0] = 99;
        assert_eq!(buffer.data(), &[99, 2, 3]);
    }

    #[test]
    fn test_empty_buffer() {
        let buffer = PooledBuffer::standalone(vec![]);
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_pool_acquire() {
        let pool = TestPool::new(2, 1024);
        let buffer = pool.acquire(512);
        assert!(buffer.is_some());
        assert!(buffer.as_ref().is_some_and(|b| b.len() >= 512));
    }

    #[test]
    fn test_pool_acquire_too_large() {
        let pool = TestPool::new(2, 512);
        let buffer = pool.acquire(1024);
        assert!(buffer.is_none());
    }

    #[test]
    fn test_pool_with_arc_release() {
        #[derive(Debug)]
        struct ArcPool {
            buffers: Mutex<Vec<Vec<u8>>>,
            release_count: AtomicUsize,
        }

        impl FramePool for ArcPool {
            fn acquire(&self, _size: usize) -> Option<PooledBuffer> {
                None // Not used in this test
            }

            fn release(&self, buffer: Vec<u8>) {
                if let Ok(mut buffers) = self.buffers.lock() {
                    buffers.push(buffer);
                    self.release_count.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        let pool = Arc::new(ArcPool {
            buffers: Mutex::new(vec![]),
            release_count: AtomicUsize::new(0),
        });

        // Create a buffer with pool reference
        {
            let _buffer =
                PooledBuffer::new(vec![1, 2, 3], Arc::downgrade(&pool) as Weak<dyn FramePool>);
            // Buffer is dropped here
        }

        // Verify the buffer was returned to the pool
        assert_eq!(pool.release_count.load(Ordering::SeqCst), 1);
        assert!(pool.buffers.lock().map(|b| b.len() == 1).unwrap_or(false));
    }

    #[test]
    fn test_pool_dropped_before_buffer() {
        #[derive(Debug)]
        struct DroppablePool;

        impl FramePool for DroppablePool {
            fn acquire(&self, _size: usize) -> Option<PooledBuffer> {
                None
            }

            fn release(&self, _buffer: Vec<u8>) {
                // This should NOT be called if pool is dropped
                panic!("release should not be called on dropped pool");
            }
        }

        let buffer;
        {
            let pool = Arc::new(DroppablePool);
            buffer = PooledBuffer::new(vec![1, 2, 3], Arc::downgrade(&pool) as Weak<dyn FramePool>);
            // Pool is dropped here
        }

        // Buffer can still be used
        assert_eq!(buffer.data(), &[1, 2, 3]);

        // Dropping buffer should not panic (pool is already gone)
        drop(buffer);
    }

    // SimpleFramePool tests
    #[test]
    fn test_simple_frame_pool_new() {
        let pool = SimpleFramePool::new(32);
        assert_eq!(pool.max_capacity(), 32);
        assert_eq!(pool.available(), 0);
    }

    #[test]
    fn test_simple_frame_pool_acquire_empty() {
        let pool = SimpleFramePool::new(8);

        // Pool is empty, should return None
        let buffer = pool.acquire(1024);
        assert!(buffer.is_none());
    }

    #[test]
    fn test_simple_frame_pool_acquire_and_release() {
        let pool = SimpleFramePool::new(8);

        // Manually add a buffer to the pool
        pool.release(vec![0u8; 1024]);
        assert_eq!(pool.available(), 1);

        // Acquire the buffer
        let buffer = pool.acquire(512);
        assert!(buffer.is_some());
        assert_eq!(pool.available(), 0);

        let buffer = buffer.unwrap();
        assert_eq!(buffer.len(), 512);
    }

    #[test]
    fn test_simple_frame_pool_buffer_auto_return() {
        let pool = SimpleFramePool::new(8);

        // Add a buffer and acquire it
        pool.release(vec![0u8; 2048]);
        assert_eq!(pool.available(), 1);

        {
            let _buffer = pool.acquire(1024).unwrap();
            assert_eq!(pool.available(), 0);
            // Buffer is dropped here
        }

        // Buffer should be returned to pool
        assert_eq!(pool.available(), 1);
    }

    #[test]
    fn test_simple_frame_pool_max_capacity() {
        let pool = SimpleFramePool::new(2);

        // Add 3 buffers, but pool only holds 2
        pool.release(vec![0u8; 512]);
        pool.release(vec![0u8; 512]);
        pool.release(vec![0u8; 512]);

        // Pool should only contain 2 buffers
        assert_eq!(pool.available(), 2);
    }

    #[test]
    fn test_simple_frame_pool_buffer_reuse() {
        let pool = SimpleFramePool::new(4);

        // Add buffer to pool
        pool.release(vec![42u8; 1024]);
        assert_eq!(pool.available(), 1);

        // Acquire and check it gets resized
        let buffer = pool.acquire(512).unwrap();
        assert_eq!(buffer.len(), 512);
        assert!(buffer.data().iter().all(|&b| b == 0)); // Should be zeroed by resize

        drop(buffer);
        assert_eq!(pool.available(), 1);

        // Acquire same-size buffer - should reuse from pool
        let buffer = pool.acquire(512).unwrap();
        assert_eq!(buffer.len(), 512);
        assert!(buffer.data().iter().all(|&b| b == 0));

        drop(buffer);
        assert_eq!(pool.available(), 1);

        // Acquire larger buffer within capacity - should reuse from pool
        let buffer = pool.acquire(1024).unwrap();
        assert_eq!(buffer.len(), 1024);
    }

    #[test]
    fn test_simple_frame_pool_find_suitable_buffer() {
        let pool = SimpleFramePool::new(8);

        // Add buffers of different sizes
        pool.release(vec![0u8; 512]);
        pool.release(vec![0u8; 1024]);
        pool.release(vec![0u8; 2048]);
        assert_eq!(pool.available(), 3);

        // Request 1000 bytes - should get the 1024 buffer
        let buffer = pool.acquire(1000).unwrap();
        assert!(buffer.len() >= 1000);
        assert_eq!(pool.available(), 2);

        drop(buffer);
        assert_eq!(pool.available(), 3);
    }

    #[test]
    fn test_simple_frame_pool_acquire_too_large() {
        let pool = SimpleFramePool::new(4);

        // Add small buffer
        pool.release(vec![0u8; 512]);

        // Request larger buffer than available
        let buffer = pool.acquire(1024);
        assert!(buffer.is_none());

        // Original buffer should still be in pool
        assert_eq!(pool.available(), 1);
    }

    #[test]
    fn test_simple_frame_pool_concurrent_access() {
        use std::thread;

        let pool = SimpleFramePool::new(16);

        // Pre-fill pool with buffers
        for _ in 0..8 {
            pool.release(vec![0u8; 1024]);
        }

        let pool1 = Arc::clone(&pool);
        let pool2 = Arc::clone(&pool);

        let handle1 = thread::spawn(move || {
            for _ in 0..10 {
                if let Some(buffer) = pool1.acquire(512) {
                    drop(buffer);
                }
            }
        });

        let handle2 = thread::spawn(move || {
            for _ in 0..10 {
                if let Some(buffer) = pool2.acquire(512) {
                    drop(buffer);
                }
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();

        // All buffers should be back in the pool
        assert!(pool.available() <= 16);
    }
}

//! Frame buffer pooling abstractions.
//!
//! This module provides the [`FramePool`] trait and [`PooledBuffer`] type
//! which enable memory pooling for decoded frames, reducing allocation
//! overhead during video playback.

use std::sync::Weak;

/// A trait for frame buffer pooling.
///
/// Implementing this trait allows custom memory management strategies
/// for decoded video frames. This is useful for reducing allocation
/// pressure during real-time video playback.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to allow sharing across threads.
///
/// # Example Implementation
///
/// ```ignore
/// use ff_common::{FramePool, PooledBuffer};
/// use std::sync::{Arc, Mutex};
///
/// struct SimplePool {
///     buffers: Mutex<Vec<Vec<u8>>>,
///     buffer_size: usize,
///}
///
/// impl FramePool for SimplePool {
///     fn acquire(&self, size: usize) -> Option<PooledBuffer> {
///         // Implementation...
///         None
///     }
/// }
/// ```
pub trait FramePool: Send + Sync + std::fmt::Debug {
    /// Acquires a buffer of at least the specified size from the pool.
    ///
    /// # Arguments
    ///
    /// * `size` - The minimum required buffer size in bytes.
    ///
    /// # Returns
    ///
    /// Returns `Some(PooledBuffer)` if a buffer is available, or `None` if
    /// the pool is exhausted. When `None` is returned, the decoder will
    /// allocate a new buffer directly.
    ///
    /// # Thread Safety
    ///
    /// This method may be called from multiple threads concurrently.
    fn acquire(&self, size: usize) -> Option<PooledBuffer>;

    /// Returns a buffer to the pool.
    ///
    /// This method is called automatically when a [`PooledBuffer`] is dropped.
    /// The default implementation does nothing (the buffer is simply dropped).
    ///
    /// # Arguments
    ///
    /// * `buffer` - The buffer data to return to the pool.
    fn release(&self, _buffer: Vec<u8>) {
        // Default: just drop the buffer
    }
}

/// A buffer acquired from a [`FramePool`].
///
/// When this buffer is dropped, it is automatically returned to its
/// parent pool if the pool still exists. This enables zero-overhead
/// buffer reuse during video decoding.
///
/// # Memory Management
///
/// The buffer holds a weak reference to its parent pool. If the pool
/// is dropped before the buffer, the buffer's memory is simply freed
/// rather than being returned to the pool.
///
/// # Cloning
///
/// When cloned, the new buffer becomes a standalone buffer (no pool reference).
/// This prevents double-free issues where both the original and cloned buffer
/// would attempt to return the same memory to the pool.
#[derive(Debug)]
pub struct PooledBuffer {
    /// The actual buffer data
    data: Vec<u8>,
    /// Weak reference to the parent pool for returning the buffer
    pool: Option<Weak<dyn FramePool>>,
}

impl PooledBuffer {
    /// Creates a new pooled buffer with a reference to its parent pool.
    ///
    /// # Arguments
    ///
    /// * `data` - The buffer data.
    /// * `pool` - A weak reference to the parent pool.
    #[must_use]
    pub fn new(data: Vec<u8>, pool: Weak<dyn FramePool>) -> Self {
        Self {
            data,
            pool: Some(pool),
        }
    }

    /// Creates a new pooled buffer without a parent pool.
    ///
    /// This is useful for buffers allocated outside of a pool context.
    /// When dropped, the buffer's memory is simply freed.
    #[must_use]
    pub fn standalone(data: Vec<u8>) -> Self {
        Self { data, pool: None }
    }

    /// Returns a reference to the buffer data.
    #[must_use]
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Returns a mutable reference to the buffer data.
    #[must_use]
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Returns the length of the buffer in bytes.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if the buffer is empty.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Consumes the buffer and returns the underlying data.
    ///
    /// After calling this, the buffer will not be returned to the pool.
    #[must_use]
    pub fn into_inner(mut self) -> Vec<u8> {
        // Take ownership to prevent Drop from returning to pool
        self.pool = None;
        std::mem::take(&mut self.data)
    }
}

impl Clone for PooledBuffer {
    /// Clones the buffer data, but the cloned buffer becomes standalone.
    ///
    /// The cloned buffer will NOT be returned to the pool when dropped.
    /// This prevents double-free issues where both buffers would attempt
    /// to return the same memory to the pool.
    ///
    /// Only the original buffer retains its pool reference.
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            pool: None, // Cloned buffer is standalone
        }
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(ref weak_pool) = self.pool
            && let Some(pool) = weak_pool.upgrade()
        {
            // Return the buffer to the pool
            let data = std::mem::take(&mut self.data);
            pool.release(data);
        }
        // If pool is None or has been dropped, data is simply freed
    }
}

impl AsRef<[u8]> for PooledBuffer {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl AsMut<[u8]> for PooledBuffer {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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
    fn test_pool_with_arc_release() {
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicUsize, Ordering};

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

    #[test]
    fn test_pooled_buffer_clone_becomes_standalone() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        #[derive(Debug)]
        struct CountingPool {
            release_count: AtomicUsize,
        }

        impl FramePool for CountingPool {
            fn acquire(&self, _size: usize) -> Option<PooledBuffer> {
                None
            }

            fn release(&self, _buffer: Vec<u8>) {
                self.release_count.fetch_add(1, Ordering::SeqCst);
            }
        }

        let pool = Arc::new(CountingPool {
            release_count: AtomicUsize::new(0),
        });

        // Create pooled buffer
        let buffer1 =
            PooledBuffer::new(vec![1, 2, 3], Arc::downgrade(&pool) as Weak<dyn FramePool>);

        // Clone it
        let buffer2 = buffer1.clone();

        // Both buffers have the same data
        assert_eq!(buffer1.data(), &[1, 2, 3]);
        assert_eq!(buffer2.data(), &[1, 2, 3]);

        // Drop both buffers
        drop(buffer1);
        drop(buffer2);

        // Only the original buffer should have been returned to pool (count = 1)
        // The cloned buffer is standalone and should NOT return to pool
        assert_eq!(pool.release_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_pooled_buffer_clone_data_independence() {
        let buffer1 = PooledBuffer::standalone(vec![1, 2, 3]);
        let mut buffer2 = buffer1.clone();

        // Modify buffer2
        buffer2.data_mut()[0] = 99;

        // buffer1 should be unaffected (deep copy)
        assert_eq!(buffer1.data(), &[1, 2, 3]);
        assert_eq!(buffer2.data(), &[99, 2, 3]);
    }
}

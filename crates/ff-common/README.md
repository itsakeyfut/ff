# ff-common

Common types and traits for the ff-* crate family.

## Overview

`ff-common` provides shared abstractions used across all ff-* crates, particularly for memory management and buffer pooling. It has no external dependencies and serves as the pure-Rust foundation of the ff-* ecosystem.

## Features

- **Frame buffer pooling**: Reusable buffer allocation via `FramePool` trait
- **Zero-copy design**: `PooledBuffer` returns memory to the pool automatically on drop
- **Thread-safe**: `FramePool` requires `Send + Sync`
- **No external dependencies**: Pure Rust, no FFmpeg linkage required

## Minimum Supported Rust Version

Rust 1.93.0 or later (edition 2024).

## Usage

### Implementing a Custom Pool

```rust
use ff_common::{FramePool, PooledBuffer};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
struct SimplePool {
    buffers: Mutex<Vec<Vec<u8>>>,
}

impl FramePool for SimplePool {
    fn acquire(&self, size: usize) -> Option<PooledBuffer> {
        let mut pool = self.buffers.lock().ok()?;
        let buf = pool
            .iter()
            .position(|b| b.len() >= size)
            .map(|i| pool.remove(i))
            .unwrap_or_else(|| vec![0u8; size]);
        Some(PooledBuffer::new(buf, Arc::downgrade(&Arc::new(self))))
    }
}
```

### Using PooledBuffer Standalone

```rust
use ff_common::PooledBuffer;

// Allocate a standalone buffer (not pooled)
let mut buf = PooledBuffer::standalone(vec![0u8; 1920 * 1080 * 4]);
buf.data_mut().fill(0xff);
assert_eq!(buf.len(), 1920 * 1080 * 4);
```

## Module Structure

```
ff-common/src/
├── lib.rs      # Crate root, re-exports
└── pool.rs     # FramePool trait, PooledBuffer struct
```

## Related Crates

This crate is part of the ff-* crate family:

- **ff-format** - Type-safe pixel/sample formats, timestamps, stream info
- **ff-probe** - Media metadata extraction
- **ff-decode** - Video/audio decoding
- **ff-encode** - Video/audio encoding
- **ff-sys** - Low-level FFmpeg FFI bindings

## License

MIT OR Apache-2.0

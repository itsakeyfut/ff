# ff-common

Shared buffer-pooling abstractions for the ff-* crate family.

> **Project status (as of 2026-03-26):** This crate is in an early phase. The high-level API is designed and reviewed by hand; AI is used as an accelerator to implement FFmpeg bindings efficiently. Code contributions are not expected at this time — questions, bug reports, and feature requests are welcome. See the [main repository](https://github.com/itsakeyfut/avio) for full context.

## Overview

`ff-common` provides the `FramePool` trait and `PooledBuffer` type used internally across the `ff-*` crates. It has no external dependencies and does not link against FFmpeg.

`PooledBuffer` wraps an allocated block of memory and returns it to the originating pool automatically when dropped — no manual free call is needed. If no pool is associated, the memory is simply deallocated. `FramePool` is `Send + Sync`, so pools can be shared across threads without additional locking.

## Usage

`ff-common` is an internal workspace crate. It is not intended for direct use in application code. The following example shows the `PooledBuffer::standalone` constructor, which allocates a buffer without a backing pool:

```rust
use ff_common::PooledBuffer;

// Allocate a 4096-byte buffer with no pool backing.
// Memory is freed normally when `buf` is dropped.
let buf = PooledBuffer::standalone(4096);
assert_eq!(buf.len(), 4096);
```

## MSRV

Rust 1.93.0 (edition 2024).

## License

MIT OR Apache-2.0

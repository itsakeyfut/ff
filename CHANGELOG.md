# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [0.1.1] - 2026-03-11

### Fixed

#### ff-sys
- Raise minimum required FFmpeg version from 6.0 to 7.0. FFmpeg 7.x converted
  scaling flags from `#define` macros to `enum SwsFlags`, making 6.x
  incompatible with the bindgen-generated `SwsFlags_SWS_*` constants ([#1](https://github.com/itsakeyfut/avio/pull/1))

---

## [0.1.0] - 2026-03-11

### Added

#### ff-sys
- Raw FFmpeg FFI bindings via bindgen
- Platform-specific build support: pkg-config (Linux), Homebrew (macOS), vcpkg (Windows)

#### ff-common
- Shared error types and common utilities

#### ff-format
- Container demux and mux support

#### ff-probe
- Media metadata extraction (codec, resolution, duration, bitrate, streams)

#### ff-decode
- Video and audio decoding
- Hardware acceleration support (NVENC, QSV, VideoToolbox, VAAPI)

#### ff-encode
- Video and audio encoding
- Hardware acceleration support
- Thumbnail generation

#### ff-filter
- Placeholder crate (filter graph not yet implemented)

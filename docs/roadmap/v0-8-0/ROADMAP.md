# v0.8.0 — Network Input & Live Streaming

**Goal**: Accept any network URL as a media source and push live output to HLS, DASH, RTMP, and SRT targets — making the library usable for live video ingest, real-time transcoding, and broadcast delivery.

**Prerequisite**: v0.7.0 complete.

**Crates in scope**: `ff-decode`, `ff-stream`

---

## Requirements

### Network Input

- RTMP streams (`rtmp://`) can be opened as a video/audio source — e.g., ingest from OBS or a hardware encoder.
- RTSP streams (`rtsp://`) can be opened — e.g., IP cameras or surveillance feeds.
- HTTP/HTTPS progressive download streams can be opened.
- HLS streams (`m3u8`) can be opened as an input source, including live playlists.
- DASH streams can be opened as an input source.
- UDP/MPEG-TS streams (`udp://`) can be opened — e.g., broadcast SDI-over-IP feeds.
- SRT streams (`srt://`) can be opened for low-latency, loss-resilient transport.
- Connection timeout and read timeout are configurable per source.
- The decoder can automatically reconnect after a dropped connection, with a configurable retry limit and backoff.
- Live streams are detected automatically: random-access seeking is disabled when the source is non-seekable (`AVFMT_TS_DISCONT`).

### Live HLS Output

- Video and audio frames can be pushed frame-by-frame to generate HLS output in real-time.
- HLS segment duration is configurable.
- The playlist sliding window size is configurable (number of segments retained in `.m3u8`); 0 means retain all.
- Segment files and the `.m3u8` playlist are written atomically to avoid serving partial files.
- The output can be served directly by a static file server (nginx, S3, CloudFront).

### Live DASH Output

- Video and audio frames can be pushed frame-by-frame to generate DASH output in real-time.
- Segment duration is configurable.
- `manifest.mpd` is updated incrementally with each new segment.

### RTMP Output

- Video and audio frames can be pushed to any RTMP ingest endpoint (YouTube Live, Twitch, Facebook Live, custom Wowza/nginx-rtmp servers).
- Video and audio codecs are selectable.
- The RTMP URL format is `rtmp://server/app/stream_key`.
- FFmpeg's built-in RTMP protocol is used; `librtmp` is not required.

### SRT Output

- Video and audio frames can be pushed over SRT for low-latency delivery.
- Caller and listener connection modes are both supported.
- Encryption passphrase and key length are configurable.

### Multi-target Output

- The same frame can be pushed to multiple output targets simultaneously (e.g., HLS + RTMP at the same time) via a fan-out wrapper, without re-encoding.

### ABR Ladder Output

- An adaptive bitrate (ABR) ladder can be produced in a single pipeline: one input is encoded at multiple quality levels and simultaneously packaged as a multi-rendition HLS or DASH output.
- Rendition parameters (resolution, bitrate, codec) are fully configurable.

### Error Handling

- `DecodeError::NetworkTimeout` is returned when the connection or read timeout is exceeded.
- `DecodeError::ConnectionFailed` is returned when the host is unreachable.
- `DecodeError::StreamInterrupted` is returned when a live stream drops mid-session.
- Reconnect logic emits `log::warn!` on each retry attempt.

---

## Design Decisions

| Topic | Decision |
|---|---|
| Existing HlsOutput / DashOutput | Unchanged — file-based offline muxing is retained as-is |
| Live vs offline output | `LiveHlsOutput` / `LiveDashOutput` use a frame-push API; offline muxers use the existing `OutputWriter` |
| RTMP | FFmpeg built-in `rtmp://` protocol; no `librtmp` dependency |
| SRT | Requires FFmpeg built with `libsrt`; gated behind `srt` feature flag |
| Timeouts | Set via `AVDictionary` (`timeout`, `stimeout`) on `avformat_open_input` |
| Multi-target | Fan-out is a thin coordinator; encoding happens once and packets are duplicated |

---

## Definition of Done

- `VideoDecoder::open_url()` tested with RTSP and HTTP sources
- `LiveHlsOutput` produces a valid `.m3u8` and `.ts` segments playable by `ffplay`
- `RtmpOutput` connects to a local nginx-rtmp server in CI
- ABR ladder test produces at least two rendition streams in a single HLS output

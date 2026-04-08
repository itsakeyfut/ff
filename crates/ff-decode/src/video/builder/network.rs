//! Network and image-sequence options for [`VideoDecoderBuilder`].

use ff_format::NetworkOptions;

use super::VideoDecoderBuilder;

impl VideoDecoderBuilder {
    /// Sets the frame rate for image sequence decoding.
    ///
    /// Only used when the path contains `%` (e.g. `"frames/frame%04d.png"`).
    /// Defaults to 25 fps when not set.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// let decoder = VideoDecoder::open("frames/frame%04d.png")?
    ///     .frame_rate(30)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn frame_rate(mut self, fps: u32) -> Self {
        self.frame_rate = Some(fps);
        self
    }

    /// Sets network options for URL-based sources.
    ///
    /// When set, the builder skips the file-existence check and passes connect
    /// and read timeouts to `avformat_open_input` via an `AVDictionary`.
    /// Call this before `.build()` when opening `rtmp://`, `rtsp://`, `http://`,
    /// `https://`, `udp://`, `srt://`, or `rtp://` URLs.
    ///
    /// # HLS / M3U8 Playlists
    ///
    /// HLS playlists (`.m3u8`) are detected automatically by `FFmpeg` — no extra
    /// configuration is required beyond calling `.network()`. Pass the full
    /// HTTP(S) URL of the master or media playlist:
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use ff_format::NetworkOptions;
    ///
    /// let decoder = VideoDecoder::open("https://example.com/live/index.m3u8")
    ///     .network(NetworkOptions::default())
    ///     .build()?;
    /// ```
    ///
    /// # DASH / MPD Streams
    ///
    /// MPEG-DASH manifests (`.mpd`) are detected automatically by `FFmpeg`'s
    /// built-in `dash` demuxer. The demuxer downloads the manifest, selects the
    /// highest-quality representation, and fetches segments automatically:
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use ff_format::NetworkOptions;
    ///
    /// let decoder = VideoDecoder::open("https://example.com/dash/manifest.mpd")
    ///     .network(NetworkOptions::default())
    ///     .build()?;
    /// ```
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use ff_format::NetworkOptions;
    ///
    /// let decoder = VideoDecoder::open("rtmp://live.example.com/app/stream_key")
    ///     .network(NetworkOptions::default())
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn network(mut self, opts: NetworkOptions) -> Self {
        self.network_opts = Some(opts);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn builder_frame_rate_should_set_fps() {
        let builder =
            VideoDecoderBuilder::new(PathBuf::from("frames/frame%04d.png")).frame_rate(30);

        assert_eq!(builder.frame_rate, Some(30));
    }

    #[test]
    fn builder_frame_rate_last_setter_wins() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("frames/frame%04d.png"))
            .frame_rate(25)
            .frame_rate(60);

        assert_eq!(builder.frame_rate, Some(60));
    }

    #[test]
    fn builder_network_should_set_network_opts() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("rtmp://example.com/live/stream"))
            .network(NetworkOptions::default());

        assert!(builder.network_opts.is_some());
    }

    #[test]
    fn builder_frame_rate_default_should_be_none() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("frames/frame%04d.png"));

        assert!(builder.frame_rate.is_none());
    }
}

//! Internal `macro_rules!` helpers that eliminate builder boilerplate shared
//! across the frame-push stream output types.
//!
//! Loaded via `#[macro_use] mod builder_macros` in `lib.rs` so every stream
//! output module can invoke the macros without a path prefix.

// ============================================================================
// impl_frame_push_stream_output!
// ============================================================================

/// Generates the [`StreamOutput`] implementation for a frame-push stream output
/// type.
///
/// **Assumes** the struct has:
/// - `inner: Option<T>` — where `T` exposes `push_video`, `push_audio`, and
///   `flush_and_close` methods.
/// - `finished: bool`
///
/// [`StreamOutput`]: crate::output::StreamOutput
macro_rules! impl_frame_push_stream_output {
    ($type:ident) => {
        impl crate::output::StreamOutput for $type {
            fn push_video(
                &mut self,
                frame: &ff_format::VideoFrame,
            ) -> Result<(), crate::error::StreamError> {
                if self.finished {
                    return Err(crate::error::StreamError::InvalidConfig {
                        reason: "push_video called after finish()".into(),
                    });
                }
                let inner = self.inner.as_mut().ok_or_else(|| {
                    crate::error::StreamError::InvalidConfig {
                        reason: "push_video called before build()".into(),
                    }
                })?;
                inner.push_video(frame)
            }

            fn push_audio(
                &mut self,
                frame: &ff_format::AudioFrame,
            ) -> Result<(), crate::error::StreamError> {
                if self.finished {
                    return Err(crate::error::StreamError::InvalidConfig {
                        reason: "push_audio called after finish()".into(),
                    });
                }
                let inner = self.inner.as_mut().ok_or_else(|| {
                    crate::error::StreamError::InvalidConfig {
                        reason: "push_audio called before build()".into(),
                    }
                })?;
                inner.push_audio(frame);
                Ok(())
            }

            fn finish(mut self: Box<Self>) -> Result<(), crate::error::StreamError> {
                if self.finished {
                    return Ok(());
                }
                self.finished = true;
                let inner =
                    self.inner
                        .take()
                        .ok_or_else(|| crate::error::StreamError::InvalidConfig {
                            reason: "finish() called before build()".into(),
                        })?;
                inner.flush_and_close();
                Ok(())
            }
        }
    };
}

// ============================================================================
// impl_live_stream_setters!
// ============================================================================

/// Generates the common setter methods for a live stream output builder.
///
/// Two variants select the `audio()` setter body:
/// - `required_audio` — `sample_rate`/`channels` fields are `u32` (RTMP, SRT).
/// - `optional_audio` — `sample_rate`/`channels` fields are `Option<u32>`
///   (`LiveHLS`, `LiveDASH`).
///
/// **Assumes** the struct has:
/// - `video_width: Option<u32>`, `video_height: Option<u32>`, `fps: Option<f64>`
/// - `video_codec: VideoCodec`, `audio_codec: AudioCodec`
/// - `video_bitrate: u64`, `audio_bitrate: u64`
/// - For `required_audio`: `sample_rate: u32`, `channels: u32`
/// - For `optional_audio`: `sample_rate: Option<u32>`, `channels: Option<u32>`
macro_rules! impl_live_stream_setters {
    ($type:ident, required_audio) => {
        impl $type {
            /// Set the video encoding parameters (width, height, fps).
            ///
            /// This method **must** be called before [`build`](Self::build).
            #[must_use]
            pub fn video(mut self, width: u32, height: u32, fps: f64) -> Self {
                self.video_width = Some(width);
                self.video_height = Some(height);
                self.fps = Some(fps);
                self
            }

            /// Set the audio sample rate and channel count.
            ///
            /// Defaults: 44 100 Hz, 2 channels (stereo).
            #[must_use]
            pub fn audio(mut self, sample_rate: u32, channels: u32) -> Self {
                self.sample_rate = sample_rate;
                self.channels = channels;
                self
            }

            /// Set the video codec.
            ///
            /// Default: [`VideoCodec::H264`].
            #[must_use]
            pub fn video_codec(mut self, codec: ff_format::VideoCodec) -> Self {
                self.video_codec = codec;
                self
            }

            /// Set the audio codec.
            ///
            /// Default: [`AudioCodec::Aac`].
            #[must_use]
            pub fn audio_codec(mut self, codec: ff_format::AudioCodec) -> Self {
                self.audio_codec = codec;
                self
            }

            /// Set the video encoder target bit rate in bits/s.
            #[must_use]
            pub fn video_bitrate(mut self, bitrate: u64) -> Self {
                self.video_bitrate = bitrate;
                self
            }

            /// Set the audio encoder target bit rate in bits/s.
            ///
            /// Default: 128 000 (128 kbit/s).
            #[must_use]
            pub fn audio_bitrate(mut self, bitrate: u64) -> Self {
                self.audio_bitrate = bitrate;
                self
            }
        }
    };

    ($type:ident, optional_audio) => {
        impl $type {
            /// Set the video encoding parameters (width, height, fps).
            ///
            /// This method **must** be called before [`build`](Self::build).
            #[must_use]
            pub fn video(mut self, width: u32, height: u32, fps: f64) -> Self {
                self.video_width = Some(width);
                self.video_height = Some(height);
                self.fps = Some(fps);
                self
            }

            /// Enable audio output with the given sample rate and channel count.
            ///
            /// If this method is not called, audio is disabled.
            #[must_use]
            pub fn audio(mut self, sample_rate: u32, channels: u32) -> Self {
                self.sample_rate = Some(sample_rate);
                self.channels = Some(channels);
                self
            }

            /// Set the video codec.
            ///
            /// Default: [`VideoCodec::H264`].
            #[must_use]
            pub fn video_codec(mut self, codec: ff_format::VideoCodec) -> Self {
                self.video_codec = codec;
                self
            }

            /// Set the audio codec.
            ///
            /// Default: [`AudioCodec::Aac`].
            #[must_use]
            pub fn audio_codec(mut self, codec: ff_format::AudioCodec) -> Self {
                self.audio_codec = codec;
                self
            }

            /// Set the video encoder target bit rate in bits/s.
            #[must_use]
            pub fn video_bitrate(mut self, bitrate: u64) -> Self {
                self.video_bitrate = bitrate;
                self
            }

            /// Set the audio encoder target bit rate in bits/s.
            ///
            /// Default: 128 000 (128 kbit/s).
            #[must_use]
            pub fn audio_bitrate(mut self, bitrate: u64) -> Self {
                self.audio_bitrate = bitrate;
                self
            }
        }
    };
}

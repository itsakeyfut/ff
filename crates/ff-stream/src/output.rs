//! Common interface for live stream outputs.

use ff_format::{AudioFrame, VideoFrame};

use crate::error::StreamError;

/// Common interface for all live stream outputs.
///
/// Implementors: [`LiveHlsOutput`](crate::live_hls::LiveHlsOutput) (and future
/// `LiveDashOutput`, `RtmpOutput`, etc.).
pub trait StreamOutput: Send {
    /// Push one video frame into the stream.
    fn push_video(&mut self, frame: &VideoFrame) -> Result<(), StreamError>;

    /// Push one audio frame into the stream.
    fn push_audio(&mut self, frame: &AudioFrame) -> Result<(), StreamError>;

    /// Flush all buffered data and close the output.
    ///
    /// Consumes the boxed value so the output cannot be used after finishing.
    fn finish(self: Box<Self>) -> Result<(), StreamError>;
}

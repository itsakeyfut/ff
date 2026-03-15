//! Error types for pipeline operations.
//!
//! This module provides [`PipelineError`], which covers all failure modes that
//! can arise when building or running a [`Pipeline`](crate::Pipeline).

/// Errors that can occur while building or running a pipeline.
///
/// # Error Categories
///
/// - **Downstream errors**: [`Decode`](Self::Decode), [`Filter`](Self::Filter),
///   [`Encode`](Self::Encode) — propagated from the underlying crates via `#[from]`
/// - **Configuration errors**: [`NoInput`](Self::NoInput), [`NoOutput`](Self::NoOutput)
///   — returned by [`PipelineBuilder::build`](crate::PipelineBuilder::build)
/// - **Runtime control**: [`Cancelled`](Self::Cancelled) — returned by
///   [`Pipeline::run`](crate::Pipeline::run) when the progress callback returns `false`
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// A decoding step failed.
    ///
    /// Wraps [`ff_decode::DecodeError`] and is produced automatically via `#[from]`
    /// when a decode operation inside the pipeline returns an error.
    #[error("decode failed: {0}")]
    Decode(#[from] ff_decode::DecodeError),

    /// A filter graph step failed.
    ///
    /// Wraps [`ff_filter::FilterError`] and is produced automatically via `#[from]`
    /// when the filter graph inside the pipeline returns an error.
    #[error("filter failed: {0}")]
    Filter(#[from] ff_filter::FilterError),

    /// An encoding step failed.
    ///
    /// Wraps [`ff_encode::EncodeError`] and is produced automatically via `#[from]`
    /// when an encode operation inside the pipeline returns an error.
    #[error("encode failed: {0}")]
    Encode(#[from] ff_encode::EncodeError),

    /// No input path was provided to the builder.
    ///
    /// At least one call to [`PipelineBuilder::input`](crate::PipelineBuilder::input)
    /// is required before [`PipelineBuilder::build`](crate::PipelineBuilder::build).
    #[error("no input specified")]
    NoInput,

    /// No output path and config were provided to the builder.
    ///
    /// A call to [`PipelineBuilder::output`](crate::PipelineBuilder::output) is
    /// required before [`PipelineBuilder::build`](crate::PipelineBuilder::build).
    #[error("no output specified")]
    NoOutput,

    /// The pipeline was cancelled by the progress callback.
    ///
    /// Returned by [`Pipeline::run`](crate::Pipeline::run) when the
    /// [`ProgressCallback`](crate::ProgressCallback) returns `false`.
    #[error("pipeline cancelled by caller")]
    Cancelled,
}

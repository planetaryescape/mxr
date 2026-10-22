//! Spans for model calls, in the shape Langfuse reads. A developer tool:
//! with the `langfuse` feature these are OpenTelemetry spans the daemon can
//! export; without it every call here is empty and compiles away, and the
//! OpenTelemetry crate isn't built, so a shipped binary carries no tracing.

#[cfg(feature = "langfuse")]
mod langfuse;
#[cfg(feature = "langfuse")]
pub use langfuse::*;

#[cfg(not(feature = "langfuse"))]
mod off;
#[cfg(not(feature = "langfuse"))]
pub use off::*;

//! Reading (blueprint 22, phase 5): newsletters as an edition you visit,
//! not a pile you owe.
//!
//! Plain data in, plain data out; the daemon reads the store and calls in.
//!
//! - [`extract`](mod@extract): one issue as readable items (shape, headline, standfirst,
//!   digest links, words), by rules and no model.
//! - [`fade`]: each source's window, from its usual gap between issues.
//! - [`pace`]: minutes from words, at the reader's own pace once measured.
//! - [`edition`]: the time bands and the ranking inside them.
//! - [`article`] and [`fetch`]: the linked article, fetched only on request,
//!   through the same guards as a remote LLM endpoint.
//! - [`export`]: highlights as Markdown.
//! - [`marginalia`]: send an item to Marginalia and read its highlights back,
//!   only once the user connected it and asked.

pub mod article;
pub mod edition;
pub mod export;
pub mod extract;
pub mod fade;
pub mod fetch;
pub mod headline;
pub mod marginalia;
pub mod pace;
pub mod urls;

pub use extract::{extract, Extraction, IssueInput, LinkItem, Paragraph, ParagraphKind, Shape};

/// Every parse this crate runs in the HTML helper process. The `mxr` binary
/// registers them in its helper mode.
pub fn html_jobs() -> Vec<&'static dyn mxr_html_worker::AnyJob> {
    let mut jobs = extract::jobs();
    jobs.extend(article::jobs());
    jobs
}

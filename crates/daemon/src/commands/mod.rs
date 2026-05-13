pub mod accounts;
pub mod bug_report;
pub mod cat;
pub mod completions;
pub mod config;
pub mod contacts;
pub mod count;
pub mod demo;
pub mod doctor;
pub mod draft_assist;
pub mod events;
pub mod export;
pub mod headers;
pub mod history;
pub mod labels;
pub mod llm;
pub mod logs;
pub mod mutations;
pub mod notify;
pub mod remind;
pub mod replies;
pub mod reset;
pub mod response_time;
pub mod rules;
pub mod saved;
pub mod screener;
pub mod search;
pub mod selection;
pub mod semantic;
pub mod sender;
pub mod setup;
pub mod snippets;
pub mod stale;
pub mod status;
pub mod storage;
pub mod subscriptions;
pub mod summarize;
pub mod sync_cmd;
pub mod thread;
pub mod version;
pub mod web;
pub mod wrapped;

use mxr_protocol::Response;

/// Extract a typed value from a daemon `Response`, converting `Response::Error`
/// into an `anyhow` error and rejecting unexpected variants.
pub(crate) fn expect_response<F, T>(resp: Response, extract: F) -> anyhow::Result<T>
where
    F: FnOnce(Response) -> Option<T>,
{
    match resp {
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        other => extract(other).ok_or_else(|| anyhow::anyhow!("unexpected response from daemon")),
    }
}

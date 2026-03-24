mod json;
mod llm;
mod markdown;
mod mbox;

pub use json::export_json;
pub use llm::export_llm_context;
pub use markdown::export_markdown;
pub use mbox::export_mbox;

use crate::mxr_core::types::ExportFormat;
use crate::mxr_reader::ReaderConfig;
use chrono::{DateTime, Utc};

/// Input data for export. The caller (daemon/CLI) assembles this from store queries.
#[derive(Debug, Clone)]
pub struct ExportThread {
    pub thread_id: String,
    pub subject: String,
    pub messages: Vec<ExportMessage>,
}

#[derive(Debug, Clone)]
pub struct ExportMessage {
    pub id: String,
    pub from_name: Option<String>,
    pub from_email: String,
    pub to: Vec<String>,
    pub date: DateTime<Utc>,
    pub subject: String,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub headers_raw: Option<String>,
    pub attachments: Vec<ExportAttachment>,
}

#[derive(Debug, Clone)]
pub struct ExportAttachment {
    pub filename: String,
    pub size_bytes: u64,
    pub local_path: Option<String>,
}

/// Export a thread in the given format. Returns the exported string.
pub fn export(
    thread: &ExportThread,
    format: &ExportFormat,
    reader_config: &ReaderConfig,
) -> String {
    match format {
        ExportFormat::Markdown => export_markdown(thread),
        ExportFormat::Json => export_json(thread),
        ExportFormat::Mbox => export_mbox(thread),
        ExportFormat::LlmContext => export_llm_context(thread, reader_config),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Fixed dates for deterministic test output.
    fn date_1() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 17, 9, 30, 0).unwrap()
    }

    fn date_2() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 17, 10, 15, 0).unwrap()
    }

    pub(crate) fn sample_thread() -> ExportThread {
        ExportThread {
            thread_id: "thread_abc".into(),
            subject: "Deployment rollback plan".into(),
            messages: vec![
                ExportMessage {
                    id: "msg_1".into(),
                    from_name: Some("Alice".into()),
                    from_email: "alice@example.com".into(),
                    to: vec!["team@example.com".into()],
                    date: date_1(),
                    subject: "Deployment rollback plan".into(),
                    body_text: Some("What's the rollback strategy for the v2.1 deploy?".into()),
                    body_html: None,
                    headers_raw: None,
                    attachments: vec![],
                },
                ExportMessage {
                    id: "msg_2".into(),
                    from_name: Some("Bob".into()),
                    from_email: "bob@example.com".into(),
                    to: vec!["team@example.com".into()],
                    date: date_2(),
                    subject: "Re: Deployment rollback plan".into(),
                    body_text: Some(
                        "Use blue-green deployment. Keep the old version running on port 8080."
                            .into(),
                    ),
                    body_html: None,
                    headers_raw: None,
                    attachments: vec![ExportAttachment {
                        filename: "runbook.pdf".into(),
                        size_bytes: 245_760,
                        local_path: Some("/tmp/mxr/runbook.pdf".into()),
                    }],
                },
            ],
        }
    }

    pub(crate) fn single_message_thread() -> ExportThread {
        ExportThread {
            thread_id: "thread_solo".into(),
            subject: "Quick question".into(),
            messages: vec![ExportMessage {
                id: "msg_solo".into(),
                from_name: None,
                from_email: "noreply@service.com".into(),
                to: vec!["user@example.com".into()],
                date: date_1(),
                subject: "Quick question".into(),
                body_text: Some("Is this working?".into()),
                body_html: Some("<p>Is this working?</p>".into()),
                headers_raw: None,
                attachments: vec![],
            }],
        }
    }

    pub(crate) fn empty_body_thread() -> ExportThread {
        ExportThread {
            thread_id: "thread_empty".into(),
            subject: "No body".into(),
            messages: vec![ExportMessage {
                id: "msg_empty".into(),
                from_name: Some("Ghost".into()),
                from_email: "ghost@void.com".into(),
                to: vec![],
                date: date_1(),
                subject: "No body".into(),
                body_text: None,
                body_html: None,
                headers_raw: None,
                attachments: vec![],
            }],
        }
    }

    #[test]
    fn export_dispatch_routes_to_correct_format() {
        let thread = sample_thread();
        let config = ReaderConfig::default();

        let md = export(&thread, &ExportFormat::Markdown, &config);
        assert!(md.starts_with("# Thread:"));

        let json = export(&thread, &ExportFormat::Json, &config);
        assert!(json.starts_with('{'));

        let mbox_out = export(&thread, &ExportFormat::Mbox, &config);
        assert!(mbox_out.starts_with("From "));

        let llm = export(&thread, &ExportFormat::LlmContext, &config);
        assert!(llm.starts_with("Thread:"));
    }

    #[test]
    fn export_format_parsing() {
        // ExportFormat is in core, just verify it exists and serializes
        let fmt = ExportFormat::Markdown;
        let json = serde_json::to_string(&fmt).unwrap();
        let roundtrip: ExportFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, ExportFormat::Markdown);
    }
}

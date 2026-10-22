//! A thread's headline facts with no bodies (`GetThreadPreviews`): what a
//! hover card or a script needs to say what a thread is.

use mxr_core::id::ThreadId;
use serde::{Deserialize, Serialize};

/// The most threads one `GetThreadPreviews` request may name.
pub const THREAD_PREVIEWS_MAX: usize = 100;

/// One thread at a glance. Sender, date, snippet come from its latest
/// message; counts and the attachment flag cover the whole thread.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadPreviewData {
    pub thread_id: ThreadId,
    pub subject: String,
    /// The latest message's sender name, when the message has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    pub sender_email: String,
    /// When the latest message arrived: unix seconds.
    pub latest_at: i64,
    pub message_count: u32,
    pub unread_count: u32,
    /// The start of the latest message's text, clipped.
    pub snippet: String,
    /// Any message in the thread carries an attachment.
    #[serde(default)]
    pub has_attachments: bool,
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests unwrap fixtures to keep failures direct"
)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_request_and_response_round_trip() {
        let thread = ThreadId::new();
        let request = crate::Request::GetThreadPreviews {
            thread_ids: vec![thread.clone()],
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["cmd"], "GetThreadPreviews");
        let back: crate::Request = serde_json::from_value(json).unwrap();
        assert!(matches!(
            back,
            crate::Request::GetThreadPreviews { thread_ids } if thread_ids == vec![thread.clone()]
        ));

        let preview = ThreadPreviewData {
            thread_id: thread,
            subject: "Invoice 4471".into(),
            sender_name: None,
            sender_email: "dana@example.com".into(),
            latest_at: 1_760_000_000,
            message_count: 3,
            unread_count: 1,
            snippet: "Please confirm".into(),
            has_attachments: true,
        };
        let response = crate::ResponseData::ThreadPreviews {
            previews: vec![preview.clone()],
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["kind"], "ThreadPreviews");
        assert!(json["previews"][0].get("sender_name").is_none());
        let back: crate::ResponseData = serde_json::from_value(json).unwrap();
        assert!(matches!(
            back,
            crate::ResponseData::ThreadPreviews { previews } if previews == vec![preview]
        ));
    }
}

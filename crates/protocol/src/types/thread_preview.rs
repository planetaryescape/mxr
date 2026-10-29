//! A thread's headline facts with no bodies (`GetThreadPreviews`): what a
//! hover card or a script needs to say what a thread is.

use mxr_core::id::{AccountId, ThreadId};
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
    /// The account the latest message came to. Absent from older daemons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<AccountId>,
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
            thread_accounts: Vec::new(),
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["cmd"], "GetThreadPreviews");
        let back: crate::Request = serde_json::from_value(json).unwrap();
        assert!(matches!(
            back,
            crate::Request::GetThreadPreviews { thread_ids, .. } if thread_ids == vec![thread.clone()]
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
            account_id: None,
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

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests unwrap fixtures to keep failures direct"
)]
mod account_fields {
    use crate::Request;
    use mxr_core::id::{AccountId, ThreadId};

    /// The account fields are additive: a client that never sends them is
    /// read as before, and one that does gets them back.
    #[test]
    fn requests_without_account_fields_still_decode_and_pairs_round_trip() {
        let thread = ThreadId::new();
        let old = serde_json::json!({ "cmd": "GetThreadPreviews", "thread_ids": [thread] });
        let Request::GetThreadPreviews {
            thread_accounts, ..
        } = serde_json::from_value(old).unwrap()
        else {
            std::panic::resume_unwind(Box::new("wrong variant"))
        };
        assert!(thread_accounts.is_empty());

        let account = AccountId::new();
        let request = Request::DismissDeskThreads {
            thread_ids: vec![],
            thread_accounts: vec![crate::ThreadAccountRef {
                thread_id: thread.clone(),
                account_id: account.clone(),
            }],
            dry_run: true,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(
            json["thread_accounts"][0]["account_id"],
            account.to_string()
        );
        let back = serde_json::from_value::<Request>(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(back).unwrap(), json);

        let bare = serde_json::to_value(Request::SummarizeThread {
            thread_id: thread,
            account_id: None,
        })
        .unwrap();
        assert!(bare.get("account_id").is_none(), "absent when unset");
    }
}

//! A directory that drives a running fake account from outside the
//! daemon, for end-to-end tests: set `MXR_FAKE_SPOOL_DIR` and
//!
//! - drop `<name>.msg` in it and that message arrives on the next sync,
//!   once. It is a few headers, a blank line and the body:
//!
//!   ```text
//!   From: Dana Lee <dana@example.com>
//!   To: alex@demo.mxr.local
//!   Subject: Lunch on Friday?
//!
//!   Are you free?
//!   ```
//!
//!   `Labels: UNREAD` replaces the default `INBOX, UNREAD`, so a message
//!   can arrive already out of the inbox.
//!
//! - write `fail` and every sync fails until it is removed. Its first line
//!   is the error: `rate_limit 120` is a rate limit asking for 120 seconds,
//!   anything else is a provider error with that text, so "oauth token
//!   revoked" and "connection refused" read as auth and offline failures.

use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageBody, SyncedMessage, UnsubscribeMethod};
use mxr_core::MxrError;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub(crate) const SPOOL_DIR_ENV: &str = "MXR_FAKE_SPOOL_DIR";
const FAIL_FILE: &str = "fail";
const MESSAGE_EXTENSION: &str = "msg";

pub(crate) struct Spool {
    dir: PathBuf,
    /// Delivered messages by provider id: each file arrives once, and
    /// `fetch_message` can still find it afterwards.
    delivered: Mutex<HashMap<String, SyncedMessage>>,
}

impl Spool {
    pub(crate) fn from_env() -> Option<Self> {
        let dir = std::env::var_os(SPOOL_DIR_ENV)?;
        Some(Self::at(PathBuf::from(dir)))
    }

    pub(crate) fn at(dir: PathBuf) -> Self {
        Self {
            dir,
            delivered: Mutex::new(HashMap::new()),
        }
    }

    fn delivered(&self) -> std::sync::MutexGuard<'_, HashMap<String, SyncedMessage>> {
        self.delivered
            .lock()
            .expect("fake spool mutex should not be poisoned")
    }

    /// The error every sync fails with while `fail` exists.
    pub(crate) fn failure(&self) -> Option<MxrError> {
        let text = std::fs::read_to_string(self.dir.join(FAIL_FILE)).ok()?;
        let line = text.lines().next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix("rate_limit") {
            let retry_after_secs = rest.trim().parse().unwrap_or(60);
            return Some(MxrError::RateLimited { retry_after_secs });
        }
        Some(MxrError::Provider(if line.is_empty() {
            "fake provider failure".to_string()
        } else {
            line.to_string()
        }))
    }

    /// Messages dropped since the last call, oldest file name first.
    pub(crate) fn take_new(&self, account_id: &AccountId) -> Vec<SyncedMessage> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == MESSAGE_EXTENSION))
            .collect();
        files.sort();
        let mut delivered = self.delivered();
        let mut fresh = Vec::new();
        for path in files {
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let provider_id = format!("spool-{stem}");
            if delivered.contains_key(&provider_id) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let message = parse_message(account_id, &provider_id, &text);
            delivered.insert(provider_id, message.clone());
            fresh.push(message);
        }
        fresh
    }

    pub(crate) fn find(&self, provider_id: &str) -> Option<SyncedMessage> {
        self.delivered().get(provider_id).cloned()
    }
}

fn parse_address(raw: &str) -> Address {
    let raw = raw.trim();
    match (raw.find('<'), raw.rfind('>')) {
        (Some(open), Some(close)) if open < close => {
            let name = raw[..open].trim().trim_matches('"').trim();
            Address {
                name: (!name.is_empty()).then(|| name.to_string()),
                email: raw[open + 1..close].trim().to_string(),
            }
        }
        _ => Address {
            name: None,
            email: raw.to_string(),
        },
    }
}

fn parse_message(account_id: &AccountId, provider_id: &str, text: &str) -> SyncedMessage {
    let (head, body) = text.split_once("\n\n").unwrap_or((text, ""));
    let mut from = Address {
        name: None,
        email: "someone@example.com".to_string(),
    };
    let mut to = Vec::new();
    let mut subject = String::new();
    let mut labels = vec!["INBOX".to_string(), "UNREAD".to_string()];
    for line in head.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "from" => from = parse_address(value),
            "to" => to = value.split(',').map(parse_address).collect(),
            "subject" => subject = value.trim().to_string(),
            "labels" => {
                labels = value
                    .split(',')
                    .map(str::trim)
                    .filter(|label| !label.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            _ => {}
        }
    }
    let id = MessageId::from_scoped_provider_id(account_id, "fake", provider_id);
    let body = body.trim().to_string();
    let envelope = Envelope {
        id: id.clone(),
        account_id: account_id.clone(),
        provider_id: provider_id.to_string(),
        thread_id: ThreadId::from_scoped_provider_id(account_id, "fake", provider_id),
        message_id_header: Some(format!("<{provider_id}@spool.mxr.local>")),
        in_reply_to: None,
        references: Vec::new(),
        from,
        to,
        cc: Vec::new(),
        bcc: Vec::new(),
        subject,
        date: chrono::Utc::now(),
        flags: if labels.iter().any(|label| label == "UNREAD") {
            mxr_core::types::MessageFlags::empty()
        } else {
            mxr_core::types::MessageFlags::READ
        },
        snippet: body.chars().take(140).collect(),
        has_attachments: false,
        size_bytes: body.len() as u64,
        unsubscribe: UnsubscribeMethod::None,
        link_count: 0,
        body_word_count: 0,
        label_provider_ids: labels,
        keywords: std::collections::BTreeSet::new(),
    };
    SyncedMessage {
        body: MessageBody {
            message_id: id,
            text_plain: Some(body),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
        envelope,
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests unwrap fixture setup")]

    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mxr-spool-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_dropped_message_arrives_once_and_stays_findable() {
        let dir = temp_dir();
        let spool = Spool::at(dir.clone());
        let account = AccountId::new();
        std::fs::write(
            dir.join("001.msg"),
            "From: Dana Lee <dana@example.com>\nTo: alex@demo.mxr.local\nSubject: Lunch?\n\nAre you free?\n",
        )
        .unwrap();
        let first = spool.take_new(&account);
        assert_eq!(first.len(), 1);
        let envelope = &first[0].envelope;
        assert_eq!(envelope.from.name.as_deref(), Some("Dana Lee"));
        assert_eq!(envelope.from.email, "dana@example.com");
        assert_eq!(envelope.to[0].email, "alex@demo.mxr.local");
        assert_eq!(envelope.subject, "Lunch?");
        assert!(envelope.label_provider_ids.contains(&"INBOX".to_string()));
        assert!(spool.take_new(&account).is_empty());
        assert!(spool.find("spool-001").is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_fail_file_fails_syncs_with_its_error_until_removed() {
        let dir = temp_dir();
        let spool = Spool::at(dir.clone());
        assert!(spool.failure().is_none());
        std::fs::write(dir.join("fail"), "rate_limit 120\n").unwrap();
        assert!(matches!(
            spool.failure(),
            Some(MxrError::RateLimited {
                retry_after_secs: 120
            })
        ));
        std::fs::write(dir.join("fail"), "oauth token revoked").unwrap();
        assert!(
            matches!(spool.failure(), Some(MxrError::Provider(text)) if text == "oauth token revoked")
        );
        std::fs::remove_file(dir.join("fail")).unwrap();
        assert!(spool.failure().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

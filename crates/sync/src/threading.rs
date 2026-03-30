//! JWZ threading algorithm — reconstruct threads from In-Reply-To + References headers.
//! See <https://www.jwz.org/doc/threading.html>.

use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct MessageForThreading {
    pub message_id: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub date: DateTime<Utc>,
    pub subject: String,
}

#[derive(Debug, Clone)]
struct Container {
    message: Option<MessageForThreading>,
    parent: Option<String>,
    children: Vec<String>,
}

impl Container {
    fn empty() -> Self {
        Self {
            message: None,
            parent: None,
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThreadTree {
    pub root_message_id: String,
    pub messages: Vec<String>,
}

/// JWZ threading: reconstruct threads from In-Reply-To + References headers.
pub fn thread_messages(messages: &[MessageForThreading]) -> Vec<ThreadTree> {
    if messages.is_empty() {
        return Vec::new();
    }

    let message_map: HashMap<&str, &MessageForThreading> = messages
        .iter()
        .map(|message| (message.message_id.as_str(), message))
        .collect();
    let mut id_table: HashMap<String, Container> = HashMap::new();

    // Step 1: Build ID table
    for msg in messages {
        let container = id_table
            .entry(msg.message_id.clone())
            .or_insert_with(Container::empty);
        container.message = Some(msg.clone());

        // Walk References header, link each pair as parent→child
        let mut prev_id: Option<&str> = None;
        for ref_id in &msg.references {
            id_table
                .entry(ref_id.clone())
                .or_insert_with(Container::empty);
            if let Some(parent_id) = prev_id {
                // Set parent_id as parent of ref_id (if not already parented and no cycle)
                let Some(ref_container) = id_table.get(ref_id) else {
                    prev_id = Some(ref_id);
                    continue;
                };
                if ref_container.parent.is_none()
                    && !would_create_cycle(&id_table, parent_id, ref_id)
                {
                    // Clone to avoid borrow issues
                    let parent_id_owned = parent_id.to_string();
                    let ref_id_owned = ref_id.clone();
                    if let Some(c) = id_table.get_mut(&ref_id_owned) {
                        c.parent = Some(parent_id_owned.clone());
                    }
                    if let Some(p) = id_table.get_mut(&parent_id_owned) {
                        if !p.children.contains(&ref_id_owned) {
                            p.children.push(ref_id_owned);
                        }
                    }
                }
            }
            prev_id = Some(ref_id);
        }

        // Set last reference (or In-Reply-To) as parent of this message
        let parent = msg.in_reply_to.as_deref().or(prev_id);
        if let Some(parent_id) = parent {
            if parent_id != msg.message_id
                && !would_create_cycle(&id_table, parent_id, &msg.message_id)
            {
                let parent_id_owned = parent_id.to_string();
                let msg_id = msg.message_id.clone();
                if let Some(c) = id_table.get_mut(&msg_id) {
                    c.parent = Some(parent_id_owned.clone());
                }
                if let Some(p) = id_table.get_mut(&parent_id_owned) {
                    if !p.children.contains(&msg_id) {
                        p.children.push(msg_id);
                    }
                }
            }
        }
    }

    // Step 2: Find root set
    // A root is either:
    // - A container with no parent that has a message
    // - A container whose parent is a phantom (no message) with no parent itself
    let mut roots: Vec<String> = Vec::new();
    for (id, container) in &id_table {
        if container.message.is_none() {
            continue;
        }
        match &container.parent {
            None => roots.push(id.clone()),
            Some(parent_id) => {
                // If parent is phantom (no message) and has no parent, promote this as root
                if let Some(parent) = id_table.get(parent_id) {
                    if parent.message.is_none() && parent.parent.is_none() {
                        roots.push(id.clone());
                    }
                }
            }
        }
    }

    // Sort roots by date (earliest first)
    roots.sort_by(|a, b| {
        let date_a = id_table
            .get(a)
            .and_then(|c| c.message.as_ref())
            .map(|m| m.date)
            .unwrap_or_default();
        let date_b = id_table
            .get(b)
            .and_then(|c| c.message.as_ref())
            .map(|m| m.date)
            .unwrap_or_default();
        date_a.cmp(&date_b).then_with(|| a.cmp(b))
    });

    // Step 3: Build thread trees
    let mut threads = Vec::new();
    for root_id in &roots {
        let mut thread_messages = Vec::new();
        collect_thread_messages(&id_table, root_id, &mut thread_messages);
        if !thread_messages.is_empty() {
            threads.push(ThreadTree {
                root_message_id: root_id.clone(),
                messages: thread_messages,
            });
        }
    }

    merge_subject_fallback_threads(threads, &message_map)
}

fn collect_thread_messages(table: &HashMap<String, Container>, id: &str, out: &mut Vec<String>) {
    if let Some(container) = table.get(id) {
        if container.message.is_some() {
            out.push(id.to_string());
        }
        for child_id in &container.children {
            collect_thread_messages(table, child_id, out);
        }
    }
}

fn would_create_cycle(table: &HashMap<String, Container>, parent_id: &str, child_id: &str) -> bool {
    // Walk up from parent_id; if we reach child_id, it's a cycle
    let mut current = Some(parent_id);
    while let Some(id) = current {
        if id == child_id {
            return true;
        }
        current = table.get(id).and_then(|c| c.parent.as_deref());
    }
    false
}

fn merge_subject_fallback_threads(
    threads: Vec<ThreadTree>,
    message_map: &HashMap<&str, &MessageForThreading>,
) -> Vec<ThreadTree> {
    let mut threads = threads;
    threads.sort_by(|left, right| {
        let left_has_headers = thread_has_headers(left, message_map);
        let right_has_headers = thread_has_headers(right, message_map);
        right_has_headers
            .cmp(&left_has_headers)
            .then_with(|| {
                thread_first_date(left, message_map).cmp(&thread_first_date(right, message_map))
            })
            .then_with(|| left.root_message_id.cmp(&right.root_message_id))
    });

    let mut merged: Vec<ThreadTree> = Vec::new();
    let mut subject_roots: HashMap<String, usize> = HashMap::new();

    for thread in threads {
        let Some(first_id) = thread.messages.first() else {
            continue;
        };
        let key = message_map
            .get(first_id.as_str())
            .map(|message| normalize_subject(&message.subject))
            .unwrap_or_default();
        let has_headers = thread_has_headers(&thread, message_map);

        if !has_headers && !key.is_empty() {
            if let Some(index) = subject_roots.get(&key).copied() {
                let target = &mut merged[index];
                for message_id in &thread.messages {
                    if !target.messages.contains(message_id) {
                        target.messages.push(message_id.clone());
                    }
                }
                sort_thread_messages(target, message_map);
                continue;
            }
        }

        let mut thread = thread;
        sort_thread_messages(&mut thread, message_map);
        if !key.is_empty() {
            subject_roots.entry(key).or_insert_with(|| merged.len());
        }
        merged.push(thread);
    }

    merged
}

fn sort_thread_messages(
    thread: &mut ThreadTree,
    message_map: &HashMap<&str, &MessageForThreading>,
) {
    thread.messages.sort_by(|left, right| {
        let left_date = message_map
            .get(left.as_str())
            .map(|message| message.date)
            .unwrap_or_default();
        let right_date = message_map
            .get(right.as_str())
            .map(|message| message.date)
            .unwrap_or_default();
        left_date.cmp(&right_date)
    });
    if let Some(first) = thread.messages.first() {
        thread.root_message_id = first.clone();
    }
}

fn thread_has_headers(
    thread: &ThreadTree,
    message_map: &HashMap<&str, &MessageForThreading>,
) -> bool {
    thread.messages.iter().any(|id| {
        message_map
            .get(id.as_str())
            .is_some_and(|message| message.in_reply_to.is_some() || !message.references.is_empty())
    })
}

fn thread_first_date(
    thread: &ThreadTree,
    message_map: &HashMap<&str, &MessageForThreading>,
) -> DateTime<Utc> {
    thread
        .messages
        .iter()
        .filter_map(|id| message_map.get(id.as_str()).map(|message| message.date))
        .min()
        .unwrap_or_default()
}

fn normalize_subject(subject: &str) -> String {
    let mut normalized = subject.trim();

    loop {
        let Some(prefix_end) = normalized.find(':') else {
            break;
        };
        let prefix = normalized[..prefix_end].trim();
        let lower = prefix.to_ascii_lowercase();
        let base = lower
            .trim_end_matches(|ch: char| {
                ch.is_ascii_digit() || matches!(ch, '[' | ']' | '(' | ')' | ' ')
            })
            .trim();

        if matches!(
            base,
            "re" | "fw" | "fwd" | "aw" | "sv" | "antw" | "rv" | "odp" | "tr" | "wg"
        ) {
            normalized = normalized[prefix_end + 1..].trim();
            continue;
        }

        break;
    }

    normalized.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, reply_to: Option<&str>, refs: &[&str], subject: &str) -> MessageForThreading {
        MessageForThreading {
            message_id: id.to_string(),
            in_reply_to: reply_to.map(|s| s.to_string()),
            references: refs.iter().map(|s| s.to_string()).collect(),
            date: Utc::now(),
            subject: subject.to_string(),
        }
    }

    #[test]
    fn jwz_threading_basic() {
        let messages = vec![
            msg("msg1@ex", None, &[], "Hello"),
            msg("msg2@ex", Some("msg1@ex"), &["msg1@ex"], "Re: Hello"),
            msg(
                "msg3@ex",
                Some("msg2@ex"),
                &["msg1@ex", "msg2@ex"],
                "Re: Re: Hello",
            ),
        ];

        let threads = thread_messages(&messages);
        assert_eq!(threads.len(), 1, "Should form one thread");
        assert_eq!(
            threads[0].messages.len(),
            3,
            "Thread should have 3 messages"
        );
        assert_eq!(threads[0].root_message_id, "msg1@ex");
    }

    #[test]
    fn jwz_threading_two_independent_threads() {
        let messages = vec![
            msg("a1@ex", None, &[], "Topic A"),
            msg("a2@ex", Some("a1@ex"), &["a1@ex"], "Re: Topic A"),
            msg("b1@ex", None, &[], "Topic B"),
            msg("b2@ex", Some("b1@ex"), &["b1@ex"], "Re: Topic B"),
        ];

        let threads = thread_messages(&messages);
        assert_eq!(threads.len(), 2, "Should form two threads");
    }

    #[test]
    fn jwz_threading_missing_references() {
        // msg2 replies to msg1, but msg1 is not in our set
        let messages = vec![
            msg("msg2@ex", Some("msg1@ex"), &["msg1@ex"], "Re: Hello"),
            msg(
                "msg3@ex",
                Some("msg2@ex"),
                &["msg1@ex", "msg2@ex"],
                "Re: Re: Hello",
            ),
        ];

        let threads = thread_messages(&messages);
        // msg2 should be the root (since msg1 is a phantom container without a message)
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].messages.len(), 2);
    }

    #[test]
    fn jwz_threading_no_replies() {
        let messages = vec![
            msg("msg1@ex", None, &[], "Hello"),
            msg("msg2@ex", None, &[], "World"),
        ];

        let threads = thread_messages(&messages);
        assert_eq!(threads.len(), 2, "Each message is its own thread");
    }

    #[test]
    fn jwz_threading_empty_input() {
        let threads = thread_messages(&[]);
        assert!(threads.is_empty());
    }

    #[test]
    fn subject_fallback_groups_headerless_replies() {
        let messages = vec![
            msg("msg1@ex", None, &[], "Hello"),
            msg("msg2@ex", None, &[], "Re: Hello"),
            msg("msg3@ex", None, &[], "AW: Hello"),
        ];

        let threads = thread_messages(&messages);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].messages.len(), 3);
    }

    #[test]
    fn subject_fallback_attaches_headerless_reply_to_header_thread() {
        let messages = vec![
            msg("msg1@ex", None, &[], "Topic"),
            msg("msg2@ex", Some("msg1@ex"), &["msg1@ex"], "Re: Topic"),
            msg("msg3@ex", None, &[], "SV: Topic"),
        ];

        let threads = thread_messages(&messages);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].messages.len(), 3);
    }

    #[test]
    fn subject_fallback_does_not_merge_independent_header_threads() {
        let messages = vec![
            msg("root-a@ex", None, &[], "Topic"),
            msg("reply-a@ex", Some("root-a@ex"), &["root-a@ex"], "Re: Topic"),
            msg("root-b@ex", None, &[], "Topic"),
            msg("reply-b@ex", Some("root-b@ex"), &["root-b@ex"], "Re: Topic"),
        ];

        let threads = thread_messages(&messages);
        assert_eq!(threads.len(), 2);
    }
}

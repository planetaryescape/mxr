//! JWZ threading algorithm — reconstruct threads from In-Reply-To + References headers.
//! See https://www.jwz.org/doc/threading.html

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
                let ref_container = id_table.get(ref_id).unwrap();
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
        date_a.cmp(&date_b)
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

    threads
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
}

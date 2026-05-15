#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// A message projected into the fields needed by RFC 5256/JWZ threading.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct Message {
    /// Caller-stable message identity returned in thread output.
    pub id: String,
    /// The RFC 5322 `Message-ID` value, if present and valid.
    pub message_id: Option<String>,
    /// The RFC 5322 `In-Reply-To` value, if present.
    pub in_reply_to: Option<String>,
    /// The ordered RFC 5322 `References` chain.
    pub references: Vec<String>,
    /// Message date used for deterministic output ordering.
    pub date: DateTime<Utc>,
    /// Message subject used only by optional subject fallback merging.
    pub subject: String,
}

/// A flat email thread.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct Thread {
    /// The canonical root message id for this thread.
    pub root_message_id: String,
    /// Thread members, ordered deterministically by date then message id.
    pub messages: Vec<String>,
}

/// Configuration for threading behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct ThreadingOptions {
    /// Merge otherwise unrelated headerless replies by normalized subject.
    pub subject_merge: bool,
    /// Hide phantom containers from root selection and thread membership.
    pub prune_phantoms: bool,
    /// Case-insensitive subject prefixes stripped by subject fallback.
    pub subject_prefixes: Vec<String>,
}

impl Default for ThreadingOptions {
    fn default() -> Self {
        Self {
            subject_merge: true,
            prune_phantoms: true,
            subject_prefixes: [
                "re", "fw", "fwd", "aw", "sv", "antw", "rv", "odp", "tr", "wg",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        }
    }
}

/// Thread messages with default RFC 5256/JWZ options.
pub fn thread_messages(messages: &[Message]) -> Vec<Thread> {
    thread_messages_with(messages, &ThreadingOptions::default())
}

/// Thread messages with explicit options.
pub fn thread_messages_with(messages: &[Message], options: &ThreadingOptions) -> Vec<Thread> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut table = HashMap::new();
    let mut output_index = HashMap::new();
    let mut next_order = 0;

    for message in messages {
        if message.id.is_empty() {
            continue;
        }

        let current_key = message_container_key(&table, message);
        ensure_container(&mut table, &current_key, &mut next_order);

        if let Some(container) = table.get_mut(&current_key) {
            container.message = Some(message.clone());
        }
        output_index.insert(message.id.clone(), current_key.clone());

        let reference_chain = effective_reference_chain(message);
        let mut previous_id: Option<&str> = None;
        for reference_id in &reference_chain {
            ensure_container(&mut table, reference_id, &mut next_order);
            if let Some(parent_id) = previous_id {
                link_parent_child(&mut table, parent_id, reference_id, false);
            }
            previous_id = Some(reference_id);
        }

        if let Some(parent_id) = reference_chain.last() {
            ensure_container(&mut table, parent_id, &mut next_order);
            link_parent_child(&mut table, parent_id, &current_key, true);
        } else {
            detach_parent(&mut table, &current_key);
        }
    }

    let mut public_roots = Vec::new();
    let mut root_ids = table
        .iter()
        .filter_map(|(id, container)| container.parent.is_none().then_some(id.clone()))
        .collect::<Vec<_>>();
    root_ids.sort_by(|left, right| compare_container_roots(&table, left, right));

    for root_id in root_ids {
        collect_public_roots(&table, &root_id, options.prune_phantoms, &mut public_roots);
    }

    let mut threads = Vec::new();
    for root_id in public_roots {
        let mut thread_messages = Vec::new();
        collect_thread_messages(&table, &root_id, &mut thread_messages);
        thread_messages
            .sort_by(|left, right| compare_message_ids(&table, &output_index, left, right));
        thread_messages.dedup();

        if !thread_messages.is_empty() {
            let root = table
                .get(&root_id)
                .and_then(|container| container.message.as_ref())
                .map(|message| message.id.clone())
                .unwrap_or(root_id);
            threads.push(Thread {
                root_message_id: root,
                messages: thread_messages,
            });
        }
    }

    threads.sort_by(|left, right| compare_threads(&table, &output_index, left, right));

    if options.subject_merge {
        merge_subject_fallback_threads(threads, &table, &output_index, options)
    } else {
        threads
    }
}

#[derive(Debug, Clone)]
struct Container {
    message: Option<Message>,
    parent: Option<String>,
    children: Vec<String>,
    order: usize,
}

impl Container {
    fn empty(order: usize) -> Self {
        Self {
            message: None,
            parent: None,
            children: Vec::new(),
            order,
        }
    }
}

fn ensure_container(table: &mut HashMap<String, Container>, id: &str, next_order: &mut usize) {
    if !table.contains_key(id) {
        let order = *next_order;
        *next_order += 1;
        table.insert(id.to_string(), Container::empty(order));
    }
}

fn message_container_key(table: &HashMap<String, Container>, message: &Message) -> String {
    if let Some(normalized) = message.message_id.as_deref().and_then(normalize_message_id) {
        if table
            .get(&normalized)
            .and_then(|container| container.message.as_ref())
            .is_none()
        {
            return normalized;
        }
    }

    synthetic_message_id(&message.id)
}

fn synthetic_message_id(id: &str) -> String {
    format!("synthetic:{id}")
}

fn effective_reference_chain(message: &Message) -> Vec<String> {
    let references = message
        .references
        .iter()
        .filter_map(|reference| normalize_message_id(reference))
        .collect::<Vec<_>>();

    if references.is_empty() {
        message
            .in_reply_to
            .as_deref()
            .and_then(normalize_message_id)
            .into_iter()
            .collect()
    } else {
        references
    }
}

fn link_parent_child(
    table: &mut HashMap<String, Container>,
    parent_id: &str,
    child_id: &str,
    replace_existing: bool,
) {
    if parent_id == child_id || would_create_cycle(table, parent_id, child_id) {
        return;
    }

    let has_parent = table
        .get(child_id)
        .is_some_and(|container| container.parent.is_some());
    if has_parent && !replace_existing {
        return;
    }

    if replace_existing {
        detach_parent(table, child_id);
    }

    if let Some(child) = table.get_mut(child_id) {
        child.parent = Some(parent_id.to_string());
    }

    if let Some(parent) = table.get_mut(parent_id) {
        let child = child_id.to_string();
        if !parent.children.contains(&child) {
            parent.children.push(child);
        }
    }
}

fn detach_parent(table: &mut HashMap<String, Container>, child_id: &str) {
    let parent_id = table.get(child_id).and_then(|child| child.parent.clone());

    if let Some(parent_id) = parent_id {
        if let Some(parent) = table.get_mut(&parent_id) {
            parent.children.retain(|id| id != child_id);
        }
    }

    if let Some(child) = table.get_mut(child_id) {
        child.parent = None;
    }
}

fn would_create_cycle(table: &HashMap<String, Container>, parent_id: &str, child_id: &str) -> bool {
    let mut current = Some(parent_id);
    while let Some(id) = current {
        if id == child_id {
            return true;
        }
        current = table
            .get(id)
            .and_then(|container| container.parent.as_deref());
    }
    false
}

fn collect_public_roots(
    table: &HashMap<String, Container>,
    id: &str,
    prune_phantoms: bool,
    out: &mut Vec<String>,
) {
    let Some(container) = table.get(id) else {
        return;
    };

    if prune_phantoms && container.message.is_none() {
        let mut children = container.children.clone();
        children.sort_by(|left, right| compare_container_roots(table, left, right));
        for child_id in children {
            collect_public_roots(table, &child_id, prune_phantoms, out);
        }
    } else {
        out.push(id.to_string());
    }
}

fn collect_thread_messages(table: &HashMap<String, Container>, id: &str, out: &mut Vec<String>) {
    let Some(container) = table.get(id) else {
        return;
    };

    if container.message.is_some() {
        if let Some(message) = &container.message {
            out.push(message.id.clone());
        }
    }

    let mut children = container.children.clone();
    children.sort_by(|left, right| compare_container_roots(table, left, right));
    for child_id in children {
        collect_thread_messages(table, &child_id, out);
    }
}

fn compare_container_roots(
    table: &HashMap<String, Container>,
    left: &str,
    right: &str,
) -> std::cmp::Ordering {
    earliest_date(table, left)
        .cmp(&earliest_date(table, right))
        .then_with(|| {
            table
                .get(left)
                .map(|container| container.order)
                .cmp(&table.get(right).map(|container| container.order))
        })
        .then_with(|| left.cmp(right))
}

fn compare_message_ids(
    table: &HashMap<String, Container>,
    output_index: &HashMap<String, String>,
    left_id: &str,
    right_id: &str,
) -> std::cmp::Ordering {
    message_date(table, output_index, left_id)
        .cmp(&message_date(table, output_index, right_id))
        .then_with(|| left_id.cmp(right_id))
}

fn compare_threads(
    table: &HashMap<String, Container>,
    output_index: &HashMap<String, String>,
    left: &Thread,
    right: &Thread,
) -> std::cmp::Ordering {
    thread_first_date(table, output_index, left)
        .cmp(&thread_first_date(table, output_index, right))
        .then_with(|| left.root_message_id.cmp(&right.root_message_id))
}

fn earliest_date(table: &HashMap<String, Container>, root_id: &str) -> DateTime<Utc> {
    let mut messages = Vec::new();
    collect_thread_messages(table, root_id, &mut messages);
    messages
        .iter()
        .map(|id| message_date_by_key(table, id))
        .min()
        .unwrap_or_default()
}

fn message_date_by_key(table: &HashMap<String, Container>, id: &str) -> DateTime<Utc> {
    table
        .get(id)
        .and_then(|container| container.message.as_ref())
        .map(|message| message.date)
        .unwrap_or_default()
}

fn message_date(
    table: &HashMap<String, Container>,
    output_index: &HashMap<String, String>,
    output_id: &str,
) -> DateTime<Utc> {
    output_index
        .get(output_id)
        .and_then(|key| table.get(key))
        .and_then(|container| container.message.as_ref())
        .map(|message| message.date)
        .unwrap_or_default()
}

fn thread_first_date(
    table: &HashMap<String, Container>,
    output_index: &HashMap<String, String>,
    thread: &Thread,
) -> DateTime<Utc> {
    thread
        .messages
        .iter()
        .map(|id| message_date(table, output_index, id))
        .min()
        .unwrap_or_default()
}

fn merge_subject_fallback_threads(
    mut threads: Vec<Thread>,
    table: &HashMap<String, Container>,
    output_index: &HashMap<String, String>,
    options: &ThreadingOptions,
) -> Vec<Thread> {
    threads.sort_by(|left, right| {
        let left_has_headers = thread_has_headers(left, table, output_index);
        let right_has_headers = thread_has_headers(right, table, output_index);
        right_has_headers
            .cmp(&left_has_headers)
            .then_with(|| compare_threads(table, output_index, left, right))
    });

    let mut merged = Vec::new();
    let mut subject_roots: HashMap<String, usize> = HashMap::new();

    for mut thread in threads {
        thread
            .messages
            .sort_by(|left, right| compare_message_ids(table, output_index, left, right));
        let key = thread
            .messages
            .first()
            .and_then(|id| output_index.get(id))
            .and_then(|key| table.get(key))
            .and_then(|container| container.message.as_ref())
            .map(|message| normalize_subject(&message.subject, &options.subject_prefixes))
            .unwrap_or_default();
        let has_headers = thread_has_headers(&thread, table, output_index);

        if !has_headers && !key.is_empty() {
            if let Some(index) = subject_roots.get(&key).copied() {
                let target: &mut Thread = &mut merged[index];
                for message_id in thread.messages {
                    if !target.messages.contains(&message_id) {
                        target.messages.push(message_id);
                    }
                }
                target
                    .messages
                    .sort_by(|left, right| compare_message_ids(table, output_index, left, right));
                continue;
            }
        }

        if !key.is_empty() {
            subject_roots.entry(key).or_insert_with(|| merged.len());
        }
        merged.push(thread);
    }

    merged.sort_by(|left, right| compare_threads(table, output_index, left, right));
    merged
}

fn thread_has_headers(
    thread: &Thread,
    table: &HashMap<String, Container>,
    output_index: &HashMap<String, String>,
) -> bool {
    thread.messages.iter().any(|id| {
        output_index
            .get(id)
            .and_then(|key| table.get(key))
            .and_then(|container| container.message.as_ref())
            .is_some_and(|message| message.in_reply_to.is_some() || !message.references.is_empty())
    })
}

fn normalize_message_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let unwrapped = trimmed
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
        .unwrap_or(trimmed)
        .trim();

    if unwrapped.is_empty() || !unwrapped.contains('@') {
        return None;
    }

    let (local, domain) = unwrapped.split_once('@')?;
    let local = local.trim();
    let domain = domain.trim();
    if local.is_empty() || domain.is_empty() {
        return None;
    }

    let local = local
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(local)
        .replace("\\\"", "\"")
        .replace("\\\\", "\\");

    Some(format!("{local}@{domain}"))
}

fn normalize_subject(subject: &str, prefixes: &[String]) -> String {
    let collapsed = subject.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut normalized = collapsed.trim();

    loop {
        if let Some(stripped) = normalized
            .strip_suffix("(fwd)")
            .or_else(|| normalized.strip_suffix("(FWD)"))
            .or_else(|| normalized.strip_suffix("(Fwd)"))
        {
            normalized = stripped.trim();
            continue;
        }

        let lower = normalized.to_ascii_lowercase();
        if lower.starts_with("[fwd:") && normalized.ends_with(']') {
            normalized = normalized[5..normalized.len() - 1].trim();
            continue;
        }

        if let Some(stripped) = strip_leading_subject_blobs(normalized) {
            normalized = stripped;
            continue;
        }

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

        if prefixes
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(base))
        {
            normalized = normalized[prefix_end + 1..].trim();
            continue;
        }

        break;
    }

    normalized.to_ascii_lowercase()
}

fn strip_leading_subject_blobs(subject: &str) -> Option<&str> {
    let mut cursor = subject.trim_start();
    let mut stripped_any = false;

    while let Some(rest) = cursor.strip_prefix('[') {
        let Some(blob_end) = rest.find(']') else {
            break;
        };
        let after_blob = rest[blob_end + 1..].trim_start();
        if after_blob.is_empty() {
            break;
        }

        stripped_any = true;
        cursor = after_blob;
    }

    stripped_any.then_some(cursor.trim())
}

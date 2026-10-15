//! Links in the assistant's replies to the emails and people it names.
//!
//! The model is asked to link them as `[words](mxr:thread/<id>)` and
//! `[words](mxr:person/<id>)`. Small local models often don't, and any
//! model can garble an id, so the reply is checked and filled in against
//! what the tools actually returned in this conversation:
//! - a link stays only when its id came back in a result;
//! - a subject or a person's name the reply quotes exactly is linked to the
//!   one email or person a result gave it, unless results gave that same
//!   text to more than one.

use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Shorter ids could match inside any result by chance.
const MIN_ID_CHARS: usize = 8;
/// Shorter names ("Hi", "Ann") are too likely to be ordinary words.
const MIN_NAME_CHARS: usize = 5;

/// `[words](mxr:thread/<id>)` or `[words](mxr:person/<id>)` in a reply.
static MXR_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[([^\]\n]*)\]\(mxr:([a-z]+)/([^)\s]+)\)").expect("valid link pattern")
});

/// Markdown a name must not be linked inside: links and code.
static LINK_OR_CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[[^\]\n]*\]\([^)\n]*\)|`[^`\n]*`").expect("valid markdown pattern")
});

/// What this conversation's tool results let a reply link.
#[derive(Default)]
pub(crate) struct KnownLinks {
    /// Every result, for checking ids the model wrote.
    results: String,
    /// Subjects and names to the link each one opens; `None` when results
    /// gave the same text to more than one email or person.
    names: HashMap<String, Option<String>>,
}

impl KnownLinks {
    /// Learn from one whole tool result, as mxr returned it.
    pub(crate) fn add_result(&mut self, result: &str) {
        self.results.push_str(result);
        self.results.push('\n');
        if let Ok(value) = serde_json::from_str::<Value>(result) {
            self.collect_names(&value);
        }
    }

    /// The reply with links checked and the emails and people it names linked.
    pub(crate) fn apply(&self, reply: &str) -> String {
        let checked = self.checked(reply);
        self.linked_names(&checked)
    }

    fn checked(&self, reply: &str) -> String {
        MXR_LINK
            .replace_all(reply, |captures: &regex::Captures<'_>| {
                let (words, kind, id) = (&captures[1], &captures[2], &captures[3]);
                let known = matches!(kind, "thread" | "person")
                    && id.len() >= MIN_ID_CHARS
                    && self.results.contains(id);
                if known {
                    captures[0].to_string()
                } else {
                    words.to_string()
                }
            })
            .into_owned()
    }

    fn collect_names(&mut self, value: &Value) {
        match value {
            Value::Object(object) => {
                let text = |key: &str| object.get(key).and_then(Value::as_str).map(str::trim);
                if let (Some(subject), Some(thread_id)) = (text("subject"), text("thread_id")) {
                    self.add_name(subject, format!("mxr:thread/{thread_id}"));
                }
                if let (Some(id), Some(title)) = (text("id"), text("title")) {
                    if id.starts_with("person:") || id.starts_with("group:") {
                        self.add_name(title, format!("mxr:person/{id}"));
                    }
                }
                for child in object.values() {
                    self.collect_names(child);
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.collect_names(item);
                }
            }
            _ => {}
        }
    }

    fn add_name(&mut self, name: &str, target: String) {
        let bare = without_reply_prefix(name);
        for form in [name, bare] {
            if form.chars().count() < MIN_NAME_CHARS || form.contains(['[', ']', '\n']) {
                continue;
            }
            self.names
                .entry(form.to_string())
                .and_modify(|known| {
                    if known.as_deref() != Some(target.as_str()) {
                        *known = None;
                    }
                })
                .or_insert_with(|| Some(target.clone()));
        }
    }

    /// Link the first mention of each known name, longest names first so a
    /// subject wins over a shorter name inside it.
    fn linked_names(&self, reply: &str) -> String {
        let mut names: Vec<(&str, &str)> = self
            .names
            .iter()
            .filter_map(|(name, target)| Some((name.as_str(), target.as_deref()?)))
            .collect();
        if names.is_empty() {
            return reply.to_string();
        }
        names.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(b.0)));

        // The reply as pieces; linked pieces are never searched again.
        let mut pieces: Vec<(String, bool)> = Vec::new();
        let mut at = 0;
        for found in LINK_OR_CODE.find_iter(reply) {
            pieces.push((reply[at..found.start()].to_string(), false));
            pieces.push((found.as_str().to_string(), true));
            at = found.end();
        }
        pieces.push((reply[at..].to_string(), false));

        for (name, target) in names {
            let Some((index, start)) =
                pieces
                    .iter()
                    .enumerate()
                    .find_map(|(index, (text, fixed))| {
                        (!fixed)
                            .then(|| word_match(text, name))
                            .flatten()
                            .map(|start| (index, start))
                    })
            else {
                continue;
            };
            let text = pieces[index].0.clone();
            let end = start + name.len();
            pieces.splice(
                index..=index,
                [
                    (text[..start].to_string(), false),
                    (format!("[{name}]({target})"), true),
                    (text[end..].to_string(), false),
                ],
            );
        }
        pieces.into_iter().map(|(text, _)| text).collect()
    }
}

/// The first place `name` stands as whole words in `text`.
fn word_match(text: &str, name: &str) -> Option<usize> {
    text.match_indices(name)
        .map(|(start, _)| start)
        .find(|&start| {
            let before = text[..start].chars().next_back();
            let after = text[start + name.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
}

/// "Re: Fwd: Plans" reads "Plans": replies often drop the prefixes.
fn without_reply_prefix(subject: &str) -> &str {
    let mut rest = subject.trim();
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(prefix) = ["re:", "fwd:", "fw:"]
            .into_iter()
            .find(|prefix| lower.starts_with(prefix))
        else {
            return rest;
        };
        rest = rest[prefix.len()..].trim_start();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn known(results: &[Value]) -> KnownLinks {
        let mut known = KnownLinks::default();
        for result in results {
            known.add_result(&result.to_string());
        }
        known
    }

    #[test]
    fn keeps_links_to_ids_a_result_returned_and_unlinks_the_rest() {
        let known = known(&[json!({"thread_id": "thread-real-1", "id": "person:a@x.com"})]);
        let reply = "See [Q1 report](mxr:thread/thread-real-1), [Ann](mxr:person/person:a@x.com), \
                     [made up](mxr:thread/thread-fake-1) and [odd](mxr:folder/thread-real-1).";
        assert_eq!(
            known.apply(reply),
            "See [Q1 report](mxr:thread/thread-real-1), [Ann](mxr:person/person:a@x.com), \
                     made up and odd."
        );
    }

    #[test]
    fn links_subjects_and_people_the_reply_names() {
        let known = known(&[json!({"messages": {"your_turn": [{
            "id": "person:wendy@work.com",
            "title": "Wendy Data",
            "topics": [{"subject": "Re: API design discussion", "thread_id": "thread-wendy-1"}],
        }]}})]);
        let reply =
            "1. **Wendy Data** (wendy@work.com): \"API design discussion\". Wendy Data again.";
        assert_eq!(
            known.apply(reply),
            "1. **[Wendy Data](mxr:person/person:wendy@work.com)** (wendy@work.com): \
             \"[API design discussion](mxr:thread/thread-wendy-1)\". Wendy Data again."
        );
    }

    #[test]
    fn leaves_names_that_point_at_more_than_one_thing_and_text_inside_links() {
        let known = known(&[json!({"envelopes": [
            {"subject": "Weekly sync", "thread_id": "thread-sync-1"},
            {"subject": "Weekly sync", "thread_id": "thread-sync-2"},
            {"subject": "Invoice 2847", "thread_id": "thread-inv-1"},
        ]})]);
        let reply = "Weekly sync twice; see [Invoice 2847](https://x.test) and `Invoice 2847`. \
                     Invoice 28470 is not it.";
        assert_eq!(known.apply(reply), reply);
    }
}

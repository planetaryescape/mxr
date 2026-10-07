//! New text for Messages: each message without the quoted history and
//! signature its thread already holds (`mxr_reader::new_text`), and the
//! ask from a cached gist. Every client reads these words from here, so
//! the CLI, TUI, web and MCP show the same text and the same "trimmed".

use super::thread_gist::{cached_gist, thread_envelopes, GistPolicy, GistSetup};
use super::HandlerError;
use crate::state::AppState;
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{AttachmentDisposition, AttachmentMeta};
use mxr_protocol::TrimmedData;
use mxr_reader::{new_text, plain_text, EarlierMessage};
use mxr_store::DeskMessage;
use std::collections::{HashMap, HashSet};

/// Earlier messages a message is matched against. Quotes almost always
/// come from the last few; a cap keeps long threads cheap.
const EARLIER_MAX: usize = 8;
/// Width a message is wrapped at to count its lines.
const WRAP_COLUMNS: usize = 72;

pub(super) struct TextRequest<'a> {
    pub wanted: &'a [MessageId],
    /// Messages you sent: they never count as the author's earlier mail.
    pub outbound: &'a HashSet<MessageId>,
}

pub(super) struct MessageText {
    pub text: String,
    pub trimmed: TrimmedData,
    pub only_quoted: bool,
    pub attachments: Vec<AttachmentMeta>,
}

struct Loaded {
    plain: String,
    text_plain: Option<String>,
    text_html: Option<String>,
    attachments: Vec<AttachmentMeta>,
}

/// The new text of each wanted message of `thread` (any order in, date
/// order assumed by position).
pub(super) async fn new_texts(
    state: &AppState,
    thread: &[DeskMessage],
    request: &TextRequest<'_>,
) -> Result<HashMap<MessageId, MessageText>, HandlerError> {
    let shown: Vec<&DeskMessage> = thread.iter().filter(|m| !m.trashed).collect();
    let positions: Vec<usize> = request
        .wanted
        .iter()
        .filter_map(|id| shown.iter().position(|m| &m.id == id))
        .collect();
    let mut needed: Vec<usize> = Vec::new();
    for &at in &positions {
        for index in at.saturating_sub(EARLIER_MAX)..=at {
            if !needed.contains(&index) {
                needed.push(index);
            }
        }
    }
    let mut loaded: HashMap<usize, Loaded> = HashMap::new();
    for index in needed {
        let message = shown[index];
        let Some(body) = state.store.get_body(&message.id).await? else {
            continue;
        };
        loaded.insert(
            index,
            Loaded {
                plain: plain_text(body.text_plain.as_deref(), body.text_html.as_deref()),
                text_plain: body.text_plain,
                text_html: body.text_html,
                attachments: body
                    .attachments
                    .into_iter()
                    .filter(|a| a.disposition != AttachmentDisposition::Inline)
                    .collect(),
            },
        );
    }
    let mut out = HashMap::new();
    for &at in &positions {
        let message = shown[at];
        let Some(body) = loaded.get(&at) else {
            out.insert(message.id.clone(), snippet_text(state, &message.id).await?);
            continue;
        };
        let author = message.from.email.to_ascii_lowercase();
        let earlier: Vec<EarlierMessage<'_>> = (at.saturating_sub(EARLIER_MAX)..at)
            .filter_map(|index| {
                let earlier = loaded.get(&index)?;
                let from = shown[index];
                Some(EarlierMessage {
                    text: &earlier.plain,
                    same_author: !request.outbound.contains(&from.id)
                        && !request.outbound.contains(&message.id)
                        && from.from.email.eq_ignore_ascii_case(&author),
                })
            })
            .collect();
        let result = new_text(
            body.text_plain.as_deref(),
            body.text_html.as_deref(),
            &earlier,
        );
        out.insert(
            message.id.clone(),
            MessageText {
                text: result.text,
                trimmed: TrimmedData {
                    quote: result.trimmed.quote,
                    signature: result.trimmed.signature,
                    footer: result.trimmed.footer,
                },
                only_quoted: result.only_quoted,
                attachments: body.attachments.clone(),
            },
        );
    }
    Ok(out)
}

/// A message whose body isn't stored yet reads as its snippet.
async fn snippet_text(state: &AppState, id: &MessageId) -> Result<MessageText, HandlerError> {
    let snippet = state
        .store
        .get_envelope(id)
        .await?
        .map(|envelope| envelope.snippet)
        .unwrap_or_default();
    Ok(MessageText {
        text: snippet,
        trimmed: TrimmedData::default(),
        only_quoted: false,
        attachments: Vec::new(),
    })
}

/// Lines the text takes wrapped at `WRAP_COLUMNS`: what "three lines or
/// fewer" is measured in.
pub(super) fn wrapped_lines(text: &str) -> usize {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().count().div_ceil(WRAP_COLUMNS).max(1))
        .sum()
}

/// The ask in a conversation's cached gist, quoted verbatim.
pub(super) struct CachedAsk {
    pub message_id: MessageId,
    pub quote: String,
    pub model: String,
}

/// The ask from the gist cache, when the gist still matches the thread.
/// Never calls a model.
pub(super) async fn cached_ask(
    state: &AppState,
    thread_id: &ThreadId,
) -> Result<Option<CachedAsk>, HandlerError> {
    let envelopes = thread_envelopes(state, thread_id).await?;
    let Some(setup) = GistSetup::new(&GistPolicy::pin(state), thread_id, &envelopes) else {
        return Ok(None);
    };
    let Some(gist) = cached_gist(state, thread_id, &setup).await? else {
        return Ok(None);
    };
    let model = gist
        .provenance
        .map_or_else(|| "the configured model".to_string(), |p| p.model);
    Ok(gist.ask.and_then(|ask| ask.quote).map(|quote| CachedAsk {
        message_id: quote.message_id,
        quote: quote.text,
        model,
    }))
}

#[cfg(test)]
mod tests {
    use super::wrapped_lines;

    #[test]
    fn lines_are_counted_wrapped() {
        assert_eq!(wrapped_lines("Thanks, on it."), 1);
        assert_eq!(wrapped_lines("a\n\nb\nc"), 3);
        assert_eq!(wrapped_lines(&"word ".repeat(40)), 3);
        assert_eq!(wrapped_lines(&"word ".repeat(60)), 5);
    }
}

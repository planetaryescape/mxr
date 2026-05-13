//! Slice 5.1 / 5.2 of docs/ai-email/05-context-briefings.md.
//!
//! Thread and recipient briefings with content-hash caching. The
//! briefing daemon module owns prompt construction, cache lookup,
//! LLM invocation, and citation validation.

use crate::state::AppState;
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::CitationRef;
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature};
use mxr_protocol::{ResponseData, ThreadBriefingData};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{new_briefing_id, BriefingKind, ContextBriefing};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
struct LlmBriefing {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    citations: Vec<LlmCite>,
}

#[derive(Debug, Deserialize)]
struct LlmCite {
    #[serde(default)]
    msg_id: String,
    #[serde(default)]
    quote: String,
}

pub(crate) async fn get_thread_briefing(
    state: &AppState,
    thread_id: &ThreadId,
    refresh: bool,
) -> super::HandlerResult {
    let envelopes = state
        .store
        .get_thread_envelopes(thread_id)
        .await
        .map_err(|e| e.to_string())?;
    if envelopes.is_empty() {
        return Err(format!("thread {thread_id} not found"));
    }
    let account_id = envelopes[0].account_id.clone();

    let content_hash = thread_content_hash(&envelopes);
    let cache_key = thread_id.to_string();

    if !refresh {
        if let Ok(Some(cached)) = state
            .store
            .get_context_briefing(&account_id, BriefingKind::Thread, &cache_key)
            .await
        {
            if cached.content_hash == content_hash {
                return Ok(ResponseData::ThreadBriefing {
                    briefing: to_thread_briefing(&cached, true),
                });
            }
        }
    }

    let mut transcript = String::new();
    let mut allowed = Vec::new();
    for env in &envelopes {
        let body = state
            .store
            .get_body(&env.id)
            .await
            .ok()
            .flatten()
            .and_then(|b| b.text_plain)
            .unwrap_or_else(|| env.snippet.clone());
        let cleaned = clean(Some(&body), None, &ReaderConfig::default()).content;
        transcript.push_str(&format!(
            "[msg_id={}]\nFrom: {}\nDate: {}\nSubject: {}\n{}\n\n",
            env.id,
            env.from.email,
            env.date.to_rfc3339(),
            env.subject,
            cleaned
        ));
        allowed.push(env.id.to_string());
    }

    let runtime = state.llm.for_feature(LlmFeature::Briefing);
    let req = CompletionRequest {
        max_tokens: Some(700),
        temperature: Some(0.1),
        messages: vec![
            ChatMessage::system(
                "Summarize what's currently true about this email thread for someone \
                 returning to it after a long gap. Output STRICT JSON: \
                 {\"summary\": str (markdown), \"citations\": [{\"msg_id\": str, \
                 \"quote\": str}]}\n\nCite ONLY msg_id values that appear in the \
                 [msg_id=...] markers. If you can't summarize meaningfully, return \
                 a one-line note and an empty citations array.",
            ),
            ChatMessage::user(format!("THREAD:\n{transcript}\n\nReturn JSON only.")),
        ],
    };

    let response = match runtime.complete(req).await {
        Ok(r) => r,
        Err(LlmError::Disabled) | Err(LlmError::PrivacyBlocked(_)) => {
            // Deterministic fallback: list participants and most recent line.
            let fallback = deterministic_thread_fallback(&envelopes);
            let entry = ContextBriefing {
                id: new_briefing_id(),
                account_id: account_id.clone(),
                kind: BriefingKind::Thread,
                subject_key: cache_key,
                content_hash,
                body_markdown: fallback,
                citations: vec![],
                generated_at: chrono::Utc::now(),
            };
            // Don't cache the fallback (it's stale-by-default).
            return Ok(ResponseData::ThreadBriefing {
                briefing: to_thread_briefing(&entry, false),
            });
        }
        Err(e) => return Err(format!("Briefing LLM error: {e}")),
    };
    let parsed: LlmBriefing = serde_json::from_str(response.content.trim())
        .map_err(|e| format!("Briefing: LLM returned non-JSON ({e})"))?;

    let allowed_set: std::collections::HashSet<&str> =
        allowed.iter().map(|s| s.as_str()).collect();
    let mut citations = Vec::new();
    for c in parsed.citations {
        if !allowed_set.contains(c.msg_id.as_str()) {
            // Reject and fall through with a note.
            tracing::warn!(msg_id = %c.msg_id, "Briefing: ignoring unknown msg_id");
            continue;
        }
        citations.push(CitationRef {
            message_id: Some(c.msg_id),
            thread_id: Some(thread_id.to_string()),
            field: "body".into(),
            quote: c.quote,
        });
    }

    let entry = ContextBriefing {
        id: new_briefing_id(),
        account_id,
        kind: BriefingKind::Thread,
        subject_key: cache_key,
        content_hash,
        body_markdown: parsed.summary,
        citations,
        generated_at: chrono::Utc::now(),
    };
    state
        .store
        .upsert_context_briefing(&entry)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::ThreadBriefing {
        briefing: to_thread_briefing(&entry, false),
    })
}

fn thread_content_hash(envelopes: &[mxr_core::types::Envelope]) -> String {
    let mut h = Sha256::new();
    for env in envelopes {
        h.update(env.id.to_string().as_bytes());
        h.update(b":");
        h.update(env.date.timestamp().to_le_bytes());
        h.update(b"|");
    }
    format!("{:x}", h.finalize())
}

fn deterministic_thread_fallback(envelopes: &[mxr_core::types::Envelope]) -> String {
    let participants: std::collections::BTreeSet<String> = envelopes
        .iter()
        .flat_map(|e| {
            std::iter::once(e.from.email.clone())
                .chain(e.to.iter().map(|a| a.email.clone()))
        })
        .collect();
    let last = envelopes.last();
    let mut out = String::new();
    out.push_str("# Thread snapshot (LLM disabled)\n\n");
    out.push_str(&format!(
        "- {} message(s), {} participant(s).\n",
        envelopes.len(),
        participants.len()
    ));
    if let Some(last) = last {
        out.push_str(&format!(
            "- Latest: {} from {} on {}.\n",
            last.subject,
            last.from.email,
            last.date.format("%Y-%m-%d")
        ));
    }
    out
}

fn to_thread_briefing(b: &ContextBriefing, from_cache: bool) -> ThreadBriefingData {
    ThreadBriefingData {
        thread_id: b.subject_key.clone(),
        body_markdown: b.body_markdown.clone(),
        citations: b
            .citations
            .iter()
            .map(|c| mxr_protocol::CitationRefData {
                message_id: c.message_id.clone(),
                thread_id: c.thread_id.clone(),
                field: c.field.clone(),
                quote: c.quote.clone(),
            })
            .collect(),
        generated_at: b.generated_at,
        from_cache,
    }
}

// --- recipient briefings (Slice 5.2) -------------------------------

pub(crate) async fn get_recipient_briefing(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
    refresh: bool,
) -> super::HandlerResult {
    let key = email.to_lowercase();
    let contact = lookup_contact_summary(state, account_id, &key).await;

    let content_hash = recipient_content_hash(account_id, &key, contact.as_ref());

    if !refresh {
        if let Ok(Some(cached)) = state
            .store
            .get_context_briefing(account_id, BriefingKind::Recipient, &key)
            .await
        {
            if cached.content_hash == content_hash {
                return Ok(ResponseData::RecipientBriefing {
                    briefing: to_recipient_briefing(&cached, true),
                });
            }
        }
    }

    // Deterministic baseline -- works even when the LLM is off.
    let baseline = recipient_baseline(&key, contact.as_ref());

    let runtime = state.llm.for_feature(LlmFeature::Briefing);
    let req = CompletionRequest {
        max_tokens: Some(500),
        temperature: Some(0.1),
        messages: vec![
            ChatMessage::system(
                "Briefly summarize the user's relationship with this contact for \
                 someone returning to it after a gap. Output STRICT JSON: \
                 {\"summary\": str (markdown)}\nIf no useful summary is possible, \
                 return an empty summary.",
            ),
            ChatMessage::user(baseline.clone()),
        ],
    };

    let body = match runtime.complete(req).await {
        Ok(r) => {
            #[derive(Deserialize)]
            struct Out {
                #[serde(default)]
                summary: String,
            }
            serde_json::from_str::<Out>(r.content.trim())
                .map(|o| o.summary)
                .unwrap_or_default()
        }
        Err(LlmError::Disabled) | Err(LlmError::PrivacyBlocked(_)) => {
            // Don't cache the deterministic-only fallback.
            return Ok(ResponseData::RecipientBriefing {
                briefing: ThreadBriefingData {
                    thread_id: key,
                    body_markdown: baseline,
                    citations: vec![],
                    generated_at: chrono::Utc::now(),
                    from_cache: false,
                },
            });
        }
        Err(e) => return Err(format!("Briefing LLM error: {e}")),
    };

    let body_markdown = if body.is_empty() { baseline } else { body };
    let entry = ContextBriefing {
        id: new_briefing_id(),
        account_id: account_id.clone(),
        kind: BriefingKind::Recipient,
        subject_key: key,
        content_hash,
        body_markdown,
        citations: vec![],
        generated_at: chrono::Utc::now(),
    };
    state
        .store
        .upsert_context_briefing(&entry)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::RecipientBriefing {
        briefing: to_recipient_briefing(&entry, false),
    })
}

struct ContactSummary {
    last_inbound_at: Option<chrono::DateTime<chrono::Utc>>,
    last_outbound_at: Option<chrono::DateTime<chrono::Utc>>,
    total_inbound: i64,
    total_outbound: i64,
    cadence_days_p50: Option<f64>,
}

async fn lookup_contact_summary(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
) -> Option<ContactSummary> {
    use sqlx::Row as _;
    let row = sqlx::query(
        "SELECT last_inbound_at, last_outbound_at, total_inbound, total_outbound, cadence_days_p50
         FROM contacts
         WHERE account_id = ? AND LOWER(email) = LOWER(?)",
    )
    .bind(account_id.as_str())
    .bind(email)
    .fetch_optional(state.store.reader())
    .await
    .ok()
    .flatten()?;
    let last_in: Option<i64> = row.try_get("last_inbound_at").ok();
    let last_out: Option<i64> = row.try_get("last_outbound_at").ok();
    Some(ContactSummary {
        last_inbound_at: last_in.and_then(|s| chrono::DateTime::from_timestamp(s, 0)),
        last_outbound_at: last_out.and_then(|s| chrono::DateTime::from_timestamp(s, 0)),
        total_inbound: row.try_get("total_inbound").unwrap_or(0),
        total_outbound: row.try_get("total_outbound").unwrap_or(0),
        cadence_days_p50: row.try_get("cadence_days_p50").ok(),
    })
}

fn recipient_content_hash(
    account_id: &AccountId,
    email: &str,
    contact: Option<&ContactSummary>,
) -> String {
    let mut h = Sha256::new();
    h.update(account_id.as_str().as_bytes());
    h.update(b"|");
    h.update(email.as_bytes());
    if let Some(c) = contact {
        h.update(b"|");
        h.update(c.last_inbound_at.map(|d| d.timestamp()).unwrap_or(0).to_le_bytes());
        h.update(c.last_outbound_at.map(|d| d.timestamp()).unwrap_or(0).to_le_bytes());
        h.update((c.total_inbound as u64).to_le_bytes());
        h.update((c.total_outbound as u64).to_le_bytes());
    }
    format!("{:x}", h.finalize())
}

fn recipient_baseline(email: &str, contact: Option<&ContactSummary>) -> String {
    let mut out = format!("# {email}\n\n");
    match contact {
        None => out.push_str("- No prior interaction recorded.\n"),
        Some(c) => {
            out.push_str(&format!(
                "- {} inbound, {} outbound.\n",
                c.total_inbound, c.total_outbound
            ));
            if let Some(when) = c.last_inbound_at {
                out.push_str(&format!(
                    "- Last inbound: {}.\n",
                    when.format("%Y-%m-%d")
                ));
            }
            if let Some(when) = c.last_outbound_at {
                out.push_str(&format!(
                    "- Last outbound: {}.\n",
                    when.format("%Y-%m-%d")
                ));
            }
            if let Some(p50) = c.cadence_days_p50 {
                out.push_str(&format!("- Reply cadence p50: {p50:.1}d.\n"));
            }
        }
    }
    out
}

fn to_recipient_briefing(b: &ContextBriefing, from_cache: bool) -> ThreadBriefingData {
    ThreadBriefingData {
        thread_id: b.subject_key.clone(),
        body_markdown: b.body_markdown.clone(),
        citations: vec![],
        generated_at: b.generated_at,
        from_cache,
    }
}

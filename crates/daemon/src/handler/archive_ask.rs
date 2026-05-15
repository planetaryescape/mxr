//! Slice 3.1 of docs/ai-email/03-archive-intelligence.md.
//!
//! `mxr ask "<question>"` runs a citation-validated retrieval against
//! the local archive. Flow:
//!   1. Apply explicit filters (from/to/after/before) up-front so the
//!      LLM only ever sees in-scope messages.
//!   2. Lexical search via SearchService (semantic fallback deferred —
//!      `executed_mode` reports what actually ran).
//!   3. Fetch top-N envelopes + reader-cleaned bodies and stamp every
//!      one with a `[msg_id=...]` marker the LLM can cite back.
//!   4. LLM `ArchiveAsk` feature → strict JSON.
//!   5. Reject citations whose msg id is not in the retrieved set.

use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::SortOrder;
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature};
use mxr_protocol::{
    ArchiveAnswerData, ArchiveAskFiltersData, ArchiveAskMode, ArchiveCitationData,
    ArchiveRetrievalData, ResponseData,
};
use mxr_reader::{clean, ReaderConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LlmAsk {
    #[serde(default)]
    answer: String,
    #[serde(default)]
    citations: Vec<LlmCitation>,
}

#[derive(Debug, Deserialize)]
struct LlmCitation {
    #[serde(default)]
    msg_id: String,
    #[serde(default)]
    quote: String,
}

#[derive(Debug, Clone)]
struct RetrievedCitation {
    message_id: mxr_core::id::MessageId,
    thread_id: mxr_core::id::ThreadId,
    subject: String,
    date: chrono::DateTime<chrono::Utc>,
    excerpt: String,
}

impl RetrievedCitation {
    fn with_quote(&self, quote: String) -> ArchiveCitationData {
        ArchiveCitationData {
            message_id: self.message_id.clone(),
            thread_id: self.thread_id.clone(),
            subject: self.subject.clone(),
            date: self.date,
            quote,
        }
    }
}

pub(crate) async fn ask(
    state: &AppState,
    question: &str,
    filters: &ArchiveAskFiltersData,
    limit: usize,
) -> super::HandlerResult {
    if question.trim().is_empty() {
        return Err("question cannot be empty".into());
    }

    let requested_mode = filters.mode;
    let candidate_pool = (limit * 4).max(1);
    let (candidate_ids, executed_mode) =
        retrieve_candidates(state, question, filters, candidate_pool, requested_mode).await?;

    let mut allowed = Vec::new();
    let mut citation_sources = std::collections::HashMap::new();
    let mut transcript = String::new();
    for id in candidate_ids.into_iter() {
        if allowed.len() >= limit {
            break;
        }
        let envelope = match state.store.get_envelope(&id).await {
            Ok(Some(env)) => env,
            _ => continue,
        };
        // (continue with envelope filtering + transcript build)
        if !pass_filter(&envelope, filters) {
            continue;
        }
        let body = state
            .store
            .get_body(&envelope.id)
            .await
            .ok()
            .flatten()
            .and_then(|b| b.text_plain)
            .unwrap_or_else(|| envelope.snippet.clone());
        let cleaned = clean(Some(&body), None, &ReaderConfig::default()).content;
        transcript.push_str(&format!(
            "[msg_id={}]\nFrom: {}\nTo: {}\nSubject: {}\nDate: {}\n{}\n\n",
            envelope.id,
            envelope.from.email,
            envelope
                .to
                .iter()
                .map(|a| a.email.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            envelope.subject,
            envelope.date.to_rfc3339(),
            cleaned,
        ));
        let msg_id = envelope.id.to_string();
        citation_sources.insert(
            msg_id.clone(),
            RetrievedCitation {
                message_id: envelope.id.clone(),
                thread_id: envelope.thread_id.clone(),
                subject: envelope.subject.clone(),
                date: envelope.date,
                excerpt: excerpt(&cleaned, 160),
            },
        );
        allowed.push(msg_id);
    }

    let runtime = state.llm.for_feature(LlmFeature::ArchiveAsk);
    let req = CompletionRequest {
        max_tokens: Some(900),
        temperature: Some(0.0),
        messages: vec![
            ChatMessage::system(
                "Answer the user's question using ONLY the provided messages. \
                 Output STRICT JSON with the schema and nothing else:\n\
                 {\"answer\": str, \"citations\": [{\"msg_id\": str, \"quote\": str}]}\n\n\
                 Cite ONLY msg_id values that appear in [msg_id=...] markers. \
                 If the messages do not contain enough evidence, set \"answer\" \
                 to a short note such as \"Not enough evidence in archive\" and \
                 return an empty citations array.",
            ),
            ChatMessage::user(format!(
                "MESSAGES:\n{transcript}\n\nQUESTION: {question}\n\nReturn JSON only."
            )),
        ],
    };

    let response = match runtime.complete(req).await {
        Ok(r) => r,
        Err(LlmError::Disabled) | Err(LlmError::PrivacyBlocked(_)) => {
            return Ok(ResponseData::ArchiveAnswer {
                answer: ArchiveAnswerData {
                    text: "Synthesis unavailable; showing retrieved archive evidence instead."
                        .into(),
                    citations: allowed
                        .iter()
                        .filter_map(|id| citation_sources.get(id))
                        .map(|source| source.with_quote(source.excerpt.clone()))
                        .collect(),
                    retrieval: ArchiveRetrievalData {
                        requested_mode,
                        executed_mode,
                        candidate_count: allowed.len() as u32,
                    },
                },
            });
        }
        Err(e) => return Err(format!("ArchiveAsk LLM error: {e}")),
    };

    let parsed: LlmAsk = serde_json::from_str(response.content.trim())
        .map_err(|e| format!("ArchiveAsk: LLM returned non-JSON ({e})"))?;

    if parsed.citations.is_empty() && !is_insufficient_evidence_answer(&parsed.answer) {
        return Err("ArchiveAsk: answer requires at least one citation".into());
    }

    let allowed_set: std::collections::HashSet<&str> = allowed.iter().map(|s| s.as_str()).collect();
    let mut citations = Vec::new();
    for citation in parsed.citations {
        if !allowed_set.contains(citation.msg_id.as_str()) {
            return Err(format!(
                "ArchiveAsk: LLM cited unknown msg_id {} (not in retrieved set)",
                citation.msg_id
            ));
        }
        let source = citation_sources
            .get(citation.msg_id.as_str())
            .ok_or_else(|| {
                format!(
                    "ArchiveAsk: missing citation metadata for {}",
                    citation.msg_id
                )
            })?;
        citations.push(source.with_quote(citation.quote));
    }

    Ok(ResponseData::ArchiveAnswer {
        answer: ArchiveAnswerData {
            text: parsed.answer,
            citations,
            retrieval: ArchiveRetrievalData {
                requested_mode,
                executed_mode,
                candidate_count: allowed.len() as u32,
            },
        },
    })
}

fn is_insufficient_evidence_answer(answer: &str) -> bool {
    let answer = answer.trim().to_ascii_lowercase();
    answer.contains("not enough evidence")
        || answer.contains("insufficient evidence")
        || answer.contains("no local evidence")
}

fn excerpt(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

/// Hybrid candidate retrieval. Returns the merged candidate list AND
/// the mode that actually executed (which may differ from the
/// requested mode — e.g. Hybrid downgrades to Lexical when semantic
/// is disabled or returns nothing).
///
/// Fusion strategy: reciprocal rank fusion over (lexical_rank,
/// semantic_rank). RRF is order-stable, doesn't require score
/// normalization across the two engines, and gracefully handles a
/// candidate appearing in only one source.
async fn retrieve_candidates(
    state: &AppState,
    question: &str,
    filters: &ArchiveAskFiltersData,
    pool_size: usize,
    requested: ArchiveAskMode,
) -> Result<(Vec<mxr_core::MessageId>, ArchiveAskMode), String> {
    let semantic_enabled = state
        .semantic
        .status_snapshot()
        .await
        .map(|s| s.enabled)
        .unwrap_or(false);

    let want_lexical =
        matches!(requested, ArchiveAskMode::Lexical | ArchiveAskMode::Hybrid) || !semantic_enabled;
    let want_semantic =
        matches!(requested, ArchiveAskMode::Semantic | ArchiveAskMode::Hybrid) && semantic_enabled;

    let lexical_ids: Vec<mxr_core::MessageId> = if want_lexical {
        let query = lexical_query_with_filters(question, filters);
        let page = if has_structured_filters(filters) {
            let ast = mxr_search::parse_query(&query).map_err(|e| e.to_string())?;
            let schema = mxr_search::MxrSchema::build();
            let query = mxr_search::QueryBuilder::new(&schema).build(&ast);
            state
                .search
                .search_ast(query, pool_size, 0, SortOrder::Relevance)
                .await
                .map_err(|e| e.to_string())?
        } else {
            state
                .search
                .search(&query, pool_size, 0, SortOrder::Relevance)
                .await
                .map_err(|e| e.to_string())?
        };
        page.results
            .iter()
            .filter_map(|h| h.message_id.parse().ok())
            .collect()
    } else {
        Vec::new()
    };

    let semantic_ids: Vec<mxr_core::MessageId> = if want_semantic {
        let hits = state
            .semantic
            .search(
                question,
                pool_size,
                &[mxr_core::types::SemanticChunkSourceKind::Body],
            )
            .await
            .map_err(|e| e.to_string())
            .unwrap_or_default();
        // Deduplicate by message id while preserving rank order
        // (semantic hits can have multiple chunks per message).
        let mut seen = std::collections::HashSet::new();
        hits.into_iter()
            .filter_map(|h| {
                if seen.insert(h.message_id.clone()) {
                    Some(h.message_id)
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // Pick executed_mode honestly: if both sources contributed, that's
    // Hybrid; if only one did, name it.
    let executed_mode = match (lexical_ids.is_empty(), semantic_ids.is_empty()) {
        (false, false) => ArchiveAskMode::Hybrid,
        (false, true) => ArchiveAskMode::Lexical,
        (true, false) => ArchiveAskMode::Semantic,
        // Both empty: report what the caller asked for, just with no
        // candidates. The doc spec requires Lexical when semantic
        // returns empty, so default to Lexical here.
        (true, true) => ArchiveAskMode::Lexical,
    };

    let merged = reciprocal_rank_fuse(&lexical_ids, &semantic_ids, pool_size);
    Ok((merged, executed_mode))
}

fn has_structured_filters(filters: &ArchiveAskFiltersData) -> bool {
    filters
        .from
        .as_deref()
        .is_some_and(|v| !v.trim().is_empty())
        || filters.to.as_deref().is_some_and(|v| !v.trim().is_empty())
        || filters.after.is_some()
        || filters.before.is_some()
}

fn lexical_query_with_filters(question: &str, filters: &ArchiveAskFiltersData) -> String {
    let mut parts = vec![question.trim().to_string()];
    if let Some(from) = filters.from.as_deref().filter(|v| !v.trim().is_empty()) {
        parts.push(format!("from:{}", quote_query_value(from)));
    }
    if let Some(to) = filters.to.as_deref().filter(|v| !v.trim().is_empty()) {
        parts.push(format!("to:{}", quote_query_value(to)));
    }
    if let Some(after) = filters.after {
        parts.push(format!("after:{}", after.format("%Y-%m-%d")));
    }
    if let Some(before) = filters.before {
        parts.push(format!("before:{}", before.format("%Y-%m-%d")));
    }
    parts.retain(|part| !part.trim().is_empty());
    parts.join(" ")
}

fn quote_query_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().any(char::is_whitespace) {
        format!("\"{}\"", trimmed.replace('"', ""))
    } else {
        trimmed.to_string()
    }
}

/// Reciprocal Rank Fusion (k=60 per the standard RRF paper).
fn reciprocal_rank_fuse(
    lexical: &[mxr_core::MessageId],
    semantic: &[mxr_core::MessageId],
    limit: usize,
) -> Vec<mxr_core::MessageId> {
    const K: f64 = 60.0;
    let mut scores: std::collections::HashMap<mxr_core::MessageId, f64> =
        std::collections::HashMap::new();
    for (rank, id) in lexical.iter().enumerate() {
        *scores.entry(id.clone()).or_default() += 1.0 / (K + rank as f64 + 1.0);
    }
    for (rank, id) in semantic.iter().enumerate() {
        *scores.entry(id.clone()).or_default() += 1.0 / (K + rank as f64 + 1.0);
    }
    let mut ranked: Vec<_> = scores.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            // Stable secondary: lexical rank first, then semantic.
            .then(a.0.to_string().cmp(&b.0.to_string()))
    });
    ranked.into_iter().take(limit).map(|(id, _)| id).collect()
}

fn pass_filter(env: &mxr_core::types::Envelope, f: &ArchiveAskFiltersData) -> bool {
    if let Some(account) = f.account_id.as_ref() {
        if env.account_id != *account {
            return false;
        }
    }
    if let Some(from) = f.from.as_deref() {
        if !env.from.email.eq_ignore_ascii_case(from) {
            return false;
        }
    }
    if let Some(to) = f.to.as_deref() {
        if !env.to.iter().any(|a| a.email.eq_ignore_ascii_case(to)) {
            return false;
        }
    }
    if let Some(after) = f.after {
        if env.date < after {
            return false;
        }
    }
    if let Some(before) = f.before {
        if env.date > before {
            return false;
        }
    }
    true
}

// `account_id` is informational on the filter struct; the dispatch
// layer fills it in. We accept it here but don't require it.
#[allow(dead_code)]
fn _silence_unused() -> Option<AccountId> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::*;
    use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
    use mxr_search::{SearchIndexEntry, SearchUpdateBatch};
    use std::sync::{Arc, Mutex};

    struct CannedLlm {
        body: String,
        last_user: Mutex<String>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for CannedLlm {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            if let Some(last) = req.messages.last() {
                *self.last_user.lock().unwrap() = last.content.clone();
            }
            Ok(CompletionResponse {
                content: self.body.clone(),
                model: "stub".into(),
                finish_reason: Some("stop".into()),
            })
        }
        fn capabilities(&self) -> LlmCapabilities {
            LlmCapabilities {
                context_window: 8192,
                supports_streaming: false,
            }
        }
        fn model_name(&self) -> &str {
            "stub"
        }
    }

    fn envelope(account_id: &AccountId, from: &str, subject: &str, days_ago: i64) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
            provider_id: format!("p-{}", uuid::Uuid::now_v7()),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: None,
                email: from.into(),
            },
            to: vec![Address {
                name: None,
                email: "user@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: subject.into(),
            date: chrono::Utc::now() - chrono::Duration::days(days_ago),
            flags: MessageFlags::empty(),
            snippet: subject.to_string(),
            has_attachments: false,
            size_bytes: 1,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
        }
    }

    async fn fixture(
        llm: Arc<CannedLlm>,
    ) -> (Arc<crate::state::AppState>, AccountId, Vec<MessageId>) {
        let (state, _) = crate::state::AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        state.llm.replace(llm);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        let mut ids = Vec::new();
        let mut entries = Vec::new();
        for i in 0..3 {
            let env = envelope(
                &account_id,
                "alice@example.com",
                &format!("status update {i}"),
                i,
            );
            state
                .store
                .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
                .await
                .unwrap();
            let body = MessageBody {
                message_id: env.id.clone(),
                text_plain: Some(format!("Body of message {i}: status update content.")),
                text_html: None,
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: MessageMetadata::default(),
            };
            state.store.insert_body(&body).await.unwrap();
            ids.push(env.id.clone());
            entries.push(SearchIndexEntry {
                envelope: env,
                body: Some(body),
                reply_later: false,
            });
        }
        state
            .search
            .apply_batch(SearchUpdateBatch {
                entries,
                removed_message_ids: vec![],
            })
            .await
            .unwrap();
        state.search.commit().await.unwrap();
        (state, account_id, ids)
    }

    #[tokio::test]
    async fn answer_includes_only_cited_msg_ids_from_retrieved_set() {
        let stub = Arc::new(CannedLlm {
            body: String::new(),
            last_user: Mutex::new(String::new()),
        });
        let (state, account_id, ids) = fixture(stub.clone()).await;
        // Replace canned body with valid citations referencing real ids.
        let body = format!(
            r#"{{"answer":"They confirmed the price.","citations":[{{"msg_id":"{}","quote":"price"}}]}}"#,
            ids[0]
        );
        // Re-replace with new stub holding the real-id body.
        let stub2 = Arc::new(CannedLlm {
            body,
            last_user: Mutex::new(String::new()),
        });
        state.llm.replace(stub2.clone());

        let resp = ask(
            &state,
            "what was the status update?",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                ..Default::default()
            },
            5,
        )
        .await
        .unwrap();
        match resp {
            ResponseData::ArchiveAnswer { answer } => {
                assert!(!answer.text.is_empty());
                assert_eq!(answer.citations.len(), 1);
                assert_eq!(answer.citations[0].message_id, ids[0]);
                assert!(answer.retrieval.candidate_count > 0);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn citations_include_thread_subject_and_date_for_open_thread_actions() {
        let stub = Arc::new(CannedLlm {
            body: String::new(),
            last_user: Mutex::new(String::new()),
        });
        let (state, account_id, ids) = fixture(stub).await;
        let env = state
            .store
            .get_envelope(&ids[0])
            .await
            .unwrap()
            .expect("fixture envelope exists");
        let body = format!(
            r#"{{"answer":"They confirmed the price.","citations":[{{"msg_id":"{}","quote":"price"}}]}}"#,
            ids[0]
        );
        let stub = Arc::new(CannedLlm {
            body,
            last_user: Mutex::new(String::new()),
        });
        state.llm.replace(stub);

        let resp = ask(
            &state,
            "what was the status update?",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                ..Default::default()
            },
            5,
        )
        .await
        .unwrap();

        let ResponseData::ArchiveAnswer { answer } = resp else {
            panic!("unexpected response");
        };
        assert_eq!(answer.citations.len(), 1);
        let citation = &answer.citations[0];
        assert_eq!(citation.message_id, env.id);
        assert_eq!(citation.thread_id, env.thread_id);
        assert_eq!(citation.subject, env.subject);
        assert_eq!(citation.date, env.date);
        assert_eq!(citation.quote, "price");
    }

    #[tokio::test]
    async fn synthesized_answer_without_citations_is_rejected() {
        let stub = Arc::new(CannedLlm {
            body: r#"{"answer":"Alice approved the price increase.","citations":[]}"#.into(),
            last_user: Mutex::new(String::new()),
        });
        let (state, account_id, _ids) = fixture(stub).await;

        let err = ask(
            &state,
            "what was approved?",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                ..Default::default()
            },
            5,
        )
        .await
        .expect_err("answers without evidence must be rejected");

        assert!(
            err.contains("citation") || err.contains("evidence"),
            "error must explain missing evidence: {err}"
        );
    }

    #[tokio::test]
    async fn invalid_citation_is_rejected() {
        let stub = Arc::new(CannedLlm {
            body: r#"{"answer":"Made up answer","citations":[{"msg_id":"00000000-0000-0000-0000-000000000099","quote":"fabricated"}]}"#.into(),
            last_user: Mutex::new(String::new()),
        });
        let (state, account_id, _ids) = fixture(stub).await;
        let res = ask(
            &state,
            "status",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                ..Default::default()
            },
            5,
        )
        .await;
        let err = res.expect_err("must reject unknown msg_id citation");
        assert!(
            err.contains("00000000-0000-0000-0000-000000000099"),
            "error must name the bad citation: {err}"
        );
    }

    #[tokio::test]
    async fn empty_question_is_rejected() {
        let stub = Arc::new(CannedLlm {
            body: r#"{"answer":"","citations":[]}"#.into(),
            last_user: Mutex::new(String::new()),
        });
        let (state, _, _) = fixture(stub).await;
        let err = ask(&state, "   ", &ArchiveAskFiltersData::default(), 5)
            .await
            .expect_err("blank question must be rejected before LLM");
        assert!(err.contains("question cannot be empty"));
    }

    /// Slice 3.1 wiring contract (C2.3): semantic disabled, lexical
    /// returns hits → executed_mode = Lexical. The default fixture
    /// has semantic disabled (in_memory_with_fake doesn't activate
    /// any profile), so this is the baseline path.
    #[tokio::test]
    async fn semantic_disabled_reports_executed_mode_lexical() {
        let stub = Arc::new(CannedLlm {
            body: r#"{"answer":"Not enough evidence in archive","citations":[]}"#.into(),
            last_user: Mutex::new(String::new()),
        });
        let (state, account_id, _ids) = fixture(stub).await;
        let resp = ask(
            &state,
            "status update",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                mode: ArchiveAskMode::Hybrid,
                ..Default::default()
            },
            5,
        )
        .await
        .unwrap();
        match resp {
            ResponseData::ArchiveAnswer { answer } => {
                assert_eq!(
                    answer.retrieval.requested_mode,
                    ArchiveAskMode::Hybrid,
                    "requested_mode preserved verbatim"
                );
                // Semantic is disabled in the fixture, so the only
                // contributor is lexical.
                assert_eq!(
                    answer.retrieval.executed_mode,
                    ArchiveAskMode::Lexical,
                    "executed_mode reports actual source even when caller asked for Hybrid"
                );
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    /// Slice 3.1 wiring contract (C2.3): when caller explicitly asks
    /// for ArchiveAskMode::Lexical, executed_mode is Lexical and the
    /// semantic engine is not consulted (so semantic-disabled doesn't
    /// matter and no time is wasted on it).
    #[tokio::test]
    async fn explicit_lexical_mode_reports_lexical() {
        let stub = Arc::new(CannedLlm {
            body: r#"{"answer":"Not enough evidence in archive","citations":[]}"#.into(),
            last_user: Mutex::new(String::new()),
        });
        let (state, account_id, _) = fixture(stub).await;
        let resp = ask(
            &state,
            "status update",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                mode: ArchiveAskMode::Lexical,
                ..Default::default()
            },
            5,
        )
        .await
        .unwrap();
        let ResponseData::ArchiveAnswer { answer } = resp else {
            panic!("unexpected");
        };
        assert_eq!(answer.retrieval.executed_mode, ArchiveAskMode::Lexical);
    }

    #[tokio::test]
    async fn rrf_is_order_stable_and_dedupes_message_ids() {
        // Pure unit on the fusion helper -- exercises the RRF math
        // without spinning up a fixture. Rank-1 in lexical AND rank-1
        // in semantic should beat rank-1 in only one source.
        let id_a = mxr_core::MessageId::new();
        let id_b = mxr_core::MessageId::new();
        let id_c = mxr_core::MessageId::new();
        // a: lexical rank 0 + semantic rank 0
        // b: lexical rank 1 (only)
        // c: semantic rank 1 (only)
        let lex = vec![id_a.clone(), id_b.clone()];
        let sem = vec![id_a.clone(), id_c.clone()];
        let merged = reciprocal_rank_fuse(&lex, &sem, 10);
        assert_eq!(merged.len(), 3, "no duplicates: {merged:?}");
        assert_eq!(merged[0], id_a, "double-source candidate ranks first");
    }

    /// Build an envelope addressed to a specific recipient. Mirrors
    /// `envelope()` but lets the test set the To: list, which the
    /// default fixture hard-codes to user@example.com.
    fn envelope_to_recipient(
        account_id: &AccountId,
        from: &str,
        to: &str,
        subject: &str,
    ) -> Envelope {
        let mut env = envelope(account_id, from, subject, 0);
        env.to = vec![Address {
            name: None,
            email: to.into(),
        }];
        env
    }

    /// `pass_filter` must honor `filters.to` -- a CLI user running
    /// `mxr ask "..." --to alice@example.com` is asking for messages
    /// addressed to alice, not messages from her. The filter struct
    /// has had this field for a while (`ArchiveAskFiltersData::to`)
    /// but the handler was ignoring it, so junk messages addressed
    /// to other recipients were leaking into the LLM prompt and
    /// poisoning the answer.
    #[tokio::test]
    async fn to_filter_drops_envelopes_not_addressed_to_recipient() {
        let (state, _) = crate::state::AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        // Three messages, all mentioning "status update" so search
        // retrieves all three, but only one is addressed to alice.
        let env_alice = envelope_to_recipient(
            &account_id,
            "team@example.com",
            "alice@example.com",
            "status update for alice",
        );
        let env_bob = envelope_to_recipient(
            &account_id,
            "team@example.com",
            "bob@example.com",
            "status update for bob",
        );
        let env_carol = envelope_to_recipient(
            &account_id,
            "team@example.com",
            "carol@example.com",
            "status update for carol",
        );

        let mut entries = Vec::new();
        for env in [&env_alice, &env_bob, &env_carol] {
            state
                .store
                .upsert_envelope_with_direction(env, MessageDirection::Inbound)
                .await
                .unwrap();
            let body = MessageBody {
                message_id: env.id.clone(),
                text_plain: Some(format!("{}: status update content", env.subject)),
                text_html: None,
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: MessageMetadata::default(),
            };
            state.store.insert_body(&body).await.unwrap();
            entries.push(SearchIndexEntry {
                envelope: env.clone(),
                body: Some(body),
                reply_later: false,
            });
        }
        state
            .search
            .apply_batch(SearchUpdateBatch {
                entries,
                removed_message_ids: vec![],
            })
            .await
            .unwrap();
        state.search.commit().await.unwrap();

        let stub = Arc::new(CannedLlm {
            body: format!(
                r#"{{"answer":"Bob has the status update.","citations":[{{"msg_id":"{}","quote":"status"}}]}}"#,
                env_bob.id
            ),
            last_user: Mutex::new(String::new()),
        });
        state.llm.replace(stub);

        let err = ask(
            &state,
            "status update",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                to: Some("alice@example.com".into()),
                ..Default::default()
            },
            5,
        )
        .await
        .expect_err("out-of-scope Bob citation must be rejected");
        assert!(
            err.contains(&env_bob.id.to_string()),
            "error must name the out-of-scope cited message: {err}"
        );
    }

    #[tokio::test]
    async fn date_filter_drops_out_of_range_envelopes_from_prompt() {
        let stub = Arc::new(CannedLlm {
            body: r#"{"answer":"Not enough evidence in archive","citations":[]}"#.into(),
            last_user: Mutex::new(String::new()),
        });
        let (state, account_id, ids) = fixture(stub.clone()).await;
        // Re-stub the same way to keep the prompt-capture mutex stable.
        let stub2 = Arc::new(CannedLlm {
            body: format!(
                r#"{{"answer":"The old message had the status.","citations":[{{"msg_id":"{}","quote":"status"}}]}}"#,
                ids[2]
            ),
            last_user: Mutex::new(String::new()),
        });
        state.llm.replace(stub2.clone());

        // Only ids[2] is 2 days old; ids[0] is 0 days, ids[1] is 1 day.
        // Filter `after` = 1.5 days ago in the past should keep ids[0]
        // and ids[1] but drop ids[2].
        let after = chrono::Utc::now() - chrono::Duration::hours(36);
        let err = ask(
            &state,
            "status",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                after: Some(after),
                ..Default::default()
            },
            5,
        )
        .await
        .expect_err("out-of-range old citation must be rejected");
        assert!(
            err.contains(&ids[2].to_string()),
            "error must name the out-of-range cited message: {err}"
        );
    }

    #[tokio::test]
    async fn structured_filters_are_applied_to_candidate_retrieval() {
        let (state, _) = crate::state::AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        let mut entries = Vec::new();
        for i in 0..6 {
            let env = envelope(
                &account_id,
                "bob@example.com",
                &format!("pricing update {i}"),
                i,
            );
            state
                .store
                .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
                .await
                .unwrap();
            let body = MessageBody {
                message_id: env.id.clone(),
                text_plain: Some("high-ranking out-of-scope pricing note".into()),
                text_html: None,
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: MessageMetadata::default(),
            };
            state.store.insert_body(&body).await.unwrap();
            entries.push(SearchIndexEntry {
                envelope: env,
                body: Some(body),
                reply_later: false,
            });
        }

        let alice = envelope(&account_id, "alice@example.com", "q2 decision", 0);
        state
            .store
            .upsert_envelope_with_direction(&alice, MessageDirection::Inbound)
            .await
            .unwrap();
        let alice_body = MessageBody {
            message_id: alice.id.clone(),
            text_plain: Some("pricing decision from Alice".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };
        state.store.insert_body(&alice_body).await.unwrap();
        entries.push(SearchIndexEntry {
            envelope: alice.clone(),
            body: Some(alice_body),
            reply_later: false,
        });

        state
            .search
            .apply_batch(SearchUpdateBatch {
                entries,
                removed_message_ids: vec![],
            })
            .await
            .unwrap();
        state.search.commit().await.unwrap();

        let stub = Arc::new(CannedLlm {
            body: format!(
                r#"{{"answer":"Alice made the pricing decision.","citations":[{{"msg_id":"{}","quote":"pricing decision"}}]}}"#,
                alice.id
            ),
            last_user: Mutex::new(String::new()),
        });
        state.llm.replace(stub);

        let resp = ask(
            &state,
            "pricing",
            &ArchiveAskFiltersData {
                account_id: Some(account_id),
                from: Some("alice@example.com".into()),
                ..Default::default()
            },
            1,
        )
        .await
        .unwrap();
        let ResponseData::ArchiveAnswer { answer } = resp else {
            panic!("unexpected response");
        };
        assert_eq!(answer.citations.len(), 1);
        assert_eq!(answer.citations[0].message_id, alice.id);
    }
}

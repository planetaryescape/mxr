use crate::schema::MxrSchema;
use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use mxr_core::types::{Envelope, MessageBody, SortOrder};
use mxr_core::MxrError;
use mxr_reader::searchable_text;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tantivy::{
    collector::{Count, TopDocs},
    query::{AllQuery, BooleanQuery, Occur, Query, QueryParser, TermQuery},
    schema::IndexRecordOption,
    schema::Value,
    Index, IndexReader, IndexWriter, Order, ReloadPolicy, TantivyDocument,
};

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: IndexWriter,
    schema: MxrSchema,
    /// Bodies whose cleaning panicked and were indexed from the fallback text.
    /// Shared with the service handle so a rebuild can report the count.
    pub(crate) clean_fallbacks: Arc<AtomicU64>,
}

/// The body text added to the index: the text rules match too, from
/// `mxr_reader::searchable_text`. The reader's built-in converter is used,
/// never `render.html_command`. Panics counted in `clean_fallbacks`.
fn indexed_body_text(body: &MessageBody, clean_fallbacks: &AtomicU64) -> String {
    let searchable = searchable_text(body.text_plain.as_deref(), body.text_html.as_deref());
    clean_fallbacks.fetch_add(
        u64::try_from(searchable.fallbacks).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
    searchable.text
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub message_id: String,
    pub account_id: String,
    pub thread_id: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct SearchPage {
    pub results: Vec<SearchResult>,
    pub total: usize,
    pub has_more: bool,
    pub next_offset: Option<usize>,
}

fn sane_search_sort_timestamp(timestamp: i64) -> i64 {
    let cutoff = (chrono::Utc::now() + chrono::Duration::days(1)).timestamp();
    if timestamp > cutoff {
        0
    } else {
        timestamp
    }
}

fn normalized_message_id_header(value: Option<&str>) -> String {
    value
        .unwrap_or("")
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_ascii_lowercase()
}

fn has_user_label(labels: &[String]) -> bool {
    labels.iter().any(|label| !is_system_label(label))
}

fn is_system_label(label: &str) -> bool {
    let label = label.to_ascii_uppercase();
    label.starts_with("CATEGORY_")
        || matches!(
            label.as_str(),
            "INBOX"
                | "SENT"
                | "DRAFT"
                | "DRAFTS"
                | "TRASH"
                | "DELETED"
                | "SPAM"
                | "JUNK"
                | "STARRED"
                | "UNREAD"
                | "IMPORTANT"
                | "CHAT"
                | "ARCHIVE"
                | "ARCHIVED"
                | "SNOOZED"
                | "MUTED"
                | "MUTE"
        )
}

fn delivered_to_values(raw_headers: Option<&str>) -> Vec<String> {
    let mut values = Vec::new();
    let Some(raw_headers) = raw_headers else {
        return values;
    };

    for line in raw_headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.eq_ignore_ascii_case("delivered-to") || name.eq_ignore_ascii_case("x-original-to") {
            let value = value.trim().trim_matches('<').trim_matches('>');
            if !value.is_empty() {
                values.push(value.to_ascii_lowercase());
            }
        }
    }

    values
}

fn content_hints(body: &MessageBody) -> String {
    let mut hints = Vec::new();
    if let Some(text) = body.text_plain.as_deref() {
        hints.push(text.to_string());
    }
    if let Some(html) = body.text_html.as_deref() {
        hints.push(html.to_string());
    }
    if let Some(list_id) = body.metadata.list_id.as_deref() {
        hints.push(list_id.to_string());
    }
    for attachment in &body.attachments {
        hints.push(attachment.filename.clone());
        hints.push(attachment.mime_type.clone());
        if let Some(content_id) = attachment.content_id.as_deref() {
            hints.push(content_id.to_string());
        }
        if let Some(content_location) = attachment.content_location.as_deref() {
            hints.push(content_location.to_string());
        }
    }
    hints.join(" ")
}

impl SearchIndex {
    pub fn schema(&self) -> &MxrSchema {
        &self.schema
    }

    pub fn open(index_path: &Path) -> Result<Self, MxrError> {
        let (index, _) = Self::open_with_rebuild_status(index_path)?;
        Ok(index)
    }

    pub fn open_with_rebuild_status(index_path: &Path) -> Result<(Self, bool), MxrError> {
        let schema_def = MxrSchema::build();
        let dir = tantivy::directory::MmapDirectory::open(index_path)
            .map_err(|e| MxrError::Search(e.to_string()))?;

        // The index is rebuildable derived data (SQLite is canonical), so
        // ANY failure to open an existing index — our schema changed, a
        // tantivy upgrade changed the on-disk format, or the directory is
        // corrupt — is handled by wiping and rebuilding rather than failing
        // daemon startup. Upgrades must never require manual intervention.
        let (index, rebuilt) = match Index::open_or_create(dir, schema_def.schema.clone()) {
            Ok(idx) => (idx, false),
            Err(e) => {
                tracing::warn!("Search index unusable ({e}); wiping and rebuilding from store");
                if index_path.exists() {
                    std::fs::remove_dir_all(index_path)
                        .map_err(|e| MxrError::Search(e.to_string()))?;
                    std::fs::create_dir_all(index_path)
                        .map_err(|e| MxrError::Search(e.to_string()))?;
                }
                let dir = tantivy::directory::MmapDirectory::open(index_path)
                    .map_err(|e| MxrError::Search(e.to_string()))?;
                (
                    Index::open_or_create(dir, schema_def.schema.clone())
                        .map_err(|e| MxrError::Search(e.to_string()))?,
                    true,
                )
            }
        };

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| MxrError::Search(e.to_string()))?;

        let writer = index
            .writer(50_000_000)
            .map_err(|e| MxrError::Search(e.to_string()))?;

        Ok((
            Self {
                index,
                reader,
                writer,
                schema: schema_def,
                clean_fallbacks: Arc::new(AtomicU64::new(0)),
            },
            rebuilt,
        ))
    }

    pub fn in_memory() -> Result<Self, MxrError> {
        let schema_def = MxrSchema::build();
        let index = Index::create_in_ram(schema_def.schema.clone());

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|e: tantivy::TantivyError| MxrError::Search(e.to_string()))?;

        let writer = index
            .writer(15_000_000)
            .map_err(|e| MxrError::Search(e.to_string()))?;

        Ok(Self {
            index,
            reader,
            writer,
            schema: schema_def,
            clean_fallbacks: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn index_envelope(&mut self, envelope: &Envelope) -> Result<(), MxrError> {
        self.index_envelope_with_reply_later(envelope, false)
    }

    pub fn index_envelope_with_reply_later(
        &mut self,
        envelope: &Envelope,
        reply_later: bool,
    ) -> Result<(), MxrError> {
        // Re-indexing is upsert-by-message-id; without delete_term we would leave
        // stale documents in Tantivy and `mxr search` would return duplicates
        // after every mutation that triggers a re-index.
        let term = tantivy::Term::from_field_text(self.schema.message_id, &envelope.id.as_str());
        self.writer.delete_term(term);

        let s = &self.schema;
        let mut doc = TantivyDocument::new();
        doc.add_text(s.message_id, envelope.id.as_str());
        doc.add_text(s.account_id, envelope.account_id.as_str());
        doc.add_text(s.thread_id, envelope.thread_id.as_str());
        doc.add_text(
            s.message_id_header,
            normalized_message_id_header(envelope.message_id_header.as_deref()),
        );
        doc.add_text(s.subject, &envelope.subject);
        doc.add_text(s.from_name, envelope.from.name.as_deref().unwrap_or(""));
        doc.add_text(s.from_email, envelope.from.email.to_ascii_lowercase());
        for addr in &envelope.to {
            doc.add_text(s.to_email, addr.email.to_ascii_lowercase());
        }
        for addr in &envelope.cc {
            doc.add_text(s.cc_email, addr.email.to_ascii_lowercase());
        }
        for addr in &envelope.bcc {
            doc.add_text(s.bcc_email, addr.email.to_ascii_lowercase());
        }
        doc.add_text(s.snippet, &envelope.snippet);
        for label in &envelope.label_provider_ids {
            doc.add_text(s.labels, label.to_lowercase());
        }
        doc.add_u64(s.size_bytes, envelope.size_bytes);
        doc.add_u64(s.flags, envelope.flags.bits() as u64);
        doc.add_bool(s.has_attachments, envelope.has_attachments);
        doc.add_bool(s.has_calendar, false);
        doc.add_bool(s.has_link, envelope.link_count > 0);
        doc.add_u64(s.link_density, envelope.link_density().as_db_u8() as u64);
        doc.add_bool(
            s.has_user_labels,
            has_user_label(&envelope.label_provider_ids),
        );
        doc.add_bool(s.is_read, envelope.flags.contains(MessageFlags::READ));
        doc.add_bool(s.is_starred, envelope.flags.contains(MessageFlags::STARRED));
        doc.add_bool(s.is_draft, envelope.flags.contains(MessageFlags::DRAFT));
        doc.add_bool(s.is_sent, envelope.flags.contains(MessageFlags::SENT));
        doc.add_bool(s.is_trash, envelope.flags.contains(MessageFlags::TRASH));
        doc.add_bool(s.is_spam, envelope.flags.contains(MessageFlags::SPAM));
        doc.add_bool(
            s.is_answered,
            envelope.flags.contains(MessageFlags::ANSWERED),
        );
        doc.add_bool(s.is_reply_later, reply_later);
        // Phase E: index each keyword as a separate STRING term so
        // `is:$foo` filters resolve via exact-match TermQuery.
        // Index keywords lowercased so `is:$Forwarded` / `is:$forwarded`
        // match the same set. The authoritative case-preserved form
        // lives in `message_keywords` for round-trip back to IMAP.
        for keyword in &envelope.keywords {
            doc.add_text(s.keywords, keyword.to_lowercase());
        }

        let timestamp = envelope.date.timestamp();
        let dt = tantivy::DateTime::from_timestamp_secs(timestamp);
        doc.add_date(s.date, dt);
        doc.add_i64(s.sort_date_ts, sane_search_sort_timestamp(timestamp));

        self.writer
            .add_document(doc)
            .map_err(|e| MxrError::Search(e.to_string()))?;
        Ok(())
    }

    pub fn index_body(&mut self, envelope: &Envelope, body: &MessageBody) -> Result<(), MxrError> {
        self.index_body_with_reply_later(envelope, body, false)
    }

    pub fn index_body_with_reply_later(
        &mut self,
        envelope: &Envelope,
        body: &MessageBody,
        reply_later: bool,
    ) -> Result<(), MxrError> {
        let term = tantivy::Term::from_field_text(self.schema.message_id, &envelope.id.as_str());
        self.writer.delete_term(term);

        let s = &self.schema;
        let mut doc = TantivyDocument::new();
        doc.add_text(s.message_id, envelope.id.as_str());
        doc.add_text(s.account_id, envelope.account_id.as_str());
        doc.add_text(s.thread_id, envelope.thread_id.as_str());
        doc.add_text(
            s.message_id_header,
            normalized_message_id_header(envelope.message_id_header.as_deref()),
        );
        doc.add_text(s.subject, &envelope.subject);
        doc.add_text(s.from_name, envelope.from.name.as_deref().unwrap_or(""));
        doc.add_text(s.from_email, envelope.from.email.to_ascii_lowercase());
        for addr in &envelope.to {
            doc.add_text(s.to_email, addr.email.to_ascii_lowercase());
        }
        for addr in &envelope.cc {
            doc.add_text(s.cc_email, addr.email.to_ascii_lowercase());
        }
        for addr in &envelope.bcc {
            doc.add_text(s.bcc_email, addr.email.to_ascii_lowercase());
        }
        doc.add_text(s.snippet, &envelope.snippet);
        for label in &envelope.label_provider_ids {
            doc.add_text(s.labels, label.to_lowercase());
        }

        let body_text = indexed_body_text(body, &self.clean_fallbacks);
        if !body_text.is_empty() {
            doc.add_text(s.body_text, body_text);
        }
        for attachment in &body.attachments {
            doc.add_text(s.attachment_filenames, attachment.filename.to_lowercase());
            // MIME types are words prose can match ("spreadsheet" for an .xlsx).
            doc.add_text(s.attachment_filenames, attachment.mime_type.to_lowercase());
        }
        if let Some(list_id) = body.metadata.list_id.as_deref() {
            doc.add_text(s.list_id, list_id);
        }
        for delivered_to in delivered_to_values(body.metadata.raw_headers.as_deref()) {
            doc.add_text(s.delivered_to, delivered_to);
        }
        doc.add_text(s.content_hints, content_hints(body));

        doc.add_u64(s.size_bytes, envelope.size_bytes);
        doc.add_u64(s.flags, envelope.flags.bits() as u64);
        doc.add_bool(s.has_attachments, envelope.has_attachments);
        doc.add_bool(s.has_calendar, body.metadata.calendar.is_some());
        doc.add_bool(s.has_link, envelope.link_count > 0);
        doc.add_u64(s.link_density, envelope.link_density().as_db_u8() as u64);
        doc.add_bool(
            s.has_user_labels,
            has_user_label(&envelope.label_provider_ids),
        );
        doc.add_bool(s.is_read, envelope.flags.contains(MessageFlags::READ));
        doc.add_bool(s.is_starred, envelope.flags.contains(MessageFlags::STARRED));
        doc.add_bool(s.is_draft, envelope.flags.contains(MessageFlags::DRAFT));
        doc.add_bool(s.is_sent, envelope.flags.contains(MessageFlags::SENT));
        doc.add_bool(s.is_trash, envelope.flags.contains(MessageFlags::TRASH));
        doc.add_bool(s.is_spam, envelope.flags.contains(MessageFlags::SPAM));
        doc.add_bool(
            s.is_answered,
            envelope.flags.contains(MessageFlags::ANSWERED),
        );
        doc.add_bool(s.is_reply_later, reply_later);
        // Index keywords lowercased so `is:$Forwarded` / `is:$forwarded`
        // match the same set. The authoritative case-preserved form
        // lives in `message_keywords` for round-trip back to IMAP.
        for keyword in &envelope.keywords {
            doc.add_text(s.keywords, keyword.to_lowercase());
        }
        let timestamp = envelope.date.timestamp();
        let dt = tantivy::DateTime::from_timestamp_secs(timestamp);
        doc.add_date(s.date, dt);
        doc.add_i64(s.sort_date_ts, sane_search_sort_timestamp(timestamp));

        self.writer
            .add_document(doc)
            .map_err(|e| MxrError::Search(e.to_string()))?;
        Ok(())
    }

    pub fn remove_document(&mut self, message_id: &MessageId) {
        let term = tantivy::Term::from_field_text(self.schema.message_id, &message_id.as_str());
        self.writer.delete_term(term);
    }

    pub fn commit(&mut self) -> Result<(), MxrError> {
        self.writer
            .commit()
            .map_err(|e| MxrError::Search(e.to_string()))?;
        self.reader
            .reload()
            .map_err(|e| MxrError::Search(e.to_string()))?;
        Ok(())
    }

    pub fn search(
        &self,
        query_str: &str,
        limit: usize,
        offset: usize,
        sort: SortOrder,
    ) -> Result<SearchPage, MxrError> {
        self.search_in_account(query_str, None, limit, offset, sort)
    }

    pub fn search_in_account(
        &self,
        query_str: &str,
        account_id: Option<&str>,
        limit: usize,
        offset: usize,
        sort: SortOrder,
    ) -> Result<SearchPage, MxrError> {
        let s = &self.schema;

        let fields = s.default_text_fields();
        let mut query_parser = QueryParser::for_index(
            &self.index,
            fields.iter().map(|(field, _)| *field).collect(),
        );
        for (field, boost) in fields {
            query_parser.set_field_boost(field, boost);
        }

        let query = query_parser
            .parse_query(query_str)
            .map_err(|e| MxrError::Search(e.to_string()))?;
        let query = match account_id {
            Some(account_id) => {
                let account_term = tantivy::Term::from_field_text(s.account_id, account_id);
                Box::new(BooleanQuery::new(vec![
                    (Occur::Must, query),
                    (
                        Occur::Must,
                        Box::new(TermQuery::new(account_term, IndexRecordOption::Basic)),
                    ),
                ])) as Box<dyn Query>
            }
            None => query,
        };

        let searcher = self.reader.searcher();
        let total = searcher
            .search(&query, &Count)
            .map_err(|e| MxrError::Search(e.to_string()))?;
        let fetch_limit = limit.saturating_add(1);
        let top_docs = match sort {
            SortOrder::Relevance => searcher
                .search(
                    &query,
                    &TopDocs::with_limit(fetch_limit)
                        .and_offset(offset)
                        .order_by_score(),
                )
                .map_err(|e| MxrError::Search(e.to_string()))?
                .into_iter()
                .collect::<Vec<_>>(),
            SortOrder::DateDesc => searcher
                .search(
                    &query,
                    &TopDocs::with_limit(fetch_limit)
                        .and_offset(offset)
                        .order_by_fast_field::<i64>("sort_date_ts", Order::Desc),
                )
                .map_err(|e| MxrError::Search(e.to_string()))?
                .into_iter()
                .map(|(sort_score, doc_address)| {
                    (sort_score.map_or(0.0, |v| v as f32), doc_address)
                })
                .collect::<Vec<_>>(),
            SortOrder::DateAsc => searcher
                .search(
                    &query,
                    &TopDocs::with_limit(fetch_limit)
                        .and_offset(offset)
                        .order_by_fast_field::<i64>("sort_date_ts", Order::Asc),
                )
                .map_err(|e| MxrError::Search(e.to_string()))?
                .into_iter()
                .map(|(sort_score, doc_address)| {
                    (sort_score.map_or(0.0, |v| v as f32), doc_address)
                })
                .collect::<Vec<_>>(),
        };

        let has_more = offset.saturating_add(limit) < total;
        let next_offset = has_more.then_some(offset.saturating_add(limit));
        let mut results = Vec::with_capacity(top_docs.len().min(limit));
        for (score, doc_address) in top_docs.into_iter().take(limit) {
            let doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| MxrError::Search(e.to_string()))?;

            let message_id = doc
                .get_first(s.message_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let account_id = doc
                .get_first(s.account_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let thread_id = doc
                .get_first(s.thread_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            results.push(SearchResult {
                message_id,
                account_id,
                thread_id,
                score,
            });
        }

        Ok(SearchPage {
            results,
            total,
            has_more,
            next_offset,
        })
    }

    /// Number of indexed documents.
    pub fn num_docs(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    /// Touch the index structures used by the first interactive search.
    ///
    /// Tantivy memory-maps its index files, so opening the index does not read
    /// their pages into memory. A small all-documents query loads the count,
    /// date sort, and first document-store blocks before a user searches.
    pub fn warm(&self) -> Result<(), MxrError> {
        const WARM_DOCUMENTS: usize = 40;

        let searcher = self.reader.searcher();
        if searcher.num_docs() == 0 {
            return Ok(());
        }

        let query = AllQuery;
        searcher
            .search(&query, &Count)
            .map_err(|error| MxrError::Search(error.to_string()))?;
        let top_docs = searcher
            .search(
                &query,
                &TopDocs::with_limit(WARM_DOCUMENTS)
                    .order_by_fast_field::<i64>("sort_date_ts", Order::Desc),
            )
            .map_err(|error| MxrError::Search(error.to_string()))?;

        for (_, address) in top_docs {
            let _: TantivyDocument = searcher
                .doc(address)
                .map_err(|error| MxrError::Search(error.to_string()))?;
        }

        Ok(())
    }

    /// Clear all documents and prepare for reindexing.
    pub fn clear(&mut self) -> Result<(), MxrError> {
        self.writer
            .delete_all_documents()
            .map_err(|e| MxrError::Search(e.to_string()))?;
        self.commit()?;
        Ok(())
    }

    pub fn search_ast(
        &self,
        query: Box<dyn Query>,
        limit: usize,
        offset: usize,
        sort: SortOrder,
    ) -> Result<SearchPage, MxrError> {
        let s = &self.schema;
        let searcher = self.reader.searcher();
        let total = searcher
            .search(&*query, &Count)
            .map_err(|e| MxrError::Search(e.to_string()))?;
        let fetch_limit = limit.saturating_add(1);
        let top_docs = match sort {
            SortOrder::Relevance => searcher
                .search(
                    &*query,
                    &TopDocs::with_limit(fetch_limit)
                        .and_offset(offset)
                        .order_by_score(),
                )
                .map_err(|e| MxrError::Search(e.to_string()))?
                .into_iter()
                .collect::<Vec<_>>(),
            SortOrder::DateDesc => searcher
                .search(
                    &*query,
                    &TopDocs::with_limit(fetch_limit)
                        .and_offset(offset)
                        .order_by_fast_field::<i64>("sort_date_ts", Order::Desc),
                )
                .map_err(|e| MxrError::Search(e.to_string()))?
                .into_iter()
                .map(|(sort_score, doc_address)| {
                    (sort_score.map_or(0.0, |v| v as f32), doc_address)
                })
                .collect::<Vec<_>>(),
            SortOrder::DateAsc => searcher
                .search(
                    &*query,
                    &TopDocs::with_limit(fetch_limit)
                        .and_offset(offset)
                        .order_by_fast_field::<i64>("sort_date_ts", Order::Asc),
                )
                .map_err(|e| MxrError::Search(e.to_string()))?
                .into_iter()
                .map(|(sort_score, doc_address)| {
                    (sort_score.map_or(0.0, |v| v as f32), doc_address)
                })
                .collect::<Vec<_>>(),
        };

        let has_more = offset.saturating_add(limit) < total;
        let next_offset = has_more.then_some(offset.saturating_add(limit));
        let mut results = Vec::with_capacity(top_docs.len().min(limit));
        for (score, doc_address) in top_docs.into_iter().take(limit) {
            let doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| MxrError::Search(e.to_string()))?;

            let message_id = doc
                .get_first(s.message_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let account_id = doc
                .get_first(s.account_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let thread_id = doc
                .get_first(s.thread_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            results.push(SearchResult {
                message_id,
                account_id,
                thread_id,
                score,
            });
        }

        Ok(SearchPage {
            results,
            total,
            has_more,
            next_offset,
        })
    }
}

#[cfg(test)]
mod reader_text_tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use crate::parse_query;
    use crate::query_builder::QueryBuilder;
    use crate::test_fixtures::TestEnvelopeBuilder;
    use mxr_core::types::MessageMetadata;
    use std::time::Instant;

    fn body_for(
        envelope: &Envelope,
        text_plain: Option<&str>,
        text_html: Option<&str>,
    ) -> MessageBody {
        MessageBody {
            message_id: envelope.id.clone(),
            text_plain: text_plain.map(str::to_string),
            text_html: text_html.map(str::to_string),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    fn search_ids(index: &SearchIndex, query: &str) -> Vec<String> {
        let ast = parse_query(query).unwrap();
        let schema = MxrSchema::build();
        let built = QueryBuilder::new(&schema).build(&ast);
        index
            .search_ast(built, 50, 0, SortOrder::Relevance)
            .unwrap()
            .results
            .into_iter()
            .map(|result| result.message_id)
            .collect()
    }

    fn index_one(text_plain: Option<&str>, text_html: Option<&str>) -> (SearchIndex, String) {
        let mut index = SearchIndex::in_memory().unwrap();
        let envelope = TestEnvelopeBuilder::new().subject("Notes").build();
        let body = body_for(&envelope, text_plain, text_html);
        index.index_body(&envelope, &body).unwrap();
        index.commit().unwrap();
        (index, envelope.id.as_str())
    }

    const HTML_MESSAGE: &str = r#"<html><head><style>.cssproperty{color:red}</style></head><body>
<div class="mxrclassword">Visible visiblebananaword sentence.</div>
<img src="https://track.example.net/pixel?id=quuxtoken">
<blockquote>earlierquotedword from the last message</blockquote>
<div>On Mon, Jan 1, someone wrote:</div>
<div class="gmail_signature">sigwordzeta</div>
</body></html>"#;

    const SCRIPT_MESSAGE: &str = r#"<p>Visible tail</p><script>var zzscriptword = 1;</script><div class="zzclassword">x</div><zztagword>tail</zztagword>"#;

    /// html2text panics on this table (unchecked column arithmetic).
    const PANICKING_HTML: &str = r#"<table><tr><td colspan="18446744073709551615">tablewordalpha</td><td>tablewordbeta</td></tr></table>"#;

    #[test]
    fn html_only_message_matches_visible_words_not_markup() {
        let (index, id) = index_one(None, Some(HTML_MESSAGE));

        assert_eq!(search_ids(&index, "body:visiblebananaword"), vec![id]);
        for hidden in ["cssproperty", "mxrclassword", "quuxtoken", "track"] {
            assert!(
                search_ids(&index, &format!("body:{hidden}")).is_empty(),
                "{hidden} is markup and must not match"
            );
        }
    }

    #[test]
    fn script_contents_class_names_and_tag_names_do_not_match() {
        let (index, id) = index_one(None, Some(SCRIPT_MESSAGE));

        assert_eq!(search_ids(&index, "body:tail"), vec![id]);
        for hidden in ["zzscriptword", "zzclassword", "zztagword"] {
            assert!(
                search_ids(&index, &format!("body:{hidden}")).is_empty(),
                "{hidden} is code or markup and must not match"
            );
        }
    }

    #[test]
    fn quoted_reply_and_signature_in_html_are_searchable() {
        let (index, id) = index_one(None, Some(HTML_MESSAGE));

        assert_eq!(
            search_ids(&index, "body:earlierquotedword"),
            vec![id.clone()]
        );
        assert_eq!(search_ids(&index, "body:sigwordzeta"), vec![id]);
    }

    #[test]
    fn plain_text_quoted_block_is_searchable() {
        let text =
            "Fresh plainwordalpha reply.\n\n> quotedplainword from before\n> more quoted text";
        let (index, id) = index_one(Some(text), None);

        assert_eq!(search_ids(&index, "body:plainwordalpha"), vec![id.clone()]);
        assert_eq!(search_ids(&index, "body:quotedplainword"), vec![id]);
    }

    #[test]
    fn inline_reply_quote_is_findable() {
        let text = "Intro.\n> Can we use ultramarine?\nYes.";
        let (index, id) = index_one(Some(text), None);

        assert_eq!(search_ids(&index, "body:ultramarine"), vec![id]);
    }

    #[test]
    fn named_quote_only_message_is_findable() {
        let text = "On Tue, Alice wrote:\n> ultramarine";
        let (index, id) = index_one(Some(text), None);

        assert_eq!(search_ids(&index, "body:ultramarine"), vec![id]);
    }

    #[test]
    fn confidential_line_does_not_hide_later_words() {
        let text = "Intro.\n> This email is confidential.\n> ultramarine\nYes, cobalt.";
        let (index, id) = index_one(Some(text), None);

        assert_eq!(search_ids(&index, "body:cobalt"), vec![id.clone()]);
        assert_eq!(search_ids(&index, "body:ultramarine"), vec![id]);
    }

    #[test]
    fn html_part_words_are_indexed_alongside_a_shorter_plain_part() {
        let html = r#"<p>Intro ultramarine.</p><div class="gmail_signature">Sam</div>"#;
        let (index, id) = index_one(Some("Intro."), Some(html));

        assert_eq!(search_ids(&index, "body:ultramarine"), vec![id]);
    }

    #[test]
    fn has_youtube_still_matches_raw_html_link() {
        let html = r#"<p>Watch this</p><a href="https://youtube.com/watch?v=abc">clip</a>"#;
        let (index, id) = index_one(None, Some(html));

        assert_eq!(search_ids(&index, "has:youtube"), vec![id]);
    }

    #[test]
    fn malformed_table_with_huge_colspan_still_indexes() {
        // PR 767 clamps spans before html2text runs, so this table converts
        // without a panic and nothing is counted as a fallback.
        let (index, id) = index_one(None, Some(PANICKING_HTML));
        assert_eq!(search_ids(&index, "body:tablewordalpha"), vec![id.clone()]);
        assert_eq!(search_ids(&index, "body:tablewordbeta"), vec![id]);
        assert_eq!(index.clean_fallbacks.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn script_words_match_no_prose_path_but_has_filters_still_read_raw_html() {
        let html = r#"<p>Watch</p><script>var zzscriptword = 1;</script><a href="https://youtube.com/x">clip</a>"#;
        let (index, id) = index_one(None, Some(html));

        // Bare-word search, the string search, and the any-words path that
        // Archive Ask uses for prose.
        assert!(search_ids(&index, "zzscriptword").is_empty());
        assert!(index
            .search("zzscriptword", 10, 0, SortOrder::Relevance)
            .unwrap()
            .results
            .is_empty());
        let schema = MxrSchema::build();
        let any_words = QueryBuilder::new(&schema)
            .build_any_words("zzscriptword")
            .unwrap();
        assert!(index
            .search_ast(any_words, 10, 0, SortOrder::Relevance)
            .unwrap()
            .results
            .is_empty());

        assert_eq!(search_ids(&index, "has:youtube"), vec![id]);
    }

    #[test]
    fn indexed_body_text_keeps_quotes_and_signatures_and_drops_markup() {
        let envelope = TestEnvelopeBuilder::new().build();
        let counter = AtomicU64::new(0);
        let text = indexed_body_text(&body_for(&envelope, None, Some(HTML_MESSAGE)), &counter);

        assert!(text.contains("visiblebananaword"));
        assert!(text.contains("earlierquotedword"));
        assert!(text.contains("sigwordzeta"));
        assert!(!text.contains('<'));
        assert!(!text.contains("cssproperty"));
        assert!(!text.contains("quuxtoken"));
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    /// Debug-build timing for the upgrade cost: run with
    /// `cargo test -p mxr-search --lib reindex_timing -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing probe, run by hand"]
    fn reindex_timing_50k_messages() {
        const COUNT: usize = 50_000;
        let envelopes: Vec<(Envelope, MessageBody)> = (0..COUNT)
            .map(|i| {
                let envelope = TestEnvelopeBuilder::new()
                    .subject(format!("Subject {i}"))
                    .build();
                let html = format!(
                    r#"<html><head><style>.c{i}{{color:red}}</style></head><body><p>Message {i} body text about invoice {i}.</p><a href="https://track.example.net/{i}">link</a><blockquote>earlier {i}</blockquote><div>-- sig {i}</div></body></html>"#
                );
                let body = body_for(&envelope, None, Some(&html));
                (envelope, body)
            })
            .collect();

        let counter = AtomicU64::new(0);
        let clean_started = Instant::now();
        for (_, body) in &envelopes {
            std::hint::black_box(indexed_body_text(body, &counter));
        }
        let clean_elapsed = clean_started.elapsed();

        let mut index = SearchIndex::in_memory().unwrap();
        let index_started = Instant::now();
        for (envelope, body) in &envelopes {
            index.index_body(envelope, body).unwrap();
        }
        index.commit().unwrap();
        let index_elapsed = index_started.elapsed();

        eprintln!(
            "reindex timing, {COUNT} messages, debug build: indexed {index_elapsed:?} (conversion alone {clean_elapsed:?})"
        );
        assert_eq!(index.num_docs(), COUNT as u64);
    }
}

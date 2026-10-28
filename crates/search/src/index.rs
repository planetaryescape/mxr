use crate::schema::MxrSchema;
use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use mxr_core::types::{Envelope, MessageBody, SortOrder};
use mxr_core::MxrError;
use mxr_reader::{clean, ReaderConfig};
use std::path::Path;
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
    /// Cleaning applied to message bodies before they are indexed. It must
    /// match what the reader shows, or search would match text no one sees.
    reader_config: ReaderConfig,
}

/// Label the reader puts where a collapsed quoted reply was.
const QUOTE_LABEL: &str = "[previous message]";

/// The body text a person sees in the reader for this message: the cleaned
/// text, without markup, quoted earlier messages or signatures.
fn indexed_body_text(body: &MessageBody, config: &ReaderConfig) -> String {
    // Mirrors the reader view, which prefers the plain part when it has text.
    let plain = body
        .text_plain
        .as_deref()
        .filter(|text| !text.trim().is_empty());
    let (text, html) = match plain {
        Some(text) => (Some(text), None),
        None => (None, body.text_html.as_deref()),
    };
    let cleaned = clean(text, html, config).content;
    // The reader shows a "[previous message]" label in place of a quoted reply
    // that was collapsed. The label is not body text, so a message that is only
    // a quote falls through to the uncleaned text below.
    if !cleaned.replace(QUOTE_LABEL, "").trim().is_empty() {
        return cleaned;
    }
    // Index a quote-only message's words anyway, so it is still findable by its body.
    let uncleaned = ReaderConfig {
        collapse_quotes: false,
        strip_signatures: false,
        strip_boilerplate: false,
        strip_tracking: false,
        ..config.clone()
    };
    clean(text, html, &uncleaned).content
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

    /// Index bodies with the cleaning the reader uses for display. Indexes
    /// opened or created without this use [`ReaderConfig::default`].
    #[must_use]
    pub fn with_reader_config(mut self, reader_config: ReaderConfig) -> Self {
        self.reader_config = reader_config;
        self
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
                reader_config: ReaderConfig::default(),
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
            reader_config: ReaderConfig::default(),
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

        doc.add_text(s.body_text, indexed_body_text(body, &self.reader_config));
        for attachment in &body.attachments {
            doc.add_text(s.attachment_filenames, attachment.filename.to_lowercase());
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

    #[test]
    fn html_only_message_matches_visible_words_not_markup() {
        let (index, id) = index_one(None, Some(HTML_MESSAGE));

        assert_eq!(search_ids(&index, "visiblebananaword"), vec![id]);
        for hidden in ["cssproperty", "mxrclassword", "quuxtoken", "track"] {
            assert!(
                search_ids(&index, hidden).is_empty(),
                "{hidden} is markup and must not match"
            );
        }
    }

    #[test]
    fn html_quoted_reply_and_signature_do_not_match() {
        let (index, _) = index_one(None, Some(HTML_MESSAGE));

        assert!(search_ids(&index, "earlierquotedword").is_empty());
        assert!(search_ids(&index, "sigwordzeta").is_empty());
    }

    #[test]
    fn plain_text_quoted_block_does_not_match() {
        let text =
            "Fresh plainwordalpha reply.\n\n> quotedplainword from before\n> more quoted text";
        let (index, id) = index_one(Some(text), None);

        assert_eq!(search_ids(&index, "plainwordalpha"), vec![id]);
        assert!(search_ids(&index, "quotedplainword").is_empty());
    }

    #[test]
    fn message_that_is_only_a_quote_stays_findable_by_its_body() {
        let text = "> only quoted fallbackword here\n> second line";
        let (index, id) = index_one(Some(text), None);

        assert_eq!(search_ids(&index, "fallbackword"), vec![id]);
    }

    #[test]
    fn has_youtube_still_matches_raw_html_link() {
        let html = r#"<p>Watch this</p><a href="https://youtube.com/watch?v=abc">clip</a>"#;
        let (index, id) = index_one(None, Some(html));

        assert_eq!(search_ids(&index, "has:youtube"), vec![id]);
    }

    #[test]
    fn clean_index_text_drops_markup_and_keeps_visible_text() {
        let text = indexed_body_text(
            &body_for(
                &TestEnvelopeBuilder::new().build(),
                None,
                Some(HTML_MESSAGE),
            ),
            &ReaderConfig::default(),
        );

        assert!(text.contains("visiblebananaword"));
        assert!(!text.contains('<'));
        assert!(!text.contains("cssproperty"));
        assert!(!text.contains("quuxtoken"));
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

        let clean_started = Instant::now();
        for (_, body) in &envelopes {
            std::hint::black_box(indexed_body_text(body, &ReaderConfig::default()));
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
            "reindex timing, {COUNT} messages, debug build: cleaned index {index_elapsed:?} (clean alone {clean_elapsed:?})"
        );
        assert_eq!(index.num_docs(), COUNT as u64);
    }
}

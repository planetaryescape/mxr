use crate::schema::MxrSchema;
use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use mxr_core::types::{Envelope, MessageBody, SortOrder};
use mxr_core::MxrError;
use std::path::Path;
use tantivy::{
    collector::{Count, TopDocs},
    query::Query,
    query::QueryParser,
    schema::Value,
    Index, IndexReader, IndexWriter, Order, ReloadPolicy, TantivyDocument,
};

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: IndexWriter,
    schema: MxrSchema,
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

        let (index, rebuilt) = match Index::open_or_create(dir, schema_def.schema.clone()) {
            Ok(idx) => (idx, false),
            Err(e) if e.to_string().contains("schema does not match") => {
                tracing::warn!("Search index schema mismatch, rebuilding: {e}");
                // Wipe and recreate
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
            Err(e) => return Err(MxrError::Search(e.to_string())),
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
        })
    }

    pub fn index_envelope(&mut self, envelope: &Envelope) -> Result<(), MxrError> {
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

        let body_text = body.text_plain.as_deref().unwrap_or("");
        doc.add_text(s.body_text, body_text);
        if let Some(html) = body.text_html.as_deref() {
            doc.add_text(s.body_text, html);
        }
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
        let s = &self.schema;

        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![
                s.subject,
                s.from_name,
                s.snippet,
                s.body_text,
                s.attachment_filenames,
                s.content_hints,
            ],
        );
        query_parser.set_field_boost(s.subject, 3.0);
        query_parser.set_field_boost(s.from_name, 2.0);
        query_parser.set_field_boost(s.snippet, 1.0);
        query_parser.set_field_boost(s.body_text, 0.5);
        query_parser.set_field_boost(s.attachment_filenames, 0.75);

        let query = query_parser
            .parse_query(query_str)
            .map_err(|e| MxrError::Search(e.to_string()))?;

        let searcher = self.reader.searcher();
        let total = searcher
            .search(&query, &Count)
            .map_err(|e| MxrError::Search(e.to_string()))?;
        let fetch_limit = limit.saturating_add(1);
        let top_docs = match sort {
            SortOrder::Relevance => searcher
                .search(&query, &TopDocs::with_limit(fetch_limit).and_offset(offset))
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
                .map(|(sort_score, doc_address)| (sort_score as f32, doc_address))
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
                .map(|(sort_score, doc_address)| (sort_score as f32, doc_address))
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
                    &TopDocs::with_limit(fetch_limit).and_offset(offset),
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
                .map(|(sort_score, doc_address)| (sort_score as f32, doc_address))
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
                .map(|(sort_score, doc_address)| (sort_score as f32, doc_address))
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

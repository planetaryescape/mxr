use crate::schema::MxrSchema;
use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use mxr_core::types::{Envelope, MessageBody};
use mxr_core::MxrError;
use std::path::Path;
use tantivy::{
    collector::TopDocs, query::Query, query::QueryParser, schema::Value, Index, IndexReader,
    IndexWriter, ReloadPolicy, TantivyDocument,
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

impl SearchIndex {
    pub fn schema(&self) -> &MxrSchema {
        &self.schema
    }

    pub fn open(index_path: &Path) -> Result<Self, MxrError> {
        let schema_def = MxrSchema::build();
        let dir = tantivy::directory::MmapDirectory::open(index_path)
            .map_err(|e| MxrError::Search(e.to_string()))?;

        let index = match Index::open_or_create(dir, schema_def.schema.clone()) {
            Ok(idx) => idx,
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
                Index::open_or_create(dir, schema_def.schema.clone())
                    .map_err(|e| MxrError::Search(e.to_string()))?
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

        Ok(Self {
            index,
            reader,
            writer,
            schema: schema_def,
        })
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
        let s = &self.schema;
        let mut doc = TantivyDocument::new();
        doc.add_text(s.message_id, envelope.id.as_str());
        doc.add_text(s.account_id, envelope.account_id.as_str());
        doc.add_text(s.thread_id, envelope.thread_id.as_str());
        doc.add_text(s.subject, &envelope.subject);
        doc.add_text(s.from_name, envelope.from.name.as_deref().unwrap_or(""));
        doc.add_text(s.from_email, &envelope.from.email);
        for addr in &envelope.to {
            doc.add_text(s.to_email, &addr.email);
        }
        doc.add_text(s.snippet, &envelope.snippet);
        doc.add_u64(s.flags, envelope.flags.bits() as u64);
        doc.add_bool(s.has_attachments, envelope.has_attachments);
        doc.add_bool(s.is_read, envelope.flags.contains(MessageFlags::READ));
        doc.add_bool(s.is_starred, envelope.flags.contains(MessageFlags::STARRED));

        let dt = tantivy::DateTime::from_timestamp_secs(envelope.date.timestamp());
        doc.add_date(s.date, dt);

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
        doc.add_text(s.subject, &envelope.subject);
        doc.add_text(s.from_name, envelope.from.name.as_deref().unwrap_or(""));
        doc.add_text(s.from_email, &envelope.from.email);
        for addr in &envelope.to {
            doc.add_text(s.to_email, &addr.email);
        }
        doc.add_text(s.snippet, &envelope.snippet);

        let body_text = body.text_plain.as_deref().unwrap_or("");
        doc.add_text(s.body_text, body_text);

        doc.add_u64(s.flags, envelope.flags.bits() as u64);
        doc.add_bool(s.has_attachments, envelope.has_attachments);
        doc.add_bool(s.is_read, envelope.flags.contains(MessageFlags::READ));
        doc.add_bool(s.is_starred, envelope.flags.contains(MessageFlags::STARRED));
        let dt = tantivy::DateTime::from_timestamp_secs(envelope.date.timestamp());
        doc.add_date(s.date, dt);

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

    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchResult>, MxrError> {
        let s = &self.schema;

        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![s.subject, s.from_name, s.snippet, s.body_text],
        );
        query_parser.set_field_boost(s.subject, 3.0);
        query_parser.set_field_boost(s.from_name, 2.0);
        query_parser.set_field_boost(s.snippet, 1.0);
        query_parser.set_field_boost(s.body_text, 0.5);

        let query = query_parser
            .parse_query(query_str)
            .map_err(|e| MxrError::Search(e.to_string()))?;

        let searcher = self.reader.searcher();
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .map_err(|e| MxrError::Search(e.to_string()))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
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

        Ok(results)
    }

    pub fn search_ast(
        &self,
        query: Box<dyn Query>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, MxrError> {
        let s = &self.schema;
        let searcher = self.reader.searcher();
        let top_docs = searcher
            .search(&*query, &TopDocs::with_limit(limit))
            .map_err(|e| MxrError::Search(e.to_string()))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
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

        Ok(results)
    }
}

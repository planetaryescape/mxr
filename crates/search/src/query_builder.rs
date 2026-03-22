use crate::ast::*;
use crate::schema::MxrSchema;
use chrono::{Datelike, Local, NaiveDate};
use std::ops::Bound;
use tantivy::query::{
    AllQuery, BooleanQuery, BoostQuery, Occur, PhraseQuery, Query, RangeQuery, TermQuery,
};
use tantivy::schema::{Field, IndexRecordOption};
use tantivy::Term;

pub struct QueryBuilder {
    subject: Field,
    from_name: Field,
    from_email: Field,
    to_email: Field,
    cc_email: Field,
    bcc_email: Field,
    snippet: Field,
    body_text: Field,
    attachment_filenames: Field,
    labels: Field,
    is_read: Field,
    is_starred: Field,
    is_draft: Field,
    is_sent: Field,
    is_trash: Field,
    is_spam: Field,
    is_answered: Field,
    has_attachments: Field,
}

impl QueryBuilder {
    pub fn new(schema: &MxrSchema) -> Self {
        Self {
            subject: schema.subject,
            from_name: schema.from_name,
            from_email: schema.from_email,
            to_email: schema.to_email,
            cc_email: schema.cc_email,
            bcc_email: schema.bcc_email,
            snippet: schema.snippet,
            body_text: schema.body_text,
            attachment_filenames: schema.attachment_filenames,
            labels: schema.labels,
            is_read: schema.is_read,
            is_starred: schema.is_starred,
            is_draft: schema.is_draft,
            is_sent: schema.is_sent,
            is_trash: schema.is_trash,
            is_spam: schema.is_spam,
            is_answered: schema.is_answered,
            has_attachments: schema.has_attachments,
        }
    }

    pub fn build(&self, node: &QueryNode) -> Box<dyn Query> {
        match node {
            QueryNode::Text(text) => self.build_text_query(text),
            QueryNode::Phrase(phrase) => self.build_phrase_query(phrase),
            QueryNode::Field { field, value } => self.build_field_query(field, value),
            QueryNode::Filter(filter) => self.build_filter_query(filter),
            QueryNode::Label(label) => self.build_label_query(label),
            QueryNode::DateRange { bound, date } => self.build_date_query(bound, date),
            QueryNode::Size { op, bytes } => self.build_size_query(op, *bytes),
            QueryNode::And(left, right) => {
                let left_q = self.build(left);
                let right_q = self.build(right);
                Box::new(BooleanQuery::new(vec![
                    (Occur::Must, left_q),
                    (Occur::Must, right_q),
                ]))
            }
            QueryNode::Or(left, right) => {
                let left_q = self.build(left);
                let right_q = self.build(right);
                Box::new(BooleanQuery::new(vec![
                    (Occur::Should, left_q),
                    (Occur::Should, right_q),
                ]))
            }
            QueryNode::Not(inner) => {
                let inner_q = self.build(inner);
                Box::new(BooleanQuery::new(vec![
                    (Occur::MustNot, inner_q),
                    // BooleanQuery with only MustNot needs an all-docs clause
                    (Occur::Should, Box::new(AllQuery)),
                ]))
            }
        }
    }

    fn build_text_query(&self, text: &str) -> Box<dyn Query> {
        let fields_boosts: Vec<(Field, f32)> = vec![
            (self.subject, 3.0),
            (self.from_name, 2.0),
            (self.from_email, 2.0),
            (self.snippet, 1.0),
            (self.body_text, 0.5),
            (self.attachment_filenames, 0.75),
        ];

        let tokens = tokenize_text_value(text);
        if tokens.is_empty() {
            return self.build_text_token_query(&fields_boosts, &text.to_lowercase());
        }
        if tokens.len() == 1 {
            return self.build_text_token_query(&fields_boosts, &tokens[0]);
        }

        let token_groups = tokens
            .into_iter()
            .map(|token| (Occur::Must, self.build_text_token_query(&fields_boosts, &token)))
            .collect();
        Box::new(BooleanQuery::new(token_groups))
    }

    fn build_phrase_query(&self, phrase: &str) -> Box<dyn Query> {
        let terms: Vec<Term> = phrase
            .split_whitespace()
            .map(|w| Term::from_field_text(self.subject, &w.to_lowercase()))
            .collect();

        if terms.len() == 1 {
            let tq = TermQuery::new(
                terms.into_iter().next().unwrap(),
                IndexRecordOption::WithFreqs,
            );
            return Box::new(BoostQuery::new(Box::new(tq), 3.0));
        }

        let phrase_q = PhraseQuery::new(terms);
        Box::new(BoostQuery::new(Box::new(phrase_q), 3.0))
    }

    fn build_field_query(&self, field: &QueryField, value: &str) -> Box<dyn Query> {
        let tantivy_field = match field {
            QueryField::From => self.from_email,
            QueryField::To => self.to_email,
            QueryField::Cc => self.cc_email,
            QueryField::Bcc => self.bcc_email,
            QueryField::Subject => self.subject,
            QueryField::Body => self.body_text,
            QueryField::Filename => self.attachment_filenames,
        };

        match field {
            QueryField::Subject | QueryField::Body | QueryField::Filename => {
                self.build_text_field_query(tantivy_field, value)
            }
            QueryField::From | QueryField::To | QueryField::Cc | QueryField::Bcc => {
                let term = Term::from_field_text(tantivy_field, value);
                Box::new(TermQuery::new(term, IndexRecordOption::WithFreqs))
            }
        }
    }

    fn build_filter_query(&self, filter: &FilterKind) -> Box<dyn Query> {
        match filter {
            FilterKind::Read => {
                let term = Term::from_field_bool(self.is_read, true);
                Box::new(TermQuery::new(term, IndexRecordOption::Basic))
            }
            FilterKind::Unread => {
                let term = Term::from_field_bool(self.is_read, false);
                Box::new(TermQuery::new(term, IndexRecordOption::Basic))
            }
            FilterKind::Starred => {
                let term = Term::from_field_bool(self.is_starred, true);
                Box::new(TermQuery::new(term, IndexRecordOption::Basic))
            }
            FilterKind::Draft => {
                let term = Term::from_field_bool(self.is_draft, true);
                Box::new(TermQuery::new(term, IndexRecordOption::Basic))
            }
            FilterKind::Sent => {
                let term = Term::from_field_bool(self.is_sent, true);
                Box::new(TermQuery::new(term, IndexRecordOption::Basic))
            }
            FilterKind::Trash => {
                let term = Term::from_field_bool(self.is_trash, true);
                Box::new(TermQuery::new(term, IndexRecordOption::Basic))
            }
            FilterKind::Spam => {
                let term = Term::from_field_bool(self.is_spam, true);
                Box::new(TermQuery::new(term, IndexRecordOption::Basic))
            }
            FilterKind::Answered => {
                let term = Term::from_field_bool(self.is_answered, true);
                Box::new(TermQuery::new(term, IndexRecordOption::Basic))
            }
            FilterKind::Inbox => self.build_label_query("INBOX"),
            FilterKind::Archived => Box::new(BooleanQuery::new(vec![
                (Occur::Should, self.build_label_query("ARCHIVE")),
                (
                    Occur::Should,
                    Box::new(BooleanQuery::new(vec![
                        (Occur::MustNot, self.build_label_query("INBOX")),
                        (Occur::MustNot, self.build_filter_query(&FilterKind::Sent)),
                        (Occur::MustNot, self.build_filter_query(&FilterKind::Draft)),
                        (Occur::MustNot, self.build_filter_query(&FilterKind::Trash)),
                        (Occur::MustNot, self.build_filter_query(&FilterKind::Spam)),
                        (Occur::Should, Box::new(AllQuery)),
                    ])),
                ),
            ])),
            FilterKind::HasAttachment => {
                let term = Term::from_field_bool(self.has_attachments, true);
                Box::new(TermQuery::new(term, IndexRecordOption::Basic))
            }
        }
    }

    fn build_label_query(&self, label: &str) -> Box<dyn Query> {
        let term = Term::from_field_text(self.labels, &label.to_lowercase());
        Box::new(TermQuery::new(term, IndexRecordOption::Basic))
    }

    fn build_date_query(&self, bound: &DateBound, date_val: &DateValue) -> Box<dyn Query> {
        let resolved = resolve_date(date_val);
        let field_name = "date".to_string();
        let start = self.date_to_tantivy(resolved);

        match bound {
            DateBound::After => Box::new(RangeQuery::new_date_bounds(
                field_name,
                Bound::Included(start),
                Bound::Unbounded,
            )),
            DateBound::Before => Box::new(RangeQuery::new_date_bounds(
                field_name,
                Bound::Unbounded,
                Bound::Excluded(start),
            )),
            DateBound::Exact => {
                let end_date = resolved.succ_opt().unwrap_or(resolved);
                let end = self.date_to_tantivy(end_date);
                Box::new(RangeQuery::new_date_bounds(
                    field_name,
                    Bound::Included(start),
                    Bound::Excluded(end),
                ))
            }
        }
    }

    fn build_size_query(&self, op: &SizeOp, bytes: u64) -> Box<dyn Query> {
        let field_name = "size_bytes".to_string();
        match op {
            SizeOp::LessThan => Box::new(RangeQuery::new_u64_bounds(
                field_name,
                Bound::Unbounded,
                Bound::Excluded(bytes),
            )),
            SizeOp::LessThanOrEqual => Box::new(RangeQuery::new_u64_bounds(
                field_name,
                Bound::Unbounded,
                Bound::Included(bytes),
            )),
            SizeOp::Equal => Box::new(RangeQuery::new_u64_bounds(
                field_name,
                Bound::Included(bytes),
                Bound::Included(bytes),
            )),
            SizeOp::GreaterThan => Box::new(RangeQuery::new_u64_bounds(
                field_name,
                Bound::Excluded(bytes),
                Bound::Unbounded,
            )),
            SizeOp::GreaterThanOrEqual => Box::new(RangeQuery::new_u64_bounds(
                field_name,
                Bound::Included(bytes),
                Bound::Unbounded,
            )),
        }
    }

    fn build_text_field_query(&self, field: Field, value: &str) -> Box<dyn Query> {
        let terms: Vec<Term> = tokenize_text_value(value)
            .into_iter()
            .map(|word| Term::from_field_text(field, &word))
            .collect();

        if terms.len() <= 1 {
            let term = terms
                .into_iter()
                .next()
                .unwrap_or_else(|| Term::from_field_text(field, &value.to_lowercase()));
            return Box::new(TermQuery::new(term, IndexRecordOption::WithFreqs));
        }

        Box::new(PhraseQuery::new(terms))
    }

    fn build_text_token_query(&self, fields_boosts: &[(Field, f32)], token: &str) -> Box<dyn Query> {
        let subqueries = fields_boosts
            .iter()
            .map(|(field, boost)| {
                let term = Term::from_field_text(*field, token);
                let tq = TermQuery::new(term, IndexRecordOption::WithFreqs);
                let boosted: Box<dyn Query> = Box::new(BoostQuery::new(Box::new(tq), *boost));
                (Occur::Should, boosted)
            })
            .collect();
        Box::new(BooleanQuery::new(subqueries))
    }

    fn date_to_tantivy(&self, date: NaiveDate) -> tantivy::DateTime {
        let dt = date.and_hms_opt(0, 0, 0).unwrap();
        let ts = dt.and_utc().timestamp();
        tantivy::DateTime::from_timestamp_secs(ts)
    }
}

fn resolve_date(date_val: &DateValue) -> NaiveDate {
    let today = Local::now().date_naive();
    match date_val {
        DateValue::Specific(d) => *d,
        DateValue::Today => today,
        DateValue::Yesterday => today.pred_opt().unwrap_or(today),
        DateValue::ThisWeek => {
            let weekday = today.weekday().num_days_from_monday();
            today - chrono::Duration::days(weekday as i64)
        }
        DateValue::ThisMonth => {
            NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today)
        }
    }
}

fn tokenize_text_value(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::SearchIndex;
    use crate::parser::parse_query;
    use mxr_core::id::*;
    use mxr_core::types::*;

    fn make_test_envelope(
        subject: &str,
        from_email: &str,
        from_name: &str,
        flags: MessageFlags,
        has_attachments: bool,
    ) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: format!("fake-{}", subject.len()),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some(from_name.to_string()),
                email: from_email.to_string(),
            },
            to: vec![Address {
                name: None,
                email: "recipient@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: subject.to_string(),
            date: chrono::Utc::now(),
            flags,
            snippet: format!("Snippet for {}", subject),
            has_attachments,
            size_bytes: 1000,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec![],
        }
    }

    fn build_test_index() -> (SearchIndex, Vec<Envelope>) {
        let mut idx = SearchIndex::in_memory().unwrap();
        let envelopes = vec![
            make_test_envelope(
                "Deployment plan for v2",
                "alice@example.com",
                "Alice",
                MessageFlags::empty(), // unread
                false,
            ),
            make_test_envelope(
                "Invoice #2847",
                "bob@example.com",
                "Bob",
                MessageFlags::READ | MessageFlags::STARRED,
                true,
            ),
            make_test_envelope(
                "Team standup notes",
                "carol@example.com",
                "Carol",
                MessageFlags::READ,
                false,
            ),
            make_test_envelope(
                "crates.io: Successfully published mxr@0.4.6",
                "noreply@crates.io",
                "crates.io",
                MessageFlags::READ,
                false,
            ),
        ];
        for env in &envelopes {
            idx.index_envelope(env).unwrap();
        }
        idx.commit().unwrap();
        (idx, envelopes)
    }

    #[test]
    fn build_text_query_with_boosts() {
        let (idx, envelopes) = build_test_index();
        let schema = MxrSchema::build();
        let qb = QueryBuilder::new(&schema);

        let node = QueryNode::Text("deployment".to_string());
        let query = qb.build(&node);
        let results = idx.search_ast(query, 10, 0, SortOrder::Relevance).unwrap();
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].message_id, envelopes[0].id.as_str());
    }

    #[test]
    fn build_field_query() {
        let (idx, envelopes) = build_test_index();
        let schema = MxrSchema::build();
        let qb = QueryBuilder::new(&schema);

        let node = QueryNode::Field {
            field: QueryField::From,
            value: "alice@example.com".to_string(),
        };
        let query = qb.build(&node);
        let results = idx.search_ast(query, 10, 0, SortOrder::Relevance).unwrap();
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].message_id, envelopes[0].id.as_str());
    }

    #[test]
    fn build_filter_query() {
        let (idx, _envelopes) = build_test_index();
        let schema = MxrSchema::build();
        let qb = QueryBuilder::new(&schema);

        // Search for unread messages
        let node = QueryNode::Filter(FilterKind::Unread);
        let query = qb.build(&node);
        let results = idx.search_ast(query, 10, 0, SortOrder::Relevance).unwrap();
        // Only the first envelope is unread (flags empty)
        assert_eq!(results.results.len(), 1);
    }

    #[test]
    fn build_date_range_query() {
        let (idx, _envelopes) = build_test_index();
        let schema = MxrSchema::build();
        let qb = QueryBuilder::new(&schema);

        // All test envelopes are dated today, so after yesterday should return all
        let yesterday = Local::now().date_naive().pred_opt().unwrap();
        let node = QueryNode::DateRange {
            bound: DateBound::After,
            date: DateValue::Specific(yesterday),
        };
        let query = qb.build(&node);
        let results = idx.search_ast(query, 10, 0, SortOrder::Relevance).unwrap();
        assert_eq!(results.results.len(), 4);
    }

    #[test]
    fn build_compound_query() {
        let (idx, envelopes) = build_test_index();
        let schema = MxrSchema::build();
        let qb = QueryBuilder::new(&schema);

        // from:bob AND is:starred
        let node = QueryNode::And(
            Box::new(QueryNode::Field {
                field: QueryField::From,
                value: "bob@example.com".to_string(),
            }),
            Box::new(QueryNode::Filter(FilterKind::Starred)),
        );
        let query = qb.build(&node);
        let results = idx.search_ast(query, 10, 0, SortOrder::Relevance).unwrap();
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].message_id, envelopes[1].id.as_str());
    }

    #[test]
    fn search_with_parsed_query() {
        let (idx, envelopes) = build_test_index();
        let schema = MxrSchema::build();
        let qb = QueryBuilder::new(&schema);

        let ast = parse_query("from:alice@example.com").unwrap();
        let query = qb.build(&ast);
        let results = idx.search_ast(query, 10, 0, SortOrder::Relevance).unwrap();
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].message_id, envelopes[0].id.as_str());
    }

    #[test]
    fn build_text_query_tokenizes_punctuation_heavy_terms() {
        let (idx, envelopes) = build_test_index();
        let schema = MxrSchema::build();
        let qb = QueryBuilder::new(&schema);

        let crates_ast = parse_query("crates.io").unwrap();
        let crates_query = qb.build(&crates_ast);
        let crates_results = idx
            .search_ast(crates_query, 10, 0, SortOrder::Relevance)
            .unwrap();
        assert_eq!(crates_results.results.len(), 1);
        assert_eq!(
            crates_results.results[0].message_id,
            envelopes[3].id.as_str()
        );

        let version_ast = parse_query("mxr@0.4.6").unwrap();
        let version_query = qb.build(&version_ast);
        let version_results = idx
            .search_ast(version_query, 10, 0, SortOrder::Relevance)
            .unwrap();
        assert_eq!(version_results.results.len(), 1);
        assert_eq!(
            version_results.results[0].message_id,
            envelopes[3].id.as_str()
        );
    }
}

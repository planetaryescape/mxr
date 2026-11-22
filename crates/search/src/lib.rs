mod content_version;
mod index;
pub mod query_builder;
mod schema;
mod service;
#[cfg(test)]
mod test_fixtures;

pub mod ast {
    pub use mail_query::{
        DateBound, DateValue, FilterKind, ParseError, ParserOptions, QueryField, QueryNode,
        RelativeUnit, SizeOp, Visitor,
    };
}

pub use mail_query::{
    DateBound, DateValue, FilterKind, ParseError, ParserOptions, QueryField, QueryNode,
    RelativeUnit, SizeOp, Visitor,
};

pub use content_version::{
    lexical_content_is_current, record_lexical_content_version, LEXICAL_CONTENT_VERSION,
};
pub use index::{ResolvedHit, SearchIndex, SearchPage, SearchResolver, SearchResult};
pub use query_builder::{build_query, CompiledQuery, QueryBuilder, MAX_PROSE_WORDS};
pub use schema::MxrSchema;
pub use service::{
    IndexAvailability, RebuildGuard, ResolveTarget, SearchIndexEntry, SearchServiceHandle,
    SearchUpdateBatch,
};

pub const FILTER_OWED_REPLY: &str = "owed-reply";

pub const FILTER_REPLY_LATER: &str = "reply-later";

/// Parse an mxr query string, with mxr-specific custom filters
/// (`is:owed-reply`, `is:reply-later`) pre-registered so they parse
/// through `FilterKind::Custom(...)` instead of returning
/// `ParseError::UnknownFilter`.
///
/// Phase E: also scans `input` for `is:$<keyword>` tokens and
/// dynamically registers each so the parser accepts any IMAP-style
/// keyword without needing an upstream `mail-query` schema change.
/// The downstream query builder dispatches `FilterKind::Custom(name)`
/// where `name.starts_with('$')` to the Tantivy keywords field.
pub fn parse_query(input: &str) -> Result<QueryNode, ParseError> {
    let mut options = ParserOptions::new();
    options.register_custom_filters([FILTER_OWED_REPLY, FILTER_REPLY_LATER]);
    let mut keyword_atoms: Vec<String> = Vec::new();
    let mut rest = input;
    while let Some(idx) = rest.find("is:$") {
        let after = &rest[idx + 4..];
        let end = after
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | '(' | '"'))
            .unwrap_or(after.len());
        let atom = &after[..end];
        if !atom.is_empty() {
            keyword_atoms.push(format!("${}", atom.to_lowercase()));
        }
        rest = &after[end..];
    }
    for atom in &keyword_atoms {
        options.register_custom_filters([atom.as_str()]);
    }
    mail_query::parse_with(input, &options)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use mxr_core::id::*;
    use mxr_core::types::*;

    fn make_envelope(subject: &str, snippet: &str, from_name: &str) -> Envelope {
        make_envelope_full(
            subject,
            snippet,
            from_name,
            "test@example.com",
            MessageFlags::READ,
            false,
        )
    }

    fn make_envelope_full(
        subject: &str,
        snippet: &str,
        from_name: &str,
        from_email: &str,
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
            cc: vec![Address {
                name: None,
                email: "team@example.com".to_string(),
            }],
            bcc: vec![Address {
                name: None,
                email: "hidden@example.com".to_string(),
            }],
            subject: subject.to_string(),
            date: chrono::Utc::now(),
            flags,
            snippet: snippet.to_string(),
            has_attachments,
            size_bytes: 1000,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec!["notifications".to_string()],
            keywords: std::collections::BTreeSet::new(),
        }
    }

    #[test]
    fn field_boost_ranking() {
        let mut idx = SearchIndex::in_memory().unwrap();

        let env_subject = make_envelope("Critical deployment issue", "Nothing special here", "Bob");
        let env_snippet = make_envelope("Regular update", "The deployment went well", "Carol");

        let subject_id = env_subject.id.as_str();

        idx.index_envelope(&env_subject).unwrap();
        idx.index_envelope(&env_snippet).unwrap();
        idx.commit().unwrap();

        let results = idx
            .search("deployment", 10, 0, SortOrder::Relevance)
            .unwrap();
        assert_eq!(results.results.len(), 2);
        assert_eq!(results.results[0].message_id, subject_id);
    }

    #[test]
    fn body_indexing() {
        let mut idx = SearchIndex::in_memory().unwrap();

        let env = make_envelope("Meeting notes", "Quick summary", "Alice");
        let env_id = env.id.as_str();

        idx.index_envelope(&env).unwrap();
        idx.commit().unwrap();

        let results = idx.search("canary", 10, 0, SortOrder::Relevance).unwrap();
        assert_eq!(results.results.len(), 0);

        let body = MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Deploy canary to 5% of traffic first".to_string()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };
        idx.index_body(&env, &body).unwrap();
        idx.commit().unwrap();

        let results = idx.search("canary", 10, 0, SortOrder::Relevance).unwrap();
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].message_id, env_id);
    }

    #[test]
    fn remove_document() {
        let mut idx = SearchIndex::in_memory().unwrap();

        let env = make_envelope("Remove me", "This should be gone", "Alice");
        idx.index_envelope(&env).unwrap();
        idx.commit().unwrap();

        let results = idx.search("remove", 10, 0, SortOrder::Relevance).unwrap();
        assert_eq!(results.results.len(), 1);

        idx.remove_document(&env.id);
        idx.commit().unwrap();

        let results = idx.search("remove", 10, 0, SortOrder::Relevance).unwrap();
        assert_eq!(results.results.len(), 0);
    }

    #[test]
    fn date_desc_search_returns_newest_first_and_sinks_future_dates() {
        let mut idx = SearchIndex::in_memory().unwrap();

        let mut newest = make_envelope("crates.io newest", "release", "Alice");
        newest.date = chrono::Utc::now();
        let mut older = make_envelope("crates.io older", "release", "Bob");
        older.date = chrono::Utc::now() - chrono::Duration::days(2);
        let mut poisoned_future = make_envelope("crates.io future", "release", "Mallory");
        poisoned_future.date = chrono::Utc::now() + chrono::Duration::days(400);

        idx.index_envelope(&older).unwrap();
        idx.index_envelope(&poisoned_future).unwrap();
        idx.index_envelope(&newest).unwrap();
        idx.commit().unwrap();

        let results = idx.search("crates.io", 10, 0, SortOrder::DateDesc).unwrap();
        let ids = results
            .results
            .iter()
            .map(|result| result.message_id.as_str().to_string())
            .collect::<Vec<_>>();

        assert_eq!(ids[0], newest.id.as_str());
        assert_eq!(ids[1], older.id.as_str());
        assert_eq!(ids[2], poisoned_future.id.as_str());
    }

    #[test]
    fn search_paginates_with_offset_and_has_more() {
        let mut idx = SearchIndex::in_memory().unwrap();
        for i in 0..5 {
            let mut env = make_envelope(&format!("deployment {i}"), "rollout", "Alice");
            env.date = chrono::Utc::now() - chrono::Duration::minutes(i);
            idx.index_envelope(&env).unwrap();
        }
        idx.commit().unwrap();

        let first_page = idx.search("deployment", 2, 0, SortOrder::DateDesc).unwrap();
        let second_page = idx.search("deployment", 2, 2, SortOrder::DateDesc).unwrap();

        assert_eq!(first_page.results.len(), 2);
        assert!(first_page.has_more);
        assert_eq!(first_page.next_offset, Some(2));
        assert_eq!(second_page.results.len(), 2);
        assert!(second_page.has_more);
        assert_eq!(second_page.next_offset, Some(4));
        assert_ne!(
            first_page.results[0].message_id,
            second_page.results[0].message_id
        );
    }

    #[test]
    fn large_limit_returns_all_matches_without_tantivy_ceiling() {
        let mut idx = SearchIndex::in_memory().unwrap();
        for i in 0..1_080 {
            let mut env = make_envelope(
                &format!("triage survey {i}"),
                "full inbox discovery",
                "Alice",
            );
            env.date = chrono::Utc::now() - chrono::Duration::seconds(i);
            idx.index_envelope(&env).unwrap();
        }
        idx.commit().unwrap();

        let results = idx.search("triage", 1_080, 0, SortOrder::DateDesc).unwrap();

        assert_eq!(results.total, 1_080);
        assert_eq!(results.results.len(), 1_080);
        assert!(!results.has_more);
        assert_eq!(results.next_offset, None);
    }

    fn build_e2e_index() -> (SearchIndex, Vec<Envelope>) {
        let mut idx = SearchIndex::in_memory().unwrap();
        let envelopes = vec![
            make_envelope_full(
                "Deployment plan for v2",
                "Rolling out to prod",
                "Alice",
                "alice@example.com",
                MessageFlags::empty(),
                false,
            ),
            make_envelope_full(
                "Invoice #2847",
                "Payment due next week",
                "Bob",
                "bob@example.com",
                MessageFlags::READ | MessageFlags::STARRED,
                true,
            ),
            make_envelope_full(
                "Team standup notes",
                "Sprint review action items",
                "Carol",
                "carol@example.com",
                MessageFlags::READ,
                false,
            ),
            make_envelope_full(
                "CI pipeline failures",
                "Build broken on main",
                "Alice",
                "alice@example.com",
                MessageFlags::empty(),
                true,
            ),
        ];
        for env in &envelopes {
            idx.index_envelope(env).unwrap();
        }
        idx.commit().unwrap();
        (idx, envelopes)
    }

    fn e2e_search(idx: &SearchIndex, query_str: &str) -> Vec<String> {
        let ast = parse_query(query_str).unwrap();
        let schema = MxrSchema::build();
        let qb = QueryBuilder::new(&schema);
        let query = qb.build(&ast);
        idx.search_ast(query, 10, 0, SortOrder::Relevance)
            .unwrap()
            .results
            .into_iter()
            .map(|r| r.message_id)
            .collect()
    }

    /// Phase E: `is:$forwarded` resolves through FilterKind::Custom and
    /// matches only envelopes carrying that keyword in their indexed set.
    #[test]
    fn e2e_keyword_filter_is_dollar_foo_matches_keyword() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let mut env_with = make_envelope_full(
            "Has the keyword",
            "...",
            "Alice",
            "alice@example.com",
            MessageFlags::empty(),
            false,
        );
        env_with.keywords.insert("$Forwarded".to_string());
        let env_without = make_envelope_full(
            "No keyword here",
            "...",
            "Bob",
            "bob@example.com",
            MessageFlags::empty(),
            false,
        );
        idx.index_envelope(&env_with).unwrap();
        idx.index_envelope(&env_without).unwrap();
        idx.commit().unwrap();

        let results = e2e_search(&idx, "is:$Forwarded");
        assert_eq!(results.len(), 1, "exactly one envelope carries $Forwarded");
        assert_eq!(results[0], env_with.id.as_str());
    }

    #[test]
    fn e2e_parse_build_search_compound() {
        let (idx, envelopes) = build_e2e_index();
        let results = e2e_search(&idx, "from:alice@example.com is:unread");
        assert_eq!(results.len(), 2);
        let alice_ids: Vec<String> = vec![
            envelopes[0].id.as_str().clone(),
            envelopes[3].id.as_str().clone(),
        ];
        for id in &results {
            assert!(alice_ids.contains(id));
        }
    }

    #[test]
    fn e2e_filter_has_attachment() {
        let (idx, envelopes) = build_e2e_index();
        let results = e2e_search(&idx, "has:attachment");
        assert_eq!(results.len(), 2);
        let attachment_ids: Vec<String> = vec![
            envelopes[1].id.as_str().clone(),
            envelopes[3].id.as_str().clone(),
        ];
        for id in &results {
            assert!(attachment_ids.contains(id));
        }
    }

    #[test]
    fn e2e_search_gmail_or_braces_and_field_groups() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let amy = make_envelope_full(
            "Dinner movie plan",
            "Friday",
            "Amy",
            "amy@example.com",
            MessageFlags::READ,
            false,
        );
        let david = make_envelope_full(
            "Lunch plan",
            "Saturday",
            "David",
            "david@example.com",
            MessageFlags::READ,
            false,
        );
        let other = make_envelope_full(
            "Dinner only",
            "Sunday",
            "Carol",
            "carol@example.com",
            MessageFlags::READ,
            false,
        );
        let amy_id = amy.id.as_str();
        let david_id = david.id.as_str();

        idx.index_envelope(&amy).unwrap();
        idx.index_envelope(&david).unwrap();
        idx.index_envelope(&other).unwrap();
        idx.commit().unwrap();

        let brace_results = e2e_search(&idx, "{from:amy@example.com from:david@example.com}");
        assert_eq!(brace_results.len(), 2);
        assert!(brace_results.contains(&amy_id));
        assert!(brace_results.contains(&david_id));
        assert_eq!(e2e_search(&idx, "subject:(dinner movie)"), vec![amy_id]);
    }
}

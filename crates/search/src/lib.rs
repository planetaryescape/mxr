pub mod ast;
mod index;
pub mod parser;
pub mod query_builder;
mod saved;
mod schema;

pub use ast::*;
pub use index::{SearchIndex, SearchPage, SearchResult};
pub use parser::{parse_query, ParseError};
pub use query_builder::QueryBuilder;
pub use saved::SavedSearchService;
pub use schema::MxrSchema;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mxr_core::id::*;
    use crate::mxr_core::types::*;

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
            label_provider_ids: vec!["notifications".to_string()],
        }
    }

    #[test]
    fn search_by_subject_keyword() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let subjects = [
            "Deployment plan for v2.3",
            "Q1 Report review",
            "This Week in Rust #580",
            "Invoice #2847",
            "Team standup notes",
            "Summer trip planning",
            "PR review: fix auth",
            "HN Weekly Digest",
            "RustConf 2026 invite",
            "CI pipeline failures",
        ];
        let mut target_id = String::new();
        for (i, subj) in subjects.iter().enumerate() {
            let env = make_envelope(subj, &format!("Snippet for msg {}", i), "Alice");
            if i == 0 {
                target_id = env.id.as_str();
            }
            idx.index_envelope(&env).unwrap();
        }
        idx.commit().unwrap();

        let results = idx
            .search("deployment", 10, 0, SortOrder::Relevance)
            .unwrap();
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].message_id, target_id);
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
        // Subject match should rank higher due to 3.0 boost vs 1.0 snippet
        assert_eq!(results.results[0].message_id, subject_id);
    }

    #[test]
    fn body_indexing() {
        let mut idx = SearchIndex::in_memory().unwrap();

        let env = make_envelope("Meeting notes", "Quick summary", "Alice");
        let env_id = env.id.as_str();

        idx.index_envelope(&env).unwrap();
        idx.commit().unwrap();

        // Search for body-only keyword should find nothing yet
        let results = idx.search("canary", 10, 0, SortOrder::Relevance).unwrap();
        assert_eq!(results.results.len(), 0);

        // Now index with body
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
    fn empty_search() {
        let idx = SearchIndex::in_memory().unwrap();
        let results = idx
            .search("nonexistent", 10, 0, SortOrder::Relevance)
            .unwrap();
        assert!(results.results.is_empty());
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
        assert_eq!(second_page.results.len(), 2);
        assert!(second_page.has_more);
        assert_ne!(
            first_page.results[0].message_id,
            second_page.results[0].message_id
        );
    }

    // -- E2E: parse → build → search integration tests --

    fn build_e2e_index() -> (SearchIndex, Vec<Envelope>) {
        let mut idx = SearchIndex::in_memory().unwrap();
        let envelopes = vec![
            make_envelope_full(
                "Deployment plan for v2",
                "Rolling out to prod",
                "Alice",
                "alice@example.com",
                MessageFlags::empty(), // unread
                false,
            ),
            make_envelope_full(
                "Invoice #2847",
                "Payment due next week",
                "Bob",
                "bob@example.com",
                MessageFlags::READ | MessageFlags::STARRED,
                true, // has attachment
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
                MessageFlags::empty(), // unread
                true,                  // has attachment
            ),
        ];
        for env in &envelopes {
            idx.index_envelope(env).unwrap();
        }
        idx.commit().unwrap();
        (idx, envelopes)
    }

    fn e2e_search(idx: &SearchIndex, query_str: &str) -> Vec<String> {
        let ast = parser::parse_query(query_str).unwrap();
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

    #[test]
    fn e2e_parse_build_search_text() {
        let (idx, envelopes) = build_e2e_index();
        let results = e2e_search(&idx, "deployment");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], envelopes[0].id.as_str());
    }

    #[test]
    fn e2e_parse_build_search_field() {
        let (idx, envelopes) = build_e2e_index();
        let results = e2e_search(&idx, "from:alice@example.com");
        assert_eq!(results.len(), 2);
        let alice_ids: Vec<String> = vec![
            envelopes[0].id.as_str().to_string(),
            envelopes[3].id.as_str().to_string(),
        ];
        for id in &results {
            assert!(alice_ids.contains(id));
        }
    }

    #[test]
    fn e2e_parse_build_search_compound() {
        let (idx, envelopes) = build_e2e_index();
        // from:alice AND is:unread — both alice messages are unread
        let results = e2e_search(&idx, "from:alice@example.com is:unread");
        assert_eq!(results.len(), 2);
        let alice_ids: Vec<String> = vec![
            envelopes[0].id.as_str().to_string(),
            envelopes[3].id.as_str().to_string(),
        ];
        for id in &results {
            assert!(alice_ids.contains(id));
        }
    }

    #[test]
    fn e2e_parse_build_search_negation() {
        let (idx, _envelopes) = build_e2e_index();
        // -is:read = unread messages (alice's two)
        let results = e2e_search(&idx, "-is:read");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn e2e_filter_has_attachment() {
        let (idx, envelopes) = build_e2e_index();
        let results = e2e_search(&idx, "has:attachment");
        assert_eq!(results.len(), 2);
        let attachment_ids: Vec<String> = vec![
            envelopes[1].id.as_str().to_string(),
            envelopes[3].id.as_str().to_string(),
        ];
        for id in &results {
            assert!(attachment_ids.contains(id));
        }
    }

    #[test]
    fn e2e_search_by_label() {
        let (idx, envelopes) = build_e2e_index();
        let results = e2e_search(&idx, "label:notifications");
        assert_eq!(results.len(), envelopes.len());
    }

    #[test]
    fn e2e_search_by_label_is_case_insensitive() {
        let (idx, envelopes) = build_e2e_index();
        let results = e2e_search(&idx, "label:NOTIFICATIONS");
        assert_eq!(results.len(), envelopes.len());
    }

    #[test]
    fn e2e_filter_starred() {
        let (idx, envelopes) = build_e2e_index();
        let results = e2e_search(&idx, "is:starred");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], envelopes[1].id.as_str());
    }

    #[test]
    fn e2e_search_cc_and_bcc_fields() {
        let (idx, envelopes) = build_e2e_index();

        let cc_results = e2e_search(&idx, "cc:team@example.com");
        assert_eq!(cc_results.len(), envelopes.len());

        let bcc_results = e2e_search(&idx, "bcc:hidden@example.com");
        assert_eq!(bcc_results.len(), envelopes.len());
    }

    #[test]
    fn e2e_search_sent_filter() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let sent = make_envelope_full(
            "Sent follow-up",
            "Done",
            "Alice",
            "alice@example.com",
            MessageFlags::READ | MessageFlags::SENT,
            false,
        );
        let inbox = make_envelope_full(
            "Inbox message",
            "Pending",
            "Bob",
            "bob@example.com",
            MessageFlags::READ,
            false,
        );
        idx.index_envelope(&sent).unwrap();
        idx.index_envelope(&inbox).unwrap();
        idx.commit().unwrap();

        let results = e2e_search(&idx, "is:sent");
        assert_eq!(results, vec![sent.id.as_str().to_string()]);
    }

    #[test]
    fn e2e_search_size_and_body_and_filename() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let env = make_envelope_full(
            "Release checklist",
            "Contains attachment",
            "Alice",
            "alice@example.com",
            MessageFlags::READ,
            true,
        );
        let body = MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Deploy canary to 10% before global rollout".to_string()),
            text_html: None,
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
                filename: "release-notes-v2.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                disposition: AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 10,
                local_path: None,
                provider_id: "att-1".to_string(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };

        idx.index_body(&env, &body).unwrap();
        idx.commit().unwrap();

        assert_eq!(
            e2e_search(&idx, "body:canary"),
            vec![env.id.as_str().to_string()]
        );
        assert_eq!(
            e2e_search(&idx, "filename:release-notes"),
            vec![env.id.as_str().to_string()]
        );
        assert_eq!(
            e2e_search(&idx, "size:>=1000"),
            vec![env.id.as_str().to_string()]
        );
    }
}

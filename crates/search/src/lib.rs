pub mod ast;
mod index;
pub mod parser;
pub mod query_builder;
mod saved;
mod schema;

pub use ast::*;
pub use index::{SearchIndex, SearchResult};
pub use parser::{parse_query, ParseError};
pub use query_builder::QueryBuilder;
pub use saved::SavedSearchService;
pub use schema::MxrSchema;

#[cfg(test)]
mod tests {
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
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: subject.to_string(),
            date: chrono::Utc::now(),
            flags,
            snippet: snippet.to_string(),
            has_attachments,
            size_bytes: 1000,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec![],
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

        let results = idx.search("deployment", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].message_id, target_id);
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

        let results = idx.search("deployment", 10).unwrap();
        assert_eq!(results.len(), 2);
        // Subject match should rank higher due to 3.0 boost vs 1.0 snippet
        assert_eq!(results[0].message_id, subject_id);
    }

    #[test]
    fn body_indexing() {
        let mut idx = SearchIndex::in_memory().unwrap();

        let env = make_envelope("Meeting notes", "Quick summary", "Alice");
        let env_id = env.id.as_str();

        idx.index_envelope(&env).unwrap();
        idx.commit().unwrap();

        // Search for body-only keyword should find nothing yet
        let results = idx.search("canary", 10).unwrap();
        assert_eq!(results.len(), 0);

        // Now index with body
        let body = MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Deploy canary to 5% of traffic first".to_string()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
        };
        idx.index_body(&env, &body).unwrap();
        idx.commit().unwrap();

        let results = idx.search("canary", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].message_id, env_id);
    }

    #[test]
    fn remove_document() {
        let mut idx = SearchIndex::in_memory().unwrap();

        let env = make_envelope("Remove me", "This should be gone", "Alice");
        idx.index_envelope(&env).unwrap();
        idx.commit().unwrap();

        let results = idx.search("remove", 10).unwrap();
        assert_eq!(results.len(), 1);

        idx.remove_document(&env.id);
        idx.commit().unwrap();

        let results = idx.search("remove", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn empty_search() {
        let idx = SearchIndex::in_memory().unwrap();
        let results = idx.search("nonexistent", 10).unwrap();
        assert!(results.is_empty());
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
        idx.search_ast(query, 10)
            .unwrap()
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
    fn e2e_filter_starred() {
        let (idx, envelopes) = build_e2e_index();
        let results = e2e_search(&idx, "is:starred");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], envelopes[1].id.as_str());
    }
}

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
                email: "test@example.com".to_string(),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: subject.to_string(),
            date: chrono::Utc::now(),
            flags: MessageFlags::READ,
            snippet: snippet.to_string(),
            has_attachments: false,
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
}

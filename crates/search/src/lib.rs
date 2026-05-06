pub mod ast;
mod index;
pub mod parser;
pub mod query_builder;
mod schema;
mod service;
#[cfg(test)]
mod test_fixtures;

pub use ast::*;
pub use index::{SearchIndex, SearchPage, SearchResult};
pub use parser::{parse_query, ParseError};
pub use query_builder::QueryBuilder;
pub use schema::MxrSchema;
pub use service::{SearchIndexEntry, SearchServiceHandle, SearchUpdateBatch};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

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
    fn e2e_search_in_inbox_alias_filters_by_inbox_label() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let mut inbox = make_envelope_full(
            "Adrian inbox",
            "Follow up",
            "Adrian",
            "adrian@example.com",
            MessageFlags::READ,
            false,
        );
        inbox.label_provider_ids = vec!["INBOX".into()];
        let mut archived = make_envelope_full(
            "Adrian archived",
            "Follow up",
            "Adrian",
            "adrian@example.com",
            MessageFlags::READ,
            false,
        );
        archived.label_provider_ids = vec![];
        let inbox_id = inbox.id.as_str();

        idx.index_envelope(&inbox).unwrap();
        idx.index_envelope(&archived).unwrap();
        idx.commit().unwrap();

        assert_eq!(e2e_search(&idx, "adrian in:inbox"), vec![inbox_id]);
    }

    #[test]
    fn e2e_search_gmail_category_and_label_status_aliases() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let mut promo = make_envelope_full(
            "Sale receipt",
            "Discount applied",
            "Shop",
            "shop@example.com",
            MessageFlags::READ,
            false,
        );
        promo.label_provider_ids = vec!["CATEGORY_PROMOTIONS".into(), "IMPORTANT".into()];
        let mut regular = make_envelope_full(
            "Regular receipt",
            "No category",
            "Shop",
            "shop@example.com",
            MessageFlags::READ,
            false,
        );
        regular.label_provider_ids = vec!["INBOX".into()];
        let promo_id = promo.id.as_str();

        idx.index_envelope(&promo).unwrap();
        idx.index_envelope(&regular).unwrap();
        idx.commit().unwrap();

        assert_eq!(
            e2e_search(&idx, "category:promotions"),
            vec![promo_id.clone()]
        );
        assert_eq!(e2e_search(&idx, "is:important"), vec![promo_id]);
    }

    #[test]
    fn e2e_search_gmail_header_fields() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let mut target = make_envelope_full(
            "List delivery",
            "Header metadata",
            "Sender",
            "sender@example.com",
            MessageFlags::READ,
            false,
        );
        target.message_id_header = Some("<200503292@example.com>".into());
        let target_id = target.id.as_str();
        let target_body = MessageBody {
            message_id: target.id.clone(),
            text_plain: Some("Mailing list update".to_string()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                list_id: Some("info.example.com".into()),
                raw_headers: Some(
                    "Delivered-To: username@example.com\nX-Original-To: alias@example.com\n".into(),
                ),
                ..MessageMetadata::default()
            },
        };
        let other = make_envelope_full(
            "Other delivery",
            "Different headers",
            "Sender",
            "sender@example.com",
            MessageFlags::READ,
            false,
        );

        idx.index_body(&target, &target_body).unwrap();
        idx.index_envelope(&other).unwrap();
        idx.commit().unwrap();

        assert_eq!(
            e2e_search(&idx, "list:info.example.com"),
            vec![target_id.clone()]
        );
        assert_eq!(
            e2e_search(&idx, "deliveredto:username@example.com"),
            vec![target_id.clone()]
        );
        assert_eq!(
            e2e_search(&idx, "rfc822msgid:200503292@example.com"),
            vec![target_id]
        );
    }

    #[test]
    fn e2e_search_gmail_size_and_date_aliases() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let mut target = make_envelope_full(
            "April invoice",
            "Window match",
            "Alice",
            "alice@example.com",
            MessageFlags::READ,
            false,
        );
        target.date = chrono::DateTime::parse_from_rfc3339("2004-04-17T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        target.size_bytes = 6 * 1024 * 1024;
        let target_id = target.id.as_str();
        let mut outside = make_envelope_full(
            "Later invoice",
            "Window miss",
            "Alice",
            "alice@example.com",
            MessageFlags::READ,
            false,
        );
        outside.date = chrono::DateTime::parse_from_rfc3339("2004-04-19T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        outside.size_bytes = 8 * 1024 * 1024;

        idx.index_envelope(&target).unwrap();
        idx.index_envelope(&outside).unwrap();
        idx.commit().unwrap();

        assert_eq!(
            e2e_search(
                &idx,
                "after:2004/04/16 before:04/18/2004 larger:5M smaller:7M",
            ),
            vec![target_id]
        );
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

    #[test]
    fn e2e_search_gmail_has_userlabels_and_rich_content() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let mut labeled = make_envelope_full(
            "Project video",
            "Has useful links",
            "Alice",
            "alice@example.com",
            MessageFlags::READ,
            false,
        );
        labeled.label_provider_ids = vec!["INBOX".into(), "ProjectX".into()];
        let labeled_id = labeled.id.as_str();
        let body = MessageBody {
            message_id: labeled.id.clone(),
            text_plain: Some("Watch https://youtube.com/watch?v=abc".to_string()),
            text_html: Some("<a href=\"https://docs.google.com/document/d/abc\">Doc</a>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };
        let mut system_only = make_envelope_full(
            "Plain inbox",
            "Only system labels",
            "Bob",
            "bob@example.com",
            MessageFlags::READ,
            false,
        );
        system_only.label_provider_ids = vec!["INBOX".into()];
        let system_only_id = system_only.id.as_str();

        idx.index_body(&labeled, &body).unwrap();
        idx.index_envelope(&system_only).unwrap();
        idx.commit().unwrap();

        assert_eq!(e2e_search(&idx, "has:userlabels"), vec![labeled_id.clone()]);
        assert_eq!(e2e_search(&idx, "has:nouserlabels"), vec![system_only_id]);
        assert_eq!(e2e_search(&idx, "has:youtube"), vec![labeled_id.clone()]);
        assert_eq!(e2e_search(&idx, "has:document"), vec![labeled_id]);
    }

    #[test]
    fn e2e_search_gmail_remaining_status_and_rich_aliases() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let mut target = make_envelope_full(
            "Rich Gmail aliases",
            "Useful references",
            "Alice",
            "alice@example.com",
            MessageFlags::READ | MessageFlags::STARRED,
            true,
        );
        target.label_provider_ids = vec!["SNOOZED".into(), "MUTED".into()];
        let target_id = target.id.as_str();
        let body = MessageBody {
            message_id: target.id.clone(),
            text_plain: Some("Open https://drive.google.com/file/d/abc".to_string()),
            text_html: Some(
                "<a href=\"https://docs.google.com/spreadsheets/d/sheet\">Sheet</a>\
                 <a href=\"https://docs.google.com/presentation/d/slides\">Slides</a>"
                    .into(),
            ),
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: target.id.clone(),
                filename: "inline.png".to_string(),
                mime_type: "image/png".to_string(),
                disposition: AttachmentDisposition::Inline,
                content_id: Some("image-1".to_string()),
                content_location: None,
                size_bytes: 10,
                local_path: None,
                provider_id: "inline-1".to_string(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };
        let other = make_envelope_full(
            "Plain message",
            "Nothing special",
            "Bob",
            "bob@example.com",
            MessageFlags::READ,
            false,
        );

        idx.index_body(&target, &body).unwrap();
        idx.index_envelope(&other).unwrap();
        idx.commit().unwrap();

        for query in [
            "in:snoozed",
            "is:muted",
            "has:yellow-star",
            "has:drive",
            "has:spreadsheet",
            "has:presentation",
            "has:inline",
        ] {
            assert_eq!(e2e_search(&idx, query), vec![target_id.clone()]);
        }
        assert_eq!(e2e_search(&idx, "in:anywhere").len(), 2);
    }

    #[test]
    fn e2e_search_gmail_around_operator() {
        let mut idx = SearchIndex::in_memory().unwrap();
        let near = make_envelope_full(
            "Trip ideas",
            "holiday plans then vacation dates",
            "Alice",
            "alice@example.com",
            MessageFlags::READ,
            false,
        );
        let near_id = near.id.as_str();
        let far = make_envelope_full(
            "Trip archive",
            "holiday plans notes budget packing calendar vacation dates",
            "Bob",
            "bob@example.com",
            MessageFlags::READ,
            false,
        );

        idx.index_envelope(&near).unwrap();
        idx.index_envelope(&far).unwrap();
        idx.commit().unwrap();

        assert_eq!(e2e_search(&idx, "holiday AROUND 3 vacation"), vec![near_id]);
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

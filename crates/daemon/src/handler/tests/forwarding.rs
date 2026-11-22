use super::*;
use mxr_core::{
    AttachmentDisposition, AttachmentMeta, Draft, DraftContent, DraftIntent, ForwardSource,
};

#[derive(Default)]
struct MimeTransport(StdMutex<Vec<Vec<u8>>>);

#[async_trait]
impl mxr_core::MailSendProvider for MimeTransport {
    fn name(&self) -> &str {
        "forward-mime-test-transport"
    }

    async fn send(
        &self,
        draft: &Draft,
        from: &mxr_core::Address,
        message_id: &str,
    ) -> Result<mxr_core::SendReceipt, mxr_core::MxrError> {
        let attachments =
            mxr_outbound::attachments::load_attachment_paths_sync(&draft.attachments).unwrap();
        let inline =
            mxr_outbound::attachments::load_inline_assets_sync(&draft.inline_assets).unwrap();
        let mime = mxr_outbound::email::build_message_with_id_and_parts(
            draft,
            from,
            false,
            &attachments,
            &inline,
            message_id,
        )
        .unwrap()
        .formatted();
        self.0.lock().unwrap().push(mime);
        Ok(mxr_core::SendReceipt {
            provider_message_id: None,
            rfc2822_message_id: message_id.into(),
            sent_at: chrono::Utc::now(),
        })
    }
}

async fn fixture(
    sender: Arc<dyn mxr_core::MailSendProvider>,
) -> (
    Arc<AppState>,
    Arc<mxr_provider_fake::FakeProvider>,
    mxr_core::MessageId,
) {
    let account_id = mxr_core::AccountId::new();
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(
            crate::test_fixtures::test_account_with_id(account_id),
            fake.clone(),
            Some(sender),
        )
        .await
        .unwrap(),
    );
    let id = sync_and_get_first_id(&state).await;
    (state, fake, id)
}

async fn source(state: &AppState, id: &mxr_core::MessageId) -> ForwardSource {
    match mutations::prepare_forward(state, id).await.unwrap() {
        ResponseData::ForwardContext { context } => context.source.unwrap(),
        other => panic!("Expected forward context, got {other:?}"),
    }
}

fn draft(source: ForwardSource, intro: &str) -> Draft {
    let now = chrono::Utc::now();
    let body = if intro.is_empty() {
        source.content.analysis_text().to_string()
    } else {
        format!("{intro}\n\n{}", source.content.analysis_text())
    };
    let fm = mxr_compose::frontmatter::ComposeFrontmatter {
        attach: mxr_compose::forward::resource_references(&source),
        forward_source: Some(source.clone()),
        ..Default::default()
    };
    let mut draft = Draft {
        revision: None,
        id: source.draft_id,
        account_id: source.account_id,
        from: None,
        reply_headers: None,
        intent: DraftIntent::Forward,
        to: vec![mxr_core::Address {
            name: None,
            email: "synthetic-recipient@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Fwd: Reports".into(),
        content: DraftContent::markdown(body),
        attachments: vec![],
        managed_attachments: Some(vec![]),
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    };
    mxr_compose::forward::apply(&mut draft, &fm).unwrap();
    draft
}

#[tokio::test]
async fn forwarding_preview_saved_draft_and_test_transport_preserve_original_and_all_mime_parts() {
    use mail_parser::MimeHeaders;
    let sender = Arc::new(MimeTransport::default());
    let (state, fake, id) = fixture(sender.clone()).await;
    let temp = tempfile::tempdir().unwrap();
    let mut envelope = state.store.get_envelope(&id).await.unwrap().unwrap();
    envelope.cc = vec![mxr_core::Address {
        name: Some("Cc Person".into()),
        email: "cc@example.com".into(),
    }];
    state.store.upsert_envelope(&envelope).await.unwrap();
    let binary = vec![0, 255, 128, 13, 10, 0, 42];
    let mut parts = Vec::new();
    for index in 0..8 {
        let bytes = if index == 6 {
            binary.clone()
        } else {
            format!("%PDF-synthetic-{index}\n").into_bytes()
        };
        let path = temp.path().join(format!("part-{index}"));
        std::fs::write(&path, bytes).unwrap();
        parts.push(AttachmentMeta {
            id: mxr_core::AttachmentId::new(),
            message_id: id.clone(),
            filename: if index < 5 {
                "report.pdf".into()
            } else {
                "asset.dat".into()
            },
            mime_type: if index < 5 {
                "application/pdf".into()
            } else if index == 5 {
                "image/png".into()
            } else if index == 7 {
                "image/gif".into()
            } else {
                "application/x-test-binary".into()
            },
            disposition: if index == 7 {
                AttachmentDisposition::Inline
            } else {
                AttachmentDisposition::Attachment
            },
            content_id: (index == 5).then(|| "logo@example.com".into()),
            content_location: (index == 7).then(|| "images/logo.gif".into()),
            size_bytes: if index == 0 {
                23
            } else {
                std::fs::metadata(&path).unwrap().len()
            },
            local_path: (index != 0).then_some(path),
            provider_id: format!("part-{index}"),
        });
    }
    let original_html = "<!DOCTYPE html><html><head><style>.original{color:red}</style><style>.background{background-image:url('cid:logo@example.com')}</style></head><body><table class=original><tr><td>Original **literal** &amp; café</td></tr></table><img src=\"CID:logo%40example.com\"><img src=\"images/logo.gif\"></body></html>";
    let original_text =
        "Original **literal** & café\n# --- context (stripped before sending) ---\n{today}\n";
    state
        .store
        .insert_body(&mxr_core::MessageBody {
            message_id: id.clone(),
            text_plain: Some(original_text.into()),
            text_html: Some(original_html.into()),
            attachments: parts,
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        })
        .await
        .unwrap();

    let source = source(&state, &id).await;
    assert_eq!(fake.attachment_fetch_count(), 1);
    assert_eq!(source.attachments.len(), 8);
    assert!(
        state
            .store
            .list_drafts(&state.default_account_id())
            .await
            .unwrap()
            .is_empty(),
        "preparing a preview must not save a draft"
    );
    let draft = draft(source, "Please review these reports.");
    for expected in [
        "From:",
        "Date:",
        "Subject:",
        "To:",
        "Cc: Cc Person <cc@example.com>",
        original_text,
    ] {
        assert!(
            draft.content.analysis_text().contains(expected),
            "missing {expected:?}"
        );
    }
    let ResponseData::DraftPreview { draft: preview, .. } = draft_preview::preview(
        &state,
        &draft,
        &mxr_protocol::DraftSafetyContextData::default(),
        &[],
    )
    .await
    .unwrap() else {
        panic!("preview");
    };
    let ResponseData::Draft { draft: saved } = mutations::save_draft(&state, &draft).await.unwrap()
    else {
        panic!("saved draft");
    };
    let loaded = state.store.get_draft(&saved.id).await.unwrap().unwrap();
    assert_eq!(preview.content, loaded.content);
    assert_eq!(preview.managed_attachments, loaded.managed_attachments);
    assert!(loaded.attachments.is_empty());
    assert_eq!(loaded.managed_attachments.as_ref().unwrap().len(), 8);
    let ResponseData::SendReceipt {
        local_message_id, ..
    } = mutations::send_draft(&state, &loaded, None).await.unwrap()
    else {
        panic!("sent receipt");
    };
    let sent_body = state
        .store
        .get_body(&local_message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sent_body.attachments.len(), 8);
    for attachment in &sent_body.attachments {
        let path = attachment.local_path.as_ref().unwrap();
        assert!(path.starts_with(state.attachment_dir()));
        assert!(!path.starts_with(state.store.draft_attachment_dir()));
        assert_eq!(
            std::fs::metadata(path).unwrap().len(),
            attachment.size_bytes
        );
    }
    let messages = sender.0.lock().unwrap();
    assert_eq!(messages.len(), 1);
    let parsed = mail_parser::MessageParser::default()
        .parse(&messages[0])
        .unwrap();
    let text = parsed.body_text(0).unwrap();
    assert_eq!(
        text.replace("\r\n", "\n"),
        loaded.content.analysis_text().replace("\r\n", "\n")
    );
    assert!(text.starts_with("Please review these reports."));
    assert!(text.contains("Original **literal** & café"));
    assert!(text.contains("{today}"));
    let html = parsed.body_html(0).unwrap();
    let DraftContent::Html {
        html: reviewed_html,
        ..
    } = &loaded.content
    else {
        panic!("reviewed HTML");
    };
    assert_eq!(html.as_ref(), reviewed_html);
    assert!(html.contains("<style>.original{color:red}</style>"));
    assert!(html.contains(
        "<table class=original><tr><td>Original **literal** &amp; café</td></tr></table>"
    ));
    assert!(html.find("Please review").unwrap() < html.find("<table class=original>").unwrap());
    let attachments: Vec<_> = parsed.attachments().collect();
    assert_eq!(attachments.len(), 8);
    let pdfs: Vec<_> = attachments
        .iter()
        .filter(|part| part.attachment_name() == Some("report.pdf"))
        .collect();
    assert_eq!(pdfs.len(), 5);
    assert_eq!(pdfs[0].contents(), b"fake attachment content");
    for (index, pdf) in pdfs.iter().enumerate().skip(1) {
        assert_eq!(
            pdf.contents(),
            format!("%PDF-synthetic-{index}\n").as_bytes()
        );
    }
    let binary_part = attachments
        .iter()
        .find(|part| {
            part.content_type().is_some_and(|ct| {
                ct.ctype() == "application" && ct.subtype() == Some("x-test-binary")
            })
        })
        .unwrap();
    assert_eq!(binary_part.contents(), binary);
    let inline = attachments
        .iter()
        .find(|part| part.content_id() == Some("logo@example.com"))
        .unwrap();
    assert_eq!(inline.attachment_name(), Some("asset.dat"));
    assert_eq!(inline.content_type().unwrap().ctype(), "image");
    assert!(inline.content_disposition().unwrap().is_inline());
    let location_part = attachments
        .iter()
        .find(|part| part.content_location() == Some("images/logo.gif"))
        .unwrap();
    assert_eq!(location_part.attachment_name(), Some("asset.dat"));
    assert!(location_part.content_disposition().unwrap().is_inline());
    assert_eq!(location_part.contents(), b"%PDF-synthetic-7\n");
    assert!(html.contains("src=\"images/logo.gif\""));
}

#[tokio::test]
async fn forwarding_attachment_fetch_failure_is_not_silently_omitted() {
    let (state, _, id) = fixture(Arc::new(MimeTransport::default())).await;
    let account_id = state.default_account_id();
    state.add_sync_provider_for_test(Arc::new(FailingSyncProvider {
        account_id,
        message: "controlled fetch failure",
        calls: Arc::new(AtomicUsize::new(0)),
    }));
    state
        .store
        .insert_body(&mxr_core::MessageBody {
            message_id: id.clone(),
            text_plain: Some("Original".into()),
            text_html: None,
            attachments: vec![AttachmentMeta {
                id: mxr_core::AttachmentId::new(),
                message_id: id.clone(),
                filename: "required.pdf".into(),
                mime_type: "application/pdf".into(),
                disposition: AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 10,
                local_path: None,
                provider_id: "required".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        })
        .await
        .unwrap();
    let error = mutations::prepare_forward(&state, &id)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("required.pdf") && error.contains("controlled fetch failure"),
        "{error}"
    );
    assert!(state
        .store
        .list_drafts(&state.default_account_id())
        .await
        .unwrap()
        .is_empty());
}

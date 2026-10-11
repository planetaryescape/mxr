use super::*;
pub(super) fn at(year: i32, month: u32, day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, 9, 0, 0)
        .single()
        .expect("time")
}

impl Fx {
    pub(super) async fn receipt(
        &mut self,
        product: &str,
        date: DateTime<Utc>,
        invoice: bool,
    ) -> MessageId {
        let kind = if invoice { "invoice" } else { "receipt" };
        let id = self
            .mail(
                ("Acme", "billing@acme.example"),
                &format!("Your Acme {product} {kind}"),
                "Thanks for your payment.\nTotal: £120.00",
                None,
                date,
            )
            .await;
        pass::scan_messages(
            &self.store,
            &PassConfig {
                now: at(2026, 10, 11),
                tz: Utc,
            },
            std::slice::from_ref(&id),
        )
        .await
        .expect("scan");
        id
    }

    pub(super) async fn annual(&mut self, product: &str) {
        self.receipt(product, at(2024, 10, 20), false).await;
        self.receipt(product, at(2025, 10, 20), false).await;
    }

    pub(super) async fn plan(&self, now: DateTime<Utc>) -> Vec<Candidate> {
        plan(
            &self.store,
            std::slice::from_ref(&self.account),
            now,
            &Utc,
            9,
        )
        .await
        .expect("plan")
    }

    pub(super) async fn apply(&self, now: DateTime<Utc>) {
        apply(&self.store, self.plan(now).await)
            .await
            .expect("apply");
    }
}

pub(super) struct Fx {
    pub(super) store: Store,
    pub(super) account: AccountId,
    next: u32,
}

impl Fx {
    pub(super) async fn new() -> Self {
        let store = Store::in_memory().await.expect("store");
        let account = Account {
            id: AccountId::new(),
            name: "Test".into(),
            email: "me@example.com".into(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".into(),
            }),
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&account).await.expect("account");
        Self {
            store,
            account: account.id,
            next: 0,
        }
    }

    pub(super) async fn mail(
        &mut self,
        from: (&str, &str),
        subject: &str,
        text: &str,
        html: Option<&str>,
        date: DateTime<Utc>,
    ) -> MessageId {
        self.next += 1;
        let envelope = Envelope {
            id: MessageId::new(),
            account_id: self.account.clone(),
            provider_id: format!("p{}", self.next),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some(from.0.to_string()),
                email: from.1.to_string(),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: subject.to_string(),
            date,
            flags: MessageFlags::empty(),
            snippet: text.chars().take(80).collect(),
            has_attachments: false,
            size_bytes: 100,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        };
        self.store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .expect("envelope");
        self.store
            .insert_body(&MessageBody {
                message_id: envelope.id.clone(),
                text_plain: Some(text.to_string()),
                text_html: html.map(str::to_string),
                attachments: vec![],
                fetched_at: date,
                metadata: MessageMetadata::default(),
            })
            .await
            .expect("body");
        envelope.id
    }
}

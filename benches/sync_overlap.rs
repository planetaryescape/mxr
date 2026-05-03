use async_trait::async_trait;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mxr_core::id::AccountId;
use mxr_core::types::{Account, BackendRef, ProviderKind, SyncCursor};
use mxr_provider_fake::FakeProvider;
use mxr_provider_imap::config::ImapConfig;
use mxr_provider_imap::session::{ImapSession, ImapSessionFactory};
use mxr_provider_imap::types::{
    FetchedMessage, FolderInfo, ImapCapabilities, MailboxInfo, NamespaceInfo, QresyncInfo,
};
use mxr_provider_imap::ImapProvider;
use mxr_search::{SearchIndex, SearchServiceHandle};
use mxr_store::Store;
use mxr_sync::SyncEngine;
use std::collections::HashMap;
use std::sync::Arc;

fn bench_sync_overlap(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

    c.bench_function("sync_fake_multi_account_overlap", |b| {
        let (engine, provider_a, provider_b) = runtime.block_on(async {
            let store = Arc::new(Store::in_memory().await.expect("store"));
            let (search, _search_worker) =
                SearchServiceHandle::start(SearchIndex::in_memory().expect("search"));
            let engine = SyncEngine::new(store.clone(), search);

            let account_a = AccountId::new();
            let account_b = AccountId::new();
            for account in [
                Account {
                    id: account_a.clone(),
                    name: "Bench A".into(),
                    email: "bench-a@example.com".into(),
                    sync_backend: Some(BackendRef {
                        provider_kind: ProviderKind::Fake,
                        config_key: "bench-a".into(),
                    }),
                    send_backend: None,
                    enabled: true,
                },
                Account {
                    id: account_b.clone(),
                    name: "Bench B".into(),
                    email: "bench-b@example.com".into(),
                    sync_backend: Some(BackendRef {
                        provider_kind: ProviderKind::Fake,
                        config_key: "bench-b".into(),
                    }),
                    send_backend: None,
                    enabled: true,
                },
            ] {
                store
                    .insert_account(&account)
                    .await
                    .expect("insert account");
            }

            (
                engine,
                FakeProvider::new(account_a),
                FakeProvider::new(account_b),
            )
        });

        b.iter(|| {
            runtime.block_on(async {
                let (left, right) = tokio::join!(
                    engine.sync_account(black_box(&provider_a)),
                    engine.sync_account(black_box(&provider_b))
                );
                black_box(left.expect("sync account a"));
                black_box(right.expect("sync account b"));
            });
        });
    });

    c.bench_function("imap_initial_sync_multi_folder_fixture", |b| {
        let provider = bench_imap_provider();
        b.iter(|| {
            runtime.block_on(async {
                let batch = mxr_core::MailSyncProvider::sync_messages(
                    black_box(&provider),
                    &SyncCursor::Initial,
                )
                .await
                .expect("imap initial sync");
                black_box(batch.upserted.len());
            });
        });
    });
}

fn bench_imap_provider() -> ImapProvider {
    let factory = BenchImapSessionFactory {
        mailboxes: vec![
            ("INBOX".to_string(), bench_folder_messages("inbox", 32)),
            (
                "Projects".to_string(),
                bench_folder_messages("projects", 32),
            ),
            ("Archive".to_string(), bench_folder_messages("archive", 32)),
            (
                "Receipts".to_string(),
                bench_folder_messages("receipts", 32),
            ),
        ],
    };

    ImapProvider::with_session_factory(
        AccountId::new(),
        ImapConfig::new(
            "bench.local".into(),
            993,
            "bench".into(),
            "bench".into(),
            false,
            true,
        ),
        Box::new(factory),
    )
}

fn bench_folder_messages(mailbox: &str, count: u32) -> Vec<FetchedMessage> {
    (1..=count)
        .map(|uid| {
            let subject = format!("{mailbox} subject {uid}");
            let raw = format!(
                "From: bench@example.com\r\nTo: me@example.com\r\nSubject: {subject}\r\nDate: Mon, 1 Jan 2024 12:00:00 +0000\r\nMessage-ID: <{mailbox}-{uid}@bench>\r\nContent-Type: text/plain\r\n\r\nBody {uid} for {mailbox}"
            );
            FetchedMessage {
                uid,
                flags: vec!["\\Seen".to_string()],
                envelope: None,
                body: Some(raw.into_bytes()),
                header: None,
                size: Some(1024),
            }
        })
        .collect()
}

#[derive(Clone)]
struct BenchImapSessionFactory {
    mailboxes: Vec<(String, Vec<FetchedMessage>)>,
}

#[async_trait]
impl ImapSessionFactory for BenchImapSessionFactory {
    async fn create_session(&self) -> mxr_provider_imap::session::Result<Box<dyn ImapSession>> {
        Ok(Box::new(BenchImapSession {
            selected_mailbox: None,
            mailboxes: self
                .mailboxes
                .iter()
                .cloned()
                .collect::<HashMap<String, Vec<FetchedMessage>>>(),
        }))
    }
}

struct BenchImapSession {
    selected_mailbox: Option<String>,
    mailboxes: HashMap<String, Vec<FetchedMessage>>,
}

#[async_trait]
impl ImapSession for BenchImapSession {
    async fn capabilities(&mut self) -> mxr_provider_imap::session::Result<ImapCapabilities> {
        Ok(ImapCapabilities::default())
    }

    async fn enable(&mut self, _capabilities: &[&str]) -> mxr_provider_imap::session::Result<()> {
        Ok(())
    }

    async fn namespace(&mut self) -> mxr_provider_imap::session::Result<Option<NamespaceInfo>> {
        Ok(None)
    }

    async fn select(&mut self, mailbox: &str) -> mxr_provider_imap::session::Result<MailboxInfo> {
        self.selected_mailbox = Some(mailbox.to_string());
        let exists = self
            .mailboxes
            .get(mailbox)
            .map(|items| items.len())
            .unwrap_or(0) as u32;
        Ok(MailboxInfo {
            uid_validity: 1,
            uid_next: exists + 1,
            exists,
            highest_modseq: None,
        })
    }

    async fn select_qresync(
        &mut self,
        mailbox: &str,
        _uid_validity: u32,
        _highest_modseq: u64,
        _known_uids: &str,
    ) -> mxr_provider_imap::session::Result<QresyncInfo> {
        let mailbox_info = self.select(mailbox).await?;
        Ok(QresyncInfo {
            mailbox: mailbox_info,
            vanished: Vec::new(),
            changed: Vec::new(),
        })
    }

    async fn uid_fetch(
        &mut self,
        _uid_set: &str,
        _query: &str,
    ) -> mxr_provider_imap::session::Result<Vec<FetchedMessage>> {
        let mailbox = self
            .selected_mailbox
            .as_deref()
            .unwrap_or("INBOX")
            .to_string();
        Ok(self.mailboxes.get(&mailbox).cloned().unwrap_or_default())
    }

    async fn uid_search(&mut self, _query: &str) -> mxr_provider_imap::session::Result<Vec<u32>> {
        Ok(Vec::new())
    }

    async fn uid_store(
        &mut self,
        _uid_set: &str,
        _flags: &str,
    ) -> mxr_provider_imap::session::Result<()> {
        Ok(())
    }

    async fn uid_copy(
        &mut self,
        _uid_set: &str,
        _mailbox: &str,
    ) -> mxr_provider_imap::session::Result<()> {
        Ok(())
    }

    async fn uid_move(
        &mut self,
        _uid_set: &str,
        _mailbox: &str,
    ) -> mxr_provider_imap::session::Result<()> {
        Ok(())
    }

    async fn uid_expunge(&mut self, _uid_set: &str) -> mxr_provider_imap::session::Result<()> {
        Ok(())
    }

    async fn expunge(&mut self) -> mxr_provider_imap::session::Result<()> {
        Ok(())
    }

    async fn list_folders(&mut self) -> mxr_provider_imap::session::Result<Vec<FolderInfo>> {
        Ok(self
            .mailboxes
            .keys()
            .map(|name| FolderInfo {
                name: name.clone(),
                special_use: (name == "INBOX").then(|| "\\Inbox".to_string()),
                ..FolderInfo::default()
            })
            .collect())
    }

    async fn create_mailbox(&mut self, _mailbox: &str) -> mxr_provider_imap::session::Result<()> {
        Ok(())
    }

    async fn rename_mailbox(
        &mut self,
        _old_mailbox: &str,
        _new_mailbox: &str,
    ) -> mxr_provider_imap::session::Result<()> {
        Ok(())
    }

    async fn delete_mailbox(&mut self, _mailbox: &str) -> mxr_provider_imap::session::Result<()> {
        Ok(())
    }

    async fn logout(&mut self) -> mxr_provider_imap::session::Result<()> {
        Ok(())
    }
}

criterion_group!(benches, bench_sync_overlap);
criterion_main!(benches);

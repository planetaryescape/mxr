use super::*;
use crate::handler::assessments;
use crate::test_fixtures::{test_account, write_migrated_database, TestEnvelopeBuilder};
use mxr_core::{Envelope, MessageBody, MessageDirection};
use std::sync::{Arc, Mutex};
use tokio::sync::{watch, Semaphore};

struct Reconciler {
    stop: watch::Sender<bool>,
    completed: Arc<Semaphore>,
    task: tokio::task::JoinHandle<()>,
}

impl Reconciler {
    fn start(state: Arc<AppState>, token: Option<Arc<Mutex<String>>>) -> Self {
        let (stop, shutdown) = watch::channel(false);
        let completed = Arc::new(Semaphore::new(0));
        let observed = completed.clone();
        let hook: Arc<dyn Fn() + Send + Sync> = Arc::new(move || observed.add_permits(1));
        let task = tokio::spawn(async move {
            assessments::AFTER_RECONCILE_TICK
                .scope(hook, async move {
                    if let Some(token) = token {
                        let resolve: Arc<dyn Fn() -> Option<String> + Send + Sync> =
                            Arc::new(move || Some(token.lock().unwrap().clone()));
                        assessments::RECONCILER_CONTEXT_TOKEN
                            .scope(resolve, assessments::reconciler_loop(state, shutdown))
                            .await;
                    } else {
                        assessments::reconciler_loop(state, shutdown).await;
                    }
                })
                .await;
        });
        Self {
            stop,
            completed,
            task,
        }
    }

    async fn tick(&self) {
        tokio::time::timeout(std::time::Duration::from_secs(10), self.completed.acquire())
            .await
            .expect("reconciler completed a real tick")
            .unwrap()
            .forget();
    }

    async fn stop(self) {
        self.stop.send(true).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), self.task)
            .await
            .unwrap()
            .unwrap();
    }
}

async fn changes(state: &AppState) -> i64 {
    sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(state.store.writer())
        .await
        .unwrap()
}

async fn stored_snapshot(state: &AppState) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for sql in [
        "SELECT json_array(message_id,account_id,policy_version,input_fingerprint,input_revision,assessment_json) FROM mail_assessments ORDER BY account_id,message_id,policy_version",
        "SELECT json_array(account_id,thread_id,revision,dirty) FROM assessment_thread_inputs ORDER BY account_id,thread_id",
        "SELECT json_array(message_id,account_id,input_revision,input_fingerprint,context_token) FROM asks_projection_outbox ORDER BY message_id",
        "SELECT json_array(key,value) FROM assessment_authority ORDER BY key",
    ] {
        out.push(sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).fetch_all(state.store.reader()).await.unwrap());
    }
    out
}

async fn seed(state: &AppState) -> Envelope {
    let account = test_account();
    state.store.insert_account(&account).await.unwrap();
    let mail = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .recipient_address(None, &account.email)
        .subject("Agreement")
        .build();
    state
        .store
        .upsert_envelope_with_direction(&mail, MessageDirection::Inbound)
        .await
        .unwrap();
    state
        .store
        .insert_body(&MessageBody {
            message_id: mail.id.clone(),
            text_plain: Some("Please sign the agreement.".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: mail.date,
            metadata: Default::default(),
        })
        .await
        .unwrap();
    assessments::reconcile_messages(state, &account.id, std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    crate::handler::arrivals::place_pending(state, &account.id)
        .await
        .unwrap();
    mail
}

async fn assert_current(state: &AppState, mails: &[Envelope]) {
    for mail in mails {
        let asks = state
            .store
            .asks_for_messages(&mail.account_id, std::slice::from_ref(&mail.id))
            .await
            .unwrap();
        assert!(asks[&mail.id]
            .units
            .iter()
            .any(|u| u.answer == mxr_core::asks::AskAnswer::AsksYou));
        assert!(state
            .store
            .dirty_assessment_threads(&mail.account_id, None)
            .await
            .unwrap()
            .is_empty());
    }
}

#[tokio::test]
async fn settled_sender_views_reuse_facets_and_only_rebuild_changed_threads() {
    let state = AppState::in_memory_with_fake_config(mxr_config::MxrConfig::default())
        .await
        .unwrap()
        .0;
    let mail = seed(&state).await;
    let mut sent = mail.clone();
    sent.id = mxr_core::MessageId::new();
    sent.thread_id = mxr_core::ThreadId::new();
    sent.provider_id = "sender-view-prior-outbound".into();
    sent.message_id_header = Some("<sender-view-prior-outbound@example.test>".into());
    sent.in_reply_to = None;
    sent.from.email = state
        .store
        .get_account(&mail.account_id)
        .await
        .unwrap()
        .unwrap()
        .email;
    sent.to = vec![mail.from.clone()];
    state
        .store
        .upsert_envelope_with_direction(&sent, MessageDirection::Outbound)
        .await
        .unwrap();
    assessments::reconcile_messages(&state, &mail.account_id, std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    let read = || {
        state
            .store
            .desk_messages_in_threads(&mail.account_id, std::slice::from_ref(&mail.thread_id))
    };
    let messages = read().await.unwrap();
    let never: assessments::BeforeCommit =
        Arc::new(|| Box::pin(async { panic!("a settled view must not reclassify stored mail") }));
    assessments::BEFORE_COMMIT
        .scope(never, async {
            let senders = crate::handler::desk::Senders::load(&state, &mail.account_id, &messages)
                .await
                .unwrap();
            assert!(senders.assessments.contains_key(&mail.id));
            assert!(senders.asks.contains_key(&mail.id));
            assert!(
                senders.asks[&mail.id]
                    .answers
                    .contains(&mxr_core::asks::AskAnswer::AsksYou),
                "the initial agreement request must require an action before invalidation"
            );
            assert!(
                senders
                    .written_to
                    .contains(&mail.from.email.to_ascii_lowercase()),
                "validated sender observations must preserve prior outbound correspondence"
            );
            let candidate = state.store.place_message(&mail.id).await.unwrap().unwrap();
            let kinds = crate::handler::places::AccountKinds::load(
                &state,
                &mail.account_id,
                std::slice::from_ref(&candidate),
            )
            .await
            .unwrap();
            assert!(
                kinds.has_assessment(&candidate),
                "Now, Reading and Updates must also reuse settled verdicts"
            );
        })
        .await;
    state
        .store
        .insert_body(&MessageBody {
            message_id: mail.id.clone(),
            text_plain: Some("No reply needed, all sorted.".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: mail.date,
            metadata: Default::default(),
        })
        .await
        .unwrap();
    let messages = read().await.unwrap();
    let senders = crate::handler::desk::Senders::load(&state, &mail.account_id, &messages)
        .await
        .unwrap();
    assert!(senders.assessments.contains_key(&mail.id));
    assert!(!senders.asks[&mail.id]
        .answers
        .contains(&mxr_core::asks::AskAnswer::AsksYou));
}

#[test]
fn unchanged_persistent_startup_and_actual_first_and_later_reconciler_ticks_are_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    temp_env::with_vars(
        [
            ("MXR_CONFIG_DIR", Some(dir.path().join("config"))),
            ("MXR_DATA_DIR", Some(dir.path().join("data"))),
            ("MXR_SOCKET_PATH", Some(dir.path().join("socket"))),
        ],
        || {
            runtime.block_on(async {
                let mut config = mxr_config::MxrConfig::default();
                config.llm.enabled = false;
                config.search.semantic.enabled = false;
                config.search.semantic.auto_download_models = false;
                config.activity.enabled = false;
                mxr_config::save_config(&config).unwrap();
                write_migrated_database(&mxr_config::data_dir().join("mxr.db")).await;
                let state = Arc::new(AppState::new().await.unwrap());
                let mail = seed(&state).await;
                let mut configured = state.config_snapshot();
                configured
                    .todo
                    .trusted_authentication_services
                    .insert(mail.account_id.as_str(), vec!["mx.example.net".into()]);
                mxr_config::save_config(&configured).unwrap();
                state.set_config_for_test(configured).await;
                assessments::reconcile_messages(
                    &state,
                    &mail.account_id,
                    std::slice::from_ref(&mail.id),
                )
                .await
                .unwrap();
                assert_current(&state, std::slice::from_ref(&mail)).await;
                let before_restart = stored_snapshot(&state).await;
                assert!(
                    before_restart[2].is_empty(),
                    "projection is already acknowledged"
                );
                state
                    .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
                    .await;
                state.store.reader().close().await;
                state.store.writer().close().await;
                drop(state);
                let state = Arc::new(AppState::new().await.unwrap());
                assert_eq!(
                    stored_snapshot(&state).await,
                    before_restart,
                    "startup retains current home/asks, generations and acknowledged outbox"
                );
                assert_current(&state, std::slice::from_ref(&mail)).await;
                let before = changes(&state).await;
                let reconciler = Reconciler::start(state.clone(), None);
                reconciler.tick().await;
                assert_eq!(
                    changes(&state).await,
                    before,
                    "actual first tick performs no writer work"
                );
                reconciler.tick().await;
                assert_eq!(
                    changes(&state).await,
                    before,
                    "later unchanged tick also performs no writer work"
                );
                assert_eq!(stored_snapshot(&state).await, before_restart);
                assert_current(&state, std::slice::from_ref(&mail)).await;
                reconciler.stop().await;
                state
                    .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
                    .await;
                state.store.reader().close().await;
                state.store.writer().close().await;
            });
        },
    );
}

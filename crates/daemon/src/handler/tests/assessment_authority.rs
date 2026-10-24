//! Config authority is ordered against publication and survives real daemon startup.
use super::*;
use crate::handler::assessments;
use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
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
                        // Exercise the context boundary without changing process-wide TZ.
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

async fn generations(state: &AppState, mails: &[Envelope]) -> Vec<i64> {
    let mut generations = Vec::new();
    for mail in mails {
        let revisions = state
            .store
            .assessment_thread_revisions(&mail.account_id, std::slice::from_ref(&mail.thread_id))
            .await
            .unwrap();
        generations.push(revisions[&mail.thread_id]);
    }
    generations
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

#[tokio::test]
async fn actual_reconciler_distinguishes_shape_only_and_timezone_only_context_changes() {
    let mut config = mxr_config::MxrConfig::default();
    config.activity.enabled = false;
    let state = Arc::new(
        AppState::in_memory_with_fake_config(config)
            .await
            .unwrap()
            .0,
    );
    let mails = [seed(&state).await, seed(&state).await];
    let token = Arc::new(Mutex::new(mxr_store::asks_context_token().unwrap()));
    let reconciler = Reconciler::start(state.clone(), Some(token.clone()));
    reconciler.tick().await;
    assert_current(&state, &mails).await;
    let before = stored_snapshot(&state).await;
    let initial_generations = generations(&state, &mails).await;
    let mut config = state.config_snapshot();
    config.messages.large_thread_recipients += 1;
    let shape = config.messages.large_thread_recipients;
    state.set_config_for_test(config).await;
    reconciler.tick().await;
    let shape_changed = stored_snapshot(&state).await;
    let shape_generations = generations(&state, &mails).await;
    assert!(shape_generations
        .iter()
        .zip(&initial_generations)
        .all(|(after, before)| after > before));
    assert_ne!(
        shape_changed[1], before[1],
        "shape alone invalidates both accounts"
    );
    assert_current(&state, &mails).await;
    let marker: String =
        sqlx::query_scalar("SELECT value FROM assessment_authority WHERE key='global'")
            .fetch_one(state.store.reader())
            .await
            .unwrap();
    assert_eq!(
        marker,
        assessments::context_key(shape, Some(&token.lock().unwrap()))
    );
    let settled = changes(&state).await;
    reconciler.tick().await;
    assert_eq!(changes(&state).await, settled);
    *token.lock().unwrap() = "different-immutable-timezone-snapshot".into();
    reconciler.tick().await;
    let timezone_changed = stored_snapshot(&state).await;
    assert!(generations(&state, &mails)
        .await
        .iter()
        .zip(&shape_generations)
        .all(|(after, before)| after > before));
    assert_ne!(
        timezone_changed[1], shape_changed[1],
        "timezone context alone invalidates both accounts"
    );
    assert_current(&state, &mails).await;
    let marker: String =
        sqlx::query_scalar("SELECT value FROM assessment_authority WHERE key='global'")
            .fetch_one(state.store.reader())
            .await
            .unwrap();
    assert_eq!(
        marker,
        assessments::context_key(shape, Some("different-immutable-timezone-snapshot"))
    );
    let settled = changes(&state).await;
    reconciler.tick().await;
    assert_eq!(changes(&state).await, settled);
    reconciler.stop().await;
}

#[test]
fn a_settled_previous_policy_is_reconciled_by_startup_and_the_first_normal_tick() {
    for old_policy in [
        "asks-r2c-9",
        "asks-r2c-10",
        "asks-r2c-11",
        "asks-r2c-12",
        "asks-r2c-13",
    ] {
        assert_previous_policy_activates_once(old_policy);
    }
}

fn assert_previous_policy_activates_once(old_policy: &str) {
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
                let state = Arc::new(AppState::new().await.unwrap());
                let mail = seed(&state).await;
                let shape = state.config_snapshot().messages.large_thread_recipients;
                let token = mxr_store::asks_context_token().unwrap();
                // Persist the previous daemon's actual policy-aware startup marker.
                let old_marker = format!("{}:{old_policy}:{shape}:{:?}", mxr_core::assessment::CLASSIFIER_POLICY_VERSION, Some(token.as_str()));
                state.store.sync_assessment_context(&old_marker).await.unwrap();
                assessments::reconcile_account(&state, &mail.account_id).await.unwrap();
                sqlx::query("UPDATE mail_assessments SET policy_version=?1, assessment_json=json_set(assessment_json,'$.policy_version',?1) WHERE message_id=?2 AND policy_version=?3")
                    .bind(old_policy).bind(mail.id.as_str()).bind(mxr_core::asks::ASKS_POLICY_VERSION).execute(state.store.writer()).await.unwrap();
                assert!(state.store.dirty_assessment_threads(&mail.account_id, None).await.unwrap().is_empty());
                assert!(stored_snapshot(&state).await[2].is_empty(), "old profile is fully settled");
                assert!(state.store.asks_for_messages(&mail.account_id, std::slice::from_ref(&mail.id)).await.unwrap().is_empty());
                state.shutdown_runtime_tasks(std::time::Duration::from_secs(2)).await;
                state.store.reader().close().await;
                state.store.writer().close().await;
                drop(state);

                let state = Arc::new(AppState::new().await.unwrap());
                let reconciler = Reconciler::start(state.clone(), None);
                reconciler.tick().await;
                assert_current(&state, std::slice::from_ref(&mail)).await;
                let history: i64 = sqlx::query_scalar("SELECT count(*) FROM mail_assessments WHERE message_id=? AND policy_version=?")
                    .bind(mail.id.as_str()).bind(old_policy).fetch_one(state.store.reader()).await.unwrap();
                assert_eq!(history, 1, "activation retains previous policy history");
                let current = state.store.asks_for_messages(&mail.account_id, std::slice::from_ref(&mail.id)).await.unwrap();
                assert_eq!(current[&mail.id].policy_version, mxr_core::asks::ASKS_POLICY_VERSION);
                let current_marker: String = sqlx::query_scalar("SELECT value FROM assessment_authority WHERE key='global'").fetch_one(state.store.reader()).await.unwrap();
                assert_eq!(current_marker, assessments::context_key(shape, Some(&token)));
                eprintln!("POLICY_ACTIVATION old={old_policy} old_marker={old_marker:?} current={} current_marker={current_marker:?} retained_history={history}", current[&mail.id].policy_version);
                reconciler.stop().await;
                let settled = stored_snapshot(&state).await;
                state.shutdown_runtime_tasks(std::time::Duration::from_secs(2)).await;
                state.store.reader().close().await;
                state.store.writer().close().await;
                drop(state);

                let state = Arc::new(AppState::new().await.unwrap());
                assert_eq!(stored_snapshot(&state).await, settled, "subsequent startup retains the activated answer");
                let before = changes(&state).await;
                let reconciler = Reconciler::start(state.clone(), None);
                reconciler.tick().await;
                assert_eq!(changes(&state).await, before, "unchanged restart and first tick do not invalidate again");
                eprintln!("POLICY_RESTART old={old_policy} current={} writes_before={before} writes_after={} snapshot_unchanged=true", mxr_core::asks::ASKS_POLICY_VERSION, changes(&state).await);
                assert_eq!(stored_snapshot(&state).await, settled);
                assert_current(&state, std::slice::from_ref(&mail)).await;
                reconciler.stop().await;
                state.shutdown_runtime_tasks(std::time::Duration::from_secs(2)).await;
                state.store.reader().close().await;
                state.store.writer().close().await;
            });
        },
    );
}

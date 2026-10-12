use super::*;
mod retries;
use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountedModel {
    calls: AtomicUsize,
    delay_ms: AtomicUsize,
    response: std::sync::Mutex<String>,
    gate: tokio::sync::Semaphore,
    fail: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl LlmProvider for CountedModel {
    async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.gate.acquire().await.unwrap().forget();
        let delay = self.delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay as u64)).await;
        }
        if self.fail.load(Ordering::SeqCst) {
            return Err(LlmError::Unreachable("fixture endpoint unavailable".into()));
        }
        Ok(CompletionResponse {
            content: self.response.lock().unwrap().clone(),
            model: "resolution-fixture".into(),
            finish_reason: Some("stop".into()),
        })
    }

    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            context_window: 32_000,
            supports_streaming: false,
        }
    }

    fn model_name(&self) -> &str {
        "resolution-fixture"
    }
}

async fn ambiguous_pairs(
    messages: usize,
    tasks: usize,
) -> (Fixture, Arc<CountedModel>, Vec<MessageId>) {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let original = put(
        &fx,
        Mail::new(
            ("Council", "bills@example.com"),
            "Council tax",
            "Please pay council tax.",
            now - Duration::days(3),
        ),
    )
    .await;
    let ResponseData::TodoChange { change } = request(
        &fx,
        Request::CreateTodo {
            message_id: original,
            title: "Pay council tax".into(),
            kind: Some("bill".into()),
            due: None,
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    else {
        panic!()
    };
    let task = fx
        .state
        .store
        .get_todo(&change.changed[0].id)
        .await
        .unwrap()
        .unwrap();
    for index in 1..tasks {
        let mut task = task.clone();
        task.id = format!("resolution-fixture-{index}");
        task.dedup_key = task.id.clone();
        fx.state.store.insert_todo(&task).await.unwrap();
    }
    let mut ids = Vec::new();
    for index in 0..messages {
        ids.push(
            put(
                &fx,
                Mail::new(
                    ("Council", "bills@example.com"),
                    "Council tax update",
                    format!("We have an update about council tax reference {index}."),
                    now - Duration::hours(24) + Duration::minutes(index as i64),
                ),
            )
            .await,
        );
    }
    let model = Arc::new(CountedModel {
        calls: AtomicUsize::new(0),
        delay_ms: AtomicUsize::new(0),
        response: std::sync::Mutex::new(r#"{"action":"none","quote":"","due_words":null}"#.into()),
        gate: tokio::sync::Semaphore::new(tokio::sync::Semaphore::MAX_PERMITS),
        fail: std::sync::atomic::AtomicBool::new(false),
    });
    fx.state.llm.replace(model.clone());
    (fx, model, ids)
}

async fn drain(fx: &Fixture, now: DateTime<Utc>) -> bool {
    crate::handler::todo_resolution::drain(&fx.state, now)
        .await
        .unwrap()
}

async fn wait_for_call(model: &CountedModel) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while model.calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

fn gate(model: &CountedModel) {
    model
        .gate
        .forget_permits(tokio::sync::Semaphore::MAX_PERMITS);
}

#[tokio::test]
async fn sync_makes_zero_model_calls_and_ticks_eventually_drain_every_pair() {
    for (messages, tasks) in [(5, 2), (20, 5), (40, 10)] {
        let (fx, model, ids) = ambiguous_pairs(messages, tasks).await;
        let started = std::time::Instant::now();
        todos::scan_messages(&fx.state, &ids).await;
        assert_eq!(model.calls.load(Ordering::SeqCst), 0);
        let queued = messages * tasks;
        assert_eq!(
            fx.state
                .store
                .todo_resolution_queue_counts(Utc::now())
                .await
                .unwrap(),
            (queued as i64, queued as i64)
        );
        let sync_ms = started.elapsed().as_millis();
        let now = Utc::now();
        let mut fingerprints = HashMap::new();
        todos::tick(&fx.state, now, &mut fingerprints)
            .await
            .unwrap();
        assert!(model.calls.load(Ordering::SeqCst) <= 4);
        loop {
            let before = model.calls.load(Ordering::SeqCst);
            let remaining = drain(&fx, now).await;
            assert!(model.calls.load(Ordering::SeqCst) - before <= 4);
            if !remaining {
                break;
            }
        }
        assert_eq!(model.calls.load(Ordering::SeqCst), queued);
        assert_eq!(
            fx.state
                .store
                .todo_resolution_queue_counts(now)
                .await
                .unwrap(),
            (0, 0)
        );
        todos::scan_messages(&fx.state, &ids).await;
        assert!(!drain(&fx, now).await);
        assert_eq!(model.calls.load(Ordering::SeqCst), queued);
        println!("resolution bounded: messages={messages} tasks={tasks} sync_calls=0 sync_ms={sync_ms} total_calls={queued} max_batch_calls=4 elapsed_ms={}", started.elapsed().as_millis());
    }
}

#[tokio::test]
async fn slow_model_shares_one_eight_second_batch_budget_and_keeps_remaining_work() {
    let (fx, model, ids) = ambiguous_pairs(5, 1).await;
    model.delay_ms.store(3_000, Ordering::SeqCst);
    todos::scan_messages(&fx.state, &ids).await;
    let now = Utc::now();
    let started = std::time::Instant::now();
    assert!(drain(&fx, now).await);
    assert_eq!(model.calls.load(Ordering::SeqCst), 3);
    assert!(started.elapsed() < std::time::Duration::from_secs(9));
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(now)
            .await
            .unwrap(),
        (3, 2)
    );
    println!(
        "resolution slow batch: calls=3 elapsed_ms={} pending=3 due=2",
        started.elapsed().as_millis()
    );
    model.delay_ms.store(0, Ordering::SeqCst);
    assert!(!drain(&fx, now).await);
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(now)
            .await
            .unwrap(),
        (1, 0)
    );
    assert!(!drain(&fx, now + Duration::hours(1)).await);
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(now + Duration::hours(1))
            .await
            .unwrap(),
        (0, 0)
    );
    assert_eq!(model.calls.load(Ordering::SeqCst), 6);
}

#[tokio::test]
async fn cancellation_retains_reserved_pair_without_immediate_duplicate_call() {
    let (fx, model, ids) = ambiguous_pairs(1, 1).await;
    gate(&model);
    todos::scan_messages(&fx.state, &ids).await;
    let now = Utc::now();
    let state = fx.state.clone();
    let worker =
        tokio::spawn(async move { crate::handler::todo_resolution::drain(&state, now).await });
    wait_for_call(&model).await;
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(now)
            .await
            .unwrap(),
        (1, 0)
    );
    assert!(!drain(&fx, now).await);
    model.gate.add_permits(1);
    assert!(!drain(&fx, now + Duration::hours(1)).await);
    assert_eq!(model.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(now + Duration::hours(1))
            .await
            .unwrap(),
        (0, 0)
    );
}

#[tokio::test]
async fn changed_pair_enqueued_during_call_survives_old_result_and_becomes_offer() {
    let (fx, model, ids) = ambiguous_pairs(1, 1).await;
    gate(&model);
    todos::scan_messages(&fx.state, &ids).await;
    let now = Utc::now();
    let state = fx.state.clone();
    let worker =
        tokio::spawn(async move { crate::handler::todo_resolution::drain(&state, now).await });
    wait_for_call(&model).await;
    // An unchanged rescan must not clear the reservation.
    todos::scan_messages(&fx.state, &ids).await;
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(now)
            .await
            .unwrap(),
        (1, 0)
    );
    let mut body = fx.state.store.get_body(&ids[0]).await.unwrap().unwrap();
    let quote = "Your council tax obligation has been discharged.";
    body.text_plain = Some(quote.into());
    fx.state.store.insert_body(&body).await.unwrap();
    todos::scan_messages(&fx.state, &ids).await;
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(Utc::now())
            .await
            .unwrap(),
        (1, 1)
    );
    model.gate.add_permits(1);
    worker.await.unwrap().unwrap();
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(Utc::now())
            .await
            .unwrap(),
        (1, 1)
    );
    *model.response.lock().unwrap() =
        serde_json::json!({"action":"complete","quote":quote,"due_words":null}).to_string();
    model.gate.add_permits(1);
    assert!(!drain(&fx, Utc::now()).await);
    let todo = fx
        .state
        .store
        .list_open_todos_for_resolution_context(&fx.account, "bills@example.com", &ThreadId::new())
        .await
        .unwrap()
        .pop()
        .unwrap();
    let offer = fx
        .state
        .store
        .latest_todo_resolution(&todo.id, &fx.account)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.source_quote, quote);
    assert_eq!(offer.source_method, "model");
}

#[tokio::test]
async fn newer_contradiction_fences_inflight_model_and_disabled_model_keeps_queue_idle() {
    let (fx, model, ids) = ambiguous_pairs(1, 1).await;
    todos::scan_messages(&fx.state, &ids).await;
    let now = Utc::now();
    fx.state.llm.replace(Arc::new(mxr_llm::NoopProvider));
    assert!(!drain(&fx, now).await);
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(now)
            .await
            .unwrap(),
        (1, 1)
    );
    fx.state.llm.replace(model.clone());
    gate(&model);
    *model.response.lock().unwrap() = serde_json::json!({"action":"complete","quote":"We have an update about council tax reference 0.","due_words":null}).to_string();
    let state = fx.state.clone();
    let worker =
        tokio::spawn(async move { crate::handler::todo_resolution::drain(&state, now).await });
    wait_for_call(&model).await;
    let newer = put(
        &fx,
        Mail::new(
            ("Council", "bills@example.com"),
            "Council tax failure",
            "Council tax has not been paid.",
            now,
        ),
    )
    .await;
    todos::scan_messages(&fx.state, &[newer]).await;
    model.gate.add_permits(1);
    worker.await.unwrap().unwrap();
    let todo = fx
        .state
        .store
        .list_open_todos_for_resolution_context(&fx.account, "bills@example.com", &ThreadId::new())
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert!(fx
        .state
        .store
        .latest_todo_resolution(&todo.id, &fx.account)
        .await
        .unwrap()
        .is_none());
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn disabling_or_privacy_blocking_between_requests_pauses_remaining_pairs() {
    for privacy in [false, true] {
        let (fx, model, ids) = ambiguous_pairs(5, 1).await;
        gate(&model);
        todos::scan_messages(&fx.state, &ids).await;
        let now = Utc::now();
        let state = fx.state.clone();
        let worker =
            tokio::spawn(async move { crate::handler::todo_resolution::drain(&state, now).await });
        wait_for_call(&model).await;
        if privacy {
            fx.state.llm.replace_feature_providers(
                HashMap::new(),
                HashMap::from([(
                    mxr_llm::LlmFeature::Commitments,
                    "fixture privacy block".into(),
                )]),
            );
        } else {
            fx.state.llm.replace(Arc::new(mxr_llm::NoopProvider));
        }
        model.gate.add_permits(1);
        assert!(!worker.await.unwrap().unwrap());
        assert_eq!(model.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            fx.state
                .store
                .todo_resolution_queue_counts(now)
                .await
                .unwrap(),
            (4, 4)
        );
        let due = fx
            .state
            .store
            .list_due_todo_resolutions(now, 10)
            .await
            .unwrap();
        assert!(due.iter().all(|work| work.attempts == 0));
        assert!(!drain(&fx, now).await);
        fx.state.llm.replace(model.clone());
        fx.state
            .llm
            .replace_feature_providers(HashMap::new(), HashMap::new());
        model.gate.add_permits(4);
        assert!(!drain(&fx, now).await);
        assert_eq!(model.calls.load(Ordering::SeqCst), 5);
    }
}

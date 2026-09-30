//! List-row gists (`GetThreadGists`): the cache answers at once, and the
//! background writer takes people's conversations in the order asked,
//! once each, a bounded number at a time, and says when each is ready.

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::ThreadId;
use mxr_core::types::{
    Address, Envelope, EventSource, MessageBody, MessageDirection, MessageFlags,
};
use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
use mxr_protocol::{
    AiLocalityData, AiSourceData, GistModelData, ThreadGistBatchData, ThreadGistData,
    ThreadGistSkipReasonData,
};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;
use tokio::sync::Semaphore;

const MAYA: &str = "maya@example.com";
const ASK: &str = "Can you confirm who owns the rollout check before Monday?";

/// A model that answers only when the test lets it, and records how many
/// calls ran at once and which prompts it saw, in order.
struct GatedLlm {
    gate: Semaphore,
    active: AtomicUsize,
    max_active: AtomicUsize,
    calls: AtomicUsize,
    prompts: StdMutex<Vec<String>>,
    base_url: Option<String>,
    /// Answer with something that isn't a gist.
    junk: std::sync::atomic::AtomicBool,
}

impl GatedLlm {
    fn new(open: bool, base_url: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            gate: Semaphore::new(if open { Semaphore::MAX_PERMITS } else { 0 }),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            prompts: StdMutex::new(Vec::new()),
            base_url: base_url.map(str::to_string),
            junk: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn release(&self, calls: usize) {
        self.gate.add_permits(calls);
    }

    /// Which of `threads` each prompt was about, in call order.
    fn order(&self, threads: &[(ThreadId, String)]) -> Vec<ThreadId> {
        self.prompts
            .lock()
            .unwrap()
            .iter()
            .filter_map(|prompt| {
                threads
                    .iter()
                    .find(|(_, message_id)| prompt.contains(message_id.as_str()))
                    .map(|(thread, _)| thread.clone())
            })
            .collect()
    }
}

#[async_trait]
impl LlmProvider for GatedLlm {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.prompts.lock().unwrap().push(
            req.messages
                .iter()
                .map(|message| message.content.clone())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(now, Ordering::SeqCst);
        self.gate.acquire().await.expect("gate open").forget();
        self.active.fetch_sub(1, Ordering::SeqCst);
        if self.junk.load(Ordering::SeqCst) {
            return Ok(CompletionResponse {
                content: "I can't help with that.".into(),
                model: "qwen2.5:7b".into(),
                finish_reason: Some("stop".into()),
            });
        }
        Ok(CompletionResponse {
            content: serde_json::json!({
                "gist": "Canary stays at 5% until the dashboard is quiet.",
                "ask": {"summary": "confirm who owns the rollout check", "msg_id": "", "quote": ASK},
            })
            .to_string(),
            model: "qwen2.5:7b".into(),
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
        "qwen2.5:7b"
    }
    fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }
}

/// A conversation from `from` with a body carrying the ask; returns the
/// thread and its message id (which the prompt names).
async fn conversation(fx: &Fixture, from: &str) -> (ThreadId, String) {
    let thread = ThreadId::new();
    let envelope = fx
        .message(&thread, from, ME, Duration::hours(2), None)
        .await;
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some(format!("Rollout risk: watch sync latency.\n{ASK}\n\nMaya")),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        })
        .await
        .unwrap();
    (thread, envelope.id.to_string())
}

async fn gists(fx: &Fixture, thread_ids: &[ThreadId], generate: bool) -> ThreadGistBatchData {
    match request(
        fx,
        Request::GetThreadGists {
            thread_ids: thread_ids.to_vec(),
            generate,
        },
    )
    .await
    {
        ResponseData::ThreadGists { batch } => batch,
        other => panic!("expected ThreadGists, got {other:?}"),
    }
}

/// The next `count` gist events, in arrival order.
async fn ready_events(
    events: &mut tokio::sync::broadcast::Receiver<IpcMessage>,
    count: usize,
) -> Vec<ThreadGistData> {
    let mut out = Vec::new();
    while out.len() < count {
        let message = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .expect("a gist event in time")
            .expect("event channel open");
        if let IpcPayload::Event(DaemonEvent::ThreadGistReady { gist }) = message.payload {
            out.push(gist);
        }
    }
    out
}

async fn wait_for(what: &str, mut check: impl FnMut() -> bool) {
    for _ in 0..500 {
        if check() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

async fn with_concurrency(fx: &Fixture, writers: usize) {
    let mut config = fx.state.config_snapshot();
    config.llm.gist_concurrency = writers;
    fx.state.set_config_for_test(config).await;
}

#[tokio::test]
async fn cached_gists_come_back_at_once_without_a_model_call() {
    let fx = Fixture::new().await;
    let (thread, _) = conversation(&fx, MAYA).await;
    let llm = GatedLlm::new(true, None);
    fx.state.llm.replace(llm.clone());
    // The reader's request writes the gist once.
    request(
        &fx,
        Request::GetThreadGist {
            thread_id: thread.clone(),
            refresh: false,
        },
    )
    .await;
    assert_eq!(llm.calls.load(Ordering::SeqCst), 1);

    // Close the gate: a model call now would hang the request.
    let closed = GatedLlm::new(false, None);
    fx.state.llm.replace(closed.clone());
    let batch = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        gists(&fx, std::slice::from_ref(&thread), true),
    )
    .await
    .expect("the batch never waits on a model");
    assert_eq!(batch.model, GistModelData::Available);
    assert_eq!(batch.gists.len(), 1);
    let gist = &batch.gists[0];
    assert!(gist.from_cache);
    assert_eq!(
        gist.ask
            .as_ref()
            .and_then(|ask| ask.quote.as_ref())
            .map(|q| q.text.as_str()),
        Some(ASK),
        "the verified quote travels with the list gist"
    );
    assert!(batch.queued.is_empty());
    assert_eq!(closed.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn without_generate_a_missing_gist_is_only_reported() {
    let fx = Fixture::new().await;
    let (thread, _) = conversation(&fx, MAYA).await;
    let llm = GatedLlm::new(true, None);
    fx.state.llm.replace(llm.clone());
    let batch = gists(&fx, std::slice::from_ref(&thread), false).await;
    assert!(batch.gists.is_empty() && batch.queued.is_empty());
    assert_eq!(
        batch.skipped[0].reason,
        ThreadGistSkipReasonData::NotGenerated
    );
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(llm.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn with_no_model_nothing_is_queued_or_called() {
    let fx = Fixture::new().await;
    let (thread, _) = conversation(&fx, MAYA).await;
    let batch = gists(&fx, std::slice::from_ref(&thread), true).await;
    assert_eq!(batch.model, GistModelData::Disabled);
    assert!(batch.gists.is_empty() && batch.queued.is_empty() && batch.skipped.is_empty());
}

#[tokio::test]
async fn only_conversations_with_people_go_to_the_model() {
    let fx = Fixture::new().await;
    let (person, _) = conversation(&fx, MAYA).await;
    let (newsletter, _) = conversation(&fx, "digest@news.example.com").await;
    let (receipts, _) = conversation(&fx, "receipts@shop.example.com").await;
    let llm = GatedLlm::new(true, None);
    fx.state.llm.replace(llm.clone());
    let mut events = fx.state.event_tx.subscribe();

    let batch = gists(
        &fx,
        &[newsletter.clone(), person.clone(), receipts.clone()],
        true,
    )
    .await;
    assert_eq!(batch.queued, vec![person.clone()]);
    let skipped: Vec<_> = batch
        .skipped
        .iter()
        .map(|skip| (skip.thread_id.clone(), skip.reason))
        .collect();
    assert_eq!(
        skipped,
        vec![
            (newsletter, ThreadGistSkipReasonData::NotPeople),
            (receipts, ThreadGistSkipReasonData::NotPeople),
        ]
    );
    let ready = ready_events(&mut events, 1).await;
    assert_eq!(ready[0].thread_id, person);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        llm.calls.load(Ordering::SeqCst),
        1,
        "one call, for the person"
    );
}

#[tokio::test]
async fn gists_are_written_in_the_order_asked_newest_request_first_each_once() {
    let fx = Fixture::new().await;
    let mut threads = Vec::new();
    for _ in 0..4 {
        threads.push(conversation(&fx, MAYA).await);
    }
    let ids: Vec<ThreadId> = threads.iter().map(|(id, _)| id.clone()).collect();
    let llm = GatedLlm::new(false, None);
    fx.state.llm.replace(llm.clone());
    let mut events = fx.state.event_tx.subscribe();

    // The first request's first row goes straight to the one writer.
    let first = gists(&fx, &ids[0..2], true).await;
    assert_eq!(first.queued, ids[0..2].to_vec());
    wait_for("the first call", || llm.calls.load(Ordering::SeqCst) == 1).await;

    // The user scrolled: the new rows jump the queue, and asking again for
    // one already queued or being written never adds it twice.
    let second = gists(&fx, &[ids[3].clone(), ids[1].clone(), ids[0].clone()], true).await;
    assert_eq!(second.queued, vec![ids[3].clone(), ids[1].clone()]);
    assert_eq!(
        second.in_flight,
        vec![ids[0].clone()],
        "being written, not queued again"
    );
    let (pending, in_flight, writers) = fx.state.gist_queue.snapshot();
    assert_eq!(pending, vec![ids[3].clone(), ids[1].clone()]);
    assert_eq!(in_flight, vec![ids[0].clone()]);
    assert_eq!(writers, 1);

    let third = gists(&fx, &[ids[2].clone(), ids[2].clone()], true).await;
    assert_eq!(
        third.queued,
        vec![ids[2].clone()],
        "duplicates in a request collapse"
    );

    llm.release(4);
    let ready = ready_events(&mut events, 4).await;
    let order = llm.order(&threads);
    assert_eq!(
        order,
        vec![
            ids[0].clone(),
            ids[2].clone(),
            ids[3].clone(),
            ids[1].clone()
        ]
    );
    assert_eq!(
        ready
            .iter()
            .map(|gist| gist.thread_id.clone())
            .collect::<Vec<_>>(),
        order
    );
    assert_eq!(
        llm.calls.load(Ordering::SeqCst),
        4,
        "each conversation once"
    );
    wait_for("the writer to retire", || {
        fx.state.gist_queue.snapshot().2 == 0
    })
    .await;
}

#[tokio::test]
async fn writers_are_bounded_by_gist_concurrency() {
    for writers in [1, 2] {
        let fx = Fixture::new().await;
        with_concurrency(&fx, writers).await;
        let mut ids = Vec::new();
        for _ in 0..5 {
            ids.push(conversation(&fx, MAYA).await.0);
        }
        let llm = GatedLlm::new(false, None);
        fx.state.llm.replace(llm.clone());
        let mut events = fx.state.event_tx.subscribe();

        gists(&fx, &ids, true).await;
        wait_for("the writers to start", || {
            llm.active.load(Ordering::SeqCst) == writers
        })
        .await;
        // More requests while they are busy start no extra writers.
        gists(&fx, &ids, true).await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(llm.active.load(Ordering::SeqCst), writers);

        llm.release(5);
        let ready = ready_events(&mut events, 5).await;
        assert_eq!(ready.len(), 5);
        assert_eq!(llm.max_active.load(Ordering::SeqCst), writers);
        assert_eq!(llm.calls.load(Ordering::SeqCst), 5);
    }
}

#[tokio::test]
async fn a_cloud_model_without_opt_in_writes_list_gists_from_the_conversation_only() {
    let fx = Fixture::new().await;
    let (thread, _) = conversation(&fx, MAYA).await;
    let mut config = fx.state.config_snapshot();
    config.llm.enabled = true;
    config.llm.base_url = "https://api.example-cloud.com/v1".into();
    config.llm.allow_cloud_relationship_data = false;
    fx.state.set_config_for_test(config).await;
    let llm = GatedLlm::new(true, Some("https://api.example-cloud.com/v1"));
    fx.state.llm.replace(llm.clone());
    let mut events = fx.state.event_tx.subscribe();

    gists(&fx, std::slice::from_ref(&thread), true).await;
    let gist = ready_events(&mut events, 1).await.remove(0);
    let provenance = gist.provenance.expect("provenance on every list gist");
    assert_eq!(provenance.locality, AiLocalityData::Cloud);
    assert_eq!(provenance.sources, vec![AiSourceData::ThisThread]);
    assert!(!llm.prompts.lock().unwrap()[0].contains("History with"));
}

#[tokio::test]
async fn a_privacy_block_queues_nothing() {
    let fx = Fixture::new().await;
    let (thread, _) = conversation(&fx, MAYA).await;
    let llm = GatedLlm::new(true, None);
    fx.state.llm.replace(llm.clone());
    fx.state.llm.replace_feature_providers(
        std::collections::HashMap::new(),
        std::collections::HashMap::from([(
            mxr_llm::LlmFeature::Summarize,
            "cloud endpoint without opt-in".to_string(),
        )]),
    );
    let batch = gists(&fx, std::slice::from_ref(&thread), true).await;
    assert_eq!(batch.model, GistModelData::Blocked);
    assert!(batch.queued.is_empty());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(llm.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_batch_is_capped() {
    let fx = Fixture::new().await;
    let ids: Vec<ThreadId> = (0..=mxr_protocol::THREAD_GISTS_MAX_BATCH)
        .map(|_| ThreadId::new())
        .collect();
    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetThreadGists {
            thread_ids: ids,
            generate: false,
        }),
    };
    match handle_request(&fx.state, &msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("at most 100"), "{message}");
        }
        other => panic!("expected an error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_conversation_the_model_failed_on_waits_before_it_is_tried_again() {
    let fx = Fixture::new().await;
    let (thread, _) = conversation(&fx, MAYA).await;
    let llm = GatedLlm::new(true, None);
    llm.junk.store(true, Ordering::SeqCst);
    fx.state.llm.replace(llm.clone());

    let first = gists(&fx, std::slice::from_ref(&thread), true).await;
    assert_eq!(first.queued, vec![thread.clone()]);
    wait_for("the failed attempt", || {
        let (pending, in_flight, _) = fx.state.gist_queue.snapshot();
        llm.calls.load(Ordering::SeqCst) == 1 && pending.is_empty() && in_flight.is_empty()
    })
    .await;

    let again = gists(&fx, std::slice::from_ref(&thread), true).await;
    assert!(again.queued.is_empty());
    assert_eq!(
        again.skipped[0].reason,
        ThreadGistSkipReasonData::RecentlyFailed
    );

    // A new message changes the conversation: it is worth another try.
    fx.message(&thread, MAYA, ME, Duration::minutes(1), None)
        .await;
    let changed = gists(&fx, std::slice::from_ref(&thread), true).await;
    assert_eq!(changed.queued, vec![thread]);
}

/// Generation time per conversation against a real local model, with the
/// settings BK runs (Ollama on this machine, one writer, the background
/// timeout). Ignored by default: it needs the model running.
///
/// `MXR_GIST_BENCH_MODEL=gemma4:latest scripts/cargo-test -p mxr --lib \
///   list_gist_throughput -- --ignored --nocapture`
#[tokio::test]
#[ignore = "needs a local model; set MXR_GIST_BENCH_MODEL"]
async fn list_gist_throughput_against_a_local_model() {
    let Ok(model) = std::env::var("MXR_GIST_BENCH_MODEL") else {
        eprintln!("MXR_GIST_BENCH_MODEL not set; skipping");
        return;
    };
    let fx = Fixture::new().await;
    let mut config = fx.state.config_snapshot();
    config.llm.enabled = true;
    config.llm.base_url = "http://localhost:11434/v1".into();
    config.llm.model = model.clone();
    config.llm.context_window = 131_072;
    config.llm.request_timeout_secs = 120;
    config.llm.background_request_timeout_secs = 120;
    fx.state.set_config_for_test(config).await;

    let paragraphs = [
        "Thanks for sending the draft over. I read it on the train this morning and the structure works; the second section still reads like two ideas stitched together, and the numbers in the table don't match the ones in the summary.",
        "Quick update from our side: legal signed off on the data processing terms yesterday, so the only thing left before we can start the pilot is the security questionnaire. Priya has most of it filled in.",
        "Can you confirm whether the Thursday slot still works for the walkthrough? If not, Friday after 2pm is open for all three of us.",
        "I'm also attaching the revised budget. We cut the travel line by a third and moved the contractor hours into Q1, which should keep us under the cap the board set in March.",
    ];
    let people = [
        "maya@example.com",
        "tom@acme.example",
        "priya@example.org",
        "li@partner.example",
    ];
    let mut threads = Vec::new();
    for (index, messages) in [1usize, 1, 2, 3, 4, 6].iter().enumerate() {
        let thread = ThreadId::new();
        for position in 0..*messages {
            let from = if position % 2 == 1 {
                ME
            } else {
                people[index % people.len()]
            };
            let envelope = fx
                .message(
                    &thread,
                    from,
                    if from == ME {
                        people[index % people.len()]
                    } else {
                        ME
                    },
                    Duration::hours((messages - position) as i64),
                    None,
                )
                .await;
            let body = (0..=position % 3)
                .map(|p| paragraphs[(index + position + p) % paragraphs.len()])
                .collect::<Vec<_>>()
                .join("\n\n");
            fx.state
                .store
                .insert_body(&MessageBody {
                    message_id: envelope.id.clone(),
                    text_plain: Some(body),
                    text_html: None,
                    attachments: vec![],
                    fetched_at: chrono::Utc::now(),
                    metadata: Default::default(),
                })
                .await
                .unwrap();
        }
        threads.push(thread);
    }

    let mut events = fx.state.event_tx.subscribe();
    let started = std::time::Instant::now();
    let batch = gists(&fx, &threads, true).await;
    assert_eq!(batch.queued.len(), threads.len(), "{batch:?}");
    let mut last = started;
    let mut times = Vec::new();
    for _ in 0..threads.len() {
        let gist = ready_events_within(&mut events, 1, 600).await.remove(0);
        let now = std::time::Instant::now();
        times.push(now.duration_since(last).as_secs_f64());
        last = now;
        eprintln!(
            "{:.1}s  ask={}  {}",
            times.last().unwrap(),
            gist.ask.is_some(),
            gist.gist.unwrap_or_default()
        );
    }
    let total = started.elapsed().as_secs_f64();
    let warm = &times[1..];
    eprintln!(
        "model={model} conversations={} total={total:.1}s first={:.1}s warm_mean={:.1}s warm_max={:.1}s",
        times.len(),
        times[0],
        warm.iter().sum::<f64>() / warm.len().max(1) as f64,
        warm.iter().copied().fold(0.0, f64::max)
    );
}

async fn ready_events_within(
    events: &mut tokio::sync::broadcast::Receiver<IpcMessage>,
    count: usize,
    seconds: u64,
) -> Vec<ThreadGistData> {
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while out.len() < count {
        let message = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("gists in time")
            .expect("event channel open");
        if let IpcPayload::Event(DaemonEvent::ThreadGistReady { gist }) = message.payload {
            out.push(gist);
        }
    }
    out
}

#[tokio::test]
async fn a_gist_that_went_stale_during_the_call_is_not_announced_and_is_written_again() {
    let fx = Fixture::new().await;
    let (thread, _) = conversation(&fx, MAYA).await;
    let llm = GatedLlm::new(false, None);
    fx.state.llm.replace(llm.clone());
    let mut events = fx.state.event_tx.subscribe();

    gists(&fx, std::slice::from_ref(&thread), true).await;
    wait_for("the first call", || llm.calls.load(Ordering::SeqCst) == 1).await;
    // A reply lands while the model is writing.
    let reply = fx
        .message(&thread, MAYA, ME, Duration::minutes(-1), None)
        .await;
    llm.release(1);
    wait_for("the second call", || llm.calls.load(Ordering::SeqCst) == 2).await;
    llm.release(1);

    let ready = ready_events(&mut events, 1).await;
    assert_eq!(
        ready[0].newest_message_id.as_ref(),
        Some(&reply.id),
        "only the gist for the conversation as it is now is announced"
    );
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    while let Ok(message) = events.try_recv() {
        assert!(
            !matches!(
                message.payload,
                IpcPayload::Event(DaemonEvent::ThreadGistReady { .. })
            ),
            "the stale gist never follows"
        );
    }
    // The cache holds the current gist, not the stale one.
    let cached = gists(&fx, std::slice::from_ref(&thread), false).await;
    assert_eq!(cached.gists[0].newest_message_id.as_ref(), Some(&reply.id));
}

#[tokio::test]
async fn a_hundred_conversation_request_reports_what_was_queued_and_what_was_not() {
    let fx = Fixture::new().await;
    let mut ids = Vec::new();
    for _ in 0..mxr_protocol::THREAD_GISTS_MAX_BATCH {
        ids.push(conversation(&fx, MAYA).await.0);
    }
    let llm = GatedLlm::new(false, None);
    fx.state.llm.replace(llm.clone());

    let batch = gists(&fx, &ids, true).await;
    let full: Vec<ThreadId> = batch
        .skipped
        .iter()
        .filter(|skip| skip.reason == ThreadGistSkipReasonData::QueueFull)
        .map(|skip| skip.thread_id.clone())
        .collect();
    assert_eq!(
        batch.queued,
        ids[..64].to_vec(),
        "first asked, first queued"
    );
    assert!(batch.in_flight.is_empty());
    assert_eq!(
        full,
        ids[64..].to_vec(),
        "the rest are reported as not queued"
    );
    wait_for("the writer", || llm.calls.load(Ordering::SeqCst) == 1).await;
    let (pending, in_flight, _) = fx.state.gist_queue.snapshot();
    assert_eq!(pending.len() + in_flight.len(), 64);
    llm.release(64);
}

/// A model that, each time it is asked about one conversation, waits for
/// the test to add a message to it first: that conversation's gist always
/// goes stale mid-call.
struct BusyThreadLlm {
    marker: String,
    calls_on_busy: AtomicUsize,
    calls: AtomicUsize,
    hook: tokio::sync::mpsc::UnboundedSender<tokio::sync::oneshot::Sender<()>>,
}

#[async_trait]
impl LlmProvider for BusyThreadLlm {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let prompt: String = req.messages.iter().map(|m| m.content.clone()).collect();
        if prompt.contains(&self.marker) {
            self.calls_on_busy.fetch_add(1, Ordering::SeqCst);
            let (done, wait) = tokio::sync::oneshot::channel();
            let _ = self.hook.send(done);
            let _ = wait.await;
        }
        Ok(CompletionResponse {
            content: r#"{"gist": "Canary stays at 5%.", "ask": null}"#.into(),
            model: "qwen2.5:7b".into(),
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
        "qwen2.5:7b"
    }
}

#[tokio::test]
async fn a_conversation_that_keeps_changing_never_starves_the_others() {
    let fx = Fixture::new().await;
    let (busy, busy_first_message) = conversation(&fx, MAYA).await;
    let (b, _) = conversation(&fx, MAYA).await;
    let (c, _) = conversation(&fx, MAYA).await;
    let (hook, mut asked) = tokio::sync::mpsc::unbounded_channel();
    let llm = Arc::new(BusyThreadLlm {
        marker: busy_first_message,
        calls_on_busy: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
        hook,
    });
    fx.state.llm.replace(llm.clone());
    let mut events = fx.state.event_tx.subscribe();

    // Someone writes in `busy` during every call about it.
    let writer_fx = fx.state.clone();
    let busy_thread = busy.clone();
    let account = fx.account.clone();
    let inbox = fx.inbox.clone();
    let feeder = tokio::spawn(async move {
        let mut minutes = 1;
        while let Some(done) = asked.recv().await {
            let id = mxr_core::id::MessageId::new();
            let envelope = Envelope {
                id: id.clone(),
                account_id: account.clone(),
                provider_id: format!("busy-{id}"),
                thread_id: busy_thread.clone(),
                message_id_header: Some(format!("<{id}@example.com>")),
                in_reply_to: None,
                references: vec![],
                from: Address {
                    name: None,
                    email: MAYA.into(),
                },
                to: vec![Address {
                    name: None,
                    email: ME.into(),
                }],
                cc: vec![],
                bcc: vec![],
                subject: "Re: Launch plan".into(),
                date: chrono::Utc::now() + Duration::minutes(minutes),
                flags: MessageFlags::empty(),
                snippet: String::new(),
                has_attachments: false,
                size_bytes: 10,
                unsubscribe: UnsubscribeMethod::None,
                link_count: 0,
                body_word_count: 0,
                label_provider_ids: vec![],
                keywords: std::collections::BTreeSet::new(),
            };
            minutes += 1;
            writer_fx
                .store
                .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
                .await
                .unwrap();
            let _ = writer_fx
                .store
                .set_message_labels(&id, std::slice::from_ref(&inbox), EventSource::User)
                .await;
            let _ = done.send(());
        }
    });

    let batch = gists(&fx, &[busy.clone(), b.clone(), c.clone()], true).await;
    assert_eq!(batch.queued, vec![busy.clone(), b.clone(), c.clone()]);
    let ready = ready_events(&mut events, 2).await;
    let written: HashSet<ThreadId> = ready.iter().map(|g| g.thread_id.clone()).collect();
    assert_eq!(
        written,
        HashSet::from([b.clone(), c.clone()]),
        "the others are written"
    );

    wait_for("the busy conversation to back off", || {
        let (pending, in_flight, _) = fx.state.gist_queue.snapshot();
        pending.is_empty() && in_flight.is_empty()
    })
    .await;
    assert_eq!(
        llm.calls_on_busy.load(Ordering::SeqCst),
        1 + 2,
        "one try plus two retries, then it waits"
    );
    let again = gists(&fx, std::slice::from_ref(&busy), true).await;
    assert!(again.queued.is_empty());
    assert_eq!(
        again.skipped[0].reason,
        ThreadGistSkipReasonData::RecentlyFailed
    );
    feeder.abort();
}

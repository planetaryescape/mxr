use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::ThreadId;
use mxr_core::types::MessageBody;
use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
use mxr_protocol::{
    AiLocalityData, AiSourceData, GistModelData, ThreadGistBatchData, ThreadGistData,
    ThreadGistSkipReasonData, ThreadGistStatusData,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;
use tokio::sync::Semaphore;

const MAYA: &str = "maya@example.com";
const ASK: &str = "Can you confirm who owns the rollout check before Monday?";

struct GatedLlm {
    gate: Semaphore,
    active: AtomicUsize,
    max_active: AtomicUsize,
    calls: AtomicUsize,
    prompts: StdMutex<Vec<String>>,
    base_url: Option<String>,
    junk: std::sync::atomic::AtomicBool,
    answer: StdMutex<Option<String>>,
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
            answer: StdMutex::new(None),
        })
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
        let fixed = self.answer.lock().unwrap().clone();
        if let Some(content) = fixed {
            return Ok(CompletionResponse {
                content,
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

async fn conversation(fx: &Fixture, from: &str) -> (ThreadId, String) {
    let thread = ThreadId::new();
    let envelope = fx
        .message(&thread, from, ME, Duration::hours(2), None)
        .await;
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some(format!(
                "Rollout risk: watch sync latency while the canary sits at 5%.\n{ASK}\n\nMaya"
            )),
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
    gists_for(fx, thread_ids, generate, false).await
}

async fn gists_for(
    fx: &Fixture,
    thread_ids: &[ThreadId],
    generate: bool,
    automated: bool,
) -> ThreadGistBatchData {
    match request(
        fx,
        Request::GetThreadGists {
            thread_ids: thread_ids.to_vec(),
            generate,
            automated,
        },
    )
    .await
    {
        ResponseData::ThreadGists { batch } => batch,
        other => panic!("expected ThreadGists, got {other:?}"),
    }
}

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

#[tokio::test]
async fn cached_gists_come_back_at_once_without_a_model_call() {
    let fx = Fixture::new().await;
    let (thread, _) = conversation(&fx, MAYA).await;
    let llm = GatedLlm::new(true, None);
    fx.state.llm.replace(llm.clone());
    request(
        &fx,
        Request::GetThreadGist {
            thread_id: thread.clone(),
            refresh: false,
        },
    )
    .await;
    assert_eq!(llm.calls.load(Ordering::SeqCst), 1);

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
async fn a_gist_with_a_number_the_mail_does_not_write_is_dropped_not_cached() {
    let fx = Fixture::new().await;
    let (thread, _) = conversation(&fx, MAYA).await;
    let llm = GatedLlm::new(true, None);
    *llm.answer.lock().unwrap() =
        Some(r#"{"gist": "Canary stays at 7% until the dashboard is quiet.", "ask": null}"#.into());
    fx.state.llm.replace(llm.clone());
    let read = || async {
        match request(
            &fx,
            Request::GetThreadGist {
                thread_id: thread.clone(),
                refresh: false,
            },
        )
        .await
        {
            ResponseData::ThreadGist { gist } => gist,
            other => panic!("expected ThreadGist, got {other:?}"),
        }
    };
    let first = read().await;
    assert_eq!(first.status, ThreadGistStatusData::Failed);
    assert!(first.gist.is_none());
    *llm.answer.lock().unwrap() = None;
    let second = read().await;
    assert_eq!(second.status, ThreadGistStatusData::Ready);
    assert_eq!(llm.calls.load(Ordering::SeqCst), 2);
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

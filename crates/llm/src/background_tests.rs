//! Background work through an endpoint outage: the breaker, its cheap check,
//! the ledger, recovery and the state saved across restarts.

use super::background::{BREAKER_MIN_COOLDOWN, BREAKER_THRESHOLD, LEDGER_MAX_ATTEMPTS};
use super::*;
use std::sync::atomic::AtomicU32;
use std::time::SystemTime;

const FEATURE: LlmFeature = LlmFeature::RelationshipSummary;
const FP: &str = "fingerprint";

/// An endpoint that is up or down, with or without a cheap check, recording
/// which inputs reached the model.
struct Switchable {
    up: AtomicBool,
    with_probe: bool,
    probe_delay: Duration,
    probes: AtomicU32,
    sent: Mutex<Vec<String>>,
}

impl Switchable {
    fn down(with_probe: bool) -> Arc<Self> {
        Arc::new(Self {
            up: AtomicBool::new(false),
            with_probe,
            probe_delay: Duration::ZERO,
            probes: AtomicU32::new(0),
            sent: Mutex::new(Vec::new()),
        })
    }

    fn slow_probe(delay: Duration) -> Arc<Self> {
        Arc::new(Self {
            up: AtomicBool::new(false),
            with_probe: true,
            probe_delay: delay,
            probes: AtomicU32::new(0),
            sent: Mutex::new(Vec::new()),
        })
    }

    fn set_up(&self, up: bool) {
        self.up.store(up, Ordering::SeqCst);
    }

    fn sent(&self) -> Vec<String> {
        self.sent.lock().unwrap().clone()
    }

    fn probes(&self) -> u32 {
        self.probes.load(Ordering::SeqCst)
    }

    fn reply(&self) -> Result<(), LlmError> {
        if self.up.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(LlmError::Unreachable("connection refused".into()))
        }
    }
}

#[async_trait]
impl LlmProvider for Switchable {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.sent
            .lock()
            .unwrap()
            .push(req.messages[0].content.clone());
        self.reply().map(|()| CompletionResponse {
            content: "ok".into(),
            model: "switchable".into(),
            finish_reason: None,
        })
    }

    async fn probe(&self, _timeout: Duration) -> Option<Result<(), LlmError>> {
        if !self.with_probe {
            return None;
        }
        self.probes.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.probe_delay).await;
        Some(self.reply())
    }

    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            context_window: 8192,
            supports_streaming: false,
        }
    }

    fn model_name(&self) -> &str {
        "switchable"
    }
}

fn ask(input: &str) -> CompletionRequest {
    CompletionRequest {
        messages: vec![ChatMessage::user(input)],
        max_tokens: None,
        temperature: None,
    }
}

/// What a background worker does with each input it is handed.
async fn run_inputs(feature: &FeatureLlmRuntime, keys: &[String]) {
    for key in keys {
        if !feature.background_attempt_due(key, FP) {
            continue;
        }
        match feature.complete_background(ask(key)).await {
            Ok(_) => feature.record_background_attempt(key, FP, true),
            Err(error) => feature.record_background_failure(key, FP, &error),
        }
    }
}

/// What the daemon's recovery loop does on each tick.
async fn recovery_tick(runtime: &Arc<LlmRuntime>) -> bool {
    if runtime.background_recovery_due(&[FEATURE]).await.is_empty() {
        return false;
    }
    let keys = runtime.take_deferred_background(FEATURE);
    run_inputs(&runtime.for_feature(FEATURE), &keys).await;
    true
}

async fn open_the_breaker(feature: &FeatureLlmRuntime) {
    for n in 0..BREAKER_THRESHOLD {
        let error = feature
            .complete_background(ask(&format!("opener-{n}")))
            .await
            .unwrap_err();
        assert!(error.is_endpoint_failure(), "{error:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_two_day_outage_gives_up_no_input_and_runs_each_once_after() {
    let endpoint = Switchable::down(true);
    let runtime = Arc::new(LlmRuntime::new(endpoint.clone()));
    let feature = runtime.for_feature(FEATURE);
    let inputs: Vec<String> = (0..40).map(|n| format!("contact-{n}")).collect();

    run_inputs(&feature, &inputs).await;
    assert_eq!(
        endpoint.sent().len(),
        BREAKER_THRESHOLD as usize,
        "the breaker opened after five failures and turned the rest away"
    );

    let tick = Duration::from_secs(30);
    let outage = Duration::from_secs(48 * 60 * 60);
    let mut elapsed = Duration::ZERO;
    while elapsed < outage {
        tokio::time::advance(tick).await;
        elapsed += tick;
        // New mail for a few contacts each hour sends them through again.
        if elapsed.as_secs().is_multiple_of(3600) {
            let hour = (elapsed.as_secs() / 3600) as usize;
            let some: Vec<String> = inputs
                .iter()
                .cycle()
                .skip(hour * 3)
                .take(3)
                .cloned()
                .collect();
            run_inputs(&feature, &some).await;
        }
        assert!(
            !recovery_tick(&runtime).await,
            "no recovery pass while the endpoint is down ({elapsed:?})"
        );
    }
    assert_eq!(
        endpoint.sent().len(),
        BREAKER_THRESHOLD as usize,
        "after the breaker opened only the cheap check reached the endpoint"
    );
    // One check per cooldown: 5 minutes, then every 10 for two days.
    assert!(
        (280..=290).contains(&endpoint.probes()),
        "{} probes",
        endpoint.probes()
    );
    for key in &inputs {
        assert!(
            feature.background_attempt_due(key, FP),
            "{key} was charged for the outage"
        );
    }

    let recovered = runtime.subscribe_background_recovery();
    endpoint.set_up(true);
    endpoint.sent.lock().unwrap().clear();
    let back = Instant::now();
    while !recovery_tick(&runtime).await {
        tokio::time::advance(tick).await;
        assert!(
            back.elapsed() <= Duration::from_secs(10 * 60) + tick,
            "recovery waits at most the longest cooldown"
        );
    }
    assert!(recovered.has_changed().unwrap(), "recovery was signalled");

    let mut sent = endpoint.sent();
    assert_eq!(sent.len(), inputs.len(), "{sent:?}");
    sent.sort();
    sent.dedup();
    assert_eq!(sent.len(), inputs.len(), "each input ran exactly once");
    for key in &inputs {
        assert!(!feature.background_attempt_due(key, FP), "{key} succeeded");
    }
    assert!(runtime.take_deferred_background(FEATURE).is_empty());
}

#[tokio::test(start_paused = true)]
async fn one_caller_checks_the_endpoint_while_the_rest_are_turned_away() {
    let endpoint = Switchable::slow_probe(Duration::from_secs(5));
    let runtime = Arc::new(LlmRuntime::new(endpoint.clone()));
    let feature = runtime.for_feature(FEATURE);
    open_the_breaker(&feature).await;
    feature.record_background_failure(
        "waiting",
        FP,
        &LlmError::CircuitOpen {
            retry_after_secs: 1,
        },
    );
    tokio::time::advance(BREAKER_MIN_COOLDOWN).await;
    endpoint.set_up(true);

    let (first, second, recovery) = tokio::join!(
        feature.complete_background(ask("first")),
        feature.complete_background(ask("second")),
        runtime.background_recovery_due(&[FEATURE]),
    );
    first.unwrap();
    assert!(
        matches!(second, Err(LlmError::CircuitOpen { .. })),
        "{second:?}"
    );
    assert!(
        recovery.is_empty(),
        "the recovery loop waits for the check in flight"
    );
    assert_eq!(endpoint.probes(), 1);
    assert_eq!(endpoint.sent().last().map(String::as_str), Some("first"));
    assert_eq!(runtime.background_recovery_due(&[FEATURE]).await, [FEATURE]);
}

#[tokio::test(start_paused = true)]
async fn a_caller_that_gives_up_mid_check_frees_the_check() {
    let endpoint = Switchable::slow_probe(Duration::from_secs(5));
    let runtime = Arc::new(LlmRuntime::new(endpoint.clone()));
    let feature = runtime.for_feature(FEATURE);
    open_the_breaker(&feature).await;
    tokio::time::advance(BREAKER_MIN_COOLDOWN).await;

    let gave_up = tokio::time::timeout(
        Duration::from_secs(1),
        feature.complete_background(ask("impatient")),
    )
    .await;
    assert!(gave_up.is_err());

    endpoint.set_up(true);
    feature.complete_background(ask("next")).await.unwrap();
    assert_eq!(endpoint.probes(), 2, "the next caller ran its own check");
}

#[tokio::test(start_paused = true)]
async fn the_pause_reports_when_it_ends() {
    let endpoint = Switchable::down(true);
    let runtime = Arc::new(LlmRuntime::new(endpoint.clone()));
    assert_eq!(runtime.background_paused_until(), None);
    open_the_breaker(&runtime.for_feature(FEATURE)).await;

    let until = runtime.background_paused_until().expect("paused");
    let left = until.duration_since(SystemTime::now()).unwrap();
    assert!(
        left <= BREAKER_MIN_COOLDOWN && left > BREAKER_MIN_COOLDOWN - Duration::from_secs(5),
        "{left:?}"
    );
    tokio::time::advance(BREAKER_MIN_COOLDOWN).await;
    assert_eq!(runtime.background_paused_until(), None, "due a check");
}

#[tokio::test(start_paused = true)]
async fn a_failed_check_spends_no_input_and_doubles_the_cooldown() {
    let endpoint = Switchable::down(true);
    let runtime = Arc::new(LlmRuntime::new(endpoint.clone()));
    let feature = runtime.for_feature(FEATURE);
    open_the_breaker(&feature).await;
    tokio::time::advance(BREAKER_MIN_COOLDOWN).await;

    let error = feature.complete_background(ask("input")).await.unwrap_err();
    assert!(
        matches!(error, LlmError::CircuitOpen { retry_after_secs } if retry_after_secs == 600),
        "{error:?}"
    );
    assert_eq!(endpoint.probes(), 1);
    assert!(
        !endpoint.sent().contains(&"input".to_string()),
        "the input never reached the endpoint"
    );
}

/// The endpoint answers, but not usably: a refusal or an empty reply.
struct Unhelpful {
    calls: AtomicU32,
}

#[async_trait]
impl LlmProvider for Unhelpful {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(LlmError::Empty)
    }

    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            context_window: 8192,
            supports_streaming: false,
        }
    }

    fn model_name(&self) -> &str {
        "unhelpful"
    }
}

#[tokio::test(start_paused = true)]
async fn a_failing_input_backs_off_and_gives_up_but_never_opens_the_breaker() {
    let endpoint = Arc::new(Unhelpful { calls: 0.into() });
    let runtime = Arc::new(LlmRuntime::new(endpoint.clone()));
    let feature = runtime.for_feature(FEATURE);

    for _ in 0..BREAKER_THRESHOLD * 2 {
        let error = feature.complete_background(ask("x")).await.unwrap_err();
        assert!(matches!(error, LlmError::Empty), "{error:?}");
        assert!(!error.is_endpoint_failure());
    }
    assert_eq!(
        endpoint.calls.load(Ordering::SeqCst),
        BREAKER_THRESHOLD * 2,
        "the endpoint answered every time, so the breaker stayed closed"
    );

    for attempt in 1..=LEDGER_MAX_ATTEMPTS {
        assert!(feature.background_attempt_due("bad", FP), "try {attempt}");
        feature.record_background_failure("bad", FP, &LlmError::Empty);
        assert!(!feature.background_attempt_due("bad", FP), "backing off");
        tokio::time::advance(Duration::from_secs(24 * 60 * 60)).await;
    }
    assert!(!feature.background_attempt_due("bad", FP), "given up");
    assert!(runtime.take_deferred_background(FEATURE).is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_call_from_before_a_config_reload_does_not_count_against_the_new_endpoint() {
    struct SlowDown;
    #[async_trait]
    impl LlmProvider for SlowDown {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Err(LlmError::Unreachable("old endpoint".into()))
        }
        fn capabilities(&self) -> LlmCapabilities {
            LlmCapabilities {
                context_window: 8192,
                supports_streaming: false,
            }
        }
        fn model_name(&self) -> &str {
            "slow"
        }
    }

    let runtime = Arc::new(LlmRuntime::new(Arc::new(SlowDown)));
    let feature = runtime.for_feature(FEATURE);
    let fresh = Switchable::down(false);
    let (stale, ()) = tokio::join!(feature.complete_background(ask("stale")), async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        runtime.replace(fresh.clone());
    });
    assert!(stale.unwrap_err().is_endpoint_failure());

    // Four failures on the new endpoint leave the breaker closed; the stale
    // failure would have made them five.
    for n in 0..BREAKER_THRESHOLD - 1 {
        let _ = feature.complete_background(ask(&format!("new-{n}"))).await;
    }
    let _ = feature.complete_background(ask("fifth")).await;
    assert_eq!(fresh.sent().last().map(String::as_str), Some("fifth"));
    assert!(matches!(
        feature.complete_background(ask("sixth")).await,
        Err(LlmError::CircuitOpen { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn saved_state_survives_a_restart() {
    let endpoint = Switchable::down(false);
    let runtime = Arc::new(LlmRuntime::new(endpoint.clone()));
    let feature = runtime.for_feature(FEATURE);
    feature.record_background_failure("soon", FP, &LlmError::Empty);
    tokio::time::advance(Duration::from_secs(3 * 60 * 60)).await;
    feature.record_background_attempt("done", FP, true);
    feature.record_background_failure("later", FP, &LlmError::Empty);
    for _ in 0..LEDGER_MAX_ATTEMPTS {
        feature.record_background_failure("given-up", FP, &LlmError::Empty);
    }
    feature.record_background_failure("waiting", FP, &LlmError::Timeout(Duration::from_secs(45)));
    open_the_breaker(&feature).await;

    // Whole seconds, as saved.
    let wall = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let saved_at = Clocks {
        now: Instant::now(),
        wall,
    };
    let state = runtime
        .take_unsaved_background_state_at(saved_at)
        .expect("changed");
    assert!(runtime.take_unsaved_background_state().is_none(), "saved");
    let json = serde_json::to_string(&state).unwrap();
    assert!(!json.contains("opener-"), "prompts are never saved: {json}");
    let state: BackgroundState = serde_json::from_str(&json).unwrap();
    assert!(state.is_supported());

    // A daemon started a minute later: the breaker is still open.
    let soon_after = Arc::new(LlmRuntime::new(endpoint.clone()));
    soon_after.restore_background_state_at(
        &state,
        Clocks {
            now: Instant::now(),
            wall: wall + Duration::from_secs(60),
        },
    );
    let error = soon_after
        .for_feature(FEATURE)
        .complete_background(ask("x"))
        .await
        .unwrap_err();
    assert!(
        matches!(error, LlmError::CircuitOpen { retry_after_secs } if retry_after_secs == 240),
        "{error:?}"
    );

    // One started 30 minutes later: "soon" (due 2 h before the save) is
    // due, "later" (due 1 h after it) still waits half an hour.
    let restarted = Arc::new(LlmRuntime::new(endpoint.clone()));
    restarted.restore_background_state_at(
        &state,
        Clocks {
            now: Instant::now(),
            wall: wall + Duration::from_secs(30 * 60),
        },
    );
    let feature = restarted.for_feature(FEATURE);
    assert!(feature.background_attempt_due("soon", FP));
    assert!(!feature.background_attempt_due("later", FP));
    assert!(!feature.background_attempt_due("done", FP));
    assert!(!feature.background_attempt_due("given-up", FP));
    assert!(feature.background_attempt_due("given-up", "new mail"));
    tokio::time::advance(Duration::from_secs(30 * 60)).await;
    assert!(feature.background_attempt_due("later", FP));
    assert!(
        restarted.take_unsaved_background_state().is_none(),
        "a restore is what the file already says"
    );

    // A config reload clears the verdicts but keeps the owed inputs, and the
    // next save says so.
    restarted.replace(endpoint.clone());
    let reloaded = restarted.take_unsaved_background_state().expect("changed");
    let after_reload = Arc::new(LlmRuntime::new(endpoint.clone()));
    after_reload.restore_background_state(&reloaded);
    assert!(after_reload
        .for_feature(FEATURE)
        .background_attempt_due("done", FP));
    assert_eq!(after_reload.take_deferred_background(FEATURE), ["waiting"]);
}

#[test]
fn an_unknown_feature_or_garbage_is_not_a_state() {
    assert!(serde_json::from_str::<BackgroundState>("{not json").is_err());
    let unknown = r#"{"version":1,"breaker":{"consecutive_failures":0,"cooldown_secs":0,"open_until":null},"attempts":[],"deferred":[{"feature":"no_such_feature","key":"k"}]}"#;
    assert!(serde_json::from_str::<BackgroundState>(unknown).is_err());
    let future = r#"{"version":99,"breaker":{"consecutive_failures":0,"cooldown_secs":0,"open_until":null},"attempts":[],"deferred":[]}"#;
    assert!(!serde_json::from_str::<BackgroundState>(future)
        .unwrap()
        .is_supported());
}

#[tokio::test(start_paused = true)]
async fn a_recovered_route_signals_recovery() {
    use crate::health::{Admission, FailureReason, RouteFailure};
    let health = RouteHealth::default();
    let runtime = LlmRuntime::with_route_health(Arc::new(NoopProvider), health.clone());
    let recovered = runtime.subscribe_background_recovery();
    let route = Route {
        provider: RouteProvider::OpenRouter,
        model: "m".into(),
    };
    let Admission::Use(ticket) = health.admit(&route) else {
        panic!("a new route is used");
    };
    ticket.failed(RouteFailure::Cooling {
        reason: FailureReason::plain("unreachable"),
        retry_after: None,
    });
    assert!(!recovered.has_changed().unwrap());
    tokio::time::advance(Duration::from_secs(60)).await;
    let Admission::Probe(ticket) = health.admit(&route) else {
        panic!("the wait is over");
    };
    ticket.succeeded();
    assert!(recovered.has_changed().unwrap());
}

// --- What the HTTP client says about each failure --------------------------

fn client_for(server: &wiremock::MockServer, api_key: Option<&str>) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: format!("{}/v1", server.uri()),
        api_key: api_key.map(str::to_string),
        model: "local-model".into(),
        context_window: 8192,
        request_timeout: Duration::from_secs(1),
    })
}

#[tokio::test]
async fn endpoint_failures_are_told_apart_from_input_failures() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let empty = serde_json::json!({
        "model": "local-model",
        "choices": [{"message": {"role": "assistant", "content": ""}, "finish_reason": "stop"}]
    });
    let cases = [
        ("503", ResponseTemplate::new(503), true),
        ("500", ResponseTemplate::new(500), true),
        ("404 model", ResponseTemplate::new(404), true),
        (
            "400 naming the model",
            ResponseTemplate::new(400)
                .set_body_string(r#"{"error":{"message":"The model `x` does not exist"}}"#),
            true,
        ),
        (
            "timeout",
            ResponseTemplate::new(200)
                .set_body_json(empty.clone())
                .set_delay(Duration::from_secs(3)),
            true,
        ),
        (
            "400 about the request",
            ResponseTemplate::new(400)
                .set_body_string(r#"{"error":{"message":"max_tokens must be positive"}}"#),
            false,
        ),
        (
            "empty reply",
            ResponseTemplate::new(200).set_body_json(empty),
            false,
        ),
    ];
    for (case, reply, endpoint) in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(reply)
            .mount(&server)
            .await;
        let error = client_for(&server, None)
            .complete(ask("hello"))
            .await
            .unwrap_err();
        assert_eq!(error.is_endpoint_failure(), endpoint, "{case}: {error:?}");
        match case {
            "empty reply" => assert!(matches!(error, LlmError::Empty), "{case}"),
            "timeout" => assert!(matches!(error, LlmError::Timeout(_)), "{case}"),
            "400 about the request" => assert!(matches!(error, LlmError::Other(_)), "{case}"),
            _ => assert!(matches!(error, LlmError::Endpoint(_)), "{case}: {error:?}"),
        }
        if !matches!(error, LlmError::Empty | LlmError::Timeout(_)) {
            assert!(
                error.to_string().starts_with("LLM error: "),
                "{case}: the text reads as before: {error}"
            );
        }
    }
}

#[tokio::test]
async fn the_cheap_check_lists_models_and_reads_failures_like_a_request() {
    use wiremock::matchers::{any, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let elsewhere = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&elsewhere)
        .await;
    let cases = [
        ("200", ResponseTemplate::new(200), Some(true)),
        ("no listing", ResponseTemplate::new(404), Some(true)),
        (
            "listing not permitted",
            ResponseTemplate::new(403),
            Some(true),
        ),
        ("503", ResponseTemplate::new(503), Some(false)),
        ("bad key", ResponseTemplate::new(401), Some(false)),
        ("rate limited", ResponseTemplate::new(429), Some(false)),
        (
            "redirect",
            ResponseTemplate::new(302).insert_header("location", elsewhere.uri().as_str()),
            Some(false),
        ),
    ];
    for (case, reply, answers) in cases {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("authorization", "Bearer sk-test-key-123"))
            .respond_with(reply)
            .mount(&server)
            .await;
        let result = client_for(&server, Some("sk-test-key-123"))
            .probe(Duration::from_secs(1))
            .await;
        assert_eq!(
            result.as_ref().map(Result::is_ok),
            answers,
            "{case}: {result:?}"
        );
        if let Some(Err(error)) = result {
            assert!(error.is_endpoint_failure(), "{case}: {error:?}");
        }
    }
    assert!(
        elsewhere
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "the check never follows a redirect"
    );

    // Nothing listening: unreachable.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: format!("http://127.0.0.1:{port}/v1"),
        api_key: None,
        model: "local-model".into(),
        context_window: 8192,
        request_timeout: Duration::from_secs(1),
    });
    let result = provider.probe(Duration::from_secs(1)).await;
    assert!(
        matches!(result, Some(Err(ref error)) if error.is_endpoint_failure()),
        "{result:?}"
    );
}

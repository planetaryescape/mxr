use super::*;

struct StaticProvider(&'static str);

#[async_trait]
impl LlmProvider for StaticProvider {
    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            content: self.0.into(),
            model: self.0.into(),
            finish_reason: None,
        })
    }

    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            context_window: 1,
            supports_streaming: false,
        }
    }

    fn model_name(&self) -> &str {
        self.0
    }
}

struct FailingProvider;

#[async_trait]
impl LlmProvider for FailingProvider {
    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Err(LlmError::Unreachable(
            "https://private.invalid/api/key=secret".into(),
        ))
    }

    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            context_window: 1,
            supports_streaming: false,
        }
    }

    fn model_name(&self) -> &str {
        "failing"
    }
}

fn request() -> CompletionRequest {
    CompletionRequest {
        messages: vec![ChatMessage::user("fixture")],
        max_tokens: None,
        temperature: None,
    }
}

#[tokio::test]
async fn outcomes_are_route_specific_sanitized_and_reset_on_provider_replacement() {
    let runtime = Arc::new(LlmRuntime::new(Arc::new(StaticProvider("base"))));
    let feature = LlmFeature::DraftAssist;
    assert_eq!(runtime.observation(Some(feature)), None);

    let mut providers = HashMap::new();
    providers.insert(feature, Arc::new(FailingProvider) as Arc<dyn LlmProvider>);
    runtime.replace_feature_providers(providers, HashMap::new());
    assert!(matches!(
        runtime.for_feature(feature).complete(request()).await,
        Err(LlmError::Unreachable(_))
    ));
    assert_eq!(
        runtime.observation(Some(feature)),
        Some(CompletionObservation::Failed(LlmFailureKind::Unreachable))
    );

    runtime.complete(request()).await.unwrap();
    assert_eq!(
        runtime.observation(None),
        Some(CompletionObservation::Succeeded)
    );
    assert_eq!(
        runtime.observation(Some(feature)),
        Some(CompletionObservation::Failed(LlmFailureKind::Unreachable))
    );
    assert!(!format!("{:?}", runtime.observations()).contains("private.invalid"));

    runtime.replace_feature_providers(HashMap::new(), HashMap::new());
    assert_eq!(runtime.observation(Some(feature)), None);
    runtime
        .for_feature(feature)
        .complete(request())
        .await
        .unwrap();
    assert_eq!(
        runtime.observation(Some(feature)),
        Some(CompletionObservation::Succeeded)
    );
}

#[tokio::test]
async fn an_in_flight_completion_from_a_replaced_provider_is_ignored() {
    struct StartedProvider(Arc<tokio::sync::Notify>);

    #[async_trait]
    impl LlmProvider for StartedProvider {
        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, LlmError> {
            self.0.notify_one();
            tokio::time::sleep(Duration::from_millis(30)).await;
            Ok(CompletionResponse {
                content: "old".into(),
                model: "old".into(),
                finish_reason: None,
            })
        }

        fn capabilities(&self) -> LlmCapabilities {
            LlmCapabilities {
                context_window: 1,
                supports_streaming: false,
            }
        }

        fn model_name(&self) -> &str {
            "old"
        }
    }

    let started = Arc::new(tokio::sync::Notify::new());
    let runtime = Arc::new(LlmRuntime::new(Arc::new(StartedProvider(started.clone()))));
    let pending_runtime = runtime.clone();
    let pending = tokio::spawn(async move { pending_runtime.complete(request()).await });
    started.notified().await;
    runtime.replace(Arc::new(StaticProvider("replacement")));
    pending.await.unwrap().unwrap();
    assert_eq!(runtime.observation(None), None);
}

#[tokio::test]
async fn a_background_timeout_is_observed_but_disabled_calls_are_not() {
    struct SlowProvider;

    #[async_trait]
    impl LlmProvider for SlowProvider {
        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, LlmError> {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(CompletionResponse {
                content: "late".into(),
                model: "slow".into(),
                finish_reason: None,
            })
        }

        fn capabilities(&self) -> LlmCapabilities {
            LlmCapabilities {
                context_window: 1,
                supports_streaming: false,
            }
        }

        fn model_name(&self) -> &str {
            "slow"
        }
    }

    let runtime = Arc::new(LlmRuntime::new(Arc::new(SlowProvider)));
    runtime.set_background_timeout(Duration::from_millis(20));
    let feature = LlmFeature::RelationshipSummary;
    assert!(matches!(
        runtime
            .for_feature(feature)
            .complete_background(request())
            .await,
        Err(LlmError::Timeout(_))
    ));
    assert_eq!(
        runtime.observation(Some(feature)),
        Some(CompletionObservation::Failed(LlmFailureKind::Timeout))
    );

    let disabled = LlmRuntime::new(Arc::new(NoopProvider));
    assert!(matches!(
        disabled.complete(request()).await,
        Err(LlmError::Disabled)
    ));
    assert_eq!(disabled.observation(None), None);
}

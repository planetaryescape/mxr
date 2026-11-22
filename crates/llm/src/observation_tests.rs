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

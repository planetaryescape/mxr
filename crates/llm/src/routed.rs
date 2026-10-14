//! A model served through more than one provider: Anthropic's own API
//! first, OpenRouter when Anthropic isn't working.
//!
//! Which provider is "not working" comes from [`RouteHealth`], shared by
//! every caller, so a batch of background work makes one failed attempt
//! instead of one per item.

use crate::anthropic::{AnthropicClient, ANTHROPIC_BASE_URL};
use crate::failure::{AttemptError, ErrorClass};
use crate::health::{Admission, RouteFailure, RouteHealth, RouteState, RouteView, Ticket};
use crate::route::{is_claude_model, ApiKey, ApiKeySource, Route, RouteProvider};
use crate::{
    CallKind, CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider,
    OpenAiCompatibleConfig, OpenAiCompatibleProvider, ToolChatRequest, ToolChatResponse,
};
use async_trait::async_trait;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

pub(crate) const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Room for adaptive thinking on top of the answer a caller asked for:
/// thinking tokens count against `max_tokens`, and a budget sized for the
/// answer alone can be spent before the first word.
const THINKING_HEADROOM: u32 = 2048;
const FALLBACK_ANSWER_TOKENS: u32 = 1024;
const MAX_OUTPUT_TOKENS: u32 = 64_000;

/// Background calls spend at most this share of their budget on attempts,
/// so a slow route is recorded as failed (and skipped from then on) before
/// the caller's own timeout cuts the call off without a verdict.
const BACKGROUND_BUDGET_SHARE: f64 = 0.9;

pub struct RoutedProviderConfig {
    /// The model as configured, e.g. `claude-haiku-5-5`.
    pub model: String,
    /// In the order they are tried. See [`crate::routes_for_model`].
    pub routes: Vec<Route>,
    pub context_window: u32,
    pub request_timeout: Duration,
    pub keys: Arc<dyn ApiKeySource>,
    pub health: RouteHealth,
    /// Where each provider lives. The public APIs unless a test points them
    /// at a local server.
    pub anthropic_base_url: String,
    pub openrouter_base_url: String,
}

impl RoutedProviderConfig {
    pub fn new(
        model: String,
        routes: Vec<Route>,
        context_window: u32,
        request_timeout: Duration,
        keys: Arc<dyn ApiKeySource>,
        health: RouteHealth,
    ) -> Self {
        Self {
            model,
            routes,
            context_window,
            request_timeout,
            keys,
            health,
            anthropic_base_url: ANTHROPIC_BASE_URL.to_string(),
            openrouter_base_url: OPENROUTER_BASE_URL.to_string(),
        }
    }
}

pub struct RoutedProvider {
    model: String,
    routes: Vec<Route>,
    context_window: u32,
    keys: Arc<dyn ApiKeySource>,
    health: RouteHealth,
    anthropic: AnthropicClient,
    openrouter: Option<OpenAiCompatibleProvider>,
}

impl RoutedProvider {
    pub fn new(config: RoutedProviderConfig) -> Self {
        let openrouter = config
            .routes
            .iter()
            .find(|route| route.provider == RouteProvider::OpenRouter)
            .map(|route| {
                OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
                    base_url: config.openrouter_base_url.clone(),
                    // The key is read per request and passed in.
                    api_key: None,
                    model: route.model.clone(),
                    context_window: config.context_window,
                    request_timeout: config.request_timeout,
                })
            });
        Self {
            anthropic: AnthropicClient::new(&config.anthropic_base_url, config.request_timeout),
            openrouter,
            model: config.model,
            routes: config.routes,
            context_window: config.context_window,
            keys: config.keys,
            health: config.health,
        }
    }

    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    /// The request as the routes want it. Claude 5.x answers any non-default
    /// sampling parameter with HTTP 400, on either provider, so none is sent;
    /// and `max_tokens` leaves room for thinking.
    fn prepare(&self, req: CompletionRequest) -> CompletionRequest {
        if !is_claude_model(&self.model) {
            return req;
        }
        CompletionRequest {
            temperature: None,
            max_tokens: Some(
                req.max_tokens
                    .unwrap_or(FALLBACK_ANSWER_TOKENS)
                    .saturating_add(THINKING_HEADROOM)
                    .min(MAX_OUTPUT_TOKENS),
            ),
            ..req
        }
    }

    /// One call on one route, recording what it says about the route.
    async fn attempt<T, Fut>(
        &self,
        route: &Route,
        call: Fut,
        ticket: Ticket,
        model_of: fn(&mut T) -> &mut String,
    ) -> Attempt<T>
    where
        Fut: Future<Output = Result<T, AttemptError>>,
    {
        match call.await {
            Ok(mut response) => {
                ticket.succeeded();
                let model = model_of(&mut response);
                *model = answered_by(route.provider, model, &route.model);
                Attempt::Done(Ok(response))
            }
            Err(AttemptError {
                error,
                class: ErrorClass::Request,
            }) => {
                // The provider answered; this request was the problem.
                ticket.succeeded();
                Attempt::Done(Err(error))
            }
            Err(AttemptError {
                error,
                class: ErrorClass::Route(failure),
            }) => {
                ticket.failed(failure);
                Attempt::RouteFailed { error, failure }
            }
        }
    }

    async fn complete_on(
        &self,
        route: &Route,
        key: ApiKey,
        req: &CompletionRequest,
        timeout: Option<Duration>,
    ) -> Result<CompletionResponse, AttemptError> {
        match route.provider {
            RouteProvider::Anthropic => {
                self.anthropic
                    .complete(&key, &route.model, req, timeout)
                    .await
            }
            RouteProvider::OpenRouter => match &self.openrouter {
                Some(openrouter) => openrouter.attempt(req, Some(key.expose()), timeout).await,
                None => Err(no_openrouter_route()),
            },
        }
    }

    async fn chat_on(
        &self,
        route: &Route,
        key: ApiKey,
        req: &ToolChatRequest,
        timeout: Option<Duration>,
    ) -> Result<ToolChatResponse, AttemptError> {
        match route.provider {
            RouteProvider::Anthropic => self.anthropic.chat(&key, &route.model, req, timeout).await,
            RouteProvider::OpenRouter => match &self.openrouter {
                Some(openrouter) => {
                    openrouter
                        .chat_attempt(req, Some(key.expose()), timeout)
                        .await
                }
                None => Err(no_openrouter_route()),
            },
        }
    }

    async fn run(
        &self,
        req: CompletionRequest,
        kind: CallKind,
    ) -> Result<CompletionResponse, LlmError> {
        let req = self.prepare(req);
        self.run_routes(
            kind,
            |route, key, timeout| self.complete_on(route, key, &req, timeout),
            |response: &mut CompletionResponse| &mut response.model,
        )
        .await
    }

    async fn run_chat(
        &self,
        req: ToolChatRequest,
        kind: CallKind,
    ) -> Result<ToolChatResponse, LlmError> {
        self.run_routes(
            kind,
            |route, key, timeout| self.chat_on(route, key, &req, timeout),
            |response: &mut ToolChatResponse| &mut response.model,
        )
        .await
    }

    async fn run_routes<'a, T, F, Fut>(
        &'a self,
        kind: CallKind,
        call: F,
        model_of: fn(&mut T) -> &mut String,
    ) -> Result<T, LlmError>
    where
        F: Fn(&'a Route, ApiKey, Option<Duration>) -> Fut,
        Fut: Future<Output = Result<T, AttemptError>>,
    {
        let started = Instant::now();
        let background_budget = match kind {
            CallKind::Foreground => None,
            CallKind::Background { budget } => Some(budget.mul_f64(BACKGROUND_BUDGET_SHARE)),
        };

        // Keys are read once per request: a key stored or removed while this
        // request runs applies to the next one.
        let keyed: Vec<(&Route, ApiKey)> = self
            .routes
            .iter()
            .filter_map(|route| Some((route, self.keys.api_key(route.provider)?)))
            .collect();
        if keyed.is_empty() {
            return Err(LlmError::Other(no_key_message(&self.routes)));
        }

        let mut skipped: Vec<(&Route, &ApiKey, RouteView)> = Vec::new();
        let mut failed: Vec<(&Route, RouteFailure)> = Vec::new();
        let mut last_error = None;
        for (index, (route, key)) in keyed.iter().enumerate() {
            let ticket = match self.health.admit(route) {
                Admission::Use(ticket) | Admission::Probe(ticket) => ticket,
                Admission::Skip(view) => {
                    skipped.push((route, key, view));
                    continue;
                }
            };
            let timeout = background_budget.map(|total| {
                let remaining = total.saturating_sub(started.elapsed());
                let fallbacks = keyed[index + 1..]
                    .iter()
                    .filter(|(later, _)| self.is_available(later))
                    .count();
                remaining / u32::try_from(fallbacks + 1).unwrap_or(1)
            });
            match self
                .attempt(route, call(route, key.clone(), timeout), ticket, model_of)
                .await
            {
                Attempt::Done(result) => return result,
                Attempt::RouteFailed { error, failure } => {
                    failed.push((route, failure));
                    last_error = Some(error);
                }
            }
        }

        if failed.is_empty() {
            // Every route is out. Background work waits for the soonest one
            // to come back instead of spending a request on a known failure.
            let soonest = skipped
                .iter()
                .min_by_key(|(_, _, view)| view.retry_in)
                .copied();
            let Some((route, key, view)) = soonest else {
                return Err(LlmError::Other(no_key_message(&self.routes)));
            };
            if matches!(kind, CallKind::Background { .. }) {
                return Err(LlmError::CircuitOpen {
                    retry_after_secs: view.retry_in.as_secs().max(1),
                });
            }
            // A person is waiting: try the soonest-recovering route once.
            let ticket = self.health.force(route);
            return match self
                .attempt(route, call(route, key.clone(), None), ticket, model_of)
                .await
            {
                Attempt::Done(result) => result,
                Attempt::RouteFailed { failure, .. } => {
                    failed.push((route, failure));
                    skipped.retain(|(other, _, _)| *other != route);
                    Err(all_routes_out(&failed, &skipped))
                }
            };
        }

        if failed.len() + skipped.len() == 1 {
            // One route in play: its own error says it best.
            if let Some(error) = last_error {
                return Err(error);
            }
        }
        Err(all_routes_out(&failed, &skipped))
    }

    /// Whether a request would be sent to `route` now.
    fn is_available(&self, route: &Route) -> bool {
        matches!(
            self.health.view(route).state,
            RouteState::Ok | RouteState::Recovering
        )
    }
}

enum Attempt<T> {
    Done(Result<T, LlmError>),
    RouteFailed {
        error: LlmError,
        failure: RouteFailure,
    },
}

#[async_trait]
impl LlmProvider for RoutedProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.run(req, CallKind::Foreground).await
    }

    async fn complete_as(
        &self,
        req: CompletionRequest,
        kind: CallKind,
    ) -> Result<CompletionResponse, LlmError> {
        self.run(req, kind).await
    }

    async fn chat_as(
        &self,
        req: ToolChatRequest,
        kind: CallKind,
    ) -> Result<ToolChatResponse, LlmError> {
        self.run_chat(req, kind).await
    }

    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            context_window: self.context_window,
            supports_streaming: false,
        }
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn leaves_machine(&self) -> bool {
        true
    }

    fn tracks_route_health(&self) -> bool {
        true
    }
}

/// What provenance shows: the model and the provider that answered.
fn answered_by(provider: RouteProvider, reported: &str, requested: &str) -> String {
    let model = if reported.trim().is_empty() {
        requested
    } else {
        reported
    };
    format!("{model} via {}", provider.label())
}

fn no_openrouter_route() -> AttemptError {
    AttemptError::request(LlmError::Other(
        "no OpenRouter route is configured for this model".to_string(),
    ))
}

fn no_key_message(routes: &[Route]) -> String {
    let providers: Vec<&str> = routes.iter().map(|route| route.provider.key()).collect();
    format!(
        "no API key for {}; store one with `mxr llm key set <provider>` or set {}",
        providers.join(" or "),
        routes
            .iter()
            .map(|route| route.provider.env_var())
            .collect::<Vec<_>>()
            .join(" or ")
    )
}

/// One line naming each route and why it didn't answer.
fn all_routes_out(
    failed: &[(&Route, RouteFailure)],
    skipped: &[(&Route, &ApiKey, RouteView)],
) -> LlmError {
    let mut parts: Vec<(RouteProvider, String)> = Vec::new();
    for (route, failure) in failed {
        let reason = match failure {
            RouteFailure::Cooling { reason, .. } | RouteFailure::Broken { reason } => reason,
        };
        parts.push((route.provider, format!("failed ({reason})")));
    }
    for (route, _, view) in skipped {
        parts.push((route.provider, describe_view(view)));
    }
    let listed: Vec<String> = parts
        .into_iter()
        .map(|(provider, state)| format!("{}: {state}", provider.key()))
        .collect();
    LlmError::Other(format!("no LLM route is working ({})", listed.join("; ")))
}

fn describe_view(view: &RouteView) -> String {
    let reason = view
        .reason
        .map_or_else(String::new, |reason| format!(" ({reason})"));
    match view.state {
        RouteState::Ok => "ok".to_string(),
        RouteState::Cooling => format!("cooling for {}{reason}", format_wait(view.retry_in)),
        RouteState::Broken => format!("broken{reason}"),
        RouteState::Recovering => format!("waiting for a retry{reason}"),
        RouteState::Probing => format!("being retried by another request{reason}"),
    }
}

fn format_wait(wait: Duration) -> String {
    let secs = wait.as_secs();
    if secs >= 120 {
        format!("{}m", secs.div_ceil(60))
    } else {
        format!("{}s", secs.max(1))
    }
}

/// One route's standing, for `mxr llm status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteStatus {
    pub route: Route,
    pub key_present: bool,
    pub view: RouteView,
    /// The route a request would go to right now.
    pub next: bool,
}

/// The routes in order with their keys and health, marking the one a
/// request would use now.
pub fn route_statuses(
    routes: &[Route],
    keys: &dyn ApiKeySource,
    health: &RouteHealth,
) -> Vec<RouteStatus> {
    let mut found_next = false;
    routes
        .iter()
        .map(|route| {
            let key_present = keys.api_key(route.provider).is_some();
            let view = health.view(route);
            let usable =
                key_present && matches!(view.state, RouteState::Ok | RouteState::Recovering);
            let next = usable && !found_next;
            found_next |= next;
            RouteStatus {
                route: route.clone(),
                key_present,
                view,
                next,
            }
        })
        .collect()
}

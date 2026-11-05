//! A model served through more than one provider: the vendor's own API
//! first (Anthropic's or OpenAI's), OpenRouter when that isn't working.
//! With `[llm.fallback]`, the user's own endpoint (`llm.base_url`) comes
//! first and the hosted model's routes stand behind it.
//!
//! Which provider is "not working" comes from [`RouteHealth`], shared by
//! every caller, so a batch of background work makes one failed attempt
//! instead of one per item.

use crate::anthropic::{AnthropicClient, ANTHROPIC_BASE_URL};
use crate::background::PROBE_TIMEOUT;
use crate::endpoint::is_loopback_endpoint;
use crate::failure::{AttemptError, ErrorClass};
use crate::health::{Admission, RouteFailure, RouteHealth, RouteState, RouteView, Ticket};
use crate::route::{
    is_claude_model, is_openai_model, openai_tools_need_no_reasoning, ApiKey, ApiKeySource,
    EndpointRoute, Route, RouteKey, RouteProvider,
};
use crate::{
    CallKind, CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider,
    OpenAiCompatibleConfig, OpenAiCompatibleProvider, TextCallback, ToolChatRequest,
    ToolChatResponse,
};
use async_trait::async_trait;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

pub(crate) const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub(crate) const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

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
    /// The hosted model as configured, e.g. `claude-haiku-5-5`.
    pub model: String,
    /// In the order they are tried. See [`crate::routes_for_model`]. May be
    /// empty when `endpoint` is set: the endpoint alone, with route health
    /// (a feature whose data must stay off the cloud).
    pub routes: Vec<Route>,
    /// The hosted model's context window.
    pub context_window: u32,
    /// The user's own endpoint, tried before `routes`, with its own key and
    /// context window. `None` for a hosted model on its own.
    pub endpoint: Option<OpenAiCompatibleConfig>,
    pub request_timeout: Duration,
    pub keys: Arc<dyn ApiKeySource>,
    pub health: RouteHealth,
    /// Where each provider lives. The public APIs unless a test points them
    /// at a local server.
    pub anthropic_base_url: String,
    pub openai_base_url: String,
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
            endpoint: None,
            request_timeout,
            keys,
            health,
            anthropic_base_url: ANTHROPIC_BASE_URL.to_string(),
            openai_base_url: OPENAI_BASE_URL.to_string(),
            openrouter_base_url: OPENROUTER_BASE_URL.to_string(),
        }
    }
}

pub struct RoutedProvider {
    model: String,
    routes: Vec<Route>,
    context_window: u32,
    endpoint: Option<EndpointLeg>,
    keys: Arc<dyn ApiKeySource>,
    health: RouteHealth,
    anthropic: AnthropicClient,
    openai: Option<OpenAiCompatibleProvider>,
    openrouter: Option<OpenAiCompatibleProvider>,
}

impl RoutedProvider {
    pub fn new(config: RoutedProviderConfig) -> Self {
        let client_for = |provider: RouteProvider, base_url: &str| {
            config
                .routes
                .iter()
                .find(|route| route.provider == provider)
                .map(|route| {
                    OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
                        base_url: base_url.to_string(),
                        // The key is read per request and passed in.
                        api_key: None,
                        model: route.model.clone(),
                        context_window: config.context_window,
                        request_timeout: config.request_timeout,
                    })
                })
        };
        Self {
            anthropic: AnthropicClient::new(&config.anthropic_base_url, config.request_timeout),
            openai: client_for(RouteProvider::OpenAI, &config.openai_base_url)
                .map(|client| client.for_openai(openai_tools_need_no_reasoning(&config.model))),
            openrouter: client_for(RouteProvider::OpenRouter, &config.openrouter_base_url),
            endpoint: config.endpoint.map(|endpoint| EndpointLeg {
                key: RouteKey::Endpoint(EndpointRoute::new(&endpoint.base_url, &endpoint.model)),
                context_window: endpoint.context_window,
                client: OpenAiCompatibleProvider::new(endpoint),
            }),
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

    /// The user's endpoint tried before the hosted routes, if any.
    pub fn endpoint(&self) -> Option<&EndpointRoute> {
        match &self.endpoint.as_ref()?.key {
            RouteKey::Endpoint(endpoint) => Some(endpoint),
            RouteKey::Hosted(_) => None,
        }
    }

    /// The request as the routes want it. Claude 5.x and OpenAI's current
    /// models answer any non-default sampling parameter with HTTP 400, on
    /// either provider, so none is sent; and `max_tokens` leaves room for
    /// thinking.
    fn prepare(&self, req: CompletionRequest) -> CompletionRequest {
        if !is_claude_model(&self.model) && !is_openai_model(&self.model) {
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
        target: &Target<'_>,
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
                // The user's endpoint keeps the label it always had; a
                // hosted answer names who served it.
                if let Target::Hosted(route, _) = target {
                    let model = model_of(&mut response);
                    *model = answered_by(route.provider, model, &route.model);
                }
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
        target: Target<'_>,
        req: &CompletionRequest,
        endpoint_req: &CompletionRequest,
        timeout: Option<Duration>,
    ) -> Result<CompletionResponse, AttemptError> {
        let (route, key) = match target {
            Target::Endpoint(endpoint) => {
                return endpoint
                    .client
                    .attempt(endpoint_req, endpoint.client.api_key.as_deref(), timeout)
                    .await
            }
            Target::Hosted(route, key) => (route, key),
        };
        match route.provider {
            RouteProvider::Anthropic => {
                self.anthropic
                    .complete(&key, &route.model, req, timeout)
                    .await
            }
            RouteProvider::OpenAI => match &self.openai {
                Some(openai) => openai.attempt(req, Some(key.expose()), timeout).await,
                None => Err(no_route(RouteProvider::OpenAI)),
            },
            RouteProvider::OpenRouter => match &self.openrouter {
                Some(openrouter) => openrouter.attempt(req, Some(key.expose()), timeout).await,
                None => Err(no_route(RouteProvider::OpenRouter)),
            },
        }
    }

    async fn chat_on(
        &self,
        target: Target<'_>,
        req: &ToolChatRequest,
        timeout: Option<Duration>,
    ) -> Result<ToolChatResponse, AttemptError> {
        let (route, key) = match target {
            Target::Endpoint(endpoint) => {
                return endpoint
                    .client
                    .chat_attempt(req, endpoint.client.api_key.as_deref(), timeout)
                    .await
            }
            Target::Hosted(route, key) => (route, key),
        };
        match route.provider {
            RouteProvider::Anthropic => self.anthropic.chat(&key, &route.model, req, timeout).await,
            RouteProvider::OpenAI => match &self.openai {
                Some(openai) => openai.chat_attempt(req, Some(key.expose()), timeout).await,
                None => Err(no_route(RouteProvider::OpenAI)),
            },
            RouteProvider::OpenRouter => match &self.openrouter {
                Some(openrouter) => {
                    openrouter
                        .chat_attempt(req, Some(key.expose()), timeout)
                        .await
                }
                None => Err(no_route(RouteProvider::OpenRouter)),
            },
        }
    }

    async fn run(
        &self,
        req: CompletionRequest,
        kind: CallKind,
    ) -> Result<CompletionResponse, LlmError> {
        // The user's endpoint gets the request as the caller wrote it, as
        // it did before a fallback stood behind it.
        let endpoint_req = self.endpoint.is_some().then(|| req.clone());
        let req = self.prepare(req);
        let endpoint_req = endpoint_req.as_ref().unwrap_or(&req);
        self.run_routes(
            kind,
            |target, timeout| self.complete_on(target, &req, endpoint_req, timeout),
            |response: &mut CompletionResponse| &mut response.model,
        )
        .await
    }

    async fn chat_stream_on(
        &self,
        target: Target<'_>,
        req: &ToolChatRequest,
        timeout: Option<Duration>,
        on_text: TextCallback<'_>,
    ) -> Result<ToolChatResponse, AttemptError> {
        let (route, key) = match target {
            Target::Endpoint(endpoint) => {
                return endpoint
                    .client
                    .chat_stream_attempt(req, endpoint.client.api_key.as_deref(), timeout, on_text)
                    .await
            }
            Target::Hosted(route, key) => (route, key),
        };
        match route.provider {
            RouteProvider::Anthropic => {
                self.anthropic
                    .chat_stream(&key, &route.model, req, timeout, on_text)
                    .await
            }
            RouteProvider::OpenAI => match &self.openai {
                Some(openai) => {
                    openai
                        .chat_stream_attempt(req, Some(key.expose()), timeout, on_text)
                        .await
                }
                None => Err(no_route(RouteProvider::OpenAI)),
            },
            RouteProvider::OpenRouter => match &self.openrouter {
                Some(openrouter) => {
                    openrouter
                        .chat_stream_attempt(req, Some(key.expose()), timeout, on_text)
                        .await
                }
                None => Err(no_route(RouteProvider::OpenRouter)),
            },
        }
    }

    async fn run_chat_stream(
        &self,
        req: ToolChatRequest,
        kind: CallKind,
        on_text: TextCallback<'_>,
    ) -> Result<ToolChatResponse, LlmError> {
        self.run_routes(
            kind,
            |target, timeout| self.chat_stream_on(target, &req, timeout, on_text),
            |response: &mut ToolChatResponse| &mut response.model,
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
            |target, timeout| self.chat_on(target, &req, timeout),
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
        F: Fn(Target<'a>, Option<Duration>) -> Fut,
        Fut: Future<Output = Result<T, AttemptError>>,
    {
        let started = Instant::now();
        let background_budget = match kind {
            CallKind::Foreground => None,
            CallKind::Background { budget } => Some(budget.mul_f64(BACKGROUND_BUDGET_SHARE)),
        };

        let targets = self.targets();
        if targets.is_empty() {
            return Err(LlmError::Other(no_key_message(&self.routes)));
        }
        // This target's share of what is left of the background budget, so
        // every route still worth trying gets a turn.
        let share = |index: usize| {
            background_budget.map(|total| {
                let remaining = total.saturating_sub(started.elapsed());
                let fallbacks = targets[index + 1..]
                    .iter()
                    .filter(|later| self.is_available(later))
                    .count();
                remaining / u32::try_from(fallbacks + 1).unwrap_or(1)
            })
        };

        let mut skipped: Vec<(Target<'a>, RouteView)> = Vec::new();
        let mut failed: Vec<(Target<'a>, RouteFailure)> = Vec::new();
        let mut last_error = None;
        for (index, target) in targets.iter().enumerate() {
            let ticket = match self.health.admit(target.key()) {
                Admission::Use(ticket) => ticket,
                Admission::Probe(ticket) => match target {
                    Target::Endpoint(endpoint) => {
                        match self.probe_endpoint(endpoint, ticket, share(index)).await {
                            ProbeOutcome::Up(ticket) => ticket,
                            ProbeOutcome::Down { error, failure } => {
                                failed.push((target.clone(), failure));
                                last_error = Some(error);
                                continue;
                            }
                            ProbeOutcome::Taken(view) => {
                                skipped.push((target.clone(), view));
                                continue;
                            }
                        }
                    }
                    Target::Hosted(..) => ticket,
                },
                Admission::Skip(view) => {
                    skipped.push((target.clone(), view));
                    continue;
                }
            };
            match self
                .attempt(target, call(target.clone(), share(index)), ticket, model_of)
                .await
            {
                Attempt::Done(result) => return result,
                Attempt::RouteFailed { error, failure } => {
                    failed.push((target.clone(), failure));
                    last_error = Some(error);
                }
            }
        }

        if failed.is_empty() {
            // Every route is out. Background work waits for the soonest one
            // to come back instead of spending a request on a known failure.
            let soonest = skipped
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, view))| view.retry_in)
                .map(|(index, (_, view))| (index, *view));
            let Some((index, view)) = soonest else {
                return Err(LlmError::Other(no_key_message(&self.routes)));
            };
            if matches!(kind, CallKind::Background { .. }) {
                return Err(LlmError::CircuitOpen {
                    retry_after_secs: view.retry_in.as_secs().max(1),
                });
            }
            // A person is waiting: try the soonest-recovering route once.
            let (target, _) = skipped.remove(index);
            let ticket = self.health.force(target.key());
            return match self
                .attempt(&target, call(target.clone(), None), ticket, model_of)
                .await
            {
                Attempt::Done(result) => result,
                Attempt::RouteFailed { failure, .. } => {
                    failed.push((target, failure));
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

    /// Where a request can go now, in order: the user's endpoint, then each
    /// hosted route with a key. Keys are read once per request: a key
    /// stored or removed while this request runs applies to the next one.
    fn targets(&self) -> Vec<Target<'_>> {
        self.endpoint
            .iter()
            .map(Target::Endpoint)
            .chain(self.routes.iter().filter_map(|route| {
                Some(Target::Hosted(route, self.keys.api_key(route.provider)?))
            }))
            .collect()
    }

    /// The user's endpoint is due a retry: ask whether it answers
    /// (`GET /models`) before spending the prompt on it, so an endpoint that
    /// is still asleep costs no request and this call moves straight on to
    /// the fallback.
    async fn probe_endpoint(
        &self,
        endpoint: &EndpointLeg,
        ticket: Ticket,
        share: Option<Duration>,
    ) -> ProbeOutcome {
        let timeout = share.map_or(PROBE_TIMEOUT, |share| share.min(PROBE_TIMEOUT));
        match endpoint.client.probe_attempt(timeout).await {
            Ok(()) => {
                ticket.succeeded();
                // Back to healthy; take an ordinary ticket for the real
                // request. Another caller may have failed it meanwhile.
                match self.health.admit(endpoint.key.clone()) {
                    Admission::Use(ticket) | Admission::Probe(ticket) => ProbeOutcome::Up(ticket),
                    Admission::Skip(view) => ProbeOutcome::Taken(view),
                }
            }
            Err(AttemptError {
                error,
                class: ErrorClass::Route(failure),
            }) => {
                ticket.failed(failure);
                ProbeOutcome::Down { error, failure }
            }
            // The check itself couldn't run; the real request is the check.
            Err(AttemptError {
                class: ErrorClass::Request,
                ..
            }) => ProbeOutcome::Up(ticket),
        }
    }

    /// Whether a request would be sent to `target` now.
    fn is_available(&self, target: &Target<'_>) -> bool {
        matches!(
            self.health.view(target.key()).state,
            RouteState::Ok | RouteState::Recovering
        )
    }
}

/// The user's endpoint as one of the routes.
struct EndpointLeg {
    key: RouteKey,
    context_window: u32,
    client: OpenAiCompatibleProvider,
}

/// One place this request may go.
#[derive(Clone)]
enum Target<'a> {
    Endpoint(&'a EndpointLeg),
    Hosted(&'a Route, ApiKey),
}

impl Target<'_> {
    fn key(&self) -> RouteKey {
        match self {
            Self::Endpoint(endpoint) => endpoint.key.clone(),
            Self::Hosted(route, _) => RouteKey::Hosted((*route).clone()),
        }
    }

    /// How errors and logs name it.
    fn name(&self) -> &'static str {
        match self {
            Self::Endpoint(_) => "base_url",
            Self::Hosted(route, _) => route.provider.key(),
        }
    }
}

enum ProbeOutcome {
    /// It answers: send the request with this ticket.
    Up(Ticket),
    Down {
        error: LlmError,
        failure: RouteFailure,
    },
    /// Someone else's request failed it while the check ran.
    Taken(RouteView),
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

    async fn chat_stream_as(
        &self,
        req: ToolChatRequest,
        kind: CallKind,
        on_text: TextCallback<'_>,
    ) -> Result<ToolChatResponse, LlmError> {
        self.run_chat_stream(req, kind, on_text).await
    }

    fn capabilities(&self) -> LlmCapabilities {
        // Prompts are built once, before a route is chosen, so they must fit
        // the smallest window in the chain.
        let hosted = (!self.routes.is_empty()).then_some(self.context_window);
        let endpoint = self
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.context_window);
        LlmCapabilities {
            context_window: hosted
                .into_iter()
                .chain(endpoint)
                .min()
                .unwrap_or(self.context_window),
            supports_streaming: false,
        }
    }

    fn model_name(&self) -> &str {
        self.endpoint
            .as_ref()
            .map_or(&self.model, |endpoint| endpoint.client.model_name())
    }

    /// Only the user's endpoint, with no hosted route behind it: its URL,
    /// as the plain provider would say.
    fn base_url(&self) -> Option<&str> {
        match &self.endpoint {
            Some(endpoint) if self.routes.is_empty() => endpoint.client.base_url(),
            _ => None,
        }
    }

    /// Any hosted route can be used (a key may be stored at any moment), so
    /// a chain with one leaves the machine whether or not the endpoint is
    /// local.
    fn leaves_machine(&self) -> bool {
        !self.routes.is_empty()
            || self
                .endpoint
                .as_ref()
                .and_then(|endpoint| endpoint.client.base_url())
                .is_some_and(|url| !is_loopback_endpoint(url))
    }

    fn tracks_route_health(&self) -> bool {
        true
    }

    fn background_retry_in(&self) -> Option<Duration> {
        let targets = self.targets();
        if targets.iter().any(|target| self.is_available(target)) {
            return None;
        }
        // A route being probed by another request shows zero; wait a moment.
        targets
            .iter()
            .map(|target| {
                self.health
                    .view(target.key())
                    .retry_in
                    .max(Duration::from_secs(1))
            })
            .min()
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

fn no_route(provider: RouteProvider) -> AttemptError {
    AttemptError::request(LlmError::Other(format!(
        "no {} route is configured for this model",
        provider.label()
    )))
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
    failed: &[(Target<'_>, RouteFailure)],
    skipped: &[(Target<'_>, RouteView)],
) -> LlmError {
    let mut parts: Vec<(&'static str, String)> = Vec::new();
    for (target, failure) in failed {
        let reason = match failure {
            RouteFailure::Cooling { reason, .. } | RouteFailure::Broken { reason } => reason,
        };
        parts.push((target.name(), format!("failed ({reason})")));
    }
    for (target, view) in skipped {
        parts.push((target.name(), describe_view(view)));
    }
    let listed: Vec<String> = parts
        .into_iter()
        .map(|(name, state)| format!("{name}: {state}"))
        .collect();
    LlmError::Endpoint(format!("no LLM route is working ({})", listed.join("; ")))
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
    statuses_after(routes, keys, health, false)
}

/// The user's endpoint's standing, for `mxr llm status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointStatus {
    pub route: EndpointRoute,
    pub view: RouteView,
    /// A request would go here now.
    pub next: bool,
}

/// A fallback chain in order: the user's endpoint, then the hosted routes,
/// marking the one a request would use now.
pub fn chain_statuses(
    endpoint: &EndpointRoute,
    routes: &[Route],
    keys: &dyn ApiKeySource,
    health: &RouteHealth,
) -> (EndpointStatus, Vec<RouteStatus>) {
    let view = health.view(endpoint);
    let next = matches!(view.state, RouteState::Ok | RouteState::Recovering);
    (
        EndpointStatus {
            route: endpoint.clone(),
            view,
            next,
        },
        statuses_after(routes, keys, health, next),
    )
}

fn statuses_after(
    routes: &[Route],
    keys: &dyn ApiKeySource,
    health: &RouteHealth,
    mut found_next: bool,
) -> Vec<RouteStatus> {
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

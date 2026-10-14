use super::{LlmError, LlmFeature};
use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Sanitized process-local outcome of the last request for one provider route.
pub enum CompletionObservation {
    Succeeded,
    Failed(LlmFailureKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Typed failure category retained without provider error text.
pub enum LlmFailureKind {
    Unreachable,
    RateLimited,
    Timeout,
    Unauthorized,
    Empty,
    Other,
}

#[derive(Default)]
pub(crate) struct CompletionObservations {
    generation: RwLock<u64>,
    routes: Mutex<HashMap<Option<LlmFeature>, CompletionObservation>>,
}

impl CompletionObservations {
    pub(crate) fn with_generation<T>(&self, select: impl FnOnce(u64) -> T) -> T {
        let generation = self
            .generation
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        select(*generation)
    }

    pub(crate) fn replace<T>(&self, update: impl FnOnce() -> T) -> T {
        let mut generation = self
            .generation
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = update();
        *generation = generation.wrapping_add(1);
        self.routes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        result
    }

    pub(crate) fn record<T>(
        &self,
        generation: u64,
        feature: Option<LlmFeature>,
        result: &Result<T, LlmError>,
    ) {
        let current = self
            .generation
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if generation != *current {
            return;
        }
        let observation = match result {
            Ok(_) => CompletionObservation::Succeeded,
            Err(error) => match failure_kind(error) {
                Some(kind) => CompletionObservation::Failed(kind),
                None => return,
            },
        };
        self.routes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(feature, observation);
    }

    pub(crate) fn get(&self, feature: Option<LlmFeature>) -> Option<CompletionObservation> {
        self.routes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&feature)
            .copied()
    }

    pub(crate) fn snapshot(&self) -> Vec<(Option<LlmFeature>, CompletionObservation)> {
        let mut routes: Vec<_> = self
            .routes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|(feature, observation)| (*feature, *observation))
            .collect();
        routes.sort_by_key(|(feature, _)| feature.map_or("default", LlmFeature::key));
        routes
    }
}

fn failure_kind(error: &LlmError) -> Option<LlmFailureKind> {
    match error {
        LlmError::Disabled | LlmError::PrivacyBlocked(_) | LlmError::CircuitOpen { .. } => None,
        LlmError::Unreachable(_) => Some(LlmFailureKind::Unreachable),
        LlmError::RateLimited { .. } => Some(LlmFailureKind::RateLimited),
        LlmError::Timeout(_) => Some(LlmFailureKind::Timeout),
        LlmError::Unauthorized => Some(LlmFailureKind::Unauthorized),
        LlmError::Empty => Some(LlmFailureKind::Empty),
        LlmError::Other(_) => Some(LlmFailureKind::Other),
    }
}

//! Recorded model responses for `mxr classify eval` (blueprint 37, "Recorded
//! model responses").
//!
//! A language model at temperature 0 is not reproducible on batched servers,
//! so CI never calls one. Every model call an eval makes goes through
//! [`RecordedProvider`], which keys it by model, prompt version, schema
//! version and a hash of the canonical request, and serves the stored
//! response.
//!
//! * Replay is the default. A call with no recording is a hard error naming
//!   the key, never a silent live call.
//! * Record is opt-in ([`RecordMode::from_env`], `MXR_LLM_RECORD=1`). It calls
//!   the wrapped provider and writes the recording. It is never on in CI: when
//!   the `CI` environment variable is set, replay wins whatever
//!   `MXR_LLM_RECORD` says.
//!
//! One file per key, named by the key's hash, in a fixed field order, so the
//! same inputs always give the same files and a diff shows exactly the claims
//! that changed. This wrapper is for evals and tests only: nothing in the
//! daemon's provider selection builds it.

use crate::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;

/// The environment variable that turns recording on.
pub const RECORD_ENV: &str = "MXR_LLM_RECORD";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordMode {
    /// Serve recordings; a missing one is an error.
    Replay,
    /// Call the wrapped provider and write what it answers.
    Record,
}

impl RecordMode {
    /// Record only when `MXR_LLM_RECORD` is `1` or `true` and `CI` is not set;
    /// replay otherwise.
    pub fn from_env() -> Self {
        Self::mode_for(
            std::env::var("CI").ok().as_deref(),
            std::env::var(RECORD_ENV).ok().as_deref(),
        )
    }

    /// The mode for the given values of `CI` and `MXR_LLM_RECORD`. A `CI`
    /// that is set at all, even empty, forces replay: a recording run needs a
    /// live model and a person to review the diff, never a CI job.
    pub fn mode_for(ci: Option<&str>, record: Option<&str>) -> Self {
        match (ci, record) {
            (None, Some("1" | "true")) => Self::Record,
            _ => Self::Replay,
        }
    }
}

/// What identifies a recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingKey {
    pub model: String,
    pub prompt_version: String,
    pub schema_version: String,
    /// SHA-256 over the canonical request: its messages, `max_tokens` and
    /// `temperature`.
    pub input_hash: String,
}

impl RecordingKey {
    /// `model/prompt/schema/hash`, the text an error names.
    pub fn describe(&self) -> String {
        format!(
            "{} / prompt {} / schema {} / input {}",
            self.model, self.prompt_version, self.schema_version, self.input_hash
        )
    }

    /// The file a recording lives in: a hash of the whole key, so no field
    /// needs escaping and a change to any one moves to another file.
    fn file_name(&self) -> String {
        let mut hasher = Sha256::new();
        for part in [
            &self.model,
            &self.prompt_version,
            &self.schema_version,
            &self.input_hash,
        ] {
            hasher.update((part.len() as u64).to_be_bytes());
            hasher.update(part.as_bytes());
        }
        format!("{}.json", hex(&hasher.finalize()))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The hash of a request's canonical form. The role is written by name, so
/// the hash does not move if the enum's layout does.
pub fn input_hash(req: &CompletionRequest) -> String {
    #[derive(Serialize)]
    struct Canonical<'a> {
        messages: Vec<(String, &'a str)>,
        max_tokens: Option<u32>,
        /// Written as text: `f32` has no stable JSON form.
        temperature: Option<String>,
    }
    let canonical = Canonical {
        messages: req
            .messages
            .iter()
            .map(|message| (format!("{:?}", message.role), message.content.as_str()))
            .collect(),
        max_tokens: req.max_tokens,
        temperature: req.temperature.map(|t| format!("{t:?}")),
    };
    let json = serde_json::to_vec(&canonical).unwrap_or_default();
    hex(&Sha256::digest(json))
}

/// One checked-in recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Recording {
    key: RecordingKey,
    /// The request, so a reviewer reads what was asked beside the answer.
    request: Vec<(String, String)>,
    response: RecordedResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RecordedResponse {
    content: String,
    model: String,
    finish_reason: Option<String>,
}

pub struct RecordedProvider {
    inner: Option<Arc<dyn LlmProvider>>,
    model: String,
    prompt_version: String,
    schema_version: String,
    dir: PathBuf,
}

impl RecordedProvider {
    /// Serve recordings of `model` from `dir`. Never calls a model.
    pub fn replay(
        dir: impl Into<PathBuf>,
        model: &str,
        prompt_version: &str,
        schema_version: &str,
    ) -> Self {
        Self {
            inner: None,
            model: model.to_string(),
            prompt_version: prompt_version.to_string(),
            schema_version: schema_version.to_string(),
            dir: dir.into(),
        }
    }

    /// Call `inner` and write its answers to `dir`.
    pub fn record(
        inner: Arc<dyn LlmProvider>,
        dir: impl Into<PathBuf>,
        prompt_version: &str,
        schema_version: &str,
    ) -> Self {
        Self {
            model: inner.model_name().to_string(),
            inner: Some(inner),
            prompt_version: prompt_version.to_string(),
            schema_version: schema_version.to_string(),
            dir: dir.into(),
        }
    }

    /// Replay, or record when [`RECORD_ENV`] says so (and `CI` is not set) and `live` can build the
    /// provider to call. `live` is not called in replay.
    pub fn from_env(
        dir: impl Into<PathBuf>,
        model: &str,
        prompt_version: &str,
        schema_version: &str,
        live: impl FnOnce() -> Arc<dyn LlmProvider>,
    ) -> Self {
        match RecordMode::from_env() {
            RecordMode::Replay => Self::replay(dir, model, prompt_version, schema_version),
            RecordMode::Record => Self::record(live(), dir, prompt_version, schema_version),
        }
    }

    pub fn mode(&self) -> RecordMode {
        if self.inner.is_some() {
            RecordMode::Record
        } else {
            RecordMode::Replay
        }
    }

    pub fn key_for(&self, req: &CompletionRequest) -> RecordingKey {
        RecordingKey {
            model: self.model.clone(),
            prompt_version: self.prompt_version.clone(),
            schema_version: self.schema_version.clone(),
            input_hash: input_hash(req),
        }
    }

    fn path(&self, key: &RecordingKey) -> PathBuf {
        self.dir.join(key.file_name())
    }

    fn read(&self, key: &RecordingKey) -> Result<Option<Recording>, LlmError> {
        let path = self.path(key);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(LlmError::Other(format!(
                    "can't read the recording {}: {error}",
                    path.display()
                )))
            }
        };
        let recording: Recording = serde_json::from_str(&text).map_err(|error| {
            LlmError::Other(format!(
                "the recording {} is not valid: {error}",
                path.display()
            ))
        })?;
        // A file under the wrong name would serve another request's answer.
        if &recording.key != key {
            return Err(LlmError::Other(format!(
                "the recording {} holds a different key ({})",
                path.display(),
                recording.key.describe()
            )));
        }
        Ok(Some(recording))
    }

    fn write(&self, recording: &Recording) -> Result<(), LlmError> {
        let io =
            |error: std::io::Error| LlmError::Other(format!("can't write a recording: {error}"));
        std::fs::create_dir_all(&self.dir).map_err(io)?;
        let mut text = serde_json::to_string_pretty(recording)
            .map_err(|error| LlmError::Other(format!("can't write a recording: {error}")))?;
        text.push('\n');
        let path = self.path(&recording.key);
        // Unchanged recordings are left alone so a re-record touches only
        // what changed.
        if std::fs::read_to_string(&path).is_ok_and(|existing| existing == text) {
            return Ok(());
        }
        let staging = path.with_extension("json.tmp");
        std::fs::write(&staging, text).map_err(io)?;
        std::fs::rename(&staging, &path).map_err(io)
    }
}

#[async_trait]
impl LlmProvider for RecordedProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let key = self.key_for(&req);
        let Some(inner) = &self.inner else {
            let recording = self.read(&key)?.ok_or_else(|| {
                LlmError::Other(format!(
                    "no recording for {} in {}; re-record with {RECORD_ENV}=1",
                    key.describe(),
                    self.dir.display()
                ))
            })?;
            return Ok(CompletionResponse {
                content: recording.response.content,
                model: recording.response.model,
                finish_reason: recording.response.finish_reason,
            });
        };
        let request = req
            .messages
            .iter()
            .map(|m| (format!("{:?}", m.role), m.content.clone()))
            .collect();
        let response = inner.complete(req).await?;
        self.write(&Recording {
            key,
            request,
            response: RecordedResponse {
                content: response.content.clone(),
                model: response.model.clone(),
                finish_reason: response.finish_reason.clone(),
            },
        })?;
        Ok(response)
    }

    fn capabilities(&self) -> LlmCapabilities {
        self.inner.as_ref().map_or(
            LlmCapabilities {
                context_window: u32::MAX,
                supports_streaming: false,
            },
            |inner| inner.capabilities(),
        )
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChatMessage;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl LlmProvider for Fake {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(CompletionResponse {
                content: format!(
                    "answer to {}",
                    req.messages.last().map_or("", |m| &m.content)
                ),
                model: "fake-1".into(),
                finish_reason: Some("stop".into()),
            })
        }
        fn capabilities(&self) -> LlmCapabilities {
            LlmCapabilities {
                context_window: 1000,
                supports_streaming: false,
            }
        }
        fn model_name(&self) -> &str {
            "fake-1"
        }
    }

    fn request(text: &str) -> CompletionRequest {
        CompletionRequest {
            messages: vec![ChatMessage::system("be brief"), ChatMessage::user(text)],
            max_tokens: Some(50),
            temperature: Some(0.0),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mxr-recorded-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn recorder(dir: &Path) -> (RecordedProvider, Arc<Fake>) {
        let fake = Arc::new(Fake {
            calls: AtomicUsize::new(0),
        });
        (
            RecordedProvider::record(fake.clone(), dir, "p1", "s1"),
            fake,
        )
    }

    fn files(dir: &Path) -> Vec<(String, String)> {
        let mut out: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    std::fs::read_to_string(entry.path()).unwrap(),
                )
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn ci_always_replays_whatever_the_record_variable_says() {
        let mode = RecordMode::mode_for;
        assert_eq!(mode(None, Some("1")), RecordMode::Record);
        assert_eq!(mode(None, Some("true")), RecordMode::Record);
        assert_eq!(mode(None, None), RecordMode::Replay);
        assert_eq!(mode(None, Some("0")), RecordMode::Replay);
        assert_eq!(mode(Some("true"), Some("1")), RecordMode::Replay);
        assert_eq!(mode(Some(""), Some("true")), RecordMode::Replay);
        assert_eq!(mode(Some("1"), None), RecordMode::Replay);
    }

    #[tokio::test]
    async fn replay_returns_what_record_wrote_without_calling_the_model() {
        let dir = scratch("roundtrip");
        let (recording, fake) = recorder(&dir);
        let live = recording
            .complete(request("is this a bill?"))
            .await
            .unwrap();
        assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
        assert_eq!(files(&dir).len(), 1);

        let replay = RecordedProvider::replay(&dir, "fake-1", "p1", "s1");
        assert_eq!(replay.mode(), RecordMode::Replay);
        let served = replay.complete(request("is this a bill?")).await.unwrap();
        assert_eq!(
            (served.content, served.model, served.finish_reason),
            (live.content, live.model, live.finish_reason)
        );
        assert_eq!(
            fake.calls.load(Ordering::SeqCst),
            1,
            "replay called no model"
        );
    }

    #[tokio::test]
    async fn a_missing_recording_is_a_hard_error_that_names_the_key() {
        let dir = scratch("missing");
        let replay = RecordedProvider::replay(&dir, "fake-1", "p1", "s1");
        let error = replay.complete(request("anything")).await.unwrap_err();
        let text = error.to_string();
        assert!(
            text.contains("no recording for fake-1 / prompt p1 / schema s1 / input "),
            "{text}"
        );
        assert!(text.contains(RECORD_ENV), "{text}");
    }

    #[tokio::test]
    async fn changing_the_model_prompt_schema_or_input_misses() {
        let dir = scratch("key");
        let (recording, _) = recorder(&dir);
        recording.complete(request("same")).await.unwrap();
        let hit = |model: &str, prompt: &str, schema: &str| {
            RecordedProvider::replay(&dir, model, prompt, schema)
        };
        assert!(hit("fake-1", "p1", "s1")
            .complete(request("same"))
            .await
            .is_ok());
        for (provider, text) in [
            (hit("fake-2", "p1", "s1"), "same"),
            (hit("fake-1", "p2", "s1"), "same"),
            (hit("fake-1", "p1", "s2"), "same"),
            (hit("fake-1", "p1", "s1"), "different"),
        ] {
            let error = provider.complete(request(text)).await.unwrap_err();
            assert!(error.to_string().contains("no recording for"), "{error}");
        }
        // Settings the model sees are part of the input.
        let mut hotter = request("same");
        hotter.temperature = Some(0.7);
        assert!(hit("fake-1", "p1", "s1").complete(hotter).await.is_err());
        let mut longer = request("same");
        longer.max_tokens = Some(500);
        assert!(hit("fake-1", "p1", "s1").complete(longer).await.is_err());
    }

    #[tokio::test]
    async fn the_same_inputs_give_the_same_files_in_any_order() {
        let (a, b) = (scratch("order-a"), scratch("order-b"));
        let inputs = ["one", "two", "three"];
        let (first, _) = recorder(&a);
        for text in inputs {
            first.complete(request(text)).await.unwrap();
        }
        let (second, _) = recorder(&b);
        for text in inputs.iter().rev() {
            second.complete(request(text)).await.unwrap();
        }
        assert_eq!(files(&a), files(&b));
        assert_eq!(files(&a).len(), 3);
        // Recording again changes nothing.
        let before = files(&a);
        let (again, _) = recorder(&a);
        again.complete(request("one")).await.unwrap();
        assert_eq!(files(&a), before);
        // The file ends in a newline and holds the request beside the answer.
        let text = &before[0].1;
        assert!(text.ends_with("}\n"));
        assert!(text.contains("be brief"));
    }

    #[tokio::test]
    async fn a_recording_filed_under_the_wrong_name_is_refused() {
        let dir = scratch("swapped");
        let (recording, _) = recorder(&dir);
        recording.complete(request("x")).await.unwrap();
        let replay = RecordedProvider::replay(&dir, "fake-1", "p1", "s1");
        let key = replay.key_for(&request("y"));
        let other = replay.key_for(&request("x"));
        std::fs::copy(replay.path(&other), replay.path(&key)).unwrap();
        let error = replay.complete(request("y")).await.unwrap_err();
        assert!(error.to_string().contains("different key"), "{error}");
    }

    #[tokio::test]
    async fn an_error_from_the_model_is_not_recorded() {
        struct Failing;
        #[async_trait]
        impl LlmProvider for Failing {
            async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, LlmError> {
                Err(LlmError::Empty)
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
        let dir = scratch("failing");
        let recording = RecordedProvider::record(Arc::new(Failing), &dir, "p", "s");
        assert!(recording.complete(request("x")).await.is_err());
        assert!(!dir.exists() || files(&dir).is_empty());
    }
}

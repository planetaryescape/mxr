use super::*;
use async_trait::async_trait;
use mxr_llm::{LlmError, ToolCall, ToolChatMessage, ToolChatRequest, ToolChatResponse, ToolSpec};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

struct ScriptedModel {
    replies: Mutex<VecDeque<Result<ToolChatResponse, LlmError>>>,
    requests: Mutex<Vec<ToolChatRequest>>,
    window: Option<u32>,
    leaves_machine: Option<bool>,
}

impl ScriptedModel {
    fn new(replies: Vec<ToolChatResponse>) -> Self {
        Self::with_results(replies.into_iter().map(Ok).collect(), None)
    }
    fn with_results(replies: Vec<Result<ToolChatResponse, LlmError>>, window: Option<u32>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
            window,
            leaves_machine: None,
        }
    }
    fn requests(&self) -> Vec<ToolChatRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl ChatModel for ScriptedModel {
    async fn chat(
        &self,
        request: ToolChatRequest,
        on_text: &(dyn for<'t> Fn(&'t str) + Send + Sync),
    ) -> Result<ToolChatResponse, LlmError> {
        self.requests.lock().unwrap().push(request);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(LlmError::Other("script ran out".into())));
        if let Ok(reply) = &reply {
            if !reply.text.is_empty() {
                on_text(reply.text.get(..1).unwrap_or_default());
                on_text(&reply.text);
            }
        }
        reply
    }
    fn context_window(&self) -> Option<u32> {
        self.window
    }
    fn leaves_machine(&self) -> Option<bool> {
        self.leaves_machine
    }
}

struct RecordingHost {
    tools: Vec<ToolSpec>,
    calls: Mutex<Vec<(String, Value)>>,
}

impl RecordingHost {
    fn new() -> Self {
        let tool = |name: &str| ToolSpec {
            name: name.into(),
            description: name.into(),
            input_schema: json!({"type": "object"}),
        };
        Self {
            tools: vec![
                tool("mxr_search"),
                tool("mxr_mutate"),
                tool("mxr_brand_new"),
            ],
            calls: Mutex::new(Vec::new()),
        }
    }
    fn calls(&self) -> Vec<(String, Value)> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl ToolHost for RecordingHost {
    fn tools(&self) -> &[ToolSpec] {
        &self.tools
    }
    async fn call(&self, name: &str, arguments: Value) -> ToolOutput {
        self.calls
            .lock()
            .unwrap()
            .push((name.to_string(), arguments.clone()));
        if name == "mxr_mutate" && arguments.get("confirm").is_none() {
            return ToolOutput {
                content: json!({"blocked": true, "reason": "confirm=true is required"}).to_string(),
                is_error: false,
            };
        }
        ToolOutput {
            content: json!({"tool": name, "ok": true}).to_string(),
            is_error: false,
        }
    }
}

#[derive(Default)]
struct MemorySink {
    appended: Mutex<Vec<ToolChatMessage>>,
    metas: Mutex<Vec<MessageMeta>>,
    written: Mutex<Vec<String>>,
    summaries: Mutex<Vec<Summary>>,
    progress: Mutex<Vec<Progress>>,
}

#[async_trait]
impl TurnSink for MemorySink {
    async fn append(&self, message: &ToolChatMessage, meta: &MessageMeta) -> Result<(), String> {
        self.appended.lock().unwrap().push(message.clone());
        self.metas.lock().unwrap().push(meta.clone());
        Ok(())
    }
    async fn progress(&self, progress: Progress) {
        self.progress.lock().unwrap().push(progress);
    }
    fn writing(&self, text: &str) {
        self.written.lock().unwrap().push(text.to_string());
    }
    async fn summarized(&self, summary: &Summary) -> Result<(), String> {
        self.summaries.lock().unwrap().push(summary.clone());
        Ok(())
    }
}

fn calls(calls: &[(&str, &str, Value)]) -> ToolChatResponse {
    ToolChatResponse {
        text: String::new(),
        tool_calls: calls
            .iter()
            .map(|(id, name, arguments)| ToolCall {
                id: (*id).into(),
                name: (*name).into(),
                arguments: arguments.clone(),
            })
            .collect(),
        model: "m".into(),
        finish_reason: None,
        usage: None,
        provider_turn: None,
    }
}

fn answer(text: &str) -> ToolChatResponse {
    ToolChatResponse {
        text: text.into(),
        ..calls(&[])
    }
}

fn agent<'a>(model: &'a ScriptedModel, host: &'a RecordingHost, sink: &'a MemorySink) -> Agent<'a> {
    Agent {
        model,
        host,
        sink,
        cancel: CancellationToken::new(),
        limits: Limits::default(),
        auto_approve: false,
        summary: Mutex::new(None),
    }
}

fn tool_result(history: &[ToolChatMessage], id: &str) -> (String, bool) {
    history
        .iter()
        .find_map(|message| match message {
            ToolChatMessage::Tool {
                call_id,
                content,
                is_error,
            } if call_id == id => Some((content.clone(), *is_error)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no result for {id}"))
}

#[tokio::test]
async fn decisions_for_calls_that_are_not_waiting_are_refused() {
    let model = ScriptedModel::new(vec![]);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    let result = agent(&model, &host, &sink)
        .resume(
            &mut history,
            vec![],
            HashMap::from([("ghost".into(), Decision::Approve)]),
        )
        .await;
    assert!(result.is_err());
    assert!(host.calls().is_empty());
}

#[tokio::test]
async fn reads_run_at_once_and_their_results_are_fenced() {
    let model = ScriptedModel::new(vec![
        calls(&[(
            "c1",
            "mxr_search",
            json!({"query": "bank", "reason": "Find bank mail."}),
        )]),
        answer("You have one."),
    ]);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    let outcome = agent(&model, &host, &sink)
        .send(&mut history, &[], "any bank mail?".into())
        .await;
    assert_eq!(outcome, TurnOutcome::Finished);
    assert_eq!(
        host.calls(),
        vec![("mxr_search".into(), json!({"query": "bank"}))]
    );
    let (content, _) = tool_result(&history, "c1");
    assert!(content.starts_with(mxr_llm::UNTRUSTED_MAIL_BEGIN));
    let tools = &model.requests()[0].tools;
    assert!(tools.iter().any(|tool| tool.name == ASK_USER_TOOL));
    assert!(tools.iter().all(|tool| tool.input_schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("reason"))));
    assert_eq!(*sink.appended.lock().unwrap(), history);
}

#[tokio::test]
async fn a_change_waits_with_its_preview_and_approval_runs_the_stored_arguments() {
    let commit = json!({"action": "archive", "message_ids": ["m1", "m2"], "confirm": true, "reason": "Archive them."});
    let model = ScriptedModel::new(vec![
        calls(&[("c1", "mxr_mutate", commit)]),
        answer("Archived 2."),
    ]);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    let assistant = agent(&model, &host, &sink);
    let outcome = assistant
        .send(&mut history, &[], "archive those".into())
        .await;
    let TurnOutcome::AwaitingUser(pending) = outcome else {
        panic!("expected a wait, got {outcome:?}");
    };
    assert_eq!(
        host.calls(),
        vec![(
            "mxr_mutation_preview".into(),
            json!({"action": "archive", "message_ids": ["m1", "m2"]})
        )]
    );
    let PendingKind::Approval {
        preview, reason, ..
    } = &pending[0].kind
    else {
        panic!("expected an approval");
    };
    assert_eq!(reason, "Archive them.");
    assert!(preview.is_some());

    let outcome = assistant
        .resume(
            &mut history,
            pending,
            HashMap::from([("c1".to_string(), Decision::Approve)]),
        )
        .await
        .unwrap();
    assert_eq!(outcome, TurnOutcome::Finished);
    assert_eq!(
        host.calls()[1],
        (
            "mxr_mutate".into(),
            json!({"action": "archive", "message_ids": ["m1", "m2"], "confirm": true})
        )
    );
    assert!(tool_result(&history, "c1")
        .0
        .starts_with("The user approved this call."));
}

#[tokio::test]
async fn a_rejected_change_never_runs_and_the_model_hears_why() {
    let model = ScriptedModel::new(vec![
        calls(&[(
            "c1",
            "mxr_mutate",
            json!({"action": "trash", "message_ids": ["m1"], "confirm": true}),
        )]),
        answer("Okay, left it."),
    ]);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    let assistant = agent(&model, &host, &sink);
    let TurnOutcome::AwaitingUser(pending) =
        assistant.send(&mut history, &[], "trash it".into()).await
    else {
        panic!("expected a wait");
    };
    let decision = Decision::Reject {
        reason: Some("archive instead".into()),
    };
    assistant
        .resume(
            &mut history,
            pending,
            HashMap::from([("c1".into(), decision)]),
        )
        .await
        .unwrap();
    assert!(host
        .calls()
        .iter()
        .all(|(_, args)| args["confirm"] != json!(true)));
    let (content, is_error) = tool_result(&history, "c1");
    assert!(is_error);
    assert!(content.contains("archive instead"));
}

#[tokio::test]
async fn ask_user_pauses_and_only_a_valid_answer_resumes() {
    let question = json!({"questions": [{"question": "Which account?", "options": [{"label": "Work"}, {"label": "Home"}], "allow_other": false}], "reason": "Two accounts match."});
    let model = ScriptedModel::new(vec![
        calls(&[("q1", ASK_USER_TOOL, question)]),
        answer("Using Work."),
    ]);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    let assistant = agent(&model, &host, &sink);
    let TurnOutcome::AwaitingUser(pending) =
        assistant.send(&mut history, &[], "clean up".into()).await
    else {
        panic!("expected a question");
    };
    assert!(matches!(pending[0].kind, PendingKind::Question { .. }));

    let bad = Decision::Answer {
        answers: vec![Answer {
            selected: vec!["Both".into()],
            other: None,
        }],
    };
    assert!(assistant
        .resume(
            &mut history,
            pending.clone(),
            HashMap::from([("q1".into(), bad)])
        )
        .await
        .is_err());
    assert!(assistant
        .resume(
            &mut history,
            pending.clone(),
            HashMap::from([("q1".into(), Decision::Approve)])
        )
        .await
        .is_err());

    let good = Decision::Answer {
        answers: vec![Answer {
            selected: vec!["Work".into()],
            other: None,
        }],
    };
    let outcome = assistant
        .resume(&mut history, pending, HashMap::from([("q1".into(), good)]))
        .await
        .unwrap();
    assert_eq!(outcome, TurnOutcome::Finished);
    let (content, _) = tool_result(&history, "q1");
    assert!(content.contains("\"selected\":[\"Work\"]"));
}

fn long_history() -> Vec<ToolChatMessage> {
    vec![
        ToolChatMessage::User {
            content: "first question".into(),
        },
        ToolChatMessage::Assistant {
            text: "x".repeat(20_000),
            tool_calls: Vec::new(),
            provider_turn: None,
        },
    ]
}

#[tokio::test]
async fn too_long_for_the_model_summarises_once_and_retries() {
    let model = ScriptedModel::with_results(
        vec![
            Err(LlmError::Other(
                "400: maximum context length is 8192 tokens".into(),
            )),
            Ok(answer("The user asked a first question.")),
            Ok(answer("Here you go.")),
        ],
        None,
    );
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = long_history();
    let outcome = agent(&model, &host, &sink)
        .send(&mut history, &[], "second question".into())
        .await;
    assert_eq!(outcome, TurnOutcome::Finished);
    let requests = model.requests();
    assert_eq!(requests.len(), 3);
    assert!(
        requests[1].tools.is_empty(),
        "the summary call can't use tools"
    );
    let retry = serde_json::to_string(&requests[2].messages).unwrap();
    assert!(retry.contains("The user asked a first question."));
    assert!(!retry.contains("first question\""));
    assert!(retry.contains("second question"));
    assert_eq!(sink.summaries.lock().unwrap()[0].through, 2);
    assert_eq!(history.len(), 4);
}

#[cfg(feature = "langfuse")]
#[tokio::test]
async fn a_tool_span_names_the_call_it_answers() {
    use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};

    let exporter = InMemorySpanExporter::default();
    opentelemetry::global::set_tracer_provider(
        SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build(),
    );
    let model = ScriptedModel::new(vec![
        calls(&[(
            "traced-call",
            "mxr_search",
            json!({"query": "bank", "reason": "Find bank mail."}),
        )]),
        answer("You have one."),
    ]);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    agent(&model, &host, &sink)
        .send(&mut Vec::new(), &[], "any bank mail?".into())
        .await;

    let attribute = |span: &opentelemetry_sdk::trace::SpanData, key: &str| {
        span.attributes
            .iter()
            .find(|attribute| attribute.key.as_str() == key)
            .map(|attribute| attribute.value.as_str().into_owned())
    };
    let spans = exporter.get_finished_spans().unwrap();
    let tool = spans
        .iter()
        .find(|span| {
            attribute(span, "langfuse.observation.metadata.tool_call_id").as_deref()
                == Some("traced-call")
        })
        .expect("the tool call is traced with its call id");
    assert_eq!(tool.name, "mxr_search");
    assert_eq!(
        attribute(tool, "langfuse.observation.type").as_deref(),
        Some("tool")
    );
    assert_eq!(
        attribute(tool, "langfuse.observation.input").as_deref(),
        Some(r#"{"query":"bank"}"#)
    );
}

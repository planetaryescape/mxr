use super::*;
use async_trait::async_trait;
use mxr_llm::{LlmError, ToolCall, ToolChatMessage, ToolChatRequest, ToolChatResponse, ToolSpec};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Replies in order and records every request it was sent.
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
    fn running_in(mut self, leaves_machine: bool) -> Self {
        self.leaves_machine = Some(leaves_machine);
        self
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
        // mxr's own guard: a change without its confirm flag doesn't run.
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
    // The tool list the model saw includes ask_user and requires a reason.
    let tools = &model.requests()[0].tools;
    assert!(tools.iter().any(|tool| tool.name == ASK_USER_TOOL));
    assert!(tools.iter().all(|tool| tool.input_schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("reason"))));
    assert_eq!(*sink.appended.lock().unwrap(), history);
}

#[tokio::test]
async fn a_call_mxr_refuses_counts_as_failed_not_done() {
    let model = ScriptedModel::new(vec![
        calls(&[(
            "c1",
            "mxr_mutate",
            json!({"action": "star", "message_ids": ["m1"], "reason": "Star it."}),
        )]),
        answer("It didn't run."),
    ]);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    agent(&model, &host, &sink)
        .send(&mut history, &[], "star m1".into())
        .await;
    let (content, is_error) = tool_result(&history, "c1");
    assert!(is_error, "a blocked call must not read as done: {content}");
    assert_eq!(
        crate::outcome::call_state(&content, is_error),
        crate::outcome::CallState::Failed
    );
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
    // Only the preview ran, through the read tool that lists the messages.
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
async fn writing_instead_of_answering_declines_what_was_waiting() {
    let model = ScriptedModel::new(vec![
        calls(&[("c1", "mxr_brand_new", json!({}))]),
        answer("Sure."),
    ]);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    let assistant = agent(&model, &host, &sink);
    let TurnOutcome::AwaitingUser(pending) = assistant
        .send(&mut history, &[], "do the new thing".into())
        .await
    else {
        panic!("an unclassified tool must wait");
    };
    assert!(
        host.calls().is_empty(),
        "no preview exists for an unknown tool"
    );
    let outcome = assistant
        .send(&mut history, &pending, "never mind".into())
        .await;
    assert_eq!(outcome, TurnOutcome::Finished);
    assert!(host.calls().is_empty());
    assert!(tool_result(&history, "c1").0.contains("not_answered"));
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
async fn a_turn_stops_at_its_step_limit() {
    let replies = (0..3)
        .map(|n| calls(&[(&*format!("c{n}"), "mxr_search", json!({}))]))
        .collect();
    let model = ScriptedModel::new(replies);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    let mut assistant = agent(&model, &host, &sink);
    assistant.limits.max_steps = 2;
    let outcome = assistant.send(&mut history, &[], "loop".into()).await;
    assert_eq!(outcome, TurnOutcome::StepLimit);
    assert_eq!(model.requests().len(), 2);
}

#[test]
fn an_interrupted_call_is_closed_in_model_input_but_not_in_history() {
    let history = vec![
        ToolChatMessage::User {
            content: "go".into(),
        },
        ToolChatMessage::Assistant {
            text: String::new(),
            tool_calls: vec![
                ToolCall {
                    id: "a".into(),
                    name: "mxr_search".into(),
                    arguments: json!({}),
                },
                ToolCall {
                    id: "b".into(),
                    name: "mxr_search".into(),
                    arguments: json!({}),
                },
            ],
            provider_turn: None,
        },
        ToolChatMessage::Tool {
            call_id: "a".into(),
            content: "{}".into(),
            is_error: false,
        },
        ToolChatMessage::User {
            content: "still there?".into(),
        },
    ];
    let input = model_input(&history);
    assert!(matches!(input[0], ToolChatMessage::System { .. }));
    let ids: Vec<&str> = input
        .iter()
        .filter_map(|message| match message {
            ToolChatMessage::Tool { call_id, .. } => Some(call_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(ids, vec!["a", "b"]);
    assert!(matches!(input.last(), Some(ToolChatMessage::User { .. })));
    assert_eq!(model_input(&history), input, "the repair is deterministic");
}

/// Answers every call with a fixed result per tool.
struct CannedHost {
    tools: Vec<ToolSpec>,
    results: HashMap<&'static str, Value>,
    calls: Mutex<Vec<(String, Value)>>,
}

#[async_trait]
impl ToolHost for CannedHost {
    fn tools(&self) -> &[ToolSpec] {
        &self.tools
    }
    async fn call(&self, name: &str, arguments: Value) -> ToolOutput {
        self.calls
            .lock()
            .unwrap()
            .push((name.to_string(), arguments));
        ToolOutput {
            content: self
                .results
                .get(name)
                .cloned()
                .unwrap_or_else(|| json!({}))
                .to_string(),
            is_error: false,
        }
    }
}

#[tokio::test]
async fn an_id_only_preview_gets_the_messages_it_names() {
    let tool = |name: &str| ToolSpec {
        name: name.into(),
        description: name.into(),
        input_schema: json!({"type": "object"}),
    };
    let host = CannedHost {
        tools: vec![
            tool("mxr_archive_submit"),
            tool("mxr_archive_outcome"),
            tool("mxr_mutation_preview"),
        ],
        results: HashMap::from([
            (
                "mxr_archive_outcome",
                json!({"operation": {"items": [{"message_id": "m1"}, {"message_id": "m2"}]}}),
            ),
            (
                "mxr_mutation_preview",
                json!({"preview": {"envelopes": [{"id": "m1", "subject": "One"}, {"id": "m2", "subject": "Two"}]}}),
            ),
        ]),
        calls: Mutex::new(Vec::new()),
    };
    let model = ScriptedModel::new(vec![calls(&[(
        "c1",
        "mxr_archive_submit",
        json!({"operation_id": "op", "confirm": true}),
    )])]);
    let sink = MemorySink::default();
    let mut history = Vec::new();
    let assistant = Agent {
        model: &model,
        host: &host,
        sink: &sink,
        cancel: CancellationToken::new(),
        limits: Limits::default(),
        auto_approve: false,
        summary: Mutex::new(None),
    };
    let TurnOutcome::AwaitingUser(pending) = assistant
        .send(&mut history, &[], "archive them".into())
        .await
    else {
        panic!("expected a wait");
    };
    let PendingKind::Approval { preview, .. } = &pending[0].kind else {
        panic!("expected an approval");
    };
    let preview = preview.as_ref().unwrap();
    assert_eq!(preview["messages"]["envelopes"][1]["subject"], "Two");
    assert_eq!(
        preview["detail"]["operation"]["items"][0]["message_id"],
        "m1"
    );
    let looked_up = host.calls.lock().unwrap()[1].clone();
    assert_eq!(looked_up.0, "mxr_mutation_preview");
    assert_eq!(looked_up.1["message_ids"], json!(["m1", "m2"]));
}

#[tokio::test]
async fn a_trusting_conversation_runs_reversible_changes_but_still_asks_for_the_rest() {
    let model = ScriptedModel::new(vec![calls(&[
        (
            "a",
            "mxr_mutate",
            json!({"action": "archive", "message_ids": ["m1"], "confirm": true}),
        ),
        (
            "t",
            "mxr_mutate",
            json!({"action": "trash", "message_ids": ["m2"], "confirm": true}),
        ),
    ])]);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    let mut assistant = agent(&model, &host, &sink);
    assistant.auto_approve = true;
    let TurnOutcome::AwaitingUser(pending) = assistant.send(&mut history, &[], "tidy".into()).await
    else {
        panic!("the trash must still wait");
    };
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].call_id, "t");
    let archive = &host.calls()[0];
    assert_eq!(archive.1["action"], "archive");
    assert_eq!(archive.1["confirm"], true);
    assert!(tool_result(&history, "a")
        .0
        .starts_with(crate::outcome::AUTO_APPROVED_PREFIX));
}

#[tokio::test]
async fn the_reply_streams_to_the_sink_as_it_is_written() {
    let model = ScriptedModel::new(vec![answer("Hello there.")]);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    agent(&model, &host, &sink)
        .send(&mut history, &[], "hi".into())
        .await;
    assert_eq!(*sink.written.lock().unwrap(), vec!["H", "Hello there."]);
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
    // The retry reads the summary instead of the first exchange, and keeps
    // the current request in full.
    let retry = serde_json::to_string(&requests[2].messages).unwrap();
    assert!(retry.contains("The user asked a first question."));
    assert!(!retry.contains("first question\""));
    assert!(retry.contains("second question"));
    assert_eq!(sink.summaries.lock().unwrap()[0].through, 2);
    // History itself is untouched.
    assert_eq!(history.len(), 4);
}

#[tokio::test]
async fn a_crowded_window_is_summarised_before_the_call() {
    let model = ScriptedModel::with_results(
        vec![Ok(answer("Summary.")), Ok(answer("Done."))],
        Some(8_000),
    );
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = long_history();
    agent(&model, &host, &sink)
        .send(&mut history, &[], "next".into())
        .await;
    assert_eq!(model.requests().len(), 2);
    assert_eq!(sink.summaries.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn an_approved_call_mxr_blocks_is_stored_as_a_failure_and_shows_as_running() {
    let model = ScriptedModel::new(vec![answer("Done.")]);
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    let assistant = agent(&model, &host, &sink);
    // An approved change whose arguments lack `confirm`: mxr's guard answers
    // `blocked`, as it does for a got-it without its token.
    let pending = vec![PendingItem {
        call_id: "c1".into(),
        kind: PendingKind::Approval {
            tool: "mxr_mutate".into(),
            arguments: json!({"action": "archive", "message_ids": ["m1"]}),
            reason: "Archive it.".into(),
            preview: None,
            preview_error: None,
        },
    }];
    assistant
        .resume(
            &mut history,
            pending,
            HashMap::from([("c1".to_string(), Decision::Approve)]),
        )
        .await
        .unwrap();
    let stored = sink.appended.lock().unwrap().clone();
    let Some(ToolChatMessage::Tool {
        content, is_error, ..
    }) = stored.first()
    else {
        panic!("expected the stored result, got {stored:?}");
    };
    assert!(*is_error, "a blocked approved call is an error");
    assert_eq!(call_state(content, *is_error), CallState::Failed);
    // Stored before the fix, with the error flag off, it still reads as failed.
    assert_eq!(call_state(content, false), CallState::Failed);
    let progress = sink.progress.lock().unwrap().clone();
    assert!(
        progress.iter().any(|p| matches!(p,
            Progress::Calling { tool, reason, arguments }
                if tool == "mxr_mutate" && reason == "Archive it." && arguments["action"] == "archive")),
        "the approved call shows as running: {progress:?}"
    );
}

/// The same conversation continued by a different model, as when the user
/// switches from a local model to a hosted one and back: each model gets the
/// whole history, every result still follows its call, and each reply
/// records which model wrote it and where that runs.
#[tokio::test]
async fn a_conversation_survives_switching_models_and_each_reply_keeps_its_own_mark() {
    let host = RecordingHost::new();
    let sink = MemorySink::default();
    let mut history = Vec::new();
    let named = |model: &str, mut response: ToolChatResponse| {
        response.model = model.into();
        response
    };

    let local = ScriptedModel::new(vec![
        named(
            "gemma4:latest",
            calls(&[("call_a", "mxr_search", json!({"q": "bank"}))]),
        ),
        named("gemma4:latest", answer("Two from the bank.")),
    ])
    .running_in(false);
    let outcome = agent(&local, &host, &sink)
        .send(&mut history, &[], "bank mail?".into())
        .await;
    assert!(matches!(outcome, TurnOutcome::Finished));

    let hosted = ScriptedModel::new(vec![
        named(
            "claude-haiku-5-5 via Anthropic",
            calls(&[("toolu_b", "mxr_search", json!({"q": "rent"}))]),
        ),
        named("claude-haiku-5-5 via Anthropic", answer("One about rent.")),
    ])
    .running_in(true);
    let outcome = agent(&hosted, &host, &sink)
        .send(&mut history, &[], "and rent?".into())
        .await;
    assert!(matches!(outcome, TurnOutcome::Finished));

    let local_again = ScriptedModel::new(vec![named("gemma4:latest", answer("Anything else?"))])
        .running_in(false);
    let outcome = agent(&local_again, &host, &sink)
        .send(&mut history, &[], "thanks".into())
        .await;
    assert!(matches!(outcome, TurnOutcome::Finished));

    // The hosted model saw the local model's whole turn, and the local model
    // saw both, every call answered by the result with its id.
    let hosted_saw = &hosted.requests()[0].messages;
    assert!(hosted_saw
        .iter()
        .any(|m| matches!(m, ToolChatMessage::Tool { call_id, .. } if call_id == "call_a")));
    let last_saw = &local_again.requests()[0].messages;
    for id in ["call_a", "toolu_b"] {
        let call = last_saw.iter().position(|m| matches!(m, ToolChatMessage::Assistant { tool_calls, .. } if tool_calls.iter().any(|c| c.id == id)));
        let result = last_saw
            .iter()
            .position(|m| matches!(m, ToolChatMessage::Tool { call_id, .. } if call_id == id));
        assert!(
            call.is_some() && result > call,
            "{id} must be answered after it is called"
        );
    }

    // Each reply was stored with the model and place that wrote it.
    let replies: Vec<(Option<String>, Option<bool>)> = sink
        .appended
        .lock()
        .unwrap()
        .iter()
        .zip(sink.metas.lock().unwrap().iter())
        .filter(|(message, _)| matches!(message, ToolChatMessage::Assistant { text, .. } if !text.is_empty()))
        .map(|(_, meta)| (meta.model.clone(), meta.leaves_machine))
        .collect();
    assert_eq!(
        replies,
        [
            (Some("gemma4:latest".into()), Some(false)),
            (Some("claude-haiku-5-5 via Anthropic".into()), Some(true)),
            (Some("gemma4:latest".into()), Some(false)),
        ]
    );
}

/// A traced tool call names the model's call id, so Langfuse can pair the
/// result with its request. The call id is unique to this test because
/// the tracer provider is global to the test binary.
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

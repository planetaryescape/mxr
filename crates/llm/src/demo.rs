//! Canned LLM provider for `mxr demo`.
//!
//! Returns realistic-looking responses without making any network calls,
//! so the demo profile can showcase summarize / briefing / ask / draft-assist
//! / voice / commitments / decisions without an API key configured.
//!
//! The runtime dispatches on the **system prompt** rather than on `LlmFeature`
//! because feature context isn't carried inside `CompletionRequest`. Each
//! feature's handler uses a stable, distinctive system prompt (see
//! `crates/daemon/src/handler/{summarize,briefing,draft_assist,…}.rs`), so a
//! small keyword match selects the right canned template. The fallback
//! template is still useful — better to return a vaguely-helpful reply than
//! to error mid-recording.

use async_trait::async_trait;

use crate::{
    ChatRole, CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider,
};

#[derive(Debug, Clone, Default)]
pub struct DemoLlmProvider;

impl DemoLlmProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl LlmProvider for DemoLlmProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let kind = classify(&req);
        let content = render(kind, &req);
        Ok(CompletionResponse {
            content,
            model: "mxr-demo-canned".to_string(),
            finish_reason: Some("stop".to_string()),
        })
    }

    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            context_window: 128_000,
            supports_streaming: false,
        }
    }

    fn model_name(&self) -> &str {
        "mxr-demo-canned"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FeatureKind {
    Summarize,
    Briefing,
    DraftAssist,
    DraftNew,
    VoiceMatch,
    HumanizeRewrite,
    AnswerCoverage,
    ArchiveAsk,
    Commitments,
    DecisionLog,
    RelationshipSummary,
    Expert,
    Generic,
}

fn classify(req: &CompletionRequest) -> FeatureKind {
    let system = req
        .messages
        .iter()
        .find(|message| matches!(message.role, ChatRole::System))
        .map(|message| message.content.to_ascii_lowercase())
        .unwrap_or_default();

    // Order matters: more-specific phrases first so e.g. "draft a briefing"
    // doesn't get caught by the draft branch.
    if system.contains("briefing") {
        FeatureKind::Briefing
    } else if system.contains("summarize email") || system.contains("summarize") {
        FeatureKind::Summarize
    } else if system.contains("draft email replies") {
        FeatureKind::DraftAssist
    } else if system.contains("draft") && (system.contains("new") || system.contains("compose")) {
        FeatureKind::DraftNew
    } else if system.contains("voice") || system.contains("register") {
        FeatureKind::VoiceMatch
    } else if system.contains("humanize") || system.contains("humanise") {
        FeatureKind::HumanizeRewrite
    } else if system.contains("answer coverage") || system.contains("coverage") {
        FeatureKind::AnswerCoverage
    } else if system.contains("commitments") || system.contains("commitment") {
        FeatureKind::Commitments
    } else if system.contains("decisions") || system.contains("decision log") {
        FeatureKind::DecisionLog
    } else if system.contains("relationship") {
        FeatureKind::RelationshipSummary
    } else if system.contains("expert") || system.contains("who answers") {
        FeatureKind::Expert
    } else if system.contains("ask") || system.contains("question") || system.contains("citation") {
        FeatureKind::ArchiveAsk
    } else {
        FeatureKind::Generic
    }
}

fn render(kind: FeatureKind, req: &CompletionRequest) -> String {
    let user = req
        .messages
        .iter()
        .find(|message| matches!(message.role, ChatRole::User))
        .map_or("", |message| message.content.as_str());
    let subject_hint = extract_subject(user);

    match kind {
        FeatureKind::Summarize => format!(
            "Summary of {subject_hint}:\n\
             - Alex and the build team agreed to ship the v0.6 cut by Friday.\n\
             - Open question: should the migration run before or after the freeze window?\n\
             - Diana flagged a perf regression in the search path that needs a follow-up.\n\
             \n\
             Next steps:\n\
             - Reply to Alex confirming the Friday cut.\n\
             - Loop Diana in on the perf thread.",
        ),
        FeatureKind::Briefing => format!(
            "Briefing — {subject_hint}\n\
             \n\
             People: Alex (lead), Diana (review), Carol (platform).\n\
             Status: on track, two open follow-ups.\n\
             Decisions: ship v0.6 on Friday; defer migration to next week.\n\
             Commitments: you owe Alex a Friday confirmation; Diana owes a perf summary.\n\
             Risk: search regression is unconfirmed but plausible.",
        ),
        FeatureKind::DraftAssist | FeatureKind::DraftNew => "Thanks for the heads-up — that timing works on my end. \
             I'll have the Friday cut ready and will loop you in once the migration window is locked. \
             If anything shifts, I'll send a follow-up before EOD Thursday so we have a clean handoff."
            .to_string(),
        FeatureKind::VoiceMatch => {
            // Echo the user input lightly massaged so voice-match feels real.
            format!(
                "Voice-matched draft:\n\n{}\n\n(Tone: warm, direct, low-formality — calibrated to your recent outbound register.)",
                user.chars().take(400).collect::<String>(),
            )
        }
        FeatureKind::HumanizeRewrite => format!(
            "{}\n\n(Rewritten for plain-spoken tone; removed AI tells like \"delve\", \"in conclusion\", and the em-dash hedge.)",
            user.chars().take(400).collect::<String>(),
        ),
        FeatureKind::AnswerCoverage => "Coverage: 2 of 3 questions answered.\n\
             - \"Is the Friday cut still on?\" — addressed.\n\
             - \"What about the migration window?\" — addressed.\n\
             - \"Who owns the perf follow-up?\" — NOT addressed."
            .to_string(),
        FeatureKind::ArchiveAsk => "Based on the thread history:\n\
             We decided on Tuesday to ship v0.6 by Friday and to defer the schema migration to the following week. \
             Alex confirmed the freeze window starts at 17:00 local. \
             Open follow-up: Diana to share a perf-regression summary by EOD Thursday.\n\
             \n\
             Citations: [msg_d4f0c1, msg_e91a32]"
            .to_string(),
        FeatureKind::Commitments => r#"[
  {
    "kind": "yours",
    "summary": "Confirm Friday v0.6 cut to Alex",
    "due": "this week",
    "evidence_message_ids": ["msg_d4f0c1"]
  },
  {
    "kind": "owed",
    "summary": "Diana to share perf-regression summary",
    "due": "Thursday EOD",
    "evidence_message_ids": ["msg_e91a32"]
  }
]"#
        .to_string(),
        FeatureKind::DecisionLog => r#"[
  {
    "topic": "v0.6 release timing",
    "decision": "Ship the Friday cut at 17:00 local. Skip the late-Thursday changes.",
    "decided_by": ["alex@work.com", "you"],
    "evidence_message_ids": ["msg_d4f0c1"]
  },
  {
    "topic": "schema migration",
    "decision": "Defer migration to the following sprint; keep current schema for v0.6.",
    "decided_by": ["alex@work.com", "carol@work.com"],
    "evidence_message_ids": ["msg_b1e2a8"]
  }
]"#
        .to_string(),
        FeatureKind::RelationshipSummary => "Relationship profile:\n\
             - High response-cadence collaborator; replies within ~2 hours on average.\n\
             - Predominantly work-context; rare off-hours threads.\n\
             - Open commitments: 1 of yours, 2 of theirs.\n\
             - Tone: direct, technical, low-formality. Matches your default voice."
            .to_string(),
        FeatureKind::Expert => "Likely best person to ask: Diana (carol@work.com is a strong second choice).\n\
             Diana has answered 4 prior questions on this topic over the past quarter, with an average\n\
             response time of 90 minutes."
            .to_string(),
        FeatureKind::Generic => "Demo response: this is a canned reply from `mxr demo`. \
             Configure an `[llm]` section and an API key to get real model output."
            .to_string(),
    }
}

/// Pulls a plausible subject hint out of the user prompt so summaries and
/// briefings read like they're about a real thread. Falls back to a generic
/// label if no subject line is present.
fn extract_subject(user: &str) -> String {
    for line in user.lines().take(40) {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("subject:") {
            let subject = rest.trim();
            if !subject.is_empty() {
                return subject.chars().take(80).collect();
            }
        }
    }
    "this thread".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChatMessage;

    fn req(system: &str, user: &str) -> CompletionRequest {
        CompletionRequest {
            messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
            max_tokens: None,
            temperature: None,
        }
    }

    #[tokio::test]
    async fn classify_summarize_branch() {
        let provider = DemoLlmProvider::new();
        let resp = provider
            .complete(req(
                "You summarize email conversation threads for a busy reader.",
                "Subject: Friday cut\n\nLet's confirm the timing.",
            ))
            .await
            .expect("complete ok");
        assert!(resp.content.starts_with("Summary of"), "{}", resp.content);
        assert!(resp.content.contains("Friday cut"));
    }

    #[tokio::test]
    async fn classify_briefing_branch() {
        let provider = DemoLlmProvider::new();
        let resp = provider
            .complete(req(
                "Produce a thread briefing with people, decisions, commitments.",
                "Subject: Q4 planning",
            ))
            .await
            .expect("complete ok");
        assert!(resp.content.starts_with("Briefing"), "{}", resp.content);
    }

    #[tokio::test]
    async fn classify_draft_branch() {
        let provider = DemoLlmProvider::new();
        let resp = provider
            .complete(req(
                "You draft email replies for a busy professional.",
                "Reply confirming Friday.",
            ))
            .await
            .expect("complete ok");
        assert!(resp.content.contains("Friday"), "{}", resp.content);
    }

    #[tokio::test]
    async fn classify_commitments_returns_json_array() {
        let provider = DemoLlmProvider::new();
        let resp = provider
            .complete(req(
                "Extract commitments from this thread.",
                "Subject: budget",
            ))
            .await
            .expect("complete ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&resp.content).expect("commitments JSON parses");
        assert!(parsed.as_array().is_some_and(|items| !items.is_empty()));
    }

    #[tokio::test]
    async fn unknown_system_prompt_falls_back_to_generic() {
        let provider = DemoLlmProvider::new();
        let resp = provider
            .complete(req("You are a helpful assistant.", "Tell me something."))
            .await
            .expect("complete ok");
        assert!(resp.content.contains("Demo response"), "{}", resp.content);
    }
}

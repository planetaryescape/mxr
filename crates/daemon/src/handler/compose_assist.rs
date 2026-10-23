//! Writing help (blueprint 34): `ComposeAssist` gathers what the model needs,
//! plans the prompt, makes one model call, checks the answer and returns a
//! proposal. It changes nothing; `ApplyComposeProposal` writes a proposal into
//! a stored draft's text region, and only if that region is still what the
//! proposal was made against.
//!
//! The privacy policy is `DraftCompose`'s: a cloud model sees the user's other
//! mail only with `llm.allow_cloud_relationship_data`, and typed text that an
//! earlier suggestion wrote from that history is refused for a cloud model
//! without the opt-in, as `DraftRefine` refuses it.

use super::compose_proposals::StoredProposal;
use super::diagnostics_impl::emit_operation_event;
use super::draft_compose::{
    complete_with_retry, max_tokens_for_words, prompt_budget_chars, target_words, voice_inputs,
    MAX_INSTRUCTION_BYTES,
};
use super::draft_prompt::{self, DraftMode, Help, PromptInput, Turn};
use super::draft_provenance::DraftPolicy;
use super::draft_refine::HISTORY_BODY_REFUSAL;
use super::draft_voice::{self, Counterparty};
use super::{draft_context, draft_output, mutations, HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_composer::check::{self, ProposalInput};
use mxr_composer::plan;
use mxr_core::id::{AccountId, DraftId, ThreadId};
use mxr_core::types::{Address, Draft, DraftContent, Envelope};
use mxr_llm::LlmFeature;
use mxr_protocol::{
    AssistActionData, AssistAdjustData, ClientKind, ComposeProposalData, DaemonEvent,
    DraftTargetData, ResponseData, TextRangeData,
};

/// The operation name on each progress event, so a client can tell this
/// request's stage lines from any other operation's.
pub(crate) const PROGRESS_OPERATION: &str = "compose-assist";

/// Stage lines are sent as `current` of this many.
const STAGES: u32 = 3;

/// Which version of the plan and check rules produced a proposal. Bumped when
/// a prompt or check change could change what a proposal says.
const COMPOSER_VERSION: &str = "c1";

/// What the request names, resolved to the conversation and people involved.
struct Gathered {
    account_id: AccountId,
    mode: DraftMode,
    counterparty: Option<Counterparty>,
    turns: Vec<Turn>,
    thread_id: Option<ThreadId>,
    /// The stored draft the request named, if it did.
    draft_id: Option<DraftId>,
    feature: LlmFeature,
}

pub(super) struct AssistRequest<'a> {
    pub draft: &'a DraftTargetData,
    pub text: &'a str,
    pub selection: Option<TextRangeData>,
    pub action: Option<AssistActionData>,
    pub instruction: Option<&'a str>,
    pub adjust: AssistAdjustData,
}

pub(super) async fn compose_assist(
    state: &AppState,
    source: ClientKind,
    request: AssistRequest<'_>,
) -> HandlerResult {
    let instruction = request.instruction.map_or("", str::trim);
    if instruction.len() > MAX_INSTRUCTION_BYTES {
        return Err(HandlerError::InvalidRequest(format!(
            "the instruction is {} bytes, over the {MAX_INSTRUCTION_BYTES} byte limit; shorten it",
            instruction.len()
        )));
    }
    let resolved = plan::resolve(request.action, request.text, request.selection)
        .map_err(|error| HandlerError::InvalidRequest(error.to_string()))?;

    let operation_id = uuid::Uuid::now_v7().to_string();
    let progress = |account_id: Option<&AccountId>, current: u32, message: String| {
        emit_operation_event(
            state,
            DaemonEvent::OperationProgress {
                operation_id: operation_id.clone(),
                operation: PROGRESS_OPERATION.to_string(),
                account_id: account_id.cloned(),
                current,
                total: Some(STAGES),
                message,
            },
        );
    };

    progress(None, 1, "Reading the conversation".to_string());
    let gathered = gather(state, request.draft).await?;
    let account_id = &gathered.account_id;
    let me = draft_voice::me(state, account_id).await;

    // The user's other mail only reaches a cloud model with their opt-in.
    let policy = DraftPolicy::pin(state, gathered.feature);
    let share = policy.share_history;
    let typed = resolved.action != AssistActionData::Write && !request.text.trim().is_empty();
    // Typed text may carry an earlier suggestion written from history.
    let typed_from_history = typed
        && !crate::history_text::source_accounts(&state.store, Some(account_id), request.text)
            .await?
            .is_empty();
    if typed_from_history && !share {
        return Err(HandlerError::InvalidRequest(
            HISTORY_BODY_REFUSAL.to_string(),
        ));
    }

    if let Some(person) = &gathered.counterparty {
        draft_context::ensure_contact_fresh(state, account_id, &person.email).await;
    }
    let (background, material) = voice_inputs(
        state,
        account_id,
        &policy,
        gathered.counterparty.as_ref(),
        gathered.thread_id.as_ref(),
        None,
    )
    .await;

    let typed_words = u32::try_from(request.text.split_whitespace().count()).unwrap_or(u32::MAX);
    let words = plan::target_words(target_words(None, &material), resolved.action, typed_words);
    let max_tokens = max_tokens_for_words(words);
    let budget_chars = prompt_budget_chars(state.llm.capabilities().context_window, max_tokens);
    let today = draft_voice::today();
    let prompt = draft_prompt::build(&PromptInput {
        mode: gathered.mode,
        me: &me,
        today: &today,
        habits: &material.habits,
        examples: &material.examples,
        background: background.as_deref(),
        turns: &gathered.turns,
        tone: None,
        target_words: words,
        instruction,
        help: Help {
            action: resolved.action,
            typed: request.text,
            selection: resolved.selection.clone(),
            adjust: request.adjust,
        },
        budget_chars,
    });

    progress(
        Some(account_id),
        2,
        format!("Writing with {}", policy.llm.model_name()),
    );
    let (raw, model) = complete_with_retry(&policy.llm, &prompt, max_tokens).await?;
    let cleaned = draft_output::clean_draft(&raw, me.name.as_deref());
    if cleaned.trim().is_empty() {
        return Err(HandlerError::Message(
            "The model sent back nothing. Try again, or pick another model.".to_string(),
        ));
    }

    progress(Some(account_id), 3, "Checking".to_string());
    let seen: Vec<&str> = prompt
        .turns_kept
        .iter()
        .map(|index| gathered.turns[*index].text.as_str())
        .collect();
    let known = [seen.join("\n"), instruction.to_string(), today.clone()].join("\n");
    let checked = check::check_proposal(&ProposalInput {
        action: resolved.action,
        typed: request.text,
        selection: resolved.selection,
        output: &cleaned,
        known: &known,
    });

    // Only the examples that fit the prompt shaped the proposal.
    let examples_used = &material.examples[..prompt.examples_used];
    let history_used = typed_from_history
        || !examples_used.is_empty()
        || !material.habits.is_empty()
        || prompt.background_used;
    let provenance = policy.provenance(
        &model,
        history_used,
        examples_used
            .iter()
            .map(|example| example.source.clone())
            .collect(),
        prompt
            .turns_kept
            .iter()
            .map(|index| gathered.turns[*index].source.clone())
            .collect(),
    );
    // Remembered so a later refine of this text can't hand it to a cloud model
    // without opt-in (see `history_text`).
    if history_used {
        crate::history_text::record(&state.store, account_id, &checked.text).await;
    }

    let proposal = ComposeProposalData {
        proposal_id: uuid::Uuid::now_v7().to_string(),
        action: resolved.action,
        base: check::text_hash(request.text),
        text: checked.text,
        changes: checked.changes,
        gaps: checked.gaps,
        provenance,
        yours_changed: checked.yours_changed,
        composer_version: COMPOSER_VERSION.to_string(),
    };
    record_activity(state, source, account_id, &proposal, &checked.fixes);
    state.compose_proposals.insert(StoredProposal {
        account_id: account_id.clone(),
        draft_id: gathered.draft_id,
        proposal: proposal.clone(),
    });
    Ok(ResponseData::ComposeProposal { proposal })
}

/// What the activity log learns: which action ran, on what model and how much
/// came back. Never the text, the instruction or the conversation.
fn record_activity(
    state: &AppState,
    source: ClientKind,
    account_id: &AccountId,
    proposal: &ComposeProposalData,
    fixes: &check::Fixes,
) {
    state.activity.record(crate::activity::OwnedEntry {
        ts: crate::activity::current_unix_ms(),
        account_id: Some(account_id.as_str()),
        source,
        action: "draft.assist".to_string(),
        target_kind: Some("draft".to_string()),
        target_id: None,
        tier: crate::activity::tier::tier_for("draft.assist"),
        context: Some(serde_json::json!({
            "action": proposal.action,
            "model": proposal.provenance.model,
            "locality": proposal.provenance.locality,
            "history_used": proposal.provenance.history_used,
            "text_bytes": proposal.text.len(),
            "yours_changed": proposal.yours_changed.len(),
            "gaps": proposal.gaps.len(),
            "figures_to_gaps": fixes.figures_to_gaps,
            "regions_removed": fixes.regions_removed,
            "outside_reverted": fixes.outside_reverted,
        })),
    });
}

async fn gather(state: &AppState, target: &DraftTargetData) -> Result<Gathered, HandlerError> {
    match target {
        DraftTargetData::Stored { draft_id } => {
            let draft = state
                .store
                .get_draft(draft_id)
                .await?
                .ok_or_else(|| HandlerError::Message(format!("Draft {draft_id} not found")))?;
            let recipient = draft.to.first().cloned();
            // The parent message, when the draft replies to one.
            let parent = match draft.reply_headers.as_ref() {
                Some(headers) => state
                    .store
                    .list_envelopes_by_message_id_header(&draft.account_id, &headers.in_reply_to)
                    .await?
                    .pop(),
                None => None,
            };
            let mut gathered = match (parent, draft.intent) {
                (Some(parent), mxr_core::DraftIntent::Forward) => {
                    let to = recipient.ok_or_else(no_recipient)?;
                    in_conversation(state, &parent.id, Some(to)).await?
                }
                (Some(parent), _) => in_conversation(state, &parent.id, None).await?,
                (None, _) => {
                    let to = recipient.ok_or_else(no_recipient)?;
                    new_message(state, draft.account_id.clone(), &to)
                }
            };
            gathered.draft_id = Some(draft_id.clone());
            Ok(gathered)
        }
        DraftTargetData::Reply { message_id, .. } => in_conversation(state, message_id, None).await,
        DraftTargetData::Forward { message_id, to } => {
            let to = to.first().cloned().ok_or_else(no_recipient)?;
            in_conversation(state, message_id, Some(to)).await
        }
        DraftTargetData::New { account_id, to, .. } => {
            let to = to.first().ok_or_else(no_recipient)?;
            Ok(new_message(state, account_id.clone(), to))
        }
    }
}

fn no_recipient() -> HandlerError {
    HandlerError::InvalidRequest("Writing help needs a recipient to write to.".to_string())
}

/// A reply to `message_id`, or a note forwarding its conversation to `forward_to`.
async fn in_conversation(
    state: &AppState,
    message_id: &mxr_core::MessageId,
    forward_to: Option<Address>,
) -> Result<Gathered, HandlerError> {
    let envelopes: Vec<Envelope> = draft_context::resolve_thread_envelopes(state, message_id).await;
    let Some(first) = envelopes.first() else {
        return Err(HandlerError::InvalidRequest(format!(
            "Message {message_id} isn't synced here, so there is no conversation to write into."
        )));
    };
    let account_id = first.account_id.clone();
    let thread_id = first.thread_id.clone();
    let me = draft_voice::me(state, &account_id).await;
    let owned = draft_voice::owned(&me);
    let target = draft_voice::pick_target(&envelopes, &owned, Some(message_id));
    let counterparty = match &forward_to {
        Some(address) => Some(Counterparty {
            email: address.email.to_ascii_lowercase(),
            name: address.name.clone(),
        }),
        None => target.and_then(|target| draft_voice::counterparty_of(target, &owned)),
    };
    let mode = match &forward_to {
        Some(_) => DraftMode::Forward {
            to: counterparty
                .as_ref()
                .map(Counterparty::label)
                .unwrap_or_default(),
        },
        None => DraftMode::Reply,
    };
    let target_id = target.map(|envelope| envelope.id.clone());
    let turns = draft_voice::conversation(state, &envelopes, &owned, target_id.as_ref()).await;
    Ok(Gathered {
        account_id,
        mode,
        counterparty,
        turns,
        thread_id: Some(thread_id),
        draft_id: None,
        feature: LlmFeature::DraftAssist,
    })
}

fn new_message(_state: &AppState, account_id: AccountId, to: &Address) -> Gathered {
    let counterparty = Counterparty {
        email: to.email.to_ascii_lowercase(),
        name: to.name.clone(),
    };
    Gathered {
        account_id,
        mode: DraftMode::New {
            to: counterparty.label(),
        },
        counterparty: Some(counterparty),
        turns: Vec::new(),
        thread_id: None,
        draft_id: None,
        feature: LlmFeature::DraftNew,
    }
}

/// Write a proposal into the stored draft's text region. The signature and the
/// forward header stay as they are. Refused when the region is no longer what
/// the proposal was made against, or the proposal was made for another draft.
pub(super) async fn apply(
    state: &AppState,
    proposal_id: &str,
    draft_id: &DraftId,
) -> HandlerResult {
    let stored = state.compose_proposals.get(proposal_id).ok_or_else(|| {
        HandlerError::InvalidRequest(
            "That proposal expired or was never made. Ask for writing help again.".to_string(),
        )
    })?;
    if stored
        .draft_id
        .as_ref()
        .is_some_and(|bound| bound != draft_id)
    {
        return Err(HandlerError::InvalidRequest(
            "That proposal was made for a different draft.".to_string(),
        ));
    }
    let draft = state
        .store
        .get_draft(draft_id)
        .await?
        .ok_or_else(|| HandlerError::Message(format!("Draft {draft_id} not found")))?;
    if draft.account_id != stored.account_id {
        return Err(HandlerError::InvalidRequest(
            "That proposal was made for another account.".to_string(),
        ));
    }
    let DraftContent::Markdown { source } = &draft.content else {
        return Err(HandlerError::InvalidRequest(
            "Writing help can't change an HTML draft.".to_string(),
        ));
    };
    let regions = check::split_body(source);
    if check::text_hash(regions.text) != stored.proposal.base {
        return Err(HandlerError::InvalidRequest(
            "The draft's text has changed since this proposal was made; nothing was written. \
             Ask for writing help again."
                .to_string(),
        ));
    }
    let updated = Draft {
        content: DraftContent::markdown(regions.with_text(&stored.proposal.text)),
        updated_at: chrono::Utc::now(),
        ..draft.clone()
    };
    // The write checks the revision read above, so a save that lands in
    // between is a conflict, not a lost update.
    mutations::update_draft(state, &updated).await?;
    state.compose_proposals.remove(proposal_id);
    Ok(ResponseData::ComposeApplied {
        draft_id: draft_id.clone(),
        text_hash: check::text_hash(&stored.proposal.text),
    })
}

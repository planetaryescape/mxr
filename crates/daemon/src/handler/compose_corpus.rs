//! The labelled compose corpus as the product asks for each draft: the
//! prompt `draft_prompt::build` gives for a case, and the grade the composer's
//! check stage gives a reply to it. Pure: no store and no model, so the same
//! code serves the snapshot test and `mxr draft eval --set quick`.
//!
//! A case's typed text and selection reach the prompt the way a `ComposeAssist`
//! request carries them: typed text is finished, a selection is edited.

use super::draft_compose::{max_tokens_for_words, target_words};
use super::draft_context::ASSEMBLED_MESSAGE_BUDGET_CHARS;
use super::draft_prompt::{self, DraftMode, Help, Me, Prompt, PromptInput, Turn, VoiceExample};
use super::draft_voice::VoiceMaterial;
use chrono::{DateTime, Utc};
use mxr_composer::check::{Answer, Output, Report, Requirements, WordBand};
use mxr_composer::plan;
use mxr_core::id::{MessageId, ThreadId};
use mxr_protocol::DraftSourceData;
use mxr_protocol::TextRangeData;
use mxr_provider_fake::fixtures::compose::{self, Case, DraftState};
use mxr_relationship::analyse_habits;

/// The prompt for one case, and how many tokens its reply may take.
pub(crate) struct Assembled {
    pub prompt: Prompt,
    pub max_tokens: u32,
}

/// What the product would send for this case. Mirrors the reply path of
/// `draft_compose` (no relationship summary, no tone, no length hint), with
/// the corpus standing in for the user's stored mail.
pub(crate) fn assemble(case: &Case, today: &str) -> Assembled {
    let me = Me {
        name: Some(compose::ME_NAME.to_string()),
        emails: vec![compose::ME_EMAIL.to_string()],
    };
    let thread_id = ThreadId::new();
    let turns: Vec<Turn> = case
        .thread
        .iter()
        .map(|message| Turn {
            from_me: message.from_me,
            who: message.who.to_string(),
            when: message.when.to_string(),
            text: message.text.to_string(),
            target: message.target,
            source: source(
                &thread_id,
                message.from_me,
                address_of(message.who),
                message.who,
            ),
        })
        .collect();
    let examples: Vec<VoiceExample> = case
        .voice
        .iter()
        .map(|sent| VoiceExample {
            to: sent.to.to_string(),
            their_message: sent.answering.map(str::to_string),
            my_email: sent.text.to_string(),
            source: source(&thread_id, true, sent.to, sent.to),
        })
        .collect();

    let bodies: Vec<&str> = case.voice.iter().map(|sent| sent.text).collect();
    let analysed = analyse_habits(&bodies, &[]);
    let mut habits = analysed.describe();
    // The same note `voice_material` adds when the user has written little
    // to this person.
    let counterparty = case
        .thread
        .iter()
        .find(|message| message.target)
        .map_or("", |message| address_of(message.who));
    let personal = case
        .voice
        .iter()
        .filter(|sent| sent.to.eq_ignore_ascii_case(counterparty))
        .count();
    if personal < 3 && !habits.is_empty() {
        habits.insert(
            0,
            "(From my emails in general; I haven't written to this person much.)".to_string(),
        );
    }
    let material = VoiceMaterial {
        examples,
        habits,
        median_words: (analysed.samples >= 3).then_some(analysed.median_words),
        samples: analysed.samples,
    };
    let (typed, selection) = match case.draft {
        DraftState::Empty => ("", None),
        DraftState::Typed(text) => (text, None),
        DraftState::Selection { typed, start, end } => (
            typed,
            Some(TextRangeData {
                start: u32::try_from(start).unwrap_or(u32::MAX),
                end: u32::try_from(end).unwrap_or(u32::MAX),
            }),
        ),
    };
    // The corpus is hand-written to be valid; a bad case is a bug in it.
    let resolved = plan::resolve(None, typed, selection).expect("corpus case resolves");
    let typed_words = u32::try_from(typed.split_whitespace().count()).unwrap_or(u32::MAX);
    let words = plan::target_words(target_words(None, &material), resolved.action, typed_words);
    let prompt = draft_prompt::build(&PromptInput {
        mode: DraftMode::Reply,
        me: &me,
        today,
        habits: &material.habits,
        examples: &material.examples,
        background: None,
        turns: &turns,
        tone: None,
        target_words: words,
        instruction: case.instruction,
        subject: "",
        mail_subject: "",
        help: Help {
            action: resolved.action,
            typed,
            selection: resolved.selection,
            adjust: Default::default(),
        },
        budget_chars: ASSEMBLED_MESSAGE_BUDGET_CHARS,
    });
    Assembled {
        prompt,
        max_tokens: max_tokens_for_words(words),
    }
}

/// Grade a reply to a case. `raw` is the model's output before clean-up and
/// `text` the draft after it; leftovers are read from `raw`.
pub(crate) fn grade(case: &Case, raw: &str, text: &str) -> Report {
    let kept = case.draft.kept_text();
    let conversation = case
        .thread
        .iter()
        .map(|message| message.text)
        .collect::<Vec<_>>()
        .join("\n");
    let answers: Vec<Answer<'static>> = case
        .expect
        .answers
        .iter()
        .map(|answer| Answer {
            question: answer.question,
            any_of: answer.any_of,
        })
        .collect();
    let (min, max) = case.expect.words;
    mxr_composer::check::grade(
        &Requirements {
            typed: &kept,
            instruction: case.instruction,
            conversation: &conversation,
            answers: &answers,
            gaps: case.expect.gaps,
            allowed_numbers: case.expect.allowed_numbers,
            words: WordBand { min, max },
        },
        &Output { raw, text },
    )
}

/// The "who" in `Name <address>`, or the whole string when there is no
/// address.
fn address_of(who: &str) -> &str {
    who.rsplit_once('<')
        .map_or(who, |(_, rest)| rest.trim_end_matches('>'))
}

fn source(thread_id: &ThreadId, from_me: bool, person: &str, who: &str) -> DraftSourceData {
    DraftSourceData {
        message_id: MessageId::new(),
        thread_id: thread_id.clone(),
        date: DateTime::<Utc>::UNIX_EPOCH,
        from_me,
        person: person.to_string(),
        person_name: who
            .split_once(" <")
            .map(|(name, _)| name.to_string())
            .filter(|name| !name.is_empty()),
    }
}

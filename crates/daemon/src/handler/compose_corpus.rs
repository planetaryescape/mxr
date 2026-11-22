use super::draft_compose::{max_tokens_for_words, target_words};
use super::draft_context::ASSEMBLED_MESSAGE_BUDGET_CHARS;
use super::draft_prompt::{self, DraftMode, Help, Me, Prompt, PromptInput, Turn, VoiceExample};
use super::draft_voice::VoiceMaterial;
use chrono::{DateTime, Utc};
use mxr_composer::check::{Answer, AnswerPolarity, Output, Report, Requirements, WordBand};
use mxr_composer::plan;
use mxr_core::id::{MessageId, ThreadId};
use mxr_protocol::DraftSourceData;
use mxr_protocol::TextRangeData;
use mxr_provider_fake::fixtures::compose::{self, Case, DraftState};
use mxr_relationship::analyse_habits;

pub(crate) struct Assembled {
    pub prompt: Prompt,
    pub max_tokens: u32,
}

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
        help: Help {
            action: resolved.action,
            typed,
            selection: resolved.selection,
            adjust: Default::default(),
            revise: None,
        },
        budget_chars: ASSEMBLED_MESSAGE_BUDGET_CHARS,
    });
    Assembled {
        prompt,
        max_tokens: max_tokens_for_words(words),
    }
}

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
            subject: answer.subject,
            polarity: match answer.polarity {
                mxr_provider_fake::fixtures::compose::AnswerPolarity::Any => AnswerPolarity::Any,
                mxr_provider_fake::fixtures::compose::AnswerPolarity::Positive => {
                    AnswerPolarity::Positive
                }
                mxr_provider_fake::fixtures::compose::AnswerPolarity::Negative => {
                    AnswerPolarity::Negative
                }
            },
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

//! The prompt for an AI draft, built from plain data so it can be tested
//! (and evaluated offline) without a store or a model.
//!
//! What the model needs to write as the user, in priority order:
//! 1. Who "I" am, and today's date.
//! 2. Real emails the user wrote (to this person first), with what they
//!    were answering. Examples outrank any idea of "good email style".
//! 3. The user's habits in plain sentences (greeting, sign-off, length).
//! 4. The conversation, turns labelled ME / THEM, the message to answer
//!    marked TARGET; when over budget the oldest turns go first.
//! 5. The task: tone, length in words, and what the user wants to say.
//!
//! Every block derived from mail sits inside the untrusted-content
//! markers; only this module's own wording stays outside.

use mxr_humanizer::writing_constraints;
use mxr_llm::{wrap_untrusted_mail, UNTRUSTED_MAIL_BEGIN, UNTRUSTED_MAIL_END};
use mxr_protocol::VoiceRegisterData;

/// The user, as the model should write.
#[derive(Debug, Clone, Default)]
pub(crate) struct Me {
    pub name: Option<String>,
    pub emails: Vec<String>,
}

impl Me {
    fn label(&self) -> String {
        match (&self.name, self.emails.first()) {
            (Some(name), Some(email)) => format!("{name} <{email}>"),
            (Some(name), None) => name.clone(),
            (None, Some(email)) => email.clone(),
            (None, None) => "the user".to_string(),
        }
    }
}

/// One message of the conversation, already cleaned of quoted history.
#[derive(Debug, Clone)]
pub(crate) struct Turn {
    pub from_me: bool,
    /// "Alice Smith <alice@x>" for their turns.
    pub who: String,
    /// Human date, e.g. "Mon 21 Sep 2026, 10:02".
    pub when: String,
    pub text: String,
    pub target: bool,
}

/// A real email the user wrote, with the message it answered when known.
#[derive(Debug, Clone)]
pub(crate) struct VoiceExample {
    pub to: String,
    pub their_message: Option<String>,
    pub my_email: String,
}

#[derive(Debug, Clone)]
pub(crate) enum DraftMode {
    /// Reply in the conversation to the TARGET turn.
    Reply,
    /// Pass the conversation on to someone new, with a note.
    Forward { to: String },
    /// A first message to someone.
    New { to: String },
}

pub(crate) struct PromptInput<'a> {
    pub mode: DraftMode,
    pub me: &'a Me,
    pub today: &'a str,
    pub habits: &'a [String],
    pub examples: &'a [VoiceExample],
    /// Short relationship summary, for understanding only.
    pub background: Option<&'a str>,
    pub turns: &'a [Turn],
    /// `Some` only when the user chose a tone; otherwise the examples lead.
    pub tone: Option<VoiceRegisterData>,
    pub target_words: u32,
    pub instruction: &'a str,
    pub budget_chars: usize,
}

pub(crate) struct Prompt {
    pub system: String,
    pub user: String,
    /// How many examples and turns fit, for logs and the activity context.
    pub examples_used: usize,
    pub turns_used: usize,
}

const EXAMPLE_MY_MAX: usize = 900;
const EXAMPLE_THEIRS_MAX: usize = 400;
const TURN_MAX: usize = 1_500;
const TARGET_MAX: usize = 6_000;

pub(crate) fn system_prompt(me: &Me, mode: &DraftMode) -> String {
    let name = me.name.clone().unwrap_or_else(|| "the user".to_string());
    let job = match mode {
        DraftMode::Reply => "a reply to the message marked TARGET",
        DraftMode::Forward { .. } => "a short note forwarding the conversation",
        DraftMode::New { .. } => "a new email",
    };
    format!(
        "You are ghostwriting {job} as {name}. Write exactly what {name} would send: \
the same voice, length, greeting and sign-off habits as the examples of {name}'s real emails. \
The examples outrank any idea you have of good email style; if they are terse, be terse.\n\
Rules:\n\
- State only facts that appear in the conversation or in {name}'s instruction. When the email needs \
something you don't know (a date, a number, a decision, a name), write [[?: what is needed]] in its \
place instead of guessing.\n\
- Write in the same language as the conversation.\n\
- Don't repeat the conversation back or answer points {name} already covered.\n\
- Output only the email body as plain text: no subject line, no preamble or notes about the draft, \
no markdown, no placeholders like [Your Name]. Sign off only the way {name} does in the examples."
    )
}

pub(crate) fn build(input: &PromptInput<'_>) -> Prompt {
    let system = system_prompt(input.me, &input.mode);
    let wrap_overhead = UNTRUSTED_MAIL_BEGIN.len() + UNTRUSTED_MAIL_END.len() + 2;

    // Fixed parts first: they are never cut.
    let about = format!(
        "[ABOUT ME]\nI am {}. Today is {}.\n\n",
        one_line(&input.me.label()),
        input.today
    );
    let habits = if input.habits.is_empty() {
        String::new()
    } else {
        format!(
            "[HOW I WRITE]\n{}\n\n",
            input
                .habits
                .iter()
                .map(|habit| format!("- {habit}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    let constraints = format!("[ALSO AVOID]\n{}\n\n", writing_constraints());
    let task = format!("[TASK]\n{}", task_line(input));

    let mut remaining = input
        .budget_chars
        .saturating_sub(system.len() + about.len() + habits.len() + constraints.len() + task.len());

    // The conversation matters most: the target always, then the newest
    // turns back in time. Examples share what is left.
    let conversation_budget = if input.turns.is_empty() {
        0
    } else {
        remaining * 6 / 10
    };
    let (conversation, turns_used) = render_conversation(
        input.turns,
        conversation_budget.saturating_sub(wrap_overhead + 40),
    );
    let conversation_block = if conversation.is_empty() {
        String::new()
    } else {
        format!("[CONVERSATION]\n{}\n\n", wrap_untrusted_mail(&conversation))
    };
    remaining = remaining.saturating_sub(conversation_block.len());

    let background_block = match input
        .background
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        Some(text) if text.len() + wrap_overhead + 80 < remaining / 4 => {
            let block = format!(
                "[BACKGROUND, for understanding only; don't state it as fact]\n{}\n\n",
                wrap_untrusted_mail(text)
            );
            remaining = remaining.saturating_sub(block.len());
            block
        }
        _ => String::new(),
    };

    let (examples, examples_used) =
        render_examples(input.examples, remaining.saturating_sub(wrap_overhead + 60));
    let examples_block = if examples.is_empty() {
        String::new()
    } else {
        format!(
            "[EXAMPLES OF MY REAL EMAILS: match this voice]\n{}\n\n",
            wrap_untrusted_mail(&examples)
        )
    };

    let user = [
        about,
        examples_block,
        habits,
        background_block,
        conversation_block,
        constraints,
        task,
    ]
    .concat();
    Prompt {
        system,
        user,
        examples_used,
        turns_used,
    }
}

fn task_line(input: &PromptInput<'_>) -> String {
    let what = match &input.mode {
        DraftMode::Reply => "Write my reply to the TARGET message.".to_string(),
        DraftMode::Forward { to } => format!(
            "Write a short note to {} forwarding this conversation.",
            one_line(to)
        ),
        DraftMode::New { to } => format!("Write a new email from me to {}.", one_line(to)),
    };
    let tone = match input.tone {
        None => "Match the tone of my examples with this person.".to_string(),
        Some(VoiceRegisterData::Casual) => {
            "Tone: casual and relaxed, the way I write to people I know well.".to_string()
        }
        Some(VoiceRegisterData::Neutral) => "Tone: friendly and plain.".to_string(),
        Some(VoiceRegisterData::Formal) => {
            "Tone: formal and polished, still in my own words.".to_string()
        }
    };
    let instruction = input.instruction.trim();
    let intent = if instruction.is_empty() {
        match input.mode {
            DraftMode::Reply => "I haven't said what to reply: answer what the TARGET asks, briefly, \
and use [[?: ...]] for anything only I can decide."
                .to_string(),
            _ => "I haven't said what to write: keep it to a short, useful opener and use [[?: ...]] \
for the specifics."
                .to_string(),
        }
    } else {
        format!("What I want to say: {instruction}")
    };
    format!(
        "{what} {tone} Aim for about {} words. {intent}",
        input.target_words
    )
}

fn render_conversation(turns: &[Turn], budget: usize) -> (String, usize) {
    if turns.is_empty() || budget == 0 {
        return (String::new(), 0);
    }
    let rendered: Vec<String> = turns.iter().map(render_turn).collect();
    // Keep the target, then add turns newest-first while they fit.
    let target = turns.iter().position(|turn| turn.target);
    let mut keep = vec![false; turns.len()];
    let mut used = 0usize;
    if let Some(index) = target {
        keep[index] = true;
        used += rendered[index].len();
    }
    for index in (0..turns.len()).rev() {
        if keep[index] {
            continue;
        }
        if used + rendered[index].len() > budget {
            continue;
        }
        keep[index] = true;
        used += rendered[index].len();
    }
    let dropped = keep.iter().take_while(|kept| !**kept).count();
    let mut out = String::new();
    if dropped > 0 {
        out.push_str(&format!("[{dropped} earlier message(s) left out]\n\n"));
    }
    for (index, text) in rendered.iter().enumerate() {
        if keep[index] {
            out.push_str(text);
        }
    }
    let count = keep.iter().filter(|kept| **kept).count();
    (out.trim_end().to_string(), count)
}

fn render_turn(turn: &Turn) -> String {
    let who = if turn.from_me {
        "ME".to_string()
    } else {
        format!("THEM ({})", one_line(&turn.who))
    };
    let marker = if turn.target { " <<< TARGET" } else { "" };
    let limit = if turn.target { TARGET_MAX } else { TURN_MAX };
    format!(
        "--- {} · {}{} ---\n{}\n\n",
        who,
        turn.when,
        marker,
        clip(&turn.text, limit)
    )
}

fn render_examples(examples: &[VoiceExample], budget: usize) -> (String, usize) {
    let mut out = String::new();
    let mut used = 0usize;
    for (index, example) in examples.iter().enumerate() {
        let mut block = format!("Example {}, to {}\n", index + 1, one_line(&example.to));
        if let Some(theirs) = example
            .their_message
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            block.push_str(&format!(
                "They wrote:\n{}\nI replied:\n",
                clip(theirs, EXAMPLE_THEIRS_MAX)
            ));
        } else {
            block.push_str("I wrote:\n");
        }
        block.push_str(&clip(&example.my_email, EXAMPLE_MY_MAX));
        block.push_str("\n\n");
        if out.len() + block.len() > budget {
            break;
        }
        out.push_str(&block);
        used += 1;
    }
    (out.trim_end().to_string(), used)
}

fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.len() <= max {
        return text.to_string();
    }
    let mut cut = max;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{} […]", text[..cut].trim_end())
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> Me {
        Me {
            name: Some("Sam Rivers".into()),
            emails: vec!["sam@example.com".into()],
        }
    }

    fn turn(from_me: bool, text: &str, target: bool) -> Turn {
        Turn {
            from_me,
            who: if from_me {
                "Sam Rivers <sam@example.com>".into()
            } else {
                "Alice <alice@x.com>".into()
            },
            when: "Mon 21 Sep 2026, 10:02".into(),
            text: text.into(),
            target,
        }
    }

    fn input<'a>(
        me: &'a Me,
        turns: &'a [Turn],
        examples: &'a [VoiceExample],
        habits: &'a [String],
        budget: usize,
    ) -> PromptInput<'a> {
        PromptInput {
            mode: DraftMode::Reply,
            me,
            today: "Sunday 27 September 2026",
            habits,
            examples,
            background: Some("Alice runs the pricing project."),
            turns,
            tone: None,
            target_words: 40,
            instruction: "say yes, Friday 2pm",
            budget_chars: budget,
        }
    }

    #[test]
    fn labels_turns_marks_the_target_and_says_who_i_am() {
        let me = me();
        let turns = [
            turn(false, "Can we meet?", false),
            turn(true, "Sure, when?", false),
            turn(false, "Friday?", true),
        ];
        let prompt = build(&input(&me, &turns, &[], &[], 30_000));
        assert!(prompt
            .system
            .contains("ghostwriting a reply to the message marked TARGET as Sam Rivers"));
        assert!(prompt
            .user
            .contains("I am Sam Rivers <sam@example.com>. Today is Sunday 27 September 2026."));
        assert!(prompt.user.contains(
            "--- THEM (Alice <alice@x.com>) · Mon 21 Sep 2026, 10:02 <<< TARGET ---\nFriday?"
        ));
        assert!(prompt
            .user
            .contains("--- ME · Mon 21 Sep 2026, 10:02 ---\nSure, when?"));
        assert!(prompt
            .user
            .contains("Aim for about 40 words. What I want to say: say yes, Friday 2pm"));
        assert!(prompt.user.contains("Match the tone of my examples"));
    }

    #[test]
    fn examples_show_what_they_wrote_and_how_i_replied_inside_the_markers() {
        let me = me();
        let examples = [VoiceExample {
            to: "Alice <alice@x.com>".into(),
            their_message: Some("Lunch Friday?".into()),
            my_email: "yes! 1pm?\ns".into(),
        }];
        let habits = ["Usually starts straight in, with no greeting.".to_string()];
        let turns = [turn(false, "Friday?", true)];
        let prompt = build(&input(&me, &turns, &examples, &habits, 30_000));
        let begin = prompt.user.find(UNTRUSTED_MAIL_BEGIN).unwrap();
        let example = prompt
            .user
            .find("They wrote:\nLunch Friday?\nI replied:\nyes! 1pm?\ns")
            .unwrap();
        assert!(begin < example);
        assert!(prompt
            .user
            .contains("- Usually starts straight in, with no greeting."));
        assert_eq!(prompt.examples_used, 1);
    }

    #[test]
    fn over_budget_drops_the_oldest_turns_and_keeps_the_target() {
        let me = me();
        let long = "x".repeat(1_400);
        let turns: Vec<Turn> = (0..20)
            .map(|index| turn(index % 2 == 1, &format!("turn {index} {long}"), index == 19))
            .collect();
        let prompt = build(&input(&me, &turns, &[], &[], 14_000));
        assert!(prompt.user.contains("turn 19"));
        assert!(prompt.user.contains("turn 18"));
        assert!(!prompt.user.contains("turn 0 "));
        assert!(prompt.user.contains("earlier message(s) left out"));
        assert!(prompt.turns_used < 20);
    }

    #[test]
    fn a_chosen_tone_reaches_the_task() {
        let me = me();
        let turns = [turn(false, "Friday?", true)];
        let mut input = input(&me, &turns, &[], &[], 30_000);
        input.tone = Some(VoiceRegisterData::Formal);
        assert!(build(&input).user.contains("Tone: formal and polished"));
    }

    #[test]
    fn no_instruction_asks_for_placeholders_not_guesses() {
        let me = me();
        let turns = [turn(false, "What's the budget?", true)];
        let mut input = input(&me, &turns, &[], &[], 30_000);
        input.instruction = "  ";
        let prompt = build(&input);
        assert!(prompt.user.contains("[[?: ...]]"));
        assert!(prompt.system.contains("[[?: what is needed]]"));
    }
}

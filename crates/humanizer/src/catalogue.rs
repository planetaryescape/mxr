pub const HUMANIZER_VERSION: &str = "3.1.0+mxr.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternGroup {
    A,
    B,
    C,
    D,
    E,
    F,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detector {
    Deterministic,
    PromptOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pattern {
    pub number: u8,
    pub name: &'static str,
    pub group: PatternGroup,
    pub instruction: &'static str,
    pub detector: Detector,
    /// A weak-alone pattern is counted only beside another tell in its paragraph.
    pub weak_alone: bool,
    pub applies_to_email: bool,
}

/// Humanizer 3.1 catalogue. Positive instructions are shared by generation,
/// checking and repair so the three paths cannot drift independently.
pub const PATTERNS: [Pattern; 26] = [
    p(1,"Not X but Y",PatternGroup::A,"Say the point directly; do not contrast it with a claim nobody made.",Detector::Deterministic,false,true),
    p(2,"One-line closers and dramatic fragments",PatternGroup::A,"End on the last concrete point; remove dramatic fragments.",Detector::Deterministic,false,true),
    p(3,"Sayings that sound deep",PatternGroup::A,"Use the concrete fact instead of a saying that only sounds profound.",Detector::Deterministic,false,true),
    p(4,"Staged run-up before the point",PatternGroup::A,"Lead with the answer or request.",Detector::PromptOnly,false,true),
    p(5,"Arguing with no one",PatternGroup::A,"State the point without answering an objection the reader did not raise.",Detector::PromptOnly,false,true),
    p(6,"Forced triads",PatternGroup::B,"Use the exact number of points the message needs.",Detector::Deterministic,false,true),
    p(7,"Repeated sentence openings",PatternGroup::B,"Vary sentence openings when the repetition is unintentional.",Detector::PromptOnly,true,true),
    p(8,"Dashes as the universal connector",PatternGroup::B,"Use dashes only where they match the writer's normal rate; otherwise use a period or comma.",Detector::Deterministic,true,true),
    p(9,"Stacked qualifiers",PatternGroup::B,"Keep one necessary qualifier and remove the rest.",Detector::Deterministic,true,true),
    p(10,"Hyphenated pairs everywhere",PatternGroup::B,"Keep compound modifiers hyphenated before nouns and open after them.",Detector::PromptOnly,true,true),
    p(11,"Passive voice and missing subjects",PatternGroup::B,"Name the actor and use an active verb when it makes the action clearer.",Detector::PromptOnly,true,true),
    p(12,"Overused AI words",PatternGroup::C,"Use ordinary, specific words in place of inflated or formal wording.",Detector::Deterministic,false,true),
    p(13,"Inflated significance",PatternGroup::C,"Keep the fact and remove claims of significance it does not support.",Detector::Deterministic,false,true),
    p(14,"Vague connection or association",PatternGroup::C,"Name the relationship the source gives; do not invent one.",Detector::PromptOnly,false,true),
    p(15,"Shallow -ing riders",PatternGroup::C,"State the supported fact directly; keep an -ing clause only when its claim is supported.",Detector::Deterministic,false,true),
    p(16,"Sales language",PatternGroup::C,"State what the thing is without advertising it.",Detector::Deterministic,false,true),
    p(17,"Borrowed authority",PatternGroup::C,"Name the source and what it said, or remove unsupported authority.",Detector::PromptOnly,false,true),
    p(18,"Avoiding is, are, and has",PatternGroup::C,"Prefer is, are and has when they state the fact clearly.",Detector::Deterministic,false,true),
    p(19,"Bold as decoration",PatternGroup::D,"Reserve bold for emphasis that helps the reader act.",Detector::Deterministic,true,true),
    p(20,"Decorative headings",PatternGroup::D,"Use sentence-case headings only when a heading helps this email.",Detector::Deterministic,true,false),
    p(21,"Curly quotation marks",PatternGroup::D,"Use the writer's usual quotation marks.",Detector::Deterministic,true,true),
    p(22,"Chatbot residue",PatternGroup::E,"Remove chatbot greetings, praise, offers and closings; keep the message itself.",Detector::Deterministic,false,true),
    p(23,"Knowledge-limit disclaimers and guesses",PatternGroup::E,"State what the source does not show and remove guesses.",Detector::Deterministic,false,true),
    p(24,"A heading repeated in the first sentence",PatternGroup::E,"Remove a first sentence that only repeats the heading.",Detector::PromptOnly,true,false),
    p(25,"Writing about the document instead of its subject",PatternGroup::E,"Write about the subject; keep process notes only when the reader needs them.",Detector::PromptOnly,true,true),
    p(26,"Re-explaining what the reader knows",PatternGroup::F,"Lead with the decision and include only context the reader needs to act.",Detector::PromptOnly,false,true),
];

const fn p(
    number: u8,
    name: &'static str,
    group: PatternGroup,
    instruction: &'static str,
    detector: Detector,
    weak_alone: bool,
    applies_to_email: bool,
) -> Pattern {
    Pattern {
        number,
        name,
        group,
        instruction,
        detector,
        weak_alone,
        applies_to_email,
    }
}

pub fn email_style_instructions() -> String {
    PATTERNS
        .iter()
        .filter(|pattern| pattern.applies_to_email)
        .map(|pattern| format!("§{} {}", pattern.number, pattern.instruction))
        .collect::<Vec<_>>()
        .join("\n")
}

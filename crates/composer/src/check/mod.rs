//! The check stage: seven graders, six of which pass or fail and one of
//! which reports a style score. A grade says what the draft had to do and
//! what it did, so a failing case reads as a sentence, not a bare count.

mod graders;
mod proposal;
mod regions;
mod text;

pub use mxr_compose::body_regions::{split_body, BodyRegions};
pub use proposal::{check_proposal, Checked, Fixes, ProposalInput};
pub use regions::text_hash;
pub use text::{invented_numbers, replay_invented_numbers};

/// The question a case asks of the target message, and the words a reply
/// that addresses it would contain (any one of them is enough).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer<'a> {
    pub question: &'a str,
    pub any_of: &'a [&'a str],
}

/// The length a reply should land in, in words, inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordBand {
    pub min: u32,
    pub max: u32,
}

/// What a case requires of the draft. Plain data: the caller builds it from
/// whatever the case is stored as.
#[derive(Debug, Clone, Copy)]
pub struct Requirements<'a> {
    /// Text the user typed into the draft. Every sentence of it must survive.
    pub typed: &'a str,
    /// What the user asked for, when they asked. Numbers in it may appear.
    pub instruction: &'a str,
    /// Every message the reply answers, as text. Numbers in it may appear.
    pub conversation: &'a str,
    pub answers: &'a [Answer<'a>],
    /// Facts the reply must leave as a `[[?: …]]` gap. Each is a word the
    /// gap has to contain.
    pub gaps: &'a [&'a str],
    /// Numbers the case allows although no input carries them.
    pub allowed_numbers: &'a [&'a str],
    pub words: WordBand,
}

/// What the model wrote, and the draft the user would see after clean-up.
#[derive(Debug, Clone, Copy)]
pub struct Output<'a> {
    /// Before clean-up. Leftovers are graded here: clean-up is a safety net,
    /// and a draft that needed it is still a failure.
    pub raw: &'a str,
    /// After clean-up. Every other check reads this.
    pub text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Check {
    Kept,
    Answers,
    Gaps,
    Invented,
    Length,
    Leftovers,
}

impl Check {
    pub const ALL: [Self; 6] = [
        Self::Kept,
        Self::Answers,
        Self::Gaps,
        Self::Invented,
        Self::Length,
        Self::Leftovers,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kept => "kept",
            Self::Answers => "answers",
            Self::Gaps => "gaps",
            Self::Invented => "invented",
            Self::Length => "length",
            Self::Leftovers => "leftovers",
        }
    }
}

/// One pass-or-fail answer to "did the draft do what this case required?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grade {
    pub check: Check,
    pub pass: bool,
    /// What the draft had to do, in the case's own terms.
    pub requirement: String,
    /// What the draft did.
    pub detail: String,
}

/// Reported, not graded: how much the draft reads like a model wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    pub score: u8,
    pub hits: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub grades: Vec<Grade>,
    pub style: Style,
}

impl Report {
    pub fn passed(&self, check: Check) -> Option<bool> {
        self.grades
            .iter()
            .find(|grade| grade.check == check)
            .map(|grade| grade.pass)
    }

    pub fn all_pass(&self) -> bool {
        self.grades.iter().all(|grade| grade.pass)
    }
}

/// Grade one output against one case.
pub fn grade(requirements: &Requirements<'_>, output: &Output<'_>) -> Report {
    let grades = vec![
        graders::kept(requirements, output),
        graders::answers(requirements, output),
        graders::gaps(requirements, output),
        graders::invented(requirements, output),
        graders::length(requirements, output),
        graders::leftovers(requirements, output),
    ];
    let humanized = mxr_humanizer::score(output.text, &mxr_humanizer::HumanizerOpts::default());
    Report {
        grades,
        style: Style {
            score: humanized.score,
            hits: humanized.hits.len(),
        },
    }
}

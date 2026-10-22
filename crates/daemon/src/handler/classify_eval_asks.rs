//! `mxr classify eval`, the asks: to-do rows graded against the items each
//! email should ask of you (blueprint 37, "The eval grades what the person
//! relies on", label schema v2).
//!
//! An email's to-do rows are matched to its expected items by kind and
//! direction, then the fields of each matched pair are compared. What is
//! reported is precision and recall per kind of ask, exact match per field,
//! duplicates and merges, twin-pair accuracy and slices. Accuracy is not
//! reported: nearly every email asks nothing, so it measures how many true
//! negatives there are.
//!
//! The grading here is pure: it reads labels and rows and touches no store,
//! so the matching, its ordering and its edge cases are unit tested below.

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use mxr_protocol::{
    ClassifyEvalAskKindData, ClassifyEvalAsksData, ClassifyEvalCountsData, ClassifyEvalFieldData,
    ClassifyEvalMismatchData, ClassifyEvalReplyData, ClassifyEvalSliceData, ClassifyEvalTwinsData,
};
use mxr_provider_fake::fixtures::scenarios::{AskKind, Direction, Due, Item, Labelled};
use mxr_store::EvalTodo;
use mxr_todo::dates::{parse_date, DateOrder, YearIntent};
use mxr_todo::TodoKind;
use std::collections::BTreeMap;

/// What the Your turn answer reads, for the output.
pub(super) const REPLY_PROJECTION: &str = "Your turn as the desk reads it for one message: `mail_kind::requires_reply` (a request directed to you, `reply_request`, from a sender that classifies as a person, mail_kind.rs:408-418, the call the You owe lane makes at desk_lanes.rs:483) with nothing you sent after it in the thread (desk_lanes.rs:438, an outbound latest message is Waiting). The lane's other gates (copied threads, decay, snoozes, the screener) are not mirrored";

/// The mismatch questions the asks add, in the order they are listed.
pub(super) const ASK_QUESTIONS: [&str; 5] = ["ask", "amount", "due", "counterparty", "reply"];

/// A to-do row, translated to chapter 37's vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Row {
    pub kind: AskKind,
    pub direction: Direction,
    /// The to-do kind the row was made as, for the mismatch text.
    pub made_as: String,
    pub counterparty: Option<String>,
    pub amount: Option<(i64, String)>,
    /// The due date as stored: the end of the day it resolved to, in the
    /// daemon's time zone.
    pub due_at: Option<DateTime<Utc>>,
    pub due_words: Option<String>,
}

/// Today's to-do kinds in chapter 37's kinds, in one place.
///
/// Judgement calls:
/// * `Return` is `send`: giving an item back is sending it.
/// * `Rsvp` is `attend`: answering an invitation is about turning up.
/// * `Document` (a passport or licence expiring), `Lease` (give notice) and
///   `Renewal` are all `renew_or_cancel`: the action is to renew, or to end.
/// * `Promise` is a promise in the owner's sent mail, so its direction is
///   `you_promised` and its kind comes from the verb its title starts with
///   ([`promise_kind`]); a title that starts with anything else is `other`.
pub(super) fn translate(todo: &TodoKind, title: &str) -> (AskKind, Direction) {
    let asked = |kind| (kind, Direction::AsksYou);
    match todo {
        TodoKind::Bill => asked(AskKind::Pay),
        TodoKind::PaymentFailed => asked(AskKind::FixPayment),
        TodoKind::Renewal | TodoKind::Document | TodoKind::Lease => asked(AskKind::RenewOrCancel),
        TodoKind::Return => asked(AskKind::Send),
        TodoKind::Rsvp => asked(AskKind::Attend),
        TodoKind::Verify => asked(AskKind::Verify),
        TodoKind::Sign => asked(AskKind::Sign),
        TodoKind::Promise => (promise_kind(title), Direction::YouPromised),
        TodoKind::Other => asked(AskKind::Other),
    }
}

/// The kind of a promise from the first word of its title ("Send the signed
/// form" is `send`); `other` when the verb is not one of the vocabulary's.
fn promise_kind(title: &str) -> AskKind {
    match title
        .split_whitespace()
        .next()
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("send" | "forward" | "share" | "return" | "submit" | "email" | "mail") => {
            AskKind::Send
        }
        Some("pay" | "transfer" | "reimburse" | "refund") => AskKind::Pay,
        Some("call" | "contact" | "ring" | "text" | "message" | "ping") => AskKind::Contact,
        Some("book" | "schedule" | "arrange") => AskKind::Book,
        Some("sign") => AskKind::Sign,
        Some("reply" | "respond" | "answer" | "confirm") => AskKind::Reply,
        Some("provide" | "give") => AskKind::Provide,
        Some("upload") => AskKind::Upload,
        _ => AskKind::Other,
    }
}

impl Row {
    pub(super) fn from_todo(todo: &EvalTodo) -> Self {
        let parsed = TodoKind::parse(&todo.kind).unwrap_or(TodoKind::Other);
        let (kind, direction) = translate(&parsed, &todo.title);
        Self {
            kind,
            direction,
            made_as: todo.kind.clone(),
            counterparty: todo.counterparty.clone(),
            amount: todo
                .amount_minor
                .zip(todo.currency.clone())
                .map(|(minor, currency)| (minor, currency.to_uppercase())),
            due_at: todo
                .due_at
                .and_then(|seconds| DateTime::from_timestamp(seconds, 0)),
            due_words: todo.due_words.clone(),
        }
    }
}

/// Lowercase words, single spaces, so "Camden  Council" equals "camden council".
fn normalise(value: &str) -> String {
    value
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Counts {
    expected: u32,
    found: u32,
    true_positives: u32,
    false_positives: u32,
    false_negatives: u32,
}

impl Counts {
    fn add(&mut self, other: Self) {
        self.expected += other.expected;
        self.found += other.found;
        self.true_positives += other.true_positives;
        self.false_positives += other.false_positives;
        self.false_negatives += other.false_negatives;
    }

    fn data(self) -> ClassifyEvalCountsData {
        let percent = |part: u32, whole: u32| {
            (whole > 0).then(|| (f64::from(part) / f64::from(whole) * 1000.0).round() / 10.0)
        };
        ClassifyEvalCountsData {
            expected: self.expected,
            found: self.found,
            true_positives: self.true_positives,
            false_positives: self.false_positives,
            false_negatives: self.false_negatives,
            precision_percent: percent(self.true_positives, self.found),
            recall_percent: percent(self.true_positives, self.expected),
        }
    }
}

/// How the fields of the matched pairs came out, per field.
#[derive(Debug, Clone, Copy, Default)]
struct FieldTally {
    pairs: u32,
    exact: u32,
    missing: u32,
    wrong: u32,
    spurious: u32,
    present: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Exact,
    Missing,
    Wrong,
    Spurious,
    /// The label's value cannot be resolved to an exact one, and the row has
    /// one: reported as present, never as exact.
    Present,
}

const FIELDS: [&str; 3] = ["amount", "due", "counterparty"];

/// What was graded for one email, kept for the checks that need the whole
/// set: merges and twin pairs.
#[derive(Debug)]
struct Outcome {
    scenario: &'static str,
    twin_of: Option<&'static str>,
    from_domain: String,
    /// The expected items and whether a row matched each.
    expected: Vec<(Item, bool)>,
    /// Misses, to be written once merges are known.
    misses: Vec<usize>,
    /// The email is right on what it asks: no missed item, no extra row,
    /// every matched field exact, and the reply owed as labelled.
    right: bool,
    /// Whether a reply is owed, as labelled; part of a twin pair's signature.
    reply: bool,
    label_review: Option<String>,
    provider_id: String,
    held_out: bool,
}

/// One email to grade.
pub(super) struct AskInput<'a> {
    pub label: &'a Labelled,
    pub rows: Vec<Row>,
    /// Whether the classifier says a reply is owed; `None` for sent mail,
    /// which owes none.
    pub reply_found: Option<bool>,
    /// The email's own date, which a due date is read against.
    pub sent: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub(super) struct AskGrader {
    emails: u32,
    total: Counts,
    kinds: BTreeMap<(AskKind, Direction), Counts>,
    fields: [FieldTally; 3],
    duplicates: u32,
    outcomes: Vec<Outcome>,
    slices: BTreeMap<(&'static str, String), (u32, Counts)>,
    reply_emails: u32,
    reply: Counts,
    mismatches: Vec<ClassifyEvalMismatchData>,
}

fn money(minor: i64, currency: &str) -> String {
    format!("{}.{:02} {currency}", minor / 100, (minor % 100).abs())
}

fn named_kind(kind: AskKind, direction: Direction) -> String {
    match direction {
        Direction::AsksYou => kind.as_str().to_string(),
        Direction::YouPromised => format!("{} (you promised)", kind.as_str()),
    }
}

impl AskGrader {
    fn mismatch(
        label: &Labelled,
        question: &str,
        expected: String,
        actual: String,
        reason: String,
    ) -> ClassifyEvalMismatchData {
        ClassifyEvalMismatchData {
            scenario: label.scenario.to_string(),
            provider_id: label.provider_id.clone(),
            question: question.to_string(),
            expected,
            actual,
            reason,
            twin_of: label.twin_of.map(str::to_string),
            held_out: label.held_out,
            // The doubt on a label's to-do answer covers its items and the
            // reply it owes.
            label_review: label
                .review
                .filter(|review| review.questions.contains(&"todo"))
                .map(|review| review.why.to_string()),
        }
    }

    /// Match `rows` to the label's items and tally the result.
    pub(super) fn grade(&mut self, input: AskInput<'_>, from_email: &str) {
        let label = input.label;
        let expected = &label.items;
        let rows = &input.rows;
        self.emails += 1;

        let matched = assign(expected, rows, input.sent);
        let (mut item_taken, mut row_taken) =
            (vec![false; expected.len()], vec![false; rows.len()]);
        for &(i, j) in &matched {
            item_taken[i] = true;
            row_taken[j] = true;
        }

        // Per kind and slice counts.
        let mut email_counts = Counts::default();
        let mut bump = |kind: AskKind, direction: Direction, f: &dyn Fn(&mut Counts)| {
            let mut one = Counts::default();
            f(&mut one);
            self.kinds.entry((kind, direction)).or_default().add(one);
            email_counts.add(one);
        };
        for item in expected {
            bump(item.kind, item.direction, &|c| c.expected += 1);
        }
        for row in rows {
            bump(row.kind, row.direction, &|c| c.found += 1);
        }
        for &(i, _) in &matched {
            bump(expected[i].kind, expected[i].direction, &|c| {
                c.true_positives += 1;
            });
        }
        let mut duplicates = 0;
        for (row, _) in rows.iter().zip(&row_taken).filter(|(_, taken)| !**taken) {
            bump(row.kind, row.direction, &|c| c.false_positives += 1);
            // A second row for an item the email does ask is a duplicate.
            let asked = expected
                .iter()
                .any(|item| item.kind == row.kind && item.direction == row.direction);
            if asked {
                duplicates += 1;
            }
            self.mismatches.push(Self::mismatch(
                label,
                "ask",
                if asked {
                    "one row".to_string()
                } else {
                    "none".to_string()
                },
                named_kind(row.kind, row.direction),
                if asked {
                    format!("a duplicate row ({})", row.made_as)
                } else {
                    format!("made as {}", row.made_as)
                },
            ));
        }
        let misses: Vec<usize> = (0..expected.len()).filter(|i| !item_taken[*i]).collect();
        for &i in &misses {
            bump(expected[i].kind, expected[i].direction, &|c| {
                c.false_negatives += 1;
            });
        }
        self.duplicates += duplicates;
        self.total.add(email_counts);

        // Fields of the matched pairs.
        let mut fields_exact = true;
        for &(i, j) in &matched {
            let (item, row) = (&expected[i], &rows[j]);
            for (slot, field) in FIELDS.iter().enumerate() {
                let verdict = verdict(field, item, row, input.sent);
                let tally = &mut self.fields[slot];
                tally.pairs += 1;
                match verdict {
                    Verdict::Exact => tally.exact += 1,
                    Verdict::Missing => tally.missing += 1,
                    Verdict::Wrong => tally.wrong += 1,
                    Verdict::Spurious => tally.spurious += 1,
                    Verdict::Present => tally.present += 1,
                }
                fields_exact &= verdict == Verdict::Exact;
                if !matches!(verdict, Verdict::Exact | Verdict::Present) {
                    let (want, got) = field_texts(field, item, row, input.sent);
                    self.mismatches.push(Self::mismatch(
                        label,
                        field,
                        want,
                        got,
                        format!("on the {} row", named_kind(item.kind, item.direction)),
                    ));
                }
            }
        }

        // Slices.
        let held = if label.held_out {
            "held_out"
        } else {
            "visible"
        };
        let slice_keys: [(&'static str, String); 4] = [
            ("sender_kind", label.sender.as_str().to_string()),
            ("locale", label.locale.to_string()),
            ("template_family", label.family.to_string()),
            ("phrasing", held.to_string()),
        ];
        for key in slice_keys {
            let entry = self.slices.entry(key).or_default();
            entry.0 += 1;
            entry.1.add(email_counts);
        }

        // Reply owed, its own question.
        if let Some(found) = input.reply_found {
            let expected_reply = label.expect.reply;
            self.reply_emails += 1;
            self.reply.expected += u32::from(expected_reply);
            self.reply.found += u32::from(found);
            match (expected_reply, found) {
                (true, true) => self.reply.true_positives += 1,
                (false, true) => self.reply.false_positives += 1,
                (true, false) => self.reply.false_negatives += 1,
                (false, false) => {}
            }
            if expected_reply != found {
                self.mismatches.push(Self::mismatch(
                    label,
                    "reply",
                    if expected_reply { "owed" } else { "none owed" }.to_string(),
                    if found { "owed" } else { "none owed" }.to_string(),
                    String::new(),
                ));
            }
        }

        self.outcomes.push(Outcome {
            scenario: label.scenario,
            twin_of: label.twin_of,
            from_domain: from_email
                .rsplit('@')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase(),
            expected: expected
                .iter()
                .enumerate()
                .map(|(i, item)| (item.clone(), item_taken[i]))
                .collect(),
            right: misses.is_empty()
                && row_taken.iter().all(|taken| *taken)
                && fields_exact
                && input
                    .reply_found
                    .is_none_or(|found| found == label.expect.reply),
            reply: label.expect.reply,
            misses,
            label_review: label
                .review
                .filter(|review| review.questions.contains(&"todo"))
                .map(|review| review.why.to_string()),
            provider_id: label.provider_id.clone(),
            held_out: label.held_out,
        });
    }

    /// Counts the merges, writes the misses and sums up. Misses are written
    /// here because whether a miss was a merge needs every email.
    pub(super) fn finish(
        mut self,
    ) -> (Option<ClassifyEvalAsksData>, Vec<ClassifyEvalMismatchData>) {
        if self.emails == 0 {
            return (None, Vec::new());
        }
        let merges = self.write_misses();
        let twins = self.twins();
        let data = ClassifyEvalAsksData {
            emails: self.emails,
            total: self.total.data(),
            kinds: self.kind_data(),
            fields: FIELDS
                .iter()
                .zip(self.fields)
                .map(|(field, tally)| ClassifyEvalFieldData {
                    field: (*field).to_string(),
                    pairs: tally.pairs,
                    exact: tally.exact,
                    exact_percent: if tally.pairs == 0 {
                        0.0
                    } else {
                        (f64::from(tally.exact) / f64::from(tally.pairs) * 1000.0).round() / 10.0
                    },
                    missing: tally.missing,
                    wrong: tally.wrong,
                    present: tally.present,
                    spurious: tally.spurious,
                })
                .collect(),
            duplicates: self.duplicates,
            merges,
            twins,
            slices: self.slice_data(),
            reply_owed: ClassifyEvalReplyData {
                emails: self.reply_emails,
                counts: self.reply.data(),
                projection: REPLY_PROJECTION.to_string(),
            },
        };
        (Some(data), self.mismatches)
    }

    /// A miss is a merge when a row of the same sender, kind and counterparty
    /// was made from another email with a different reference: the two
    /// accounts share one row. Returns the number of merges.
    fn write_misses(&mut self) -> u32 {
        let mut merges = 0;
        let mut written = Vec::new();
        for outcome in &self.outcomes {
            for &index in &outcome.misses {
                let item = &outcome.expected[index].0;
                let absorbed = self.outcomes.iter().any(|other| {
                    other.from_domain == outcome.from_domain
                        && other.provider_id != outcome.provider_id
                        && other.expected.iter().any(|(other_item, matched)| {
                            *matched
                                && other_item.kind == item.kind
                                && other_item.direction == item.direction
                                && other_item.counterparty.as_deref().map(normalise)
                                    == item.counterparty.as_deref().map(normalise)
                                && other_item.reference.is_some()
                                && other_item.reference != item.reference
                        })
                });
                merges += u32::from(absorbed);
                written.push(ClassifyEvalMismatchData {
                    scenario: outcome.scenario.to_string(),
                    provider_id: outcome.provider_id.clone(),
                    question: "ask".to_string(),
                    expected: named_kind(item.kind, item.direction),
                    actual: if absorbed {
                        "no row: merged into another email's row".to_string()
                    } else {
                        "no row".to_string()
                    },
                    reason: String::new(),
                    twin_of: outcome.twin_of.map(str::to_string),
                    held_out: outcome.held_out,
                    label_review: outcome.label_review.clone(),
                });
            }
        }
        self.mismatches.extend(written);
        merges
    }

    fn twins(&self) -> ClassifyEvalTwinsData {
        let mut twins = ClassifyEvalTwinsData::default();
        for outcome in &self.outcomes {
            let Some(plain) = outcome
                .twin_of
                .and_then(|target| self.outcomes.iter().find(|other| other.scenario == target))
            else {
                continue;
            };
            let signature = |outcome: &Outcome| {
                let mut kinds: Vec<_> = outcome
                    .expected
                    .iter()
                    .map(|(item, _)| (item.kind, item.direction))
                    .collect();
                kinds.sort_unstable();
                (kinds, outcome.reply)
            };
            let both = outcome.right && plain.right;
            twins.pairs += 1;
            twins.both_right += u32::from(both);
            if signature(outcome) != signature(plain) {
                twins.differing_pairs += 1;
                twins.differing_both_right += u32::from(both);
            }
        }
        twins
    }

    fn kind_data(&self) -> Vec<ClassifyEvalAskKindData> {
        let mut out = Vec::new();
        for direction in [Direction::AsksYou, Direction::YouPromised] {
            for kind in AskKind::ALL {
                let counts = self.kinds.get(&(kind, direction)).copied();
                // Everything a person can be asked for is always listed;
                // promises only when one appeared.
                if counts.is_none() && direction == Direction::YouPromised {
                    continue;
                }
                out.push(ClassifyEvalAskKindData {
                    kind: kind.as_str().to_string(),
                    direction: direction.as_str().to_string(),
                    counts: counts.unwrap_or_default().data(),
                });
            }
        }
        out
    }

    /// Slices with something to say: a template family appears only if it
    /// has an expected item or a row, so the rest do not bury it.
    fn slice_data(&self) -> Vec<ClassifyEvalSliceData> {
        self.slices
            .iter()
            .filter(|((dimension, _), (_, counts))| {
                *dimension != "template_family" || counts.expected > 0 || counts.found > 0
            })
            .map(
                |((dimension, value), (emails, counts))| ClassifyEvalSliceData {
                    dimension: (*dimension).to_string(),
                    value: value.clone(),
                    emails: *emails,
                    counts: counts.data(),
                },
            )
            .collect()
    }
}

/// The calendar day a label expects for a due date, read against the email's
/// own date; `None` when it cannot be resolved exactly.
///
/// * `AfterSent(h)` is the UTC day `h` hours after the email's date, which is
///   the day the corpus writes into the mail (the mail and its date move with
///   the seed clock together).
/// * `Words` are read by the to-do code's own date reader, from the email's
///   date in the daemon's time zone, so a weekday resolves the way detection
///   resolves it. That grades the row against the same reader, not an
///   independent one.
fn expected_day(due: Due, sent: DateTime<Utc>) -> Option<NaiveDate> {
    match due {
        Due::None => None,
        Due::AfterSent(hours) => Some((sent + Duration::hours(hours)).date_naive()),
        Due::Words(words) => parse_date(
            words,
            &sent.with_timezone(&Local),
            DateOrder::DayFirst,
            YearIntent::Next,
        )
        .map(|(day, _)| day),
    }
}

/// The day a row's due date falls on, in the zone detection resolved it in
/// (`handler/todos.rs` `pass_config`: `Local`; `end_of_day` in `dates.rs`).
fn row_day(row: &Row) -> Option<NaiveDate> {
    row.due_at.map(|due| due.with_timezone(&Local).date_naive())
}

fn verdict(field: &str, item: &Item, row: &Row, sent: DateTime<Utc>) -> Verdict {
    match field {
        "amount" => match (&item.amount, &row.amount) {
            (None, None) => Verdict::Exact,
            (Some(_), None) => Verdict::Missing,
            (None, Some(_)) => Verdict::Spurious,
            (Some(want), Some((minor, currency))) => {
                if want.minor == *minor && want.currency.eq_ignore_ascii_case(currency) {
                    Verdict::Exact
                } else {
                    Verdict::Wrong
                }
            }
        },
        "due" => match (item.due, row_day(row)) {
            (Due::None, None) => Verdict::Exact,
            (Due::None, Some(_)) => Verdict::Spurious,
            (_, None) => Verdict::Missing,
            (due, Some(got)) => match expected_day(due, sent) {
                Some(want) if want == got => Verdict::Exact,
                Some(_) => Verdict::Wrong,
                None => Verdict::Present,
            },
        },
        _ => match (&item.counterparty, &row.counterparty) {
            (None, None) => Verdict::Exact,
            (Some(_), None) => Verdict::Missing,
            (None, Some(_)) => Verdict::Spurious,
            (Some(want), Some(got)) => {
                if normalise(want) == normalise(got) {
                    Verdict::Exact
                } else {
                    Verdict::Wrong
                }
            }
        },
    }
}

/// The expected and the actual value of a field, for the mismatch text.
fn field_texts(field: &str, item: &Item, row: &Row, sent: DateTime<Utc>) -> (String, String) {
    let none = || "none".to_string();
    match field {
        "amount" => (
            item.amount
                .map_or_else(none, |a| money(a.minor, a.currency)),
            row.amount
                .as_ref()
                .map_or_else(none, |(minor, currency)| money(*minor, currency)),
        ),
        "due" => (
            expected_day(item.due, sent).map_or_else(none, |day| day.to_string()),
            row_day(row).map_or_else(none, |day| day.to_string()),
        ),
        _ => (
            item.counterparty.clone().unwrap_or_else(none),
            row.counterparty.clone().unwrap_or_else(none),
        ),
    }
}

/// More items or rows of one kind than this are matched greedily.
const EXACT_ASSIGNMENT_LIMIT: usize = 6;

/// How alike a row and an item are: the number of exact fields.
fn agreement(item: &Item, row: &Row, sent: DateTime<Utc>) -> usize {
    FIELDS
        .iter()
        .filter(|field| verdict(field, item, row, sent) == Verdict::Exact)
        .count()
}

/// The content a tie is broken by, never the position: items and rows of one
/// kind are put in this order before they are paired, so the same emails in
/// any order of rows pair the same way.
fn item_order(item: &Item) -> (Option<(i64, &'static str)>, String, Option<String>) {
    (
        item.amount.map(|a| (a.minor, a.currency)),
        format!("{:?}", item.due),
        item.counterparty.as_deref().map(normalise),
    )
}

type RowOrder = (Option<(i64, String)>, Option<DateTime<Utc>>, Option<String>);

fn row_order(row: &Row) -> RowOrder {
    (
        row.amount.clone(),
        row.due_at,
        row.counterparty.as_deref().map(normalise),
    )
}

/// Pair each expected item with a row of the same kind and direction so that
/// the most pairs are made and, among those, the most fields agree. Ties go
/// to the pairing found first over content-ordered items and rows. Above
/// [`EXACT_ASSIGNMENT_LIMIT`] of one kind the search is too large to be worth
/// it, so each item takes its best free row in content order instead.
fn assign(expected: &[Item], rows: &[Row], sent: DateTime<Utc>) -> Vec<(usize, usize)> {
    let mut kinds: Vec<(AskKind, Direction)> = expected
        .iter()
        .map(|item| (item.kind, item.direction))
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    let mut out = Vec::new();
    for (kind, direction) in kinds {
        let mut items: Vec<usize> = (0..expected.len())
            .filter(|i| (expected[*i].kind, expected[*i].direction) == (kind, direction))
            .collect();
        let mut candidates: Vec<usize> = (0..rows.len())
            .filter(|j| (rows[*j].kind, rows[*j].direction) == (kind, direction))
            .collect();
        items.sort_by_key(|i| item_order(&expected[*i]));
        candidates.sort_by_key(|j| row_order(&rows[*j]));
        let score = |i: usize, j: usize| agreement(&expected[i], &rows[j], sent);
        if items.len().max(candidates.len()) <= EXACT_ASSIGNMENT_LIMIT {
            let mut best: (usize, usize, Vec<(usize, usize)>) = (0, 0, Vec::new());
            search(
                &items,
                &candidates,
                &score,
                &mut Vec::new(),
                &mut vec![false; candidates.len()],
                0,
                &mut best,
            );
            out.extend(best.2);
        } else {
            let mut taken = vec![false; candidates.len()];
            for &i in &items {
                let pick = (0..candidates.len()).filter(|k| !taken[*k]).max_by(|a, b| {
                    score(i, candidates[*a])
                        .cmp(&score(i, candidates[*b]))
                        .then(b.cmp(a))
                });
                if let Some(k) = pick {
                    taken[k] = true;
                    out.push((i, candidates[k]));
                }
            }
        }
    }
    out.sort_unstable();
    out
}

/// Depth-first over the items in order: each takes a free row or is left
/// out. Keeps the pairing with the most pairs, then the highest agreement;
/// a later pairing replaces it only when strictly better, so ties stay with
/// the first, which is the content-ordered one.
fn search(
    items: &[usize],
    rows: &[usize],
    score: &dyn Fn(usize, usize) -> usize,
    chosen: &mut Vec<(usize, usize)>,
    taken: &mut Vec<bool>,
    next: usize,
    best: &mut (usize, usize, Vec<(usize, usize)>),
) {
    if next == items.len() {
        let total: usize = chosen.iter().map(|(i, j)| score(*i, *j)).sum();
        if (chosen.len(), total) > (best.0, best.1) {
            *best = (chosen.len(), total, chosen.clone());
        }
        return;
    }
    for k in 0..rows.len() {
        if !taken[k] {
            taken[k] = true;
            chosen.push((items[next], rows[k]));
            search(items, rows, score, chosen, taken, next + 1, best);
            chosen.pop();
            taken[k] = false;
        }
    }
    search(items, rows, score, chosen, taken, next + 1, best);
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_provider_fake::fixtures::scenarios::{Expect, Home, SenderKind, Who};

    fn label(scenario: &'static str, items: Vec<Item>) -> Labelled {
        Labelled {
            provider_id: format!("demo-msg-{scenario}"),
            from_email: "billing@acme.example".into(),
            scenario,
            description: "",
            expect: Expect::new(Who::Machine, Home::Updates, false),
            items,
            outbound: false,
            sender: SenderKind::Organisation,
            locale: "en-GB",
            family: scenario,
            twin_of: None,
            differs_by: "",
            review: None,
            held_out: false,
        }
    }

    /// A Wednesday at noon, UTC.
    fn sent() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-21T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn row(kind: AskKind) -> Row {
        Row {
            kind,
            direction: Direction::AsksYou,
            made_as: "bill".into(),
            counterparty: Some("Acme".into()),
            amount: None,
            due_at: None,
            due_words: None,
        }
    }

    fn pay() -> Item {
        Item::asks(AskKind::Pay, "Acme")
    }

    fn grade(
        label: &Labelled,
        rows: Vec<Row>,
    ) -> (ClassifyEvalAsksData, Vec<ClassifyEvalMismatchData>) {
        let mut grader = AskGrader::default();
        grader.grade(
            AskInput {
                label,
                rows,
                reply_found: Some(false),
                sent: sent(),
            },
            "billing@acme.example",
        );
        let (data, mismatches) = grader.finish();
        (data.expect("graded"), mismatches)
    }

    fn kind<'a>(data: &'a ClassifyEvalAsksData, name: &str) -> &'a ClassifyEvalCountsData {
        &data
            .kinds
            .iter()
            .find(|k| k.kind == name && k.direction == "asks_you")
            .expect("listed")
            .counts
    }

    #[test]
    fn an_email_that_asks_nothing_and_makes_no_row_is_not_an_error() {
        let (data, mismatches) = grade(&label("quiet", Vec::new()), Vec::new());
        assert!(mismatches.is_empty());
        assert_eq!(data.total.expected + data.total.found, 0);
        assert_eq!(data.total.precision_percent, None);
        assert_eq!(data.total.recall_percent, None);
        assert_eq!(data.emails, 1);
    }

    #[test]
    fn a_missing_row_and_an_unexpected_row_are_told_apart() {
        // Polarity: an item with no row is a miss; a row with no item is not.
        let (data, mismatches) = grade(&label("a", vec![pay()]), Vec::new());
        assert_eq!(kind(&data, "pay").false_negatives, 1);
        assert_eq!(kind(&data, "pay").recall_percent, Some(0.0));
        assert_eq!(mismatches[0].expected, "pay");
        assert_eq!(mismatches[0].actual, "no row");

        let (data, mismatches) = grade(&label("b", Vec::new()), vec![row(AskKind::Pay)]);
        assert_eq!(kind(&data, "pay").false_positives, 1);
        assert_eq!(kind(&data, "pay").precision_percent, Some(0.0));
        assert_eq!(mismatches[0].expected, "none");
        assert_eq!(mismatches[0].actual, "pay");
    }

    #[test]
    fn a_row_of_another_kind_is_a_miss_and_a_false_alarm() {
        let (data, _) = grade(&label("a", vec![pay()]), vec![row(AskKind::Sign)]);
        assert_eq!(kind(&data, "pay").false_negatives, 1);
        assert_eq!(kind(&data, "sign").false_positives, 1);
        assert_eq!(data.total.true_positives, 0);
    }

    #[test]
    fn two_rows_for_one_item_are_a_duplicate_and_a_false_alarm() {
        let (data, mismatches) = grade(
            &label("a", vec![pay()]),
            vec![row(AskKind::Pay), row(AskKind::Pay)],
        );
        assert_eq!(data.duplicates, 1);
        assert_eq!(kind(&data, "pay").true_positives, 1);
        assert_eq!(kind(&data, "pay").false_positives, 1);
        assert!(mismatches[0].actual.contains("pay"));
        assert!(mismatches[0].reason.contains("duplicate"));
    }

    /// Two items of one kind match their own rows by fields, not by the order
    /// the rows arrive in.
    #[test]
    fn matching_does_not_depend_on_row_order() {
        let items = vec![pay().amount(1_000, "GBP"), pay().amount(2_000, "GBP")];
        let rows = |first: i64, second: i64| {
            vec![
                Row {
                    amount: Some((first, "GBP".into())),
                    ..row(AskKind::Pay)
                },
                Row {
                    amount: Some((second, "GBP".into())),
                    ..row(AskKind::Pay)
                },
            ]
        };
        for order in [rows(1_000, 2_000), rows(2_000, 1_000)] {
            let (data, mismatches) = grade(&label("two", items.clone()), order);
            let amount = data.fields.iter().find(|f| f.field == "amount").unwrap();
            assert_eq!((amount.pairs, amount.exact), (2, 2), "{mismatches:?}");
            assert!(mismatches.is_empty(), "{mismatches:?}");
        }
    }

    /// The day `days` after the email's own date, as a row's due date: the
    /// end of that day in the zone detection resolves in.
    fn due_on(days: i64) -> Option<DateTime<Utc>> {
        Some(mxr_todo::dates::end_of_day(
            (sent() + Duration::days(days)).date_naive(),
            &Local,
        ))
    }

    #[test]
    fn fields_are_compared_on_matched_pairs_only() {
        let item = pay().amount(4_820, "GBP").due(Due::AfterSent(48));
        let found = Row {
            amount: Some((4_820, "GBP".into())),
            due_at: due_on(2),
            counterparty: Some("acme".into()),
            ..row(AskKind::Pay)
        };
        let (data, mismatches) = grade(&label("exact", vec![item.clone()]), vec![found.clone()]);
        assert!(data.fields.iter().all(|f| f.pairs == 1 && f.exact == 1));
        assert!(mismatches.is_empty());

        // Missing, wrong and spurious are different failures.
        let (data, _) = grade(
            &label("wrong", vec![item]),
            vec![Row {
                amount: None,
                due_at: due_on(3),
                counterparty: Some("Other".into()),
                ..found
            }],
        );
        let field = |name: &str| data.fields.iter().find(|f| f.field == name).unwrap();
        assert_eq!(field("amount").missing, 1);
        assert_eq!(field("due").wrong, 1);
        assert_eq!(field("counterparty").wrong, 1);
        let (data, _) = grade(
            &label("spurious", vec![pay()]),
            vec![Row {
                amount: Some((100, "GBP".into())),
                due_at: due_on(1),
                ..row(AskKind::Pay)
            }],
        );
        assert_eq!(
            data.fields
                .iter()
                .filter(|f| f.field != "counterparty")
                .map(|f| f.spurious)
                .sum::<u32>(),
            2
        );
    }

    /// A due date is exact only on the day the label names: a row with some
    /// other date is wrong, not "present".
    #[test]
    fn a_due_date_is_graded_on_its_day_not_its_presence() {
        let due = |expected_hours: i64, row_days: i64| {
            let (data, mismatches) = grade(
                &label("due", vec![pay().due(Due::AfterSent(expected_hours))]),
                vec![Row {
                    due_at: due_on(row_days),
                    ..row(AskKind::Pay)
                }],
            );
            let field = data
                .fields
                .iter()
                .find(|f| f.field == "due")
                .unwrap()
                .clone();
            (field, mismatches)
        };
        // 12:00 + 60 hours is the 24th, three days later.
        let (right, _) = due(60, 3);
        assert_eq!((right.exact, right.wrong, right.present), (1, 0, 0));
        let (wrong, mismatches) = due(60, 4);
        assert_eq!((wrong.exact, wrong.wrong), (0, 1));
        let m = mismatches.iter().find(|m| m.question == "due").unwrap();
        assert_eq!(
            (m.expected.as_str(), m.actual.as_str()),
            ("2026-10-24", "2026-10-25")
        );
        // A date already past is a negative offset.
        assert_eq!(due(-48, -2).0.exact, 1);
    }

    /// Words are read by the to-do code's own reader from the email's date, so
    /// "Friday" is the next Friday after a Wednesday; words it cannot read
    /// are counted as present, never as exact.
    #[test]
    fn due_words_are_resolved_from_the_email_date_or_counted_as_present() {
        let words = |words: &'static str, row_days: i64| {
            let (data, _) = grade(
                &label("words", vec![pay().due(Due::Words(words))]),
                vec![Row {
                    due_at: due_on(row_days),
                    ..row(AskKind::Pay)
                }],
            );
            data.fields
                .iter()
                .find(|f| f.field == "due")
                .unwrap()
                .clone()
        };
        let friday = words("Friday", 2);
        assert_eq!((friday.exact, friday.wrong), (1, 0));
        let wrong_friday = words("Friday", 3);
        assert_eq!((wrong_friday.exact, wrong_friday.wrong), (0, 1));
        let unreadable = words("whenever suits", 5);
        assert_eq!((unreadable.exact, unreadable.present), (0, 1));
    }

    #[test]
    fn unmatched_items_have_no_field_to_compare() {
        let (data, _) = grade(&label("a", vec![pay().amount(1, "GBP")]), Vec::new());
        assert!(data
            .fields
            .iter()
            .all(|f| f.pairs == 0 && f.exact_percent == 0.0));
    }

    #[test]
    fn the_currency_matters_not_only_the_number() {
        let (data, _) = grade(
            &label("a", vec![pay().amount(123_456, "ZAR")]),
            vec![Row {
                amount: Some((123_456, "GBP".into())),
                ..row(AskKind::Pay)
            }],
        );
        let amount = data.fields.iter().find(|f| f.field == "amount").unwrap();
        assert_eq!(amount.wrong, 1);
    }

    #[test]
    fn every_kind_of_ask_is_listed_even_with_nothing_in_it() {
        let (data, _) = grade(&label("a", Vec::new()), Vec::new());
        let listed: Vec<&str> = data.kinds.iter().map(|k| k.kind.as_str()).collect();
        let all: Vec<&str> = AskKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(listed, all);
        assert!(data.kinds.iter().all(|k| k.direction == "asks_you"));
    }

    #[test]
    fn a_promise_is_its_own_direction_and_does_not_match_an_ask() {
        let promised = Item::promised(AskKind::Send, "Priya");
        let mut sent_mail = label("promise", vec![promised]);
        sent_mail.outbound = true;
        let found = Row {
            direction: Direction::YouPromised,
            ..row(AskKind::Send)
        };
        let mut grader = AskGrader::default();
        grader.grade(
            AskInput {
                label: &sent_mail,
                rows: vec![found],
                reply_found: None,
                sent: sent(),
            },
            "alex@demo.mxr.local",
        );
        // A promise row does not satisfy an ask of the same kind.
        grader.grade(
            AskInput {
                label: &label("ask", vec![Item::asks(AskKind::Send, "Priya")]),
                rows: vec![Row {
                    direction: Direction::YouPromised,
                    ..row(AskKind::Send)
                }],
                reply_found: Some(false),
                sent: sent(),
            },
            "priya@x.example",
        );
        let (data, _) = grader.finish();
        let promised = data
            .as_ref()
            .unwrap()
            .kinds
            .iter()
            .find(|k| k.kind == "send" && k.direction == "you_promised")
            .unwrap();
        assert_eq!(promised.counts.true_positives, 1);
        let asked = data
            .as_ref()
            .unwrap()
            .kinds
            .iter()
            .find(|k| k.kind == "send" && k.direction == "asks_you")
            .unwrap();
        assert_eq!(asked.counts.true_positives, 0);
        assert_eq!(asked.counts.false_negatives, 1);
        // Sent mail owes no reply and is not asked about one.
        assert_eq!(data.unwrap().reply_owed.emails, 1);
    }

    #[test]
    fn two_accounts_from_one_sender_that_share_a_row_are_a_merge() {
        let first = label(
            "first",
            vec![pay().reference("AG-1001").amount(25_000, "GBP")],
        );
        let second = Labelled {
            provider_id: "demo-msg-second".into(),
            ..label(
                "second",
                vec![pay().reference("AG-1002").amount(25_000, "GBP")],
            )
        };
        let mut grader = AskGrader::default();
        // Only the later email kept the row, as the store does.
        grader.grade(
            AskInput {
                label: &first,
                rows: Vec::new(),
                reply_found: Some(false),
                sent: sent(),
            },
            "levies@ashgrove.example",
        );
        grader.grade(
            AskInput {
                label: &second,
                rows: vec![row(AskKind::Pay)],
                reply_found: Some(false),
                sent: sent(),
            },
            "levies@ashgrove.example",
        );
        let (data, mismatches) = grader.finish();
        let data = data.unwrap();
        assert_eq!(data.merges, 1);
        assert!(mismatches
            .iter()
            .any(|m| m.scenario == "first" && m.actual.contains("merged")));

        // The same miss from an unrelated sender is just a miss.
        let mut grader = AskGrader::default();
        grader.grade(
            AskInput {
                label: &first,
                rows: Vec::new(),
                reply_found: Some(false),
                sent: sent(),
            },
            "levies@ashgrove.example",
        );
        grader.grade(
            AskInput {
                label: &second,
                rows: vec![row(AskKind::Pay)],
                reply_found: Some(false),
                sent: sent(),
            },
            "someone@else.example",
        );
        assert_eq!(grader.finish().0.unwrap().merges, 0);
    }

    #[test]
    fn a_twin_pair_is_right_only_when_both_members_are() {
        let plain = label("plain", Vec::new());
        let twin = Labelled {
            twin_of: Some("plain"),
            provider_id: "demo-msg-twin".into(),
            ..label("twin", vec![pay()])
        };
        let run = |plain_rows: Vec<Row>, twin_rows: Vec<Row>| {
            let mut grader = AskGrader::default();
            for (l, rows) in [(&plain, plain_rows), (&twin, twin_rows)] {
                grader.grade(
                    AskInput {
                        label: l,
                        rows,
                        reply_found: Some(false),
                        sent: sent(),
                    },
                    "a@b.example",
                );
            }
            grader.finish().0.unwrap().twins
        };
        let right = run(Vec::new(), vec![row(AskKind::Pay)]);
        assert_eq!(
            (
                right.pairs,
                right.both_right,
                right.differing_pairs,
                right.differing_both_right
            ),
            (1, 1, 1, 1)
        );
        // Asking for the plain one too, because the surface looks alike.
        let one_wrong = run(vec![row(AskKind::Pay)], vec![row(AskKind::Pay)]);
        assert_eq!((one_wrong.pairs, one_wrong.both_right), (1, 0));
        assert_eq!(one_wrong.differing_both_right, 0);
        // A twin whose target is not in the set is not a pair.
        let mut grader = AskGrader::default();
        grader.grade(
            AskInput {
                label: &twin,
                rows: Vec::new(),
                reply_found: Some(false),
                sent: sent(),
            },
            "a@b.example",
        );
        assert_eq!(grader.finish().0.unwrap().twins.pairs, 0);
    }

    /// A pair of emails that ask nothing differs by whether a reply is owed;
    /// finding no reply on the open one is a wrong answer, not a right one.
    #[test]
    fn a_twin_pair_is_wrong_when_a_reply_owed_is_missed() {
        let mut open = label("open", Vec::new());
        open.expect = open.expect.replying();
        let answered = Labelled {
            twin_of: Some("open"),
            provider_id: "demo-msg-answered".into(),
            ..label("answered", Vec::new())
        };
        let run = |open_found: bool, answered_found: bool| {
            let mut grader = AskGrader::default();
            for (l, found) in [(&open, open_found), (&answered, answered_found)] {
                grader.grade(
                    AskInput {
                        label: l,
                        rows: Vec::new(),
                        reply_found: Some(found),
                        sent: sent(),
                    },
                    "a@b.example",
                );
            }
            grader.finish().0.unwrap().twins
        };
        let right = run(true, false);
        assert_eq!(
            (right.pairs, right.both_right, right.differing_both_right),
            (1, 1, 1)
        );
        let missed = run(false, false);
        assert_eq!((missed.pairs, missed.both_right), (1, 0));
        let false_alarm = run(true, true);
        assert_eq!(
            (
                false_alarm.differing_pairs,
                false_alarm.differing_both_right
            ),
            (1, 0)
        );
    }

    /// An item found with the wrong currency is not a right answer for its
    /// email, so a pair holding it is not both right.
    #[test]
    fn a_twin_pair_is_wrong_when_a_field_is_wrong() {
        let plain = label("plain", vec![pay().amount(123_456, "GBP")]);
        let twin = Labelled {
            twin_of: Some("plain"),
            provider_id: "demo-msg-twin".into(),
            ..label("twin", vec![pay().amount(123_456, "ZAR")])
        };
        let run = |currency: &str| {
            let mut grader = AskGrader::default();
            for (l, code) in [(&plain, "GBP"), (&twin, currency)] {
                grader.grade(
                    AskInput {
                        label: l,
                        rows: vec![Row {
                            amount: Some((123_456, code.into())),
                            ..row(AskKind::Pay)
                        }],
                        reply_found: Some(false),
                        sent: sent(),
                    },
                    "a@b.example",
                );
            }
            grader.finish().0.unwrap().twins
        };
        assert_eq!(run("ZAR").both_right, 1);
        assert_eq!(run("GBP").both_right, 0);
    }

    /// The same two rows in either order pair the same way and score the
    /// same: ties go by content, not by position.
    #[test]
    fn matching_two_rows_gives_the_same_result_in_either_order() {
        let items = vec![
            pay().amount(1_000, "GBP").due(Due::AfterSent(48)),
            Item::asks(AskKind::Pay, "Beta")
                .amount(2_000, "GBP")
                .due(Due::AfterSent(48)),
        ];
        let first = Row {
            amount: Some((1_000, "GBP".into())),
            due_at: due_on(2),
            counterparty: Some("Beta".into()),
            ..row(AskKind::Pay)
        };
        let second = Row {
            amount: Some((1_000, "GBP".into())),
            due_at: due_on(3),
            counterparty: Some("Acme".into()),
            ..row(AskKind::Pay)
        };
        let label = label("two", items);
        let forward = grade(&label, vec![first.clone(), second.clone()]);
        let reversed = grade(&label, vec![second, first]);
        assert_eq!(forward.0, reversed.0);
        assert_eq!(forward.1, reversed.1);
        // Item 1 (Acme, 10.00) is paired with the row that agrees on the most
        // fields over the pairing that makes one pair better and one worse.
        let counts = |data: &ClassifyEvalAsksData, field: &str| {
            data.fields
                .iter()
                .find(|f| f.field == field)
                .map(|f| f.exact)
                .unwrap()
        };
        assert_eq!(counts(&forward.0, "counterparty"), 2);
    }

    /// More rows than items: the extra row is a duplicate and the pairing
    /// does not depend on which came first.
    #[test]
    fn the_better_row_is_paired_whichever_comes_first() {
        let item = pay().amount(1_000, "GBP");
        let good = Row {
            amount: Some((1_000, "GBP".into())),
            ..row(AskKind::Pay)
        };
        let bad = Row {
            amount: Some((9_999, "GBP".into())),
            ..row(AskKind::Pay)
        };
        for rows in [vec![good.clone(), bad.clone()], vec![bad, good]] {
            let (data, _) = grade(&label("one", vec![item.clone()]), rows);
            let amount = data.fields.iter().find(|f| f.field == "amount").unwrap();
            assert_eq!((amount.exact, amount.wrong), (1, 0));
            assert_eq!(data.duplicates, 1);
        }
    }

    #[test]
    fn a_reply_owed_is_graded_on_its_own() {
        let mut owes = label("owes", Vec::new());
        owes.expect = owes.expect.replying();
        let mut grader = AskGrader::default();
        for (l, found) in [
            (&owes, true),
            (&owes, false),
            (&label("none", Vec::new()), true),
        ] {
            grader.grade(
                AskInput {
                    label: l,
                    rows: Vec::new(),
                    reply_found: Some(found),
                    sent: sent(),
                },
                "a@b.example",
            );
        }
        let (data, mismatches) = grader.finish();
        let reply = data.unwrap().reply_owed;
        assert_eq!(
            (
                reply.emails,
                reply.counts.true_positives,
                reply.counts.false_positives,
                reply.counts.false_negatives
            ),
            (3, 1, 1, 1)
        );
        assert_eq!(
            mismatches.iter().filter(|m| m.question == "reply").count(),
            2
        );
    }

    #[test]
    fn slices_split_by_sender_locale_family_and_phrasing() {
        let mut held = label("held", vec![pay()]);
        held.held_out = true;
        held.sender = SenderKind::Person;
        held.locale = "en-ZA";
        let mut grader = AskGrader::default();
        grader.grade(
            AskInput {
                label: &held,
                rows: vec![row(AskKind::Pay)],
                reply_found: Some(false),
                sent: sent(),
            },
            "a@b.example",
        );
        grader.grade(
            AskInput {
                label: &label("quiet", Vec::new()),
                rows: Vec::new(),
                reply_found: Some(false),
                sent: sent(),
            },
            "a@b.example",
        );
        let data = grader.finish().0.unwrap();
        let slice = |dimension: &str, value: &str| {
            data.slices
                .iter()
                .find(|s| s.dimension == dimension && s.value == value)
        };
        assert_eq!(
            slice("phrasing", "held_out").unwrap().counts.true_positives,
            1
        );
        assert_eq!(slice("phrasing", "visible").unwrap().emails, 1);
        assert_eq!(slice("sender_kind", "person").unwrap().emails, 1);
        assert_eq!(slice("locale", "en-ZA").unwrap().counts.expected, 1);
        assert!(slice("template_family", "held").is_some());
        // A family with nothing to say is left out.
        assert!(slice("template_family", "quiet").is_none());
    }

    #[test]
    fn todays_kinds_map_to_chapter_37_in_one_place() {
        let asked = |kind: TodoKind| translate(&kind, "").0.as_str();
        assert_eq!(asked(TodoKind::Bill), "pay");
        assert_eq!(asked(TodoKind::PaymentFailed), "fix_payment");
        assert_eq!(asked(TodoKind::Renewal), "renew_or_cancel");
        assert_eq!(asked(TodoKind::Document), "renew_or_cancel");
        assert_eq!(asked(TodoKind::Lease), "renew_or_cancel");
        assert_eq!(asked(TodoKind::Return), "send");
        assert_eq!(asked(TodoKind::Rsvp), "attend");
        assert_eq!(asked(TodoKind::Verify), "verify");
        assert_eq!(asked(TodoKind::Sign), "sign");
        assert_eq!(asked(TodoKind::Other), "other");
        assert_eq!(
            translate(&TodoKind::Promise, "Send the signed form"),
            (AskKind::Send, Direction::YouPromised)
        );
        assert_eq!(
            translate(&TodoKind::Promise, "Think about it"),
            (AskKind::Other, Direction::YouPromised)
        );
        assert_eq!(
            translate(&TodoKind::Promise, ""),
            (AskKind::Other, Direction::YouPromised)
        );
        // Every to-do kind is covered, so a new one fails to compile above.
        for kind in TodoKind::ALL {
            let _ = translate(&kind, "");
        }
    }

    #[test]
    fn an_unknown_stored_kind_is_other_not_a_crash() {
        let todo = EvalTodo {
            source_message_id: mxr_core::id::MessageId::new(),
            kind: "from_a_newer_version".into(),
            title: String::new(),
            counterparty: None,
            amount_minor: None,
            currency: None,
            due_words: None,
            due_at: None,
        };
        assert_eq!(Row::from_todo(&todo).kind, AskKind::Other);
    }
}

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use mxr_protocol::{
    ClassifyEvalAskKindData, ClassifyEvalAsksData, ClassifyEvalCountsData, ClassifyEvalFieldData,
    ClassifyEvalMismatchData, ClassifyEvalReplyData, ClassifyEvalSliceData, ClassifyEvalTwinsData,
};
use mxr_provider_fake::fixtures::scenarios::{
    AskKind, AskProvenance, Direction, Due, Item, Labelled,
};
use mxr_todo::dates::{parse_date, DateOrder, YearIntent};
use std::collections::BTreeMap;

pub(super) const REPLY_PROJECTION: &str = "Stored asks unit answer asks_you_to_reply; missing or stale rows remain pending and eval never materializes them.";

pub(super) const ASK_QUESTIONS: [&str; 8] = [
    "ask",
    "amount",
    "due",
    "counterparty",
    "reply",
    "quote",
    "actor",
    "state",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Row {
    pub kind: AskKind,
    pub direction: Direction,
    pub made_as: String,
    pub counterparty: Option<String>,
    pub amount: Option<(i64, String)>,
    pub due_at: Option<DateTime<Utc>>,
    pub due_words: Option<String>,
    pub provenance: Option<AskProvenance>,
}

impl Row {
    pub(super) fn from_asks(
        answer: &mxr_core::asks::AskAssessment,
        counterparty: &str,
        projected_todos: &[mxr_store::EvalProjectedTodo],
    ) -> Vec<Self> {
        use mxr_core::asks::{ActionKind as A, AskAnswer};
        let mut projected: std::collections::HashMap<&str, Vec<&mxr_store::EvalTodo>> =
            std::collections::HashMap::new();
        for item in projected_todos {
            projected
                .entry(&item.unit_provenance)
                .or_default()
                .push(&item.todo);
        }
        answer
            .units
            .iter()
            .enumerate()
            .filter_map(|(index, unit)| {
                if !projected.contains_key(
                    mxr_todo::asks::items::unit_provenance(answer, index, unit).as_str(),
                ) {
                    return None;
                }
                let direction = match unit.answer {
                    AskAnswer::AsksYou => Direction::AsksYou,
                    AskAnswer::YouPromised => Direction::YouPromised,
                    _ => return None,
                };
                let tags = unit.tags.as_ref()?;
                let kind = match tags.action {
                    A::Pay => AskKind::Pay,
                    A::FixPayment => AskKind::FixPayment,
                    A::Sign => AskKind::Sign,
                    A::Verify => AskKind::Verify,
                    A::Reply if mxr_todo::asks::items::is_rsvp(answer, unit) => AskKind::Attend,
                    A::Reply => AskKind::Reply,
                    A::Send => AskKind::Send,
                    A::Provide => AskKind::Provide,
                    A::Upload => AskKind::Upload,
                    A::Book => AskKind::Book,
                    A::Attend => AskKind::Attend,
                    A::RenewOrCancel => AskKind::RenewOrCancel,
                    A::Register => AskKind::Register,
                    A::Contact => AskKind::Contact,
                    A::Other => AskKind::Other,
                };
                let due = tags.due.and_then(|i| answer.facts.dates.get(i));
                Some(Self {
                    kind,
                    direction,
                    made_as: format!("{}:unit", answer.policy_version),
                    counterparty: Some(counterparty.into()),
                    amount: tags
                        .amount
                        .and_then(|i| answer.facts.amounts.get(i))
                        .map(|a| (a.minor, a.currency.clone())),
                    due_at: due.map(|d| d.at),
                    provenance: Some(AskProvenance {
                        quote: answer.quote(unit).unwrap_or_default().into(),
                        actor: serde_json::to_value(tags.actor)
                            .ok()
                            .and_then(|v| v.as_str().map(str::to_string))
                            .unwrap_or_default(),
                        state: serde_json::to_value(tags.state)
                            .ok()
                            .and_then(|v| v.as_str().map(str::to_string))
                            .unwrap_or_default(),
                    }),
                    due_words: due.map(|d| {
                        answer.facts.zones[d.zone].text[d.span.start..d.span.end].to_string()
                    }),
                })
                .map(|row| {
                    (
                        row,
                        mxr_todo::asks::items::unit_provenance(answer, index, unit),
                    )
                })
            })
            .flat_map(|(row, key)| {
                projected
                    .get(key.as_str())
                    .into_iter()
                    .flatten()
                    .map(move |todo| {
                        let mut actual = row.clone();
                        actual.counterparty = todo.counterparty.clone();
                        actual.amount = todo.amount_minor.zip(todo.currency.clone());
                        actual.due_at = todo.due_at.and_then(|at| DateTime::from_timestamp(at, 0));
                        actual.due_words = todo.due_words.clone();
                        actual
                    })
            })
            .collect()
    }
}

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
    Present,
}

const FIELDS: [&str; 6] = ["amount", "due", "counterparty", "quote", "actor", "state"];

#[derive(Debug)]
struct Outcome {
    scenario: &'static str,
    twin_of: Option<&'static str>,
    from_domain: String,
    expected: Vec<(Item, bool)>,
    misses: Vec<usize>,
    right: bool,
    reply: bool,
    label_review: Option<String>,
    provider_id: String,
    held_out: bool,
}

pub(super) struct AskInput<'a> {
    pub provenance_slices: Vec<(&'static str, String)>,
    pub label: &'a Labelled,
    pub rows: Vec<Row>,
    pub reply_found: Option<bool>,
    pub sent: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub(super) struct AskGrader {
    emails: u32,
    total: Counts,
    kinds: BTreeMap<(AskKind, Direction), Counts>,
    fields: [FieldTally; 6],
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
            label_review: label
                .review
                .filter(|review| review.questions.contains(&"todo"))
                .map(|review| review.why.to_string()),
        }
    }

    pub(super) fn corrected(&mut self) {
        self.slices
            .entry(("projection", "user_corrected".into()))
            .or_default()
            .0 += 1;
    }

    pub(super) fn pending(&mut self) {
        self.slices
            .entry(("rules_abstained", "pending".into()))
            .or_default()
            .0 += 1;
    }

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

        let mut fields_exact = true;
        for &(i, j) in &matched {
            let (item, row) = (&expected[i], &rows[j]);
            for (slot, field) in FIELDS.iter().enumerate() {
                if matches!(*field, "quote" | "actor" | "state") && item.provenance.is_none() {
                    continue;
                }
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

        let held = if label.held_out {
            "held_out"
        } else {
            "visible"
        };
        let mut slice_keys = vec![
            ("sender_kind", label.sender.as_str().to_string()),
            ("locale", label.locale.to_string()),
            ("template_family", label.family.to_string()),
            ("phrasing", held.to_string()),
        ];
        slice_keys.extend(input.provenance_slices);
        for key in slice_keys {
            let entry = self.slices.entry(key).or_default();
            entry.0 += 1;
            entry.1.add(email_counts);
        }

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
        "quote" | "actor" | "state" => {
            let Some(expected) = item.provenance.as_ref() else {
                return Verdict::Present;
            };
            let Some(actual) = row.provenance.as_ref() else {
                return Verdict::Missing;
            };
            let (want, got) = match field {
                "quote" => (&expected.quote, &actual.quote),
                "actor" => (&expected.actor, &actual.actor),
                _ => (&expected.state, &actual.state),
            };
            if want == got {
                Verdict::Exact
            } else {
                Verdict::Wrong
            }
        }
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

fn plural(count: i64, unit: &str) -> String {
    format!("{count} {unit}{}", if count == 1 { "" } else { "s" })
}

fn expected_due_text(due: Due) -> String {
    match due {
        Due::None => "none".to_string(),
        Due::Words(words) => words.to_string(),
        Due::AfterSent(hours) => {
            let (direction, hours) = if hours < 0 {
                ("before", -hours)
            } else {
                ("after", hours)
            };
            let (days, rest) = (hours / 24, hours % 24);
            let span = match (days, rest) {
                (0, rest) => plural(rest, "hour"),
                (days, 0) => plural(days, "day"),
                (days, rest) => format!("{} {}", plural(days, "day"), plural(rest, "hour")),
            };
            format!("{span} {direction} it was sent")
        }
    }
}

fn actual_due_text(item: &Item, row: &Row, sent: DateTime<Utc>) -> String {
    let Some(got) = row_day(row) else {
        return "none".to_string();
    };
    let Some(want) = expected_day(item.due, sent) else {
        return "a date".to_string();
    };
    match (got - want).num_days() {
        0 => "on the expected day".to_string(),
        days if days > 0 => format!("{} later than expected", plural(days, "day")),
        days => format!("{} earlier than expected", plural(-days, "day")),
    }
}

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
            expected_due_text(item.due),
            actual_due_text(item, row, sent),
        ),
        "quote" | "actor" | "state" => {
            let value = |p: Option<&AskProvenance>| {
                p.map_or_else(none, |p| match field {
                    "quote" => p.quote.clone(),
                    "actor" => p.actor.clone(),
                    _ => p.state.clone(),
                })
            };
            (
                value(item.provenance.as_ref()),
                value(row.provenance.as_ref()),
            )
        }
        _ => (
            item.counterparty.clone().unwrap_or_else(none),
            row.counterparty.clone().unwrap_or_else(none),
        ),
    }
}

const EXACT_ASSIGNMENT_LIMIT: usize = 6;

fn agreement(item: &Item, row: &Row, sent: DateTime<Utc>) -> usize {
    FIELDS
        .iter()
        .filter(|field| verdict(field, item, row, sent) == Verdict::Exact)
        .count()
}

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
            provenance: None,
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
                provenance_slices: vec![],
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
    fn a_missing_row_and_an_unexpected_row_are_told_apart() {
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

    fn due_on(days: i64) -> Option<DateTime<Utc>> {
        Some(mxr_todo::dates::end_of_day(
            (sent() + Duration::days(days)).date_naive(),
            &Local,
        ))
    }

    #[test]
    fn fields_are_compared_on_matched_pairs_only() {
        let item = pay()
            .amount(4_820, "GBP")
            .due(Due::AfterSent(48))
            .provenance("Please pay the invoice.", "owner", "pending");
        let found = Row {
            provenance: item.provenance.clone(),
            amount: Some((4_820, "GBP".into())),
            due_at: due_on(2),
            counterparty: Some("acme".into()),
            ..row(AskKind::Pay)
        };
        let (data, mismatches) = grade(&label("exact", vec![item.clone()]), vec![found.clone()]);
        assert!(data.fields.iter().all(|f| f.pairs == 1 && f.exact == 1));
        assert!(mismatches.is_empty());

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
        let (right, _) = due(60, 3);
        assert_eq!((right.exact, right.wrong, right.present), (1, 0, 0));
        let (wrong, mismatches) = due(60, 4);
        assert_eq!((wrong.exact, wrong.wrong), (0, 1));
        let m = mismatches.iter().find(|m| m.question == "due").unwrap();
        assert_eq!(
            (m.expected.as_str(), m.actual.as_str()),
            (
                "2 days 12 hours after it was sent",
                "1 day later than expected"
            )
        );
        assert_eq!(due(-48, -2).0.exact, 1);
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
                provenance_slices: vec![],
                label: &sent_mail,
                rows: vec![found],
                reply_found: None,
                sent: sent(),
            },
            "alex@demo.mxr.local",
        );
        grader.grade(
            AskInput {
                provenance_slices: vec![],
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
        grader.grade(
            AskInput {
                provenance_slices: vec![],
                label: &first,
                rows: Vec::new(),
                reply_found: Some(false),
                sent: sent(),
            },
            "levies@ashgrove.example",
        );
        grader.grade(
            AskInput {
                provenance_slices: vec![],
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

        let mut grader = AskGrader::default();
        grader.grade(
            AskInput {
                provenance_slices: vec![],
                label: &first,
                rows: Vec::new(),
                reply_found: Some(false),
                sent: sent(),
            },
            "levies@ashgrove.example",
        );
        grader.grade(
            AskInput {
                provenance_slices: vec![],
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
                        provenance_slices: vec![],
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
        let one_wrong = run(vec![row(AskKind::Pay)], vec![row(AskKind::Pay)]);
        assert_eq!((one_wrong.pairs, one_wrong.both_right), (1, 0));
        assert_eq!(one_wrong.differing_both_right, 0);
        let mut grader = AskGrader::default();
        grader.grade(
            AskInput {
                provenance_slices: vec![],
                label: &twin,
                rows: Vec::new(),
                reply_found: Some(false),
                sent: sent(),
            },
            "a@b.example",
        );
        assert_eq!(grader.finish().0.unwrap().twins.pairs, 0);
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
                    provenance_slices: vec![],
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
                provenance_slices: vec![],
                label: &held,
                rows: vec![row(AskKind::Pay)],
                reply_found: Some(false),
                sent: sent(),
            },
            "a@b.example",
        );
        grader.grade(
            AskInput {
                provenance_slices: vec![],
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
        assert!(slice("template_family", "quiet").is_none());
    }
}

#[cfg(test)]
mod provenance_tests {
    use super::*;
    #[test]
    fn calendar_reply_uses_the_historical_rsvp_bucket_without_changing_the_action() {
        use mxr_core::asks::*;
        let message = mxr_core::MessageId::new();
        let account = mxr_core::AccountId::new();
        let tags = AskTags {
            act: Act::Request,
            actor: Actor::Owner,
            state: AskState::Pending,
            action: ActionKind::Reply,
            amount: None,
            due: None,
            reference: None,
        };
        let mut a = AskAssessment {
            message_id: message,
            account_id: account,
            policy_version: ASKS_POLICY_VERSION.into(),
            input_fingerprint: "test".into(),
            pending_body: false,
            context_token: "test".into(),
            calendar: None,
            html: None,
            facts: AskFacts {
                zones: vec![AskZone {
                    kind: ZoneKind::Calendar,
                    author: None,
                    text: "Planning".into(),
                }],
                recipient_role: RecipientRole::AloneTo,
                owner_authored: false,
                owner_names: vec![],
                owner_addresses: vec![],
                predecessor: None,
                owner_replied_afterwards: false,
                amounts: vec![],
                dates: vec![],
                references: vec![],
                template_key: "test".into(),
            },
            units: vec![AskUnit {
                zone: 0,
                span: AskSpan { start: 0, end: 8 },
                votes: vec![],
                tags: Some(tags.clone()),
                document: DocumentReading {
                    kind: DocumentKind::Other,
                    status: PaymentStatus::None,
                    required_fields_present: true,
                },
                answer: AskAnswer::AsksYou,
            }],
        };
        let links = vec![mxr_store::EvalProjectedTodo {
            unit_provenance: mxr_todo::asks::items::unit_provenance(&a, 0, &a.units[0]),
            user_edited: 0,
            todo: mxr_store::EvalTodo {
                source_message_id: a.message_id.clone(),
                kind: "rsvp".into(),
                title: "Planning".into(),
                counterparty: Some("Organiser".into()),
                amount_minor: None,
                currency: None,
                due_words: None,
                due_at: None,
            },
        }];
        let calendar = Row::from_asks(&a, "Organiser", &links);
        assert_eq!(calendar[0].kind, AskKind::Attend);
        assert!(mxr_todo::asks::items::is_rsvp(&a, &a.units[0]));
        let provenance = calendar[0].provenance.as_ref().expect("stored provenance");
        assert_eq!(
            (&*provenance.quote, &*provenance.actor, &*provenance.state),
            ("Planning", "owner", "pending")
        );
        assert_eq!(a.units[0].tags.as_ref(), Some(&tags));
        a.facts.zones[0].kind = ZoneKind::Authored;
        assert_eq!(Row::from_asks(&a, "Sender", &links)[0].kind, AskKind::Reply);
        assert!(!mxr_todo::asks::items::is_rsvp(&a, &a.units[0]));
        assert_eq!(a.units[0].tags.as_ref(), Some(&tags));
    }

    #[test]
    fn exact_quote_actor_and_state_have_independent_verdicts() {
        let item = Item::asks(AskKind::Sign, "Alex").provenance(
            "Please sign the contract.",
            "owner",
            "pending",
        );
        let mut row = Row {
            kind: AskKind::Sign,
            direction: Direction::AsksYou,
            made_as: "stored".into(),
            counterparty: Some("Alex".into()),
            amount: None,
            due_at: None,
            due_words: None,
            provenance: item.provenance.clone(),
        };
        let sent = Utc::now();
        for field in ["quote", "actor", "state"] {
            assert_eq!(verdict(field, &item, &row, sent), Verdict::Exact);
        }
        row.provenance.as_mut().unwrap().quote = "Sign it".into();
        assert_eq!(verdict("quote", &item, &row, sent), Verdict::Wrong);
        row.provenance.as_mut().unwrap().actor = "correspondent".into();
        assert_eq!(verdict("actor", &item, &row, sent), Verdict::Wrong);
        row.provenance.as_mut().unwrap().state = "done".into();
        assert_eq!(verdict("state", &item, &row, sent), Verdict::Wrong);
        row.provenance = None;
        assert_eq!(verdict("quote", &item, &row, sent), Verdict::Missing);
    }
}

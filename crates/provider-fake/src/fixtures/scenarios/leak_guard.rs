//! The leak guard (blueprint 37, "The eval grades what the person relies
//! on"; Gururangan et al. 2018).
//!
//! A twin pair differs in the one fact that should change what the email
//! asks. If a classifier that sees only the surface of an email (the words of
//! its subject with digits masked, its sender, a length bucket) can tell a
//! pair's members apart, the corpus leaks its label through the surface and a
//! judge could score well without reading the mail. This test trains such a
//! baseline on every other labelled email and fails when it gets both members
//! of too many pairs right.

use super::{scenario_seed, DemoCuts, Labelled, ScenarioSeed, SELF_EMAIL};
use chrono::{TimeZone, Utc};
use std::collections::HashMap;

/// The surface baseline may get both members of at most this share of the
/// pairs whose answers differ. A guess for each member is right half the
/// time, so chance for a pair is a quarter; the bar is set a little above it.
/// A corpus that leaks scores far higher (see
/// `a_surface_that_separates_a_pair_is_caught`).
const BAR_PERCENT: f64 = 40.0;

/// What a surface-only judge reads.
fn surface(subject: &str, from_email: &str, body: &str) -> Vec<String> {
    let mut features = Vec::new();
    let skeleton: String = subject
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect();
    for word in skeleton.split(|c: char| !c.is_alphanumeric() && c != '#') {
        if !word.is_empty() {
            features.push(format!("subject:{word}"));
        }
    }
    features.push(format!("sender:{}", from_email.to_lowercase()));
    if let Some((_, domain)) = from_email.rsplit_once('@') {
        features.push(format!("domain:{}", domain.to_lowercase()));
    }
    features.push(format!("length:{}", (body.chars().count() / 80).min(12)));
    features
}

/// Naive Bayes over surface features with add-one smoothing: does the email
/// ask something of the owner?
struct Baseline {
    /// Per class (0: asks nothing, 1: asks), the count of each feature.
    counts: [HashMap<String, f64>; 2],
    totals: [f64; 2],
    emails: [f64; 2],
}

impl Baseline {
    fn train<'a>(examples: impl Iterator<Item = (&'a [String], bool)>) -> Self {
        let mut baseline = Self {
            counts: [HashMap::new(), HashMap::new()],
            totals: [0.0; 2],
            emails: [0.0; 2],
        };
        for (features, asks) in examples {
            let class = usize::from(asks);
            baseline.emails[class] += 1.0;
            for feature in features {
                *baseline.counts[class].entry(feature.clone()).or_default() += 1.0;
                baseline.totals[class] += 1.0;
            }
        }
        baseline
    }

    fn predicts_an_ask(&self, features: &[String]) -> bool {
        let vocabulary: f64 = self
            .counts
            .iter()
            .flat_map(HashMap::keys)
            .collect::<std::collections::HashSet<_>>()
            .len() as f64;
        let score = |class: usize| {
            let prior = ((self.emails[class] + 1.0) / (self.emails[0] + self.emails[1] + 2.0)).ln();
            features.iter().fold(prior, |sum, feature| {
                let seen = self.counts[class].get(feature).copied().unwrap_or(0.0);
                sum + ((seen + 1.0) / (self.totals[class] + vocabulary + 1.0)).ln()
            })
        };
        score(1) > score(0)
    }
}

struct Example {
    scenario: &'static str,
    twin_of: Option<&'static str>,
    features: Vec<String>,
    asks: bool,
}

fn examples(seed: &ScenarioSeed) -> Vec<Example> {
    seed.labels
        .iter()
        .filter(|label| !label.outbound)
        .filter_map(|label: &Labelled| {
            let (envelope, body) = seed
                .messages
                .iter()
                .find(|(envelope, _)| envelope.provider_id == label.provider_id)?;
            Some(Example {
                scenario: label.scenario,
                twin_of: label.twin_of,
                features: surface(
                    &envelope.subject,
                    &envelope.from.email,
                    body.text_plain.as_deref().unwrap_or(""),
                ),
                asks: !label.items.is_empty(),
            })
        })
        .collect()
}

/// The pairs whose members' answers differ, and how many of them the baseline
/// gets both members of right when it has not seen either member.
fn pairs_the_surface_gets_right(examples: &[Example]) -> (usize, usize) {
    let (mut pairs, mut right) = (0, 0);
    for twin in examples.iter().filter(|e| e.twin_of.is_some()) {
        let Some(plain) = examples.iter().find(|e| Some(e.scenario) == twin.twin_of) else {
            continue;
        };
        if twin.asks == plain.asks {
            continue;
        }
        pairs += 1;
        let baseline = Baseline::train(
            examples
                .iter()
                .filter(|e| e.scenario != twin.scenario && e.scenario != plain.scenario)
                .map(|e| (e.features.as_slice(), e.asks)),
        );
        right += usize::from(
            baseline.predicts_an_ask(&twin.features) == twin.asks
                && baseline.predicts_an_ask(&plain.features) == plain.asks,
        );
    }
    (pairs, right)
}

fn seed() -> ScenarioSeed {
    let at = Utc
        .with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
        .single()
        .expect("a valid date");
    scenario_seed(&super::account_id(), at, &DemoCuts::default())
}

/// The surface of an email says nothing the answer depends on: even trained
/// on every other email, it does not separate the members of the twin pairs
/// whose answers differ.
#[test]
fn a_surface_only_baseline_fails_the_twins() {
    let seed = seed();
    let examples = examples(&seed);
    let (pairs, right) = pairs_the_surface_gets_right(&examples);
    assert!(pairs >= 15, "only {pairs} pairs differ in what they ask");
    let percent = right as f64 / pairs as f64 * 100.0;
    eprintln!("surface baseline: both members right in {right} of {pairs} pairs ({percent:.0}%)");
    assert!(
        percent <= BAR_PERCENT,
        "the surface alone gets both members right in {right} of {pairs} pairs \
         ({percent:.0}%, bar {BAR_PERCENT}%): the corpus leaks its labels"
    );
}

/// The guard has teeth: on a corpus where one subject word decides the
/// answer, the baseline gets the pair right and the check would fail.
#[test]
fn a_surface_that_separates_a_pair_is_caught() {
    let example = |scenario, twin_of, subject: &str, asks| Example {
        scenario,
        twin_of,
        features: surface(subject, "billing@acme.example", "body"),
        asks,
    };
    let mut examples = vec![
        example("pair-plain", None, "Invoice #5", true),
        example("pair-twin", Some("pair-plain"), "Newsletter #5", false),
    ];
    for n in 0..6 {
        examples.push(example("x", None, &format!("Invoice {n}"), true));
        examples.push(example("y", None, &format!("Newsletter {n}"), false));
    }
    let (pairs, right) = pairs_the_surface_gets_right(&examples);
    assert_eq!((pairs, right), (1, 1));
}

#[test]
fn the_owner_is_never_a_baseline_example() {
    let seed = seed();
    assert!(examples(&seed).iter().all(|e| !e.scenario.is_empty()));
    assert!(seed
        .labels
        .iter()
        .filter(|label| label.from_email.eq_ignore_ascii_case(SELF_EMAIL))
        .all(|label| label.outbound));
}

//! The classifier's floor, per kind of ask (blueprint 37, R1c).
//!
//! `classify_eval_floor.json` holds the precision and recall each kind of ask
//! reached when the floor was last raised, for the quick and the full set. A
//! run that falls below any of them fails, so a change that makes the
//! classifier worse at one kind cannot hide behind a gain at another. The
//! quick set is checked by the quick-set test and the full set by the full
//! test that CI's "Classifier Eval" job runs.
//!
//! The floor only goes up. After a change that improves a kind, raise it with
//! `MXR_RAISE_CLASSIFY_FLOOR=1`, which rewrites the file with the better of
//! the old and the measured value and never lowers one; review the diff. To
//! lower a floor on purpose (a label was wrong, a kind was retired), edit the
//! file by hand and say why in the commit.

use mxr_protocol::{ClassifyEvalAsksData, ClassifyEvalCountsData};
use std::collections::BTreeMap;

const FLOOR_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/crates/daemon/src/handler/tests/classify_eval_floor.json"
);
const RAISE_ENV: &str = "MXR_RAISE_CLASSIFY_FLOOR";

/// A kind's precision and recall, in percent. Absent when it was undefined
/// (nothing found, or nothing expected) when the floor was set.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Floor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    precision: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recall: Option<f64>,
    /// The raw count behind a measurement, never stored: the percentages are
    /// rounded, and one false positive in two thousand rounds to 100.0%.
    #[serde(skip)]
    false_positives: Option<u32>,
}

type Floors = BTreeMap<String, BTreeMap<String, Floor>>;

fn read() -> Floors {
    let text = std::fs::read_to_string(FLOOR_PATH).expect("the floor file exists");
    serde_json::from_str(&text).expect("the floor file is valid")
}

/// `all`, then `kind` for what you were asked and `you_promised/kind`.
fn measured(asks: &ClassifyEvalAsksData) -> BTreeMap<String, Floor> {
    let floor = |counts: &ClassifyEvalCountsData| Floor {
        precision: counts.precision_percent,
        recall: counts.recall_percent,
        false_positives: Some(counts.false_positives),
    };
    let mut out = BTreeMap::new();
    out.insert("all".to_string(), floor(&asks.total));
    for kind in &asks.kinds {
        let name = if kind.direction == "asks_you" {
            kind.kind.clone()
        } else {
            format!("{}/{}", kind.direction, kind.kind)
        };
        out.insert(name, floor(&kind.counts));
    }
    out
}

/// What is below the floor, one line each.
fn shortfalls(floor: &BTreeMap<String, Floor>, now: &BTreeMap<String, Floor>) -> Vec<String> {
    let mut out = Vec::new();
    // A kind that found nothing when the floor was set has no precision to
    // fall below, so a false positive in it would pass. Once it finds
    // anything, its precision must be perfect: true positives may appear, a
    // wrong row may not.
    for (kind, got) in now {
        let baseline = floor.get(kind).and_then(|f| f.precision);
        // Counted, not read from the rounded percentage.
        if baseline.is_none() && got.false_positives.is_some_and(|n| n > 0) {
            out.push(format!(
                "{kind} precision: {} false positive(s) where the floor had nothing found; a new kind may add true positives but no false ones",
                got.false_positives.unwrap_or_default()
            ));
        }
    }
    for (kind, wanted) in floor {
        let got = now.get(kind).cloned().unwrap_or_default();
        for (name, want, have) in [
            ("precision", wanted.precision, got.precision),
            ("recall", wanted.recall, got.recall),
        ] {
            let Some(want) = want else { continue };
            // Undefined now (nothing found or expected) is below any floor
            // that was defined: the kind stopped being found.
            if have.is_none_or(|have| have + 1e-9 < want) {
                out.push(format!(
                    "{kind} {name}: {} is below the floor {want}",
                    have.map_or_else(|| "undefined".to_string(), |h| h.to_string())
                ));
            }
        }
    }
    out
}

fn better(old: Option<f64>, new: Option<f64>) -> Option<f64> {
    match (old, new) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

/// Fail when any kind of the `set` ("quick" or "full") is below its floor, or
/// raise the floor when asked to.
pub(super) fn check(set: &str, asks: &ClassifyEvalAsksData) {
    let now = measured(asks);
    let mut floors = read();
    if std::env::var_os(RAISE_ENV).is_some() {
        let entry = floors.entry(set.to_string()).or_default();
        for (kind, got) in &now {
            let old = entry.remove(kind).unwrap_or_default();
            entry.insert(
                kind.clone(),
                Floor {
                    precision: better(old.precision, got.precision),
                    recall: better(old.recall, got.recall),
                    false_positives: None,
                },
            );
        }
        let mut text = serde_json::to_string_pretty(&floors).expect("floors serialise");
        text.push('\n');
        std::fs::write(FLOOR_PATH, text).expect("the floor file is writable");
        return;
    }
    let floor = floors
        .get(set)
        .unwrap_or_else(|| panic!("the floor file has no `{set}` set"));
    let short = shortfalls(floor, &now);
    assert!(
        short.is_empty(),
        "the {set} set fell below its per-kind floor:\n{}\n(raise the floor after an improvement with {RAISE_ENV}=1)",
        short.join("\n")
    );
}

/// The ask results as the text a snapshot holds: counts per kind, fields,
/// duplicates and merges, twins, slices and the reply question. Percentages
/// are the floor's inputs, so they are in it too.
pub(super) fn report(asks: &ClassifyEvalAsksData) -> String {
    let percent =
        |value: Option<f64>| value.map_or_else(|| "-".to_string(), |v| format!("{v:.1}%"));
    let counts = |c: &ClassifyEvalCountsData| {
        format!(
            "expected {:>5} found {:>5} tp {:>5} fp {:>5} fn {:>5} precision {:>6} recall {:>6}",
            c.expected,
            c.found,
            c.true_positives,
            c.false_positives,
            c.false_negatives,
            percent(c.precision_percent),
            percent(c.recall_percent)
        )
    };
    let mut out = format!("asks graded on {} emails\n", asks.emails);
    out.push_str(&format!("{:<28} {}\n", "all", counts(&asks.total)));
    for kind in &asks.kinds {
        let name = if kind.direction == "asks_you" {
            kind.kind.clone()
        } else {
            format!("you_promised/{}", kind.kind)
        };
        out.push_str(&format!("{name:<28} {}\n", counts(&kind.counts)));
    }
    out.push('\n');
    for field in &asks.fields {
        out.push_str(&format!(
            "field {:<13} pairs {:>5} exact {:>5} ({:.1}%) missing {} wrong {} spurious {}\n",
            field.field,
            field.pairs,
            field.exact,
            field.exact_percent,
            field.missing,
            field.wrong,
            field.spurious
        ));
    }
    out.push_str(&format!(
        "\nduplicates {} merges {}\ntwin pairs {} both right {}; differing pairs {} both right {}\n",
        asks.duplicates,
        asks.merges,
        asks.twins.pairs,
        asks.twins.both_right,
        asks.twins.differing_pairs,
        asks.twins.differing_both_right
    ));
    out.push_str(&format!(
        "reply owed ({} emails) {}\n\n",
        asks.reply_owed.emails,
        counts(&asks.reply_owed.counts)
    ));
    for slice in &asks.slices {
        out.push_str(&format!(
            "{:<16} {:<34} emails {:>6} {}\n",
            slice.dimension,
            slice.value,
            slice.emails,
            counts(&slice.counts)
        ));
    }
    out
}

fn counts(expected: u32, found: u32, tp: u32) -> ClassifyEvalCountsData {
    let percent =
        |part: u32, whole: u32| (whole > 0).then(|| f64::from(part) / f64::from(whole) * 100.0);
    ClassifyEvalCountsData {
        expected,
        found,
        true_positives: tp,
        false_positives: found - tp,
        false_negatives: expected - tp,
        precision_percent: percent(tp, found),
        recall_percent: percent(tp, expected),
    }
}

#[test]
fn a_kind_below_its_floor_is_reported_and_one_at_it_is_not() {
    let floor = BTreeMap::from([(
        "pay".to_string(),
        Floor {
            precision: Some(50.0),
            recall: Some(40.0),
            ..Floor::default()
        },
    )]);
    let at = |p, r| {
        BTreeMap::from([(
            "pay".to_string(),
            Floor {
                precision: p,
                recall: r,
                ..Floor::default()
            },
        )])
    };
    assert!(shortfalls(&floor, &at(Some(50.0), Some(40.0))).is_empty());
    assert!(shortfalls(&floor, &at(Some(80.0), Some(90.0))).is_empty());
    assert_eq!(shortfalls(&floor, &at(Some(49.9), Some(40.0))).len(), 1);
    assert_eq!(shortfalls(&floor, &at(Some(50.0), Some(10.0))).len(), 1);
    // A kind that stopped being found is below its floor, not undefined.
    assert_eq!(shortfalls(&floor, &at(None, Some(0.0))).len(), 2);
    assert_eq!(shortfalls(&floor, &BTreeMap::new()).len(), 2);
    // A floor that was undefined constrains nothing.
    assert!(shortfalls(
        &BTreeMap::from([("x".into(), Floor::default())]),
        &at(None, None)
    )
    .is_empty());
}

/// One pay false positive fewer and one new verify false positive leaves the
/// total unchanged. The kind with no baseline precision must still fail, also
/// when the false positive is too rare to move the rounded percentage.
#[test]
fn a_false_positive_in_a_kind_with_no_baseline_is_a_shortfall() {
    let at_floor = Floor {
        precision: Some(50.0),
        recall: Some(50.0),
        ..Floor::default()
    };
    let floor = BTreeMap::from([
        ("all".to_string(), at_floor.clone()),
        ("pay".to_string(), at_floor),
        ("verify".to_string(), Floor::default()),
    ]);
    // `verify` is its precision and false-positive count.
    let at = |all: f64, pay: f64, verify: Option<(f64, u32)>| {
        let measured = |precision: f64| Floor {
            precision: Some(precision),
            recall: Some(50.0),
            false_positives: Some(0),
        };
        BTreeMap::from([
            ("all".to_string(), measured(all)),
            ("pay".to_string(), measured(pay)),
            (
                "verify".to_string(),
                Floor {
                    precision: verify.map(|(precision, _)| precision),
                    recall: None,
                    false_positives: Some(verify.map_or(0, |(_, fp)| fp)),
                },
            ),
        ])
    };
    // pay improved, verify gained a false positive, the total did not move.
    let swapped = shortfalls(&floor, &at(50.0, 100.0, Some((0.0, 1))));
    assert_eq!(swapped.len(), 1, "{swapped:?}");
    assert!(swapped[0].starts_with("verify precision"));
    // 2,001 true positives and one false positive round to 100.0%; it fails.
    let rare = shortfalls(&floor, &at(50.0, 50.0, Some((100.0, 1))));
    assert_eq!(rare.len(), 1, "{rare:?}");
    // A true positive with no false one is fine, and so is staying silent.
    assert!(shortfalls(&floor, &at(50.0, 50.0, Some((100.0, 0)))).is_empty());
    assert!(shortfalls(&floor, &at(50.0, 50.0, None)).is_empty());
}

#[test]
fn the_floor_file_names_real_kinds_for_both_sets() {
    let floors = read();
    for set in ["quick", "full"] {
        let kinds = floors
            .get(set)
            .unwrap_or_else(|| panic!("no `{set}` floor"));
        assert!(kinds.contains_key("all"), "{set}");
        for name in kinds.keys().filter(|name| *name != "all") {
            let kind = name.rsplit('/').next().unwrap_or(name);
            assert!(
                mxr_provider_fake::fixtures::scenarios::AskKind::ALL
                    .iter()
                    .any(|k| k.as_str() == kind),
                "{set}: `{name}` is not a kind of ask"
            );
        }
    }
}

#[test]
fn a_measurement_is_turned_into_floors_per_kind() {
    let asks = ClassifyEvalAsksData {
        emails: 3,
        total: counts(4, 3, 2),
        kinds: vec![mxr_protocol::ClassifyEvalAskKindData {
            kind: "pay".into(),
            direction: "asks_you".into(),
            counts: counts(4, 3, 2),
        }],
        fields: Vec::new(),
        duplicates: 0,
        merges: 0,
        twins: Default::default(),
        slices: Vec::new(),
        reply_owed: mxr_protocol::ClassifyEvalReplyData {
            emails: 0,
            counts: Default::default(),
            projection: String::new(),
        },
    };
    let got = measured(&asks);
    assert_eq!(got["pay"].recall, Some(50.0));
    assert!((got["pay"].precision.unwrap() - 66.666).abs() < 0.01);
    assert!(report(&asks).contains("pay"));
}

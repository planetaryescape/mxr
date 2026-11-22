use mxr_protocol::{ClassifyEvalAsksData, ClassifyEvalCountsData};
use std::collections::BTreeMap;

const FLOOR_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/crates/daemon/src/handler/tests/classify_eval_floor.json"
);
const RAISE_ENV: &str = "MXR_RAISE_CLASSIFY_FLOOR";

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Floor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    precision: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recall: Option<f64>,
    #[serde(skip)]
    false_positives: Option<u32>,
}

type Floors = BTreeMap<String, BTreeMap<String, Floor>>;

fn read() -> Floors {
    let text = std::fs::read_to_string(FLOOR_PATH).expect("the floor file exists");
    serde_json::from_str(&text).expect("the floor file is valid")
}

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

fn shortfalls(floor: &BTreeMap<String, Floor>, now: &BTreeMap<String, Floor>) -> Vec<String> {
    let mut out = Vec::new();
    for (kind, got) in now {
        let baseline = floor.get(kind).and_then(|f| f.precision);
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
    assert_eq!(shortfalls(&floor, &at(None, Some(0.0))).len(), 2);
    assert_eq!(shortfalls(&floor, &BTreeMap::new()).len(), 2);
    assert!(shortfalls(
        &BTreeMap::from([("x".into(), Floor::default())]),
        &at(None, None)
    )
    .is_empty());
}

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
    let swapped = shortfalls(&floor, &at(50.0, 100.0, Some((0.0, 1))));
    assert_eq!(swapped.len(), 1, "{swapped:?}");
    assert!(swapped[0].starts_with("verify precision"));
    let rare = shortfalls(&floor, &at(50.0, 50.0, Some((100.0, 1))));
    assert_eq!(rare.len(), 1, "{rare:?}");
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

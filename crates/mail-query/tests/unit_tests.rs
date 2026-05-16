//! Unit tests ported from the original mxr-search parser, plus new
//! coverage for `Custom`, `Exact`, `DateValue::Relative`, `Display`, and
//! `Visitor`.

#![allow(clippy::panic, clippy::unwrap_used)]

use chrono::NaiveDate;
use mail_query::{
    parse, parse_with, DateBound, DateValue, FilterKind, ParseError, ParserOptions, QueryField,
    QueryNode, RelativeUnit, SizeOp, Visitor,
};

#[test]
fn parse_single_word() {
    assert_eq!(parse("deployment").unwrap(), QueryNode::Text("deployment".into()));
}

#[test]
fn parse_phrase() {
    assert_eq!(
        parse("\"deployment plan\"").unwrap(),
        QueryNode::Phrase("deployment plan".into())
    );
}

#[test]
fn parse_phrase_with_escaped_inner_quote() {
    // Gmail allows: "foo \"bar\""
    assert_eq!(
        parse(r#""foo \"bar\"""#).unwrap(),
        QueryNode::Phrase(r#"foo "bar""#.into())
    );
}

#[test]
fn parse_exact_plus_word() {
    assert_eq!(parse("+stripe").unwrap(), QueryNode::Exact("stripe".into()));
}

#[test]
fn parse_exact_drops_only_leading_plus() {
    // Bare `+` with no rest stays as Text.
    assert_eq!(parse("+").unwrap(), QueryNode::Text("+".into()));
}

#[test]
fn parse_from_field() {
    assert_eq!(
        parse("from:alice@example.com").unwrap(),
        QueryNode::Field {
            field: QueryField::From,
            value: "alice@example.com".into(),
        }
    );
}

#[test]
fn parse_to_field() {
    assert_eq!(
        parse("to:bob").unwrap(),
        QueryNode::Field {
            field: QueryField::To,
            value: "bob".into(),
        }
    );
}

#[test]
fn parse_cc_bcc_and_body_fields() {
    assert_eq!(
        parse("cc:alice@example.com").unwrap(),
        QueryNode::Field {
            field: QueryField::Cc,
            value: "alice@example.com".into(),
        }
    );
    assert_eq!(
        parse("bcc:hidden@example.com").unwrap(),
        QueryNode::Field {
            field: QueryField::Bcc,
            value: "hidden@example.com".into(),
        }
    );
    assert_eq!(
        parse("body:\"deploy canary\"").unwrap(),
        QueryNode::Field {
            field: QueryField::Body,
            value: "deploy canary".into(),
        }
    );
}

#[test]
fn parse_subject_field() {
    assert_eq!(
        parse("subject:invoice").unwrap(),
        QueryNode::Field {
            field: QueryField::Subject,
            value: "invoice".into(),
        }
    );
}

#[test]
fn parse_is_unread() {
    assert_eq!(parse("is:unread").unwrap(), QueryNode::Filter(FilterKind::Unread));
}

#[test]
fn parse_is_starred() {
    assert_eq!(parse("is:starred").unwrap(), QueryNode::Filter(FilterKind::Starred));
}

#[test]
fn parse_additional_is_filters() {
    let cases = [
        ("is:sent", FilterKind::Sent),
        ("is:draft", FilterKind::Draft),
        ("is:trash", FilterKind::Trash),
        ("is:spam", FilterKind::Spam),
        ("is:answered", FilterKind::Answered),
        ("is:inbox", FilterKind::Inbox),
        ("is:archived", FilterKind::Archived),
    ];
    for (input, expected) in cases {
        assert_eq!(parse(input).unwrap(), QueryNode::Filter(expected));
    }
}

#[test]
fn unknown_filter_without_registration_errors() {
    let result = parse("is:owed-reply");
    assert_eq!(
        result,
        Err(ParseError::UnknownFilter("owed-reply".to_string()))
    );
}

#[test]
fn custom_filter_via_options() {
    let mut options = ParserOptions::new();
    options.register_custom_filter("owed-reply");
    let ast = parse_with("is:owed-reply", &options).unwrap();
    assert_eq!(
        ast,
        QueryNode::Filter(FilterKind::Custom("owed-reply".into()))
    );
}

#[test]
fn custom_filter_canonicalises_underscore_and_case() {
    let mut options = ParserOptions::new();
    options.register_custom_filter("Owed-Reply");
    let ast = parse_with("is:owed_reply", &options).unwrap();
    assert_eq!(
        ast,
        QueryNode::Filter(FilterKind::Custom("owed-reply".into()))
    );
}

#[test]
fn custom_filter_works_in_has_namespace() {
    let mut options = ParserOptions::new();
    options.register_custom_filter("reaction");
    let ast = parse_with("has:reaction", &options).unwrap();
    assert_eq!(ast, QueryNode::Filter(FilterKind::Custom("reaction".into())));
}

#[test]
fn parse_has_attachment() {
    assert_eq!(
        parse("has:attachment").unwrap(),
        QueryNode::Filter(FilterKind::HasAttachment)
    );
}

#[test]
fn parse_has_calendar() {
    assert_eq!(
        parse("has:calendar").unwrap(),
        QueryNode::Filter(FilterKind::HasCalendar)
    );
}

#[test]
fn parse_label() {
    assert_eq!(parse("label:work").unwrap(), QueryNode::Label("work".into()));
}

#[test]
fn parse_date_after() {
    assert_eq!(
        parse("after:2026-01-01").unwrap(),
        QueryNode::DateRange {
            bound: DateBound::After,
            date: DateValue::Specific(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
        }
    );
}

#[test]
fn parse_date_before() {
    assert_eq!(
        parse("before:2026-03-15").unwrap(),
        QueryNode::DateRange {
            bound: DateBound::Before,
            date: DateValue::Specific(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()),
        }
    );
}

#[test]
fn parse_date_today() {
    assert_eq!(
        parse("date:today").unwrap(),
        QueryNode::DateRange {
            bound: DateBound::Exact,
            date: DateValue::Today,
        }
    );
}

#[test]
fn parse_older_relative_duration_returns_relative_not_specific() {
    // The behaviour change: older/older_than now produces Relative, not
    // a resolved NaiveDate. Backends resolve at execution time.
    assert_eq!(
        parse("older:30d").unwrap(),
        QueryNode::DateRange {
            bound: DateBound::Before,
            date: DateValue::Relative {
                amount: 30,
                unit: RelativeUnit::Day,
            },
        }
    );
}

#[test]
fn parse_newer_relative_duration_returns_relative_not_specific() {
    assert_eq!(
        parse("newer:2w").unwrap(),
        QueryNode::DateRange {
            bound: DateBound::After,
            date: DateValue::Relative {
                amount: 2,
                unit: RelativeUnit::Week,
            },
        }
    );
}

#[test]
fn reject_invalid_relative_duration_unit() {
    assert_eq!(
        parse("older:30q"),
        Err(ParseError::InvalidDate("30q".into()))
    );
}

#[test]
fn parse_size_query() {
    assert_eq!(
        parse("size:>5mb").unwrap(),
        QueryNode::Size {
            op: SizeOp::GreaterThan,
            bytes: 5 * 1024 * 1024,
        }
    );
    assert_eq!(
        parse("size:<=42kb").unwrap(),
        QueryNode::Size {
            op: SizeOp::LessThanOrEqual,
            bytes: 42 * 1024,
        }
    );
}

#[test]
fn reject_invalid_size_unit() {
    assert_eq!(
        parse("size:>5tb"),
        Err(ParseError::InvalidSize("tb".into()))
    );
}

#[test]
fn parse_implicit_and() {
    assert_eq!(
        parse("invoice unread").unwrap(),
        QueryNode::And(
            Box::new(QueryNode::Text("invoice".into())),
            Box::new(QueryNode::Text("unread".into())),
        )
    );
}

#[test]
fn parse_explicit_and() {
    assert_eq!(
        parse("invoice AND unread").unwrap(),
        QueryNode::And(
            Box::new(QueryNode::Text("invoice".into())),
            Box::new(QueryNode::Text("unread".into())),
        )
    );
}

#[test]
fn parse_or() {
    assert_eq!(
        parse("invoice OR receipt").unwrap(),
        QueryNode::Or(
            Box::new(QueryNode::Text("invoice".into())),
            Box::new(QueryNode::Text("receipt".into())),
        )
    );
}

#[test]
fn parse_not() {
    let minus = parse("-spam").unwrap();
    let keyword = parse("NOT spam").unwrap();
    let expected = QueryNode::Not(Box::new(QueryNode::Text("spam".into())));
    assert_eq!(minus, expected);
    assert_eq!(keyword, expected);
}

#[test]
fn parse_parentheses() {
    assert_eq!(
        parse("(from:alice OR from:bob) is:unread").unwrap(),
        QueryNode::And(
            Box::new(QueryNode::Or(
                Box::new(QueryNode::Field {
                    field: QueryField::From,
                    value: "alice".into(),
                }),
                Box::new(QueryNode::Field {
                    field: QueryField::From,
                    value: "bob".into(),
                }),
            )),
            Box::new(QueryNode::Filter(FilterKind::Unread)),
        )
    );
}

#[test]
fn parse_compound_left_associative() {
    assert_eq!(
        parse("from:alice subject:invoice is:unread after:2026-01-01").unwrap(),
        QueryNode::And(
            Box::new(QueryNode::And(
                Box::new(QueryNode::And(
                    Box::new(QueryNode::Field {
                        field: QueryField::From,
                        value: "alice".into(),
                    }),
                    Box::new(QueryNode::Field {
                        field: QueryField::Subject,
                        value: "invoice".into(),
                    }),
                )),
                Box::new(QueryNode::Filter(FilterKind::Unread)),
            )),
            Box::new(QueryNode::DateRange {
                bound: DateBound::After,
                date: DateValue::Specific(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            }),
        )
    );
}

#[test]
fn empty_input_errors() {
    assert_eq!(parse(""), Err(ParseError::UnexpectedEnd));
    assert_eq!(parse("   "), Err(ParseError::UnexpectedEnd));
}

#[test]
fn unmatched_paren_errors() {
    assert_eq!(parse("(from:alice"), Err(ParseError::UnmatchedParen));
}

#[test]
fn unmatched_brace_errors() {
    assert_eq!(parse("{foo bar"), Err(ParseError::UnmatchedBrace));
}

// ---------- Display round-trip ----------

#[test]
fn display_text() {
    let ast = parse("invoice").unwrap();
    assert_eq!(ast.to_string(), "invoice");
}

#[test]
fn display_phrase() {
    let ast = parse("\"deployment plan\"").unwrap();
    assert_eq!(ast.to_string(), "\"deployment plan\"");
}

#[test]
fn display_exact() {
    let ast = parse("+stripe").unwrap();
    assert_eq!(ast.to_string(), "+stripe");
}

#[test]
fn display_field_with_simple_value() {
    let ast = parse("from:alice").unwrap();
    assert_eq!(ast.to_string(), "from:alice");
}

#[test]
fn display_field_with_email_value_does_not_quote() {
    let ast = parse("from:alice@example.com").unwrap();
    assert_eq!(ast.to_string(), "from:alice@example.com");
}

#[test]
fn display_filter() {
    let ast = parse("is:unread").unwrap();
    assert_eq!(ast.to_string(), "is:unread");
}

#[test]
fn display_label() {
    let ast = parse("label:work").unwrap();
    assert_eq!(ast.to_string(), "label:work");
}

#[test]
fn display_date_specific() {
    let ast = parse("after:2026-01-01").unwrap();
    assert_eq!(ast.to_string(), "after:2026-01-01");
}

#[test]
fn display_date_today() {
    let ast = parse("date:today").unwrap();
    assert_eq!(ast.to_string(), "date:today");
}

#[test]
fn display_date_relative() {
    let ast = parse("older:30d").unwrap();
    assert_eq!(ast.to_string(), "older_than:30d");
}

#[test]
fn display_size() {
    let ast = parse("size:>5mb").unwrap();
    let bytes = 5_u64 * 1024 * 1024;
    assert_eq!(ast.to_string(), format!("size:>{bytes}"));
}

#[test]
fn display_and_parenthesises_nested_compound() {
    let ast = parse("from:alice (is:unread OR is:starred)").unwrap();
    let s = ast.to_string();
    // Re-parse to verify structural equality, not byte identity.
    let reparsed = parse(&s).unwrap();
    assert_eq!(ast, reparsed, "rendered: {s}");
}

#[test]
fn display_roundtrips_compound_query() {
    let inputs = [
        "from:alice subject:invoice is:unread after:2026-01-01",
        "-spam",
        "(from:alice OR from:bob) is:unread",
        "+stripe OR receipt",
        "newer:2w has:attachment",
    ];
    for input in inputs {
        let ast = parse(input).unwrap();
        let rendered = ast.to_string();
        let reparsed = parse(&rendered)
            .unwrap_or_else(|err| panic!("re-parse failed for `{rendered}` (from `{input}`): {err}"));
        assert_eq!(
            ast, reparsed,
            "round-trip diverged: original `{input}` rendered `{rendered}`",
        );
    }
}

// ---------- Visitor ----------

#[test]
fn visitor_counts_filters() {
    #[derive(Default)]
    struct CountFilters(usize);
    impl Visitor for CountFilters {
        fn visit_filter(&mut self, _: &FilterKind) {
            self.0 += 1;
        }
    }
    let ast = parse("from:alice is:unread OR has:attachment").unwrap();
    let mut visitor = CountFilters::default();
    visitor.walk(&ast);
    assert_eq!(visitor.0, 2);
}

#[test]
fn visitor_collects_fields() {
    #[derive(Default)]
    struct CollectFields(Vec<(QueryField, String)>);
    impl Visitor for CollectFields {
        fn visit_field(&mut self, field: QueryField, value: &str) {
            self.0.push((field, value.to_string()));
        }
    }
    let ast = parse("from:alice subject:invoice").unwrap();
    let mut visitor = CollectFields::default();
    visitor.walk(&ast);
    assert_eq!(
        visitor.0,
        vec![
            (QueryField::From, "alice".into()),
            (QueryField::Subject, "invoice".into()),
        ]
    );
}

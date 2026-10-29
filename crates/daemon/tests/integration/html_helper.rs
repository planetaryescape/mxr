//! Hostile HTML against the real helper: the `mxr` binary in `__html-worker`
//! mode, driven through the production call sites with the same timeout and
//! memory limits the daemon uses.
//!
//! The documents are the ones the F1 review rounds found, plus the cases the
//! reader's budget does not catch at all. A document may be rendered, or the
//! helper may be stopped and the caller gets its fallback; what must hold is
//! that the call returns promptly, the parent's memory stays small, the
//! visible text survives, and the next ordinary message renders.

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mxr_html_worker::{with_supervisor, Config, Supervisor};

/// Shorter than the daemon's 5 s so the whole table of cases stays quick; the
/// mechanism is the same.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// The visible words each hostile document carries, so a fallback can be
/// checked for keeping the message's text.
const MARKER: &str = "keepmewords";

fn helper() -> Arc<Supervisor> {
    let mut config = Config::new(
        env!("CARGO_BIN_EXE_mxr"),
        vec![mxr::html_helper::HELPER_ARG.into()],
    );
    config.request_timeout = REQUEST_TIMEOUT;
    config.startup_timeout = Duration::from_secs(30);
    Arc::new(Supervisor::new(config))
}

fn resident_bytes() -> u64 {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .expect("ps prints a number")
        * 1024
}

fn formatting_element_reconstruction() -> String {
    formatting_element_reconstruction_of(6_000)
}

/// `elements` distinct formatting elements, each rebuilt after each of
/// `elements` paragraph ends: about `elements` squared DOM nodes.
fn formatting_element_reconstruction_of(elements: usize) -> String {
    let mut html = format!("<p>{MARKER}");
    for n in 0..elements {
        write!(html, "<b id=\"f{n}\">").expect("write");
    }
    for _ in 0..elements {
        html.push_str("</p><p>x");
    }
    html
}

/// The scan ends the tag at the quoted `>`, so it sees one attribute while the
/// tokenizer sees a hundred thousand.
fn quoted_gt_attribute_bypass() -> String {
    let mut html = format!("<p>{MARKER}</p><div a=\">\"");
    for n in 0..100_000 {
        write!(html, " b{n}").expect("write");
    }
    html.push_str(">tail</div>");
    html
}

/// No whitespace between attributes, so the scan counts one token.
fn adjacent_quoted_attributes() -> String {
    let mut html = format!("<p>{MARKER}</p><div ");
    for n in 0..100_000 {
        write!(html, "a{n}=\"1\"").expect("write");
    }
    html.push_str(">tail</div>");
    html
}

fn foster_parented_breaks() -> String {
    format!("<p>{MARKER}</p><table>{}</table>", "<br>".repeat(19_000))
}

/// The span decodes to 600 after the byte scan has passed it.
fn entity_encoded_rowspan() -> String {
    let cells = "<td rowspan=\"6&#48;00\">x</td>".repeat(3_000);
    format!("<p>{MARKER}</p><table><tr>{cells}</tr></table>")
}

fn table_hidden_by_a_comment() -> String {
    let mut html = format!("<p>{MARKER}</p><table><tr>");
    for n in 0..9_000 {
        write!(html, "<td>c{n}</td>").expect("write");
    }
    html.push_str("</tr><!-- </table> -->");
    for n in 0..9_000 {
        write!(html, "<tr><td>r{n}</td></tr>").expect("write");
    }
    html.push_str("</table>");
    html
}

fn fifty_thousand_rowspans() -> String {
    format!(
        "<p>{MARKER}</p><table><tr>{}</tr></table>",
        "<td rowspan=100>x</td>".repeat(50_000)
    )
}

fn hundred_thousand_deep_divs() -> String {
    format!(
        "<p>{MARKER}</p>{}deep{}",
        "<div>".repeat(100_000),
        "</div>".repeat(100_000)
    )
}

/// What the helper should have had to do for a document.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// Past the byte scan and too expensive to parse: the helper must be
    /// stopped for it, by the memory ceiling or the timeout depending on the
    /// build profile. Only a document that needs several times the ceiling
    /// qualifies; one whose cost is mostly time (a hundred thousand attributes
    /// takes 1.7 s in a release build and minutes in a debug one) is `Either`.
    Stopped,
    /// The reader's budget refuses it inside the helper, so nothing is lost.
    Budgeted,
    /// Either: it may finish inside the timeout or be stopped, depending on
    /// how fast the machine is.
    Either,
}

fn assert_survives(name: &str, html: &str, expected: Outcome) {
    let helper = helper();
    let before = resident_bytes();
    let started = Instant::now();

    let text = with_supervisor(Arc::clone(&helper), || {
        mxr_reader::html_to_plain_text(html, "test")
    });
    let elapsed = started.elapsed();

    // One request timeout, one possible retry after a crash, and the startup of
    // a debug-build helper. Without isolation these inputs run for minutes or
    // exhaust memory.
    assert!(
        elapsed < REQUEST_TIMEOUT * 2 + Duration::from_secs(30),
        "{name}: took {elapsed:?}"
    );
    assert!(
        text.contains(MARKER),
        "{name}: the message's own text was lost: {:?}",
        text.chars().take(120).collect::<String>()
    );
    assert!(
        resident_bytes() < before + 200 * 1024 * 1024,
        "{name}: this process grew from {before} to {} bytes",
        resident_bytes()
    );

    // The helper that was stopped is replaced and the next message renders.
    let next = with_supervisor(Arc::clone(&helper), || {
        mxr_reader::html_to_plain_text("<p>second message renders</p>", "test")
    });
    assert_eq!(next.trim(), "second message renders", "{name}");
    let stats = helper.stats();
    assert_eq!(stats.job_errors, 0, "{name}: {stats:?}");
    let lost = stats.timeouts + stats.deaths;
    match expected {
        Outcome::Stopped => {
            assert!(
                lost >= 1,
                "{name}: expected the helper to be stopped: {stats:?}"
            );
            assert!(stats.fallbacks >= 1, "{name}: {stats:?}");
            assert!(
                stats.restarts >= 1,
                "{name}: the next request started a new helper: {stats:?}"
            );
        }
        Outcome::Budgeted => {
            assert_eq!(
                lost, 0,
                "{name}: the budget should refuse it cheaply: {stats:?}"
            );
            assert_eq!(stats.fallbacks, 0, "{name}: {stats:?}");
        }
        Outcome::Either => {}
    }
}

#[test]
fn formatting_element_reconstruction_is_contained() {
    assert_survives(
        "formatting reconstruction",
        &formatting_element_reconstruction(),
        Outcome::Stopped,
    );
}

#[test]
fn a_formatting_reconstruction_under_64_kib_is_contained_too() {
    // Small documents are not exempt: this one is under 64 KiB and passes the
    // reader's budget, yet rebuilds millions of elements. It is why every
    // parse goes to the helper and none is kept in-process for being small.
    let html = formatting_element_reconstruction_of(3_000);
    assert!(html.len() < 64 * 1024, "{} bytes", html.len());
    assert!(mxr_reader::within_budget(&html));
    assert_survives("3k formatting reconstruction", &html, Outcome::Either);
}

#[test]
fn a_quoted_gt_attribute_bypass_is_contained() {
    assert_survives(
        "quoted > bypass",
        &quoted_gt_attribute_bypass(),
        Outcome::Either,
    );
}

#[test]
fn adjacent_quoted_attributes_are_contained() {
    assert_survives(
        "adjacent quoted attributes",
        &adjacent_quoted_attributes(),
        Outcome::Either,
    );
}

#[test]
fn foster_parented_breaks_are_contained() {
    assert_survives(
        "foster-parented br",
        &foster_parented_breaks(),
        Outcome::Either,
    );
}

#[test]
fn an_entity_encoded_rowspan_is_contained() {
    assert_survives(
        "entity-encoded rowspan",
        &entity_encoded_rowspan(),
        Outcome::Either,
    );
}

#[test]
fn a_table_hidden_by_a_comment_is_contained() {
    assert_survives(
        "comment-hidden table",
        &table_hidden_by_a_comment(),
        Outcome::Either,
    );
}

#[test]
fn fifty_thousand_rowspans_are_contained() {
    assert_survives(
        "50k rowspans",
        &fifty_thousand_rowspans(),
        Outcome::Budgeted,
    );
}

#[test]
fn a_hundred_thousand_deep_div_nest_is_contained() {
    assert_survives(
        "100k deep divs",
        &hundred_thousand_deep_divs(),
        Outcome::Budgeted,
    );
}

#[test]
fn a_normal_email_renders_the_same_through_the_helper_as_in_process() {
    let html = "<html><body><h1>Invoice 42</h1><p>Hello <b>Sam</b>, your total is \
        <a href=\"https://example.com/pay\">$12.00</a>.</p><table><tr><td>Item</td>\
        <td>Qty</td></tr><tr><td>Widget</td><td>3</td></tr></table>\
        <ul><li>one</li><li>two</li></ul></body></html>";
    let in_process = mxr_reader::html_to_plain_text(html, "test");
    let helper = helper();
    let through_helper = with_supervisor(Arc::clone(&helper), || {
        mxr_reader::html_to_plain_text(html, "test")
    });
    assert_eq!(through_helper, in_process);
    assert!(in_process.contains("Invoice 42") && in_process.contains("Widget"));
    let stats = helper.stats();
    assert_eq!((stats.requests, stats.fallbacks), (1, 0), "{stats:?}");
}

#[test]
fn the_reader_pipeline_gives_the_same_content_through_the_helper() {
    let html = "<div>Reply text</div><div class=\"gmail_quote\">On Mon, Sam wrote:\
        <blockquote>old words</blockquote></div><div class=\"gmail_signature\">Sam</div>";
    let config = mxr_reader::ReaderConfig::default();
    let in_process = mxr_reader::clean(None, Some(html), &config);
    let through_helper = with_supervisor(helper(), || mxr_reader::clean(None, Some(html), &config));
    assert_eq!(through_helper.content, in_process.content);
    assert_eq!(
        through_helper.quoted_messages.len(),
        in_process.quoted_messages.len()
    );
    assert_eq!(through_helper.signature, in_process.signature);
    assert!(in_process.content.contains("Reply text"));
    assert!(!in_process.content.contains("old words"));
}

#[test]
fn a_poison_message_is_answered_from_memory_the_second_time() {
    let helper = helper();
    let html = formatting_element_reconstruction();
    let first = Instant::now();
    let one = with_supervisor(Arc::clone(&helper), || {
        mxr_reader::html_to_plain_text(&html, "test")
    });
    let first = first.elapsed();
    let before = helper.stats();
    let second = Instant::now();
    let two = with_supervisor(Arc::clone(&helper), || {
        mxr_reader::html_to_plain_text(&html, "test")
    });
    let second = second.elapsed();
    let after = helper.stats();
    assert_eq!(one, two);
    // The first call stopped a helper; the second must not spend another
    // timeout or kill another helper.
    assert!(before.timeouts + before.deaths >= 1, "{before:?}");
    assert_eq!(after.timeouts, before.timeouts);
    assert_eq!(after.deaths, before.deaths);
    assert_eq!(after.fallbacks, before.fallbacks + 1, "{after:?}");
    assert!(
        second < Duration::from_secs(2),
        "the remembered input skips the helper: second call took {second:?} after {first:?}"
    );
}

#[test]
fn parallel_reads_complete_while_a_hostile_message_is_stopped_in_the_batch() {
    let helper = helper();
    let hostile = Arc::new(formatting_element_reconstruction());
    let threads: Vec<_> = (0..6)
        .map(|thread| {
            let helper = Arc::clone(&helper);
            let hostile = Arc::clone(&hostile);
            std::thread::spawn(move || {
                with_supervisor(helper, || {
                    let mut rendered = 0;
                    for message in 0..10 {
                        let html = format!("<p>thread {thread} message {message}</p>");
                        if thread == 0 && message == 3 {
                            let text = mxr_reader::html_to_plain_text(&hostile, "test");
                            assert!(text.contains(MARKER));
                            continue;
                        }
                        let text = mxr_reader::html_to_plain_text(&html, "test");
                        assert_eq!(text.trim(), format!("thread {thread} message {message}"));
                        rendered += 1;
                    }
                    rendered
                })
            })
        })
        .collect();
    let rendered: usize = threads
        .into_iter()
        .map(|thread| thread.join().expect("reader thread"))
        .sum();
    assert_eq!(rendered, 59, "every ordinary message rendered");
    let stats = helper.stats();
    assert!(stats.timeouts + stats.deaths >= 1, "{stats:?}");
}

#[test]
fn every_isolated_parse_runs_in_the_helper_binary() {
    // One request per registered job, each answered by the real helper rather
    // than falling back, so a job missing from its registry is caught here.
    let helper = helper();
    let ok = |kind: &str, html: &str, params: &str| {
        let outcome = helper.run(kind, html.as_bytes(), params.as_bytes(), 2 * 1024 * 1024);
        assert!(
            matches!(outcome, mxr_html_worker::Outcome::Done(_)),
            "{kind}: {outcome:?}"
        );
    };
    ok("reader.render", "<p>x</p>", "null");
    ok(
        "reader.split_quote",
        "<div class=\"gmail_quote\">q</div>",
        "false",
    );
    ok("reading.walk_html", "<p>x</p>", "null");
    ok("reading.article", "<p>x</p>", "null");
    ok("reading.issue_reader_html", "<p>x</p>", "10");
    ok("safety.html_views", "<p title=a>x</p>", "null");
    ok("outbound.validate_html", "<p>x</p>", "null");
    ok("outbound.text_alternative", "<p>x</p>", "null");
    ok("tui.rich_lines", "<p>x</p>", "60");
    ok("daemon.cid_references", "<img src=\"cid:a\">", "null");
    ok("daemon.image_sources", "<img src=\"a.png\">", "null");
    assert_eq!(helper.stats().fallbacks, 0);
}

/// A newsletter-shaped message of about 7 KB: nested tables, linked images,
/// styled text and a footer, with text that differs per message.
fn newsletter(n: usize) -> String {
    let mut html = String::from(
        "<html><head><style>.a{color:#333}</style></head><body><table width=\"600\" \
         align=\"center\"><tr><td><img src=\"https://cdn.example.com/logo.png\" alt=\"Logo\"></td></tr>",
    );
    for section in 0..12 {
        write!(
            html,
            "<tr><td style=\"padding:12px\"><h2>Issue {n} section {section}</h2>\
             <p class=\"a\">The {n}th edition covers <b>topic {section}</b> in some depth, with \
             <a href=\"https://example.com/{n}/{section}?utm_source=mail\">a link to read more</a> \
             and a few sentences of ordinary prose so the converter has words to wrap across \
             several lines of the reader's eighty columns.</p>\
             <table><tr><td>Item</td><td>Count</td></tr><tr><td>alpha {section}</td><td>{n}</td></tr></table>\
             <ul><li>first point {n}</li><li>second point {section}</li><li>third point</li></ul>\
             <img src=\"https://cdn.example.com/{n}/{section}.png\" alt=\"figure {section}\"></td></tr>"
        )
        .expect("write");
    }
    html.push_str(
        "<tr><td><p>You are receiving this because you subscribed.</p>\
         <a href=\"https://example.com/unsub\">Unsubscribe</a></td></tr></table></body></html>",
    );
    html
}

/// Measures what the helper adds per message when the search index converts a
/// newsletter. Run on a release build:
/// `cargo test --release -p mxr --test integration html_helper::overhead -- --ignored --nocapture`
#[test]
#[ignore = "a measurement, not a check: run it by hand on a release build"]
fn overhead_on_ten_thousand_newsletters() {
    const MESSAGES: usize = 10_000;
    let messages: Vec<String> = (0..MESSAGES).map(newsletter).collect();
    let average_bytes = messages.iter().map(String::len).sum::<usize>() / MESSAGES;

    let started = Instant::now();
    let in_process: Vec<String> = messages
        .iter()
        .map(|html| mxr_reader::searchable_text(None, Some(html)).text)
        .collect();
    let in_process_time = started.elapsed();

    let helper = helper();
    // Start the helper outside the measurement; its start-up is reported apart.
    let startup = Instant::now();
    let _ = with_supervisor(Arc::clone(&helper), || {
        mxr_reader::searchable_text(None, Some("<p>x</p>"))
    });
    let startup = startup.elapsed();

    let started = Instant::now();
    let through_helper: Vec<String> = with_supervisor(Arc::clone(&helper), || {
        messages
            .iter()
            .map(|html| mxr_reader::searchable_text(None, Some(html)).text)
            .collect()
    });
    let helper_time = started.elapsed();

    assert_eq!(
        in_process, through_helper,
        "the helper changed what the index sees"
    );
    let stats = helper.stats();
    assert_eq!(stats.fallbacks, 0, "{stats:?}");
    let per_message = |total: Duration| total.as_secs_f64() * 1e6 / MESSAGES as f64;
    println!(
        "{MESSAGES} newsletters, {average_bytes} bytes each\n  in-process: {:.0} us/message ({in_process_time:?} total)\n  helper:     {:.0} us/message ({helper_time:?} total)\n  overhead:   {:.0} us/message\n  helper start-up: {startup:?}",
        per_message(in_process_time),
        per_message(helper_time),
        per_message(helper_time) - per_message(in_process_time),
    );
}

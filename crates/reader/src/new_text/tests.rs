use super::*;

#[test]
fn authored_decision_text_excludes_outlook_and_forwarded_asks_but_keeps_current_requests() {
    let outlook = "From: Alex <alex@example.org>\nSent: Monday\nTo: Me <me@example.org>\nSubject: Old request\n\nCould you confirm the details?";
    assert_eq!(
        authored_text(&format!("FYI, this was handled.\n\n{outlook}"), &[]),
        "FYI, this was handled."
    );
    assert_eq!(
        authored_text(
            &format!("Could you confirm the new date?\n\n{outlook}"),
            &[]
        ),
        "Could you confirm the new date?"
    );
    assert_eq!(
        authored_text(
            &format!("---------- Forwarded message ---------\n{outlook}"),
            &[]
        ),
        ""
    );
    assert_eq!(
        authored_text(
            "> Could you confirm the old date?\n\nThanks, this is handled.",
            &[]
        ),
        "Thanks, this is handled."
    );
}

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/quotes/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).expect("quote fixture should be readable")
}

fn earlier(text: &str) -> EarlierMessage<'_> {
    EarlierMessage {
        text,
        same_author: false,
    }
}

#[test]
fn outlook_html_reply_loses_everything_from_the_header_on() {
    let html = fixture("outlook_reply.html");
    let out = new_text(None, Some(&html), &[]);
    assert_eq!(out.text, "Approved. Please book it on the team card.");
    assert!(out.trimmed.quote);
    assert!(out.trimmed.signature);
}

#[test]
fn an_unmarked_quote_is_found_in_the_earlier_message() {
    let text = fixture("plain_unprefixed_quote.txt");
    let before = fixture("earlier_rollout.txt");
    let alone = new_text(Some(&text), None, &[]);
    assert!(alone.text.contains("Decision: keep the canary"));
    assert!(!alone.trimmed.quote);

    let out = new_text(Some(&text), None, &[earlier(&before)]);
    assert_eq!(
        out.text,
        "Agreed on the rollout plan. I'll watch support tickets for the first hour.\n\nSamir"
    );
    assert!(out.trimmed.quote);
}

#[test]
fn new_text_that_merely_mentions_the_thread_stays() {
    let before = fixture("earlier_flights.txt");
    let text = "Booked. The 07:40 lands at 11:05, so we have time for lunch.\n\nRuth";
    let out = new_text(Some(text), None, &[earlier(&before)]);
    assert_eq!(out.text, text);
    assert!(!out.trimmed.any());
}

#[test]
fn prefixed_quotes_and_their_header_go() {
    let text = "Sounds good.\n\nOn Tue, 29 Sep 2026, Alex wrote:\n> Shall we meet at 3?\n> Or 4?";
    let out = new_text(Some(text), None, &[]);
    assert_eq!(out.text, "Sounds good.");
    assert!(out.trimmed.quote);
}

#[test]
fn an_inline_reply_keeps_its_answers() {
    let text = "> Can you make Thursday?\nYes, the morning.\n> And bring the lease?\nWill do.";
    let out = new_text(Some(text), None, &[]);
    assert_eq!(out.text, text);
    assert!(!out.trimmed.quote);
    assert!(out.has_preserved_context);
}

#[test]
fn a_quote_only_message_says_so_instead_of_passing_the_quote_off_as_new() {
    let text = "On Tue, Alice wrote:\n> Can we move the launch to Friday?\n> Thanks, Alice";
    let out = new_text(Some(text), None, &[]);
    assert_eq!(out.text, ONLY_QUOTED_TEXT);
    assert!(out.only_quoted);
    assert!(out.trimmed.quote);
    assert!(!out.text.contains("Friday"));
}

#[test]
fn a_bare_forward_keeps_its_headers_and_content() {
    let text = "---------- Forwarded message ---------\nFrom: Bank\nDate: Mon\nSubject: Statement\nTo: Alex\n\nYour statement is ready.";
    let out = new_text(Some(text), None, &[]);
    assert_eq!(out.text, text);
    assert!(!out.only_quoted && !out.trimmed.quote);
    assert!(out.has_preserved_context);
}

#[test]
fn forwarded_content_is_not_removed_when_it_matches_an_earlier_message() {
    let prior = "Your annual account statement is now ready for review.\nPlease inspect all charges and contact us before Friday.";
    let text = format!("Please review this.\n\n---------- Forwarded message ---------\nFrom: Bank\nDate: Monday\nSubject: Statement\nTo: Alex\n\n{prior}");
    let out = new_text(Some(&text), None, &[earlier(prior)]);
    for context in [
        "Date: Monday",
        "Subject: Statement",
        "Your annual account statement",
        "before Friday",
    ] {
        assert!(
            out.text.contains(context),
            "missing {context}: {}",
            out.text
        );
    }
    assert!(out.has_preserved_context);
    assert!(!out.trimmed.quote);
}

#[test]
fn the_authors_repeated_signature_block_is_a_signature() {
    let before = "Here is the first draft.\n\nSamir Patel\nHead of Launch, Launchpad\nsamir@launchpad.example";
    let text = "Second draft attached, with your notes in.\n\nSamir Patel\nHead of Launch, Launchpad\nsamir@launchpad.example";
    let out = new_text(
        Some(text),
        None,
        &[EarlierMessage {
            text: before,
            same_author: true,
        }],
    );
    assert_eq!(out.text, "Second draft attached, with your notes in.");
    assert!(out.trimmed.signature);
}

#[test]
fn a_short_sign_off_is_not_a_signature() {
    let before = "First draft attached.\n\nThanks,\nSamir";
    let text = "Second draft attached.\n\nThanks,\nSamir";
    let out = new_text(
        Some(text),
        None,
        &[EarlierMessage {
            text: before,
            same_author: true,
        }],
    );
    assert_eq!(out.text, text);
    assert!(!out.trimmed.signature);
}

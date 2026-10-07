use super::*;

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
fn gmail_html_reply_loses_its_quote_and_signature() {
    let html = fixture("gmail_reply.html");
    let out = new_text(None, Some(&html), &[]);
    assert_eq!(
        out.text,
        "Thursday morning works. I'll leave the side gate open for the engineer.\n\nCan you send me the signed copy by Friday?"
    );
    assert_eq!(
        out.trimmed,
        Trimmed {
            quote: true,
            signature: true
        }
    );
}

#[test]
fn apple_mail_reply_loses_the_cite_blockquotes() {
    let html = fixture("apple_reply.html");
    let out = new_text(None, Some(&html), &[]);
    assert_eq!(out.text, "Yes, go ahead with the 5% canary.");
    assert!(out.trimmed.quote);
    assert!(!out.trimmed.signature);
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
fn outlook_plain_text_header_block_is_a_boundary() {
    let text = fixture("outlook_reply_unmarked.txt");
    let out = new_text(Some(&text), None, &[]);
    assert!(out
        .text
        .starts_with("Approved. Please book it on the team card."));
    assert!(!out.text.contains("07:40"));
    assert!(out.trimmed.quote);
}

#[test]
fn an_unmarked_quote_is_found_in_the_earlier_message() {
    let text = fixture("plain_unprefixed_quote.txt");
    let before = fixture("earlier_rollout.txt");
    // Without the thread, nothing marks the quote.
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
    assert_eq!(out.text, "Yes, the morning.\nWill do.");
    assert!(out.trimmed.quote);
}

#[test]
fn a_bare_forward_keeps_its_content() {
    let text = "---------- Forwarded message ---------\nFrom: Bank\nDate: Mon\nSubject: Statement\nTo: Alex\n\nYour statement is ready.";
    let out = new_text(Some(text), None, &[]);
    assert!(out.text.contains("Your statement is ready."));
    assert!(!out.trimmed.any());
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

#[test]
fn a_mobile_footer_is_a_signature() {
    let out = new_text(Some("On my way.\n\nSent from my iPhone"), None, &[]);
    assert_eq!(out.text, "On my way.");
    assert!(out.trimmed.signature);
}

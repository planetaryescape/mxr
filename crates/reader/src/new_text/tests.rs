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

#[test]
fn authored_decision_text_uses_cached_context_for_unmarked_old_requests() {
    let prior = "Could you confirm the launch date by Monday and send the signed copy?\nPlease also share the final reference with everyone before the Friday review meeting.";
    let context = [EarlierMessage {
        text: prior,
        same_author: false,
    }];
    assert_eq!(
        authored_text(&format!("FYI, this was handled.\n\n{prior}"), &context),
        "FYI, this was handled."
    );
    assert_eq!(
        authored_text(
            &format!("Could you confirm the new date?\n\n{prior}"),
            &context
        ),
        "Could you confirm the new date?"
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
            signature: true,
            footer: false,
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
    assert_eq!(out.text, text);
    assert!(!out.trimmed.quote);
    assert!(out.has_preserved_context);
}

#[test]
fn nested_quote_levels_remain_visible_beside_the_reply() {
    let text = ">> Original question?\n> Earlier answer.\nCurrent answer.";
    let out = new_text(Some(text), None, &[]);
    assert_eq!(out.text, text);
    assert!(out.has_preserved_context);
    assert!(!out.trimmed.quote);
}

#[test]
fn a_quote_only_message_says_so_instead_of_passing_the_quote_off_as_new() {
    // Alice's words must never show as the current sender's.
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
fn inline_answers_keep_conditions_negation_and_revised_dates_with_the_questions() {
    let text = "> Can you ship Thursday?\nYes, but only if QA passes.\n> Can we skip the migration?\nNo, please do not skip it. The revised date is Friday.";
    let out = new_text(Some(text), None, &[]);
    assert_eq!(out.text, text);
    assert!(!out.trimmed.any());
}

#[test]
fn a_trailing_quote_still_folds_after_an_authored_reply() {
    let previous = "Could you send the signed lease? It needs to arrive before Thursday.";
    let text = "I will send it today.\n\nOn Mon, Alex wrote:\n> Could you send the signed lease? It needs to arrive before Thursday.";
    let out = new_text(Some(text), None, &[earlier(previous)]);
    assert_eq!(out.text, "I will send it today.");
    assert!(out.trimmed.quote);
    assert!(!out.has_preserved_context);
}

#[test]
fn repeated_history_after_an_inline_answer_still_folds() {
    let text = "> Can you make Thursday?\nYes, the morning.\n> And bring the lease?\nWill do.\n> Can you make Thursday?\n> And bring the lease?";
    let out = new_text(Some(text), None, &[]);
    assert_eq!(
        out.text,
        "> Can you make Thursday?\nYes, the morning.\n> And bring the lease?\nWill do."
    );
    assert!(out.trimmed.quote);
    assert!(out.has_preserved_context);
}

#[test]
fn inline_context_does_not_depend_on_prior_message_order() {
    let first = "Can you confirm the Thursday delivery?";
    let second = "Do not skip the inspection.";
    let text = "> Can you confirm the Thursday delivery?\nYes, if the inspection passes.\n> Do not skip the inspection.\nAgreed.";
    let forward = new_text(Some(text), None, &[earlier(first), earlier(second)]);
    let reverse = new_text(Some(text), None, &[earlier(second), earlier(first)]);
    assert_eq!(forward, reverse);
    assert_eq!(forward.text, text);
    assert!(forward.has_preserved_context);
}

#[test]
fn an_inline_html_citation_stays_beside_its_answer() {
    let html = "<html><body><blockquote type=\"cite\"><p>Can you make Thursday?</p></blockquote><p>Yes, the morning.</p><blockquote type=\"cite\"><p>And bring the lease?</p></blockquote><p>Will do.</p></body></html>";
    let out = new_text(None, Some(html), &[]);
    assert!(out.text.contains("Can you make Thursday?"), "{}", out.text);
    assert!(out.text.contains("Yes, the morning."), "{}", out.text);
    assert!(out.text.contains("And bring the lease?"), "{}", out.text);
    assert!(out.text.contains("Will do."), "{}", out.text);
    assert!(!out.trimmed.quote);
    assert!(out.has_preserved_context);
}

#[test]
fn a_gmail_inline_answer_keeps_its_attribution_through_plain_text_cleanup() {
    let html = "<html><body><div class='gmail_quote'><div class='gmail_attr'>On Tue, Alice wrote:</div><div>Can you make Thursday?</div></div><p>Yes, the morning.</p></body></html>";
    let out = new_text(None, Some(html), &[]);
    for context in ["Alice wrote", "Can you make Thursday?", "Yes, the morning."] {
        assert!(
            out.text.contains(context),
            "missing {context}: {}",
            out.text
        );
    }
    assert!(out.has_preserved_context);
    assert!(!out.only_quoted);
}

#[test]
fn a_forward_intro_stays_attributed_with_the_forwarded_content() {
    let text = "This is relevant to the lease.\n\n---------- Forwarded message ---------\nFrom: Bank\nDate: Mon\nSubject: Statement\nTo: Alex\n\nYour statement is ready.";
    let out = new_text(Some(text), None, &[]);
    assert!(out.text.starts_with("This is relevant to the lease."));
    assert!(out.text.contains("From: Bank"));
    assert!(out.text.contains("Your statement is ready."));
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
fn a_plain_inline_quote_keeps_its_named_attribution() {
    let text = "On Tue, Alice wrote:\n> Can you make Thursday?\nYes, the morning.";
    let out = new_text(Some(text), None, &[]);
    assert!(out.text.contains("Alice wrote"), "{}", out.text);
    assert!(out.text.contains("Can you make Thursday?"), "{}", out.text);
    assert!(out.text.contains("Yes, the morning."), "{}", out.text);
    assert!(out.has_preserved_context);
}

#[test]
fn an_html_forward_after_an_intro_keeps_its_attribution_and_body() {
    let html = "<html><body><p>This is relevant to the lease.</p><div class=\"gmail_quote\"><div>---------- Forwarded message ---------</div><div><b>From:</b> Bank</div><div><b>Date:</b> Mon</div><div><b>Subject:</b> Statement</div><div>Your statement is ready.</div></div></body></html>";
    let out = new_text(None, Some(html), &[]);
    assert!(
        out.text.contains("This is relevant to the lease."),
        "{}",
        out.text
    );
    assert!(out.text.contains("From:"), "{}", out.text);
    assert!(out.text.contains("Bank"), "{}", out.text);
    assert!(out.text.contains("Date:"), "{}", out.text);
    assert!(out.text.contains("Subject:"), "{}", out.text);
    assert!(
        out.text.contains("Your statement is ready."),
        "{}",
        out.text
    );
    assert!(out.has_preserved_context);
}

#[test]
fn a_short_unquoted_note_stays_clean() {
    let text = "Hi Alex,\n\nThanks for the update.\n\nBest,\nSam";
    let out = new_text(Some(text), None, &[]);
    assert_eq!(out.text, text);
    assert!(!out.trimmed.any());
}

#[test]
fn an_empty_body_is_not_called_quoted() {
    let out = new_text(Some("   "), None, &[]);
    assert_eq!(out.text, "");
    assert!(!out.only_quoted);
    assert!(!out.trimmed.any());
}

#[test]
fn removing_a_footer_or_disclaimer_marks_it_trimmed() {
    let text = "Invoice approved.\n\nYou are receiving this email because you signed up.\nClick here to unsubscribe";
    let out = new_text(Some(text), None, &[]);
    assert_eq!(out.text, "Invoice approved.");
    assert!(out.trimmed.footer);
    assert!(out.trimmed.any());
    let legal = "Approved.\n\nThis email is confidential and intended only for the recipient.";
    let out = new_text(Some(legal), None, &[]);
    assert_eq!(out.text, "Approved.");
    assert!(out.trimmed.footer);
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

#[test]
fn text_with_nothing_removed_keeps_its_own_spacing() {
    let text = "Line one   \n\n\n\nLine two\n  indented";
    let out = new_text(Some(text), None, &[]);
    assert!(!out.trimmed.any());
    assert_eq!(out.text, text);
}

//! Property test: write → read → equality, modulo allowed normalisation.
//!
//! For mboxrd and mboxcl variants, the write/read pair should round-trip
//! the body exactly (after CRLF normalisation). For mboxo and mboxcl2
//! we allow body equality since those variants have ambiguous body
//! semantics or no escape signaling.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

use mailbox_formats::{MboxReader, MboxVariant, MboxWriter, RawMessage};
use proptest::prelude::*;

fn arb_header_value() -> impl Strategy<Value = Vec<u8>> {
    // Printable ASCII, no CR/LF (header folding gets tricky).
    proptest::collection::vec(32u8..127u8, 0..64)
}

fn arb_header_name() -> impl Strategy<Value = String> {
    // RFC 5322 token: ASCII letters/digits/-.
    "[A-Za-z][A-Za-z0-9-]{0,15}".prop_map(|s| s.to_string())
}

/// Body bytes with no embedded CR (we generate clean LF bodies). The
/// writer normalises to CRLF, the reader unescapes back.
fn arb_body_line() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(0x20u8..0x7eu8, 0..40)
}

fn arb_message() -> impl Strategy<Value = RawMessage> {
    (
        proptest::collection::vec((arb_header_name(), arb_header_value()), 1..6),
        proptest::collection::vec(arb_body_line(), 0..8),
        // Avoid the very first second of the epoch to dodge edge cases
        // in the asctime formatter.
        1_700_000_000u64..1_800_000_000u64,
    )
        .prop_map(|(headers, body_lines, secs)| {
            let mut body: Vec<u8> = Vec::new();
            for (i, line) in body_lines.iter().enumerate() {
                if i > 0 {
                    body.extend_from_slice(b"\r\n");
                }
                body.extend_from_slice(line);
            }
            RawMessage::new(headers, body)
                .with_envelope_from("user@example.com")
                .with_timestamp(UNIX_EPOCH + std::time::Duration::from_secs(secs))
        })
}

fn run_roundtrip(variant: MboxVariant, msg: RawMessage) -> RawMessage {
    let mut buf = Vec::new();
    let mut w = MboxWriter::new(&mut buf, variant);
    w.write_message(&msg).unwrap();
    w.finish().unwrap();

    let mut r = MboxReader::new(Cursor::new(buf), variant);
    r.next().expect("at least one message").unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    })]

    #[test]
    fn mboxrd_roundtrip_preserves_headers(msg in arb_message()) {
        let parsed = run_roundtrip(MboxVariant::Mboxrd, msg.clone());
        prop_assert_eq!(parsed.headers, msg.headers);
        prop_assert_eq!(parsed.envelope_from.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn mboxcl_roundtrip_preserves_body_exactly(msg in arb_message()) {
        // Mboxcl framing is byte-exact via Content-Length; the body
        // round-trips losslessly even if it contains literal "From "
        // lines.
        let parsed = run_roundtrip(MboxVariant::Mboxcl, msg.clone());
        prop_assert_eq!(parsed.body, msg.body);
    }
}

#[test]
fn mboxrd_escapes_then_unescapes_from_line_in_body() {
    // Body in canonical CRLF-terminated form (matches the writer's
    // output shape).
    let msg = RawMessage::new(
        vec![("Subject".to_string(), b"t".to_vec())],
        b"From the depths\r\nNormal line\r\n".to_vec(),
    )
    .with_envelope_from("a@example.com")
    .with_timestamp(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000));
    let parsed = run_roundtrip(MboxVariant::Mboxrd, msg.clone());
    assert_eq!(parsed.body, msg.body);
}

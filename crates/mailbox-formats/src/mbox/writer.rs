//! Append-mode mbox writer.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::Result;
use crate::raw_message::RawMessage;

use super::variant::{escape_body_line, uses_content_length, writes_escape};
use super::MboxVariant;

/// Writes `RawMessage` values to an underlying `Write` in the
/// requested mbox variant.
///
/// Each call to [`write_message`][Self::write_message] emits a complete
/// message: `From ` separator line, headers, blank line, body, trailing
/// blank line. Output uses CRLF line endings (`\r\n`).
///
/// For `Mboxcl` / `Mboxcl2` the writer injects a `Content-Length:`
/// header derived from the body's byte length. If the caller already
/// supplied a `Content-Length`, the existing value is replaced (mbox
/// `Content-Length` semantics are file-local; we keep them
/// authoritative for the on-disk shape).
pub struct MboxWriter<W: Write> {
    writer: W,
    variant: MboxVariant,
}

impl<W: Write> MboxWriter<W> {
    /// Construct a writer. Use `MboxVariant::Mboxrd` if you have no
    /// strong reason to pick another; it's the safest interchange
    /// format. `MboxVariant::Auto` falls back to `Mboxrd` for writing.
    pub fn new(writer: W, variant: MboxVariant) -> Self {
        let variant = resolve_write_variant(variant);
        Self { writer, variant }
    }

    /// Append one message.
    pub fn write_message(&mut self, msg: &RawMessage) -> Result<()> {
        let from = msg.envelope_from.as_deref().unwrap_or("unknown@invalid");
        let date = asctime(msg.timestamp);
        write!(self.writer, "From {from} {date}\r\n")?;

        for (name, value) in &msg.headers {
            if uses_content_length(self.variant) && name.eq_ignore_ascii_case("content-length") {
                // We'll emit our own. Skip the caller's.
                continue;
            }
            self.writer.write_all(name.as_bytes())?;
            self.writer.write_all(b": ")?;
            self.writer.write_all(value)?;
            self.writer.write_all(b"\r\n")?;
        }

        if uses_content_length(self.variant) {
            write!(self.writer, "Content-Length: {}\r\n", msg.body.len())?;
        }

        self.writer.write_all(b"\r\n")?;

        if writes_escape(self.variant) {
            for line in split_lines(&msg.body) {
                let escaped = escape_body_line(line, self.variant);
                self.writer.write_all(escaped.as_ref())?;
                self.writer.write_all(b"\r\n")?;
            }
        } else {
            // mboxo / mboxcl / mboxcl2 — body written verbatim, just
            // line by line so we get consistent CRLF endings.
            for line in split_lines(&msg.body) {
                self.writer.write_all(line)?;
                self.writer.write_all(b"\r\n")?;
            }
        }

        // Trailing blank line acts as the inter-message separator for
        // Mboxo/Mboxrd. For Mboxcl variants it's still conventional.
        self.writer.write_all(b"\r\n")?;
        Ok(())
    }

    /// Flush and return the underlying writer.
    pub fn finish(mut self) -> Result<W> {
        self.writer.flush()?;
        Ok(self.writer)
    }
}

/// `Auto` falls back to `Mboxrd` for writing (most-current, safest
/// interchange).
fn resolve_write_variant(variant: MboxVariant) -> MboxVariant {
    match variant {
        MboxVariant::Auto => MboxVariant::Mboxrd,
        v => v,
    }
}

/// Split a byte buffer into lines, handling both LF and CRLF input.
/// The trailing line is included even if there is no trailing newline.
fn split_lines(body: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < body.len() {
        if body[i] == b'\n' {
            let mut end = i;
            if end > start && body[end - 1] == b'\r' {
                end -= 1;
            }
            out.push(&body[start..end]);
            start = i + 1;
        }
        i += 1;
    }
    if start < body.len() {
        out.push(&body[start..]);
    }
    out
}

/// `asctime(3)`-style date for the `From ` envelope line. Example:
/// `Tue Mar 17 09:30:00 2026`.
fn asctime(t: SystemTime) -> String {
    // Compute UTC components from epoch seconds. We deliberately don't
    // pull `chrono` in for this — a few division steps are simpler and
    // dep-light. The `From ` line in mbox is conventionally local time
    // but in practice readers tolerate UTC.
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let (year, month, day, hour, minute, second, weekday) = decompose(secs);
    let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{} {} {:>2} {:02}:{:02}:{:02} {}",
        day_names[weekday as usize],
        month_names[(month - 1) as usize],
        day,
        hour,
        minute,
        second,
        year
    )
}

/// Decompose epoch seconds into UTC `(year, month, day, hour, minute,
/// second, weekday)` where `weekday` is `0=Sunday`. Algorithm: Howard
/// Hinnant's date library, simplified.
fn decompose(secs: i64) -> (i32, u32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86400);
    let time_of_day = secs.rem_euclid(86400) as u32;

    // Compute civil date from days since epoch (1970-01-01).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    // Weekday: epoch was Thursday => 4. Days since 1970-01-01 mod 7,
    // shifted so Sunday=0.
    let weekday = (days.rem_euclid(7) + 4).rem_euclid(7) as u32;

    let hour = time_of_day / 3600;
    let minute = (time_of_day / 60) % 60;
    let second = time_of_day % 60;
    (year, month, day, hour, minute, second, weekday)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use std::time::Duration;

    fn msg(body: &str) -> RawMessage {
        RawMessage {
            headers: vec![
                ("From".to_string(), b"Alice <alice@example.com>".to_vec()),
                ("Subject".to_string(), b"hi".to_vec()),
            ],
            body: body.as_bytes().to_vec(),
            envelope_from: Some("alice@example.com".to_string()),
            timestamp: UNIX_EPOCH + Duration::from_secs(1742204200),
            flags: crate::Flags::empty(),
        }
    }

    #[test]
    fn mboxrd_writes_from_line_and_headers() {
        let mut w = MboxWriter::new(Vec::<u8>::new(), MboxVariant::Mboxrd);
        w.write_message(&msg("Hello.")).unwrap();
        let out = String::from_utf8(w.finish().unwrap()).unwrap();
        assert!(out.starts_with("From alice@example.com "));
        assert!(out.contains("\r\nSubject: hi\r\n"));
        assert!(out.contains("\r\n\r\nHello.\r\n\r\n"));
    }

    #[test]
    fn mboxrd_escapes_from_in_body() {
        let mut w = MboxWriter::new(Vec::<u8>::new(), MboxVariant::Mboxrd);
        w.write_message(&msg("From the depths\nNormal line"))
            .unwrap();
        let out = String::from_utf8(w.finish().unwrap()).unwrap();
        assert!(out.contains("\r\n>From the depths\r\n"));
        assert!(out.contains("\r\nNormal line\r\n"));
    }

    #[test]
    fn mboxo_does_not_escape() {
        let mut w = MboxWriter::new(Vec::<u8>::new(), MboxVariant::Mboxo);
        w.write_message(&msg("From the depths")).unwrap();
        let out = String::from_utf8(w.finish().unwrap()).unwrap();
        // Note that this is ambiguous when read back — exactly why
        // Mboxo is deprecated. We faithfully emit anyway.
        assert!(out.contains("\r\nFrom the depths\r\n"));
        assert!(!out.contains(">From the depths"));
    }

    #[test]
    fn mboxcl_emits_content_length() {
        let body = "five!";
        let mut w = MboxWriter::new(Vec::<u8>::new(), MboxVariant::Mboxcl);
        w.write_message(&msg(body)).unwrap();
        let out = String::from_utf8(w.finish().unwrap()).unwrap();
        assert!(out.contains("\r\nContent-Length: 5\r\n"));
    }

    #[test]
    fn mboxcl_replaces_caller_content_length() {
        // Caller passed a wrong Content-Length; we override.
        let mut m = msg("hi");
        m.headers
            .push(("Content-Length".to_string(), b"999".to_vec()));
        let mut w = MboxWriter::new(Vec::<u8>::new(), MboxVariant::Mboxcl);
        w.write_message(&m).unwrap();
        let out = String::from_utf8(w.finish().unwrap()).unwrap();
        assert!(out.contains("\r\nContent-Length: 2\r\n"));
        assert!(!out.contains("Content-Length: 999"));
    }

    #[test]
    fn writer_uses_crlf_endings() {
        let mut w = MboxWriter::new(Vec::<u8>::new(), MboxVariant::Mboxrd);
        w.write_message(&msg("body\nmore body")).unwrap();
        let out = w.finish().unwrap();
        // No bare LF except as part of CRLF.
        let mut prev = b'?';
        for &c in &out {
            if c == b'\n' {
                assert_eq!(prev, b'\r', "found bare LF in output");
            }
            prev = c;
        }
    }

    #[test]
    fn from_line_has_asctime_date() {
        let mut w = MboxWriter::new(Vec::<u8>::new(), MboxVariant::Mboxrd);
        w.write_message(&msg("hi")).unwrap();
        let out = String::from_utf8(w.finish().unwrap()).unwrap();
        // 1742204200 seconds since UNIX_EPOCH = Mon Mar 17 09:36:40 2025 UTC.
        let first = out.lines().next().unwrap();
        assert!(first.contains("Mon Mar 17"), "first line was: {first}");
        assert!(first.contains("09:36:40 2025"), "first line was: {first}");
    }
}

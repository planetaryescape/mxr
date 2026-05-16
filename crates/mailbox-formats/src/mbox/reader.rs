//! Streaming mbox reader.
//!
//! The reader is `BufRead`-based with bounded memory: a 10GB mbox
//! streams without loading into memory. Each call to
//! [`Iterator::next`] reads exactly one message.

use std::io::BufRead;
use std::time::SystemTime;

use crate::error::{Error, Result};
use crate::raw_message::{Flags, RawMessage};

use super::variant::{sniff_variant, unescape_body_line, uses_content_length};
use super::MboxVariant;

const SNIFF_BYTES: usize = 64 * 1024;

/// Streaming reader. Yields one [`RawMessage`] per call to `next`.
///
/// Construction with `MboxVariant::Auto` does *not* sniff eagerly; the
/// first call to `next` does the sniff using a buffered prefix of the
/// stream.
pub struct MboxReader<R: BufRead> {
    inner: R,
    variant: MboxVariant,
    /// `From ` line of the *next* message, captured by the previous
    /// `next` call. We read the separator before yielding the previous
    /// message so we know where the body ends.
    pending_from_line: Option<Vec<u8>>,
    /// Sniff prefix held back from `inner` so the actual reader sees
    /// the same bytes that informed the sniff decision.
    sniff_buffer: Option<Vec<u8>>,
    eof: bool,
}

impl<R: BufRead> MboxReader<R> {
    /// Create a new reader. Pass `MboxVariant::Mboxrd` if you know the
    /// stream is mboxrd; `Mboxo` for older `From `-only files; `Mboxcl`
    /// or `Mboxcl2` if you've parsed a `Content-Length:` header in the
    /// past; `Auto` to sniff.
    pub fn new(inner: R, variant: MboxVariant) -> Self {
        Self {
            inner,
            variant,
            pending_from_line: None,
            sniff_buffer: None,
            eof: false,
        }
    }

    /// Force variant detection now. Useful when the caller wants to
    /// know the variant up-front before iterating.
    pub fn detect_variant(&mut self) -> Result<MboxVariant> {
        if self.variant != MboxVariant::Auto {
            return Ok(self.variant);
        }
        let mut prefix = Vec::with_capacity(SNIFF_BYTES);
        let mut chunk = [0u8; 4096];
        while prefix.len() < SNIFF_BYTES {
            let read = read_some(&mut self.inner, &mut chunk[..])?;
            if read == 0 {
                break;
            }
            prefix.extend_from_slice(&chunk[..read]);
        }
        let detected = sniff_variant(&prefix).ok_or(Error::UndetectedMboxVariant)?;
        self.variant = detected;
        self.sniff_buffer = Some(prefix);
        Ok(detected)
    }

    fn ensure_variant(&mut self) -> Result<()> {
        if self.variant == MboxVariant::Auto {
            self.detect_variant()?;
        }
        Ok(())
    }

    /// Read one line, drawing from `sniff_buffer` first if present.
    /// Returns `Ok(None)` at EOF.
    fn read_line(&mut self) -> Result<Option<Vec<u8>>> {
        if let Some(buf) = self.sniff_buffer.as_mut() {
            if let Some(nl) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=nl).collect();
                if buf.is_empty() {
                    self.sniff_buffer = None;
                }
                return Ok(Some(line));
            }
            // Need more bytes; drain the buffer and continue from inner.
            let mut prefix = std::mem::take(self.sniff_buffer.as_mut().expect("just checked"));
            self.sniff_buffer = None;
            let mut tail = Vec::new();
            let bytes_read = self.inner.read_until(b'\n', &mut tail)?;
            if bytes_read == 0 && prefix.is_empty() {
                return Ok(None);
            }
            prefix.extend_from_slice(&tail);
            return Ok(Some(prefix));
        }

        let mut line = Vec::new();
        let bytes_read = self.inner.read_until(b'\n', &mut line)?;
        if bytes_read == 0 {
            Ok(None)
        } else {
            Ok(Some(line))
        }
    }
}

fn read_some<R: BufRead>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    // BufRead doesn't expose `read` directly through the trait we have;
    // `fill_buf` + `consume` gives us bounded reads. Try fill_buf first.
    let available = reader.fill_buf()?;
    if available.is_empty() {
        return Ok(0);
    }
    let n = available.len().min(buf.len());
    buf[..n].copy_from_slice(&available[..n]);
    reader.consume(n);
    Ok(n)
}

impl<R: BufRead> Iterator for MboxReader<R> {
    type Item = Result<RawMessage>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.eof {
            return None;
        }
        if let Err(e) = self.ensure_variant() {
            return Some(Err(e));
        }
        match read_message(self) {
            Ok(Some(msg)) => Some(Ok(msg)),
            Ok(None) => {
                self.eof = true;
                None
            }
            Err(e) => {
                self.eof = true;
                Some(Err(e))
            }
        }
    }
}

fn read_message<R: BufRead>(r: &mut MboxReader<R>) -> Result<Option<RawMessage>> {
    let from_line = match r.pending_from_line.take() {
        Some(line) => line,
        None => match r.read_line()? {
            Some(line) => line,
            None => return Ok(None),
        },
    };

    // Skip leading blank lines between messages.
    let mut current = from_line;
    while is_blank(&current) {
        match r.read_line()? {
            Some(next) => current = next,
            None => return Ok(None),
        }
    }

    let (envelope_from, timestamp) = parse_from_line(&current).ok_or_else(|| {
        Error::MalformedMbox(format!(
            "expected From line, got {:?}",
            String::from_utf8_lossy(&current)
        ))
    })?;

    // Headers.
    let mut headers: Vec<(String, Vec<u8>)> = Vec::new();
    let mut content_length: Option<usize> = None;
    loop {
        let line = match r.read_line()? {
            Some(line) => line,
            None => {
                return Err(Error::MalformedMbox(
                    "unexpected EOF in headers".to_string(),
                ))
            }
        };
        let trimmed = trim_eol(&line);
        if trimmed.is_empty() {
            break;
        }
        // Header folding: lines starting with WS continue the previous.
        if matches!(line.first(), Some(b' ') | Some(b'\t')) {
            if let Some((_, value)) = headers.last_mut() {
                value.push(b' ');
                value.extend_from_slice(trim_leading_ws(trimmed));
                continue;
            }
        }
        let colon = match trimmed.iter().position(|&b| b == b':') {
            Some(pos) => pos,
            None => {
                return Err(Error::MalformedMbox(format!(
                    "header missing ':': {:?}",
                    String::from_utf8_lossy(trimmed)
                )))
            }
        };
        let name_bytes = &trimmed[..colon];
        let mut value = trimmed[colon + 1..].to_vec();
        if value.first() == Some(&b' ') {
            value.remove(0);
        }
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| Error::MalformedMbox("non-ASCII header name".to_string()))?
            .to_string();
        if name.eq_ignore_ascii_case("content-length") {
            if let Ok(s) = std::str::from_utf8(&value) {
                content_length = s.trim().parse::<usize>().ok();
            }
        }
        headers.push((name, value));
    }

    // Body.
    let body = if uses_content_length(r.variant) {
        let length = content_length.ok_or_else(|| {
            Error::MalformedMbox(format!("{:?} requires Content-Length header", r.variant))
        })?;
        read_exact_body(r, length)?
    } else {
        read_until_from_separator(r)?
    };

    Ok(Some(RawMessage {
        headers,
        body,
        envelope_from: Some(envelope_from),
        timestamp,
        flags: Flags::empty(),
    }))
}

fn read_exact_body<R: BufRead>(r: &mut MboxReader<R>, length: usize) -> Result<Vec<u8>> {
    let mut body = Vec::with_capacity(length);
    while body.len() < length {
        let needed = length - body.len();
        let mut chunk = vec![0u8; needed.min(8192)];
        let n = read_some(&mut r.inner, &mut chunk[..])?;
        if n == 0 {
            return Err(Error::MalformedMbox(
                "EOF before Content-Length satisfied".to_string(),
            ));
        }
        body.extend_from_slice(&chunk[..n]);
    }
    Ok(body)
}

fn read_until_from_separator<R: BufRead>(r: &mut MboxReader<R>) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(line) = r.read_line()? {
        let trimmed = trim_eol(&line);
        if trimmed.starts_with(b"From ") && is_plausible_from_line(trimmed) {
            r.pending_from_line = Some(line);
            break;
        }
        let unescaped = unescape_body_line(trimmed, r.variant);
        body.extend_from_slice(&unescaped);
        body.extend_from_slice(b"\r\n");
    }
    trim_trailing_blank_line(&mut body);
    Ok(body)
}

/// `From <addr> <date>` lines have a space after `From` and an `@`
/// somewhere in the local part. Used to guard against false matches in
/// `Mboxo` bodies.
fn is_plausible_from_line(line: &[u8]) -> bool {
    line.starts_with(b"From ") && line.contains(&b'@')
}

fn trim_trailing_blank_line(body: &mut Vec<u8>) {
    while body.ends_with(b"\r\n\r\n") {
        body.truncate(body.len() - 2);
    }
    if body.ends_with(b"\r\n") && body.len() == 2 {
        body.clear();
    }
}

fn parse_from_line(line: &[u8]) -> Option<(String, SystemTime)> {
    let trimmed = trim_eol(line);
    let rest = trimmed.strip_prefix(b"From ")?;
    // Split on first space — left is envelope-from, right is date.
    let space = rest.iter().position(|&b| b == b' ')?;
    let from = std::str::from_utf8(&rest[..space]).ok()?.to_string();
    // We don't parse the date faithfully — set it to `now`. mbox
    // `From ` dates are conventional and rarely used for ordering by
    // downstream tools that read the file. Callers who need a real
    // timestamp should pull from the `Date:` header.
    Some((from, SystemTime::now()))
}

fn is_blank(line: &[u8]) -> bool {
    matches!(trim_eol(line), b"")
}

fn trim_eol(line: &[u8]) -> &[u8] {
    let mut end = line.len();
    if end > 0 && line[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && line[end - 1] == b'\r' {
        end -= 1;
    }
    &line[..end]
}

fn trim_leading_ws(line: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < line.len() && matches!(line[start], b' ' | b'\t') {
        start += 1;
    }
    &line[start..]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use std::io::Cursor;

    fn read_all(input: &[u8], variant: MboxVariant) -> Vec<RawMessage> {
        MboxReader::new(Cursor::new(input.to_vec()), variant)
            .collect::<Result<Vec<_>>>()
            .unwrap()
    }

    #[test]
    fn reads_single_mboxrd_message() {
        let bytes = b"From alice@example.com Tue Mar 17 09:30:00 2026\r\n\
                       From: alice@example.com\r\n\
                       Subject: hi\r\n\r\n\
                       Hello there.\r\n\r\n";
        let msgs = read_all(bytes, MboxVariant::Mboxrd);
        assert_eq!(msgs.len(), 1);
        let m = &msgs[0];
        assert_eq!(m.envelope_from.as_deref(), Some("alice@example.com"));
        assert_eq!(m.header("Subject"), Some(&b"hi"[..]));
        assert_eq!(m.body.as_slice(), b"Hello there.\r\n");
    }

    #[test]
    fn unescapes_mboxrd_body() {
        let bytes = b"From alice@example.com Tue Mar 17 09:30:00 2026\r\n\
                       Subject: t\r\n\r\n\
                       >From the depths\r\n\
                       Normal line\r\n\r\n";
        let msgs = read_all(bytes, MboxVariant::Mboxrd);
        assert_eq!(msgs[0].body, b"From the depths\r\nNormal line\r\n");
    }

    #[test]
    fn reads_two_messages() {
        let bytes = b"From a@example.com Tue Mar 17 09:30:00 2026\r\n\
                       Subject: one\r\n\r\n\
                       body1\r\n\r\n\
                       From b@example.com Wed Mar 18 09:30:00 2026\r\n\
                       Subject: two\r\n\r\n\
                       body2\r\n\r\n";
        let msgs = read_all(bytes, MboxVariant::Mboxrd);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].header("Subject"), Some(&b"one"[..]));
        assert_eq!(msgs[1].header("Subject"), Some(&b"two"[..]));
        assert_eq!(msgs[1].envelope_from.as_deref(), Some("b@example.com"));
    }

    #[test]
    fn folds_continuation_lines() {
        let bytes = b"From alice@example.com Tue Mar 17 09:30:00 2026\r\n\
                       Subject: long\r\n\
                       \tcontinued\r\n\
                       From: a@example.com\r\n\r\n\
                       body\r\n\r\n";
        let msgs = read_all(bytes, MboxVariant::Mboxrd);
        assert_eq!(msgs[0].header("Subject"), Some(&b"long continued"[..]));
    }

    #[test]
    fn reads_mboxcl_with_content_length() {
        let body = "hello\nworld";
        let bytes = format!(
            "From a@example.com Tue Mar 17 09:30:00 2026\r\n\
             Content-Length: {}\r\n\r\n\
             {}",
            body.len(),
            body
        );
        let msgs = read_all(bytes.as_bytes(), MboxVariant::Mboxcl);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].body.as_slice(), body.as_bytes());
    }

    #[test]
    fn auto_detects_mboxrd_when_escape_present() {
        let bytes = b"From alice@example.com Tue Mar 17 09:30:00 2026\r\n\
                       Subject: t\r\n\r\n\
                       >From the depths\r\n\r\n";
        let mut r = MboxReader::new(Cursor::new(bytes.to_vec()), MboxVariant::Auto);
        let detected = r.detect_variant().unwrap();
        assert_eq!(detected, MboxVariant::Mboxrd);
        let msgs: Vec<_> = r.collect::<Result<_>>().unwrap();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn auto_falls_back_to_mboxo() {
        let bytes = b"From a@example.com Tue Mar 17 09:30:00 2026\r\n\
                       Subject: t\r\n\r\n\
                       normal body\r\n\r\n";
        let mut r = MboxReader::new(Cursor::new(bytes.to_vec()), MboxVariant::Auto);
        assert_eq!(r.detect_variant().unwrap(), MboxVariant::Mboxo);
    }

    #[test]
    fn auto_returns_error_on_empty_stream() {
        let mut r = MboxReader::new(Cursor::new(Vec::<u8>::new()), MboxVariant::Auto);
        assert!(matches!(
            r.detect_variant(),
            Err(Error::UndetectedMboxVariant)
        ));
    }

    #[test]
    fn malformed_header_errors() {
        // Header line without a colon.
        let bytes = b"From a@example.com Tue Mar 17 09:30:00 2026\r\n\
                       not-a-header\r\n\r\n\
                       body\r\n\r\n";
        let result: Result<Vec<_>> =
            MboxReader::new(Cursor::new(bytes.to_vec()), MboxVariant::Mboxrd).collect();
        assert!(matches!(result, Err(Error::MalformedMbox(_))));
    }
}

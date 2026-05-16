//! Maildir reader: iterate `cur/` + `new/` entries.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::raw_message::{Flags, RawMessage};

use super::flags::parse_filename;
use super::Maildir;

/// One Maildir entry. Path is the on-disk location; `unique_id` is the
/// portion before the separator; `flags` is parsed from the suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MaildirEntry {
    pub path: PathBuf,
    pub unique_id: String,
    pub flags: Flags,
}

impl MaildirEntry {
    /// Read the message body. Headers and body are returned in a
    /// [`RawMessage`] with `envelope_from = None` (Maildir doesn't
    /// carry envelope-from per message).
    pub fn read(&self) -> Result<RawMessage> {
        let raw = fs::read(&self.path)?;
        let (headers, body) = split_headers_body(&raw);
        Ok(RawMessage {
            headers,
            body,
            envelope_from: None,
            timestamp: file_mtime(&self.path),
            flags: self.flags,
        })
    }
}

impl Maildir {
    /// Iterator over `cur/` then `new/` entries, sorted by filename
    /// for deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = Result<MaildirEntry>> + '_ {
        let cur = self.root.join("cur");
        let new = self.root.join("new");
        let sep = self.separator;

        let cur_entries = collect_entries(&cur, sep);
        let new_entries = collect_entries(&new, sep);
        cur_entries.into_iter().chain(new_entries)
    }
}

fn collect_entries(dir: &Path, separator: char) -> Vec<Result<MaildirEntry>> {
    let mut out: Vec<Result<MaildirEntry>> = Vec::new();
    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            // Folder doesn't exist or is unreadable — propagate one
            // error and stop iterating this side. Maildir::open already
            // validates the tree, so this is unusual.
            out.push(Err(e.into()));
            return out;
        }
    };

    let mut files: Vec<PathBuf> = Vec::new();
    for entry in read_dir {
        match entry {
            Ok(e) => {
                let path = e.path();
                if path.is_file() {
                    files.push(path);
                }
            }
            Err(e) => out.push(Err(e.into())),
        }
    }
    // Sort for deterministic order.
    files.sort();
    for path in files {
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        let (unique_id, flags) = parse_filename(name, separator);
        out.push(Ok(MaildirEntry {
            path,
            unique_id,
            flags,
        }));
    }
    out
}

fn split_headers_body(raw: &[u8]) -> (Vec<(String, Vec<u8>)>, Vec<u8>) {
    // Find the header/body boundary: first occurrence of \r\n\r\n or \n\n.
    let boundary = find_boundary(raw);
    let (header_bytes, body) = match boundary {
        Some((end, body_start)) => (&raw[..end], raw[body_start..].to_vec()),
        None => (raw, Vec::new()),
    };
    let headers = parse_headers(header_bytes);
    (headers, body)
}

fn find_boundary(raw: &[u8]) -> Option<(usize, usize)> {
    // \r\n\r\n => header end = i, body start = i + 4
    // \n\n     => header end = i, body start = i + 2
    let mut i = 0;
    while i + 3 < raw.len() {
        if &raw[i..i + 4] == b"\r\n\r\n" {
            return Some((i, i + 4));
        }
        if &raw[i..i + 2] == b"\n\n" {
            return Some((i, i + 2));
        }
        i += 1;
    }
    None
}

fn parse_headers(raw: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut headers: Vec<(String, Vec<u8>)> = Vec::new();
    for line in raw.split(|&b| b == b'\n') {
        let trimmed = trim_eol(line);
        if trimmed.is_empty() {
            continue;
        }
        if matches!(trimmed.first(), Some(b' ') | Some(b'\t')) {
            if let Some((_, v)) = headers.last_mut() {
                v.push(b' ');
                v.extend_from_slice(trim_leading_ws(trimmed));
            }
            continue;
        }
        let colon = match trimmed.iter().position(|&b| b == b':') {
            Some(c) => c,
            None => continue,
        };
        let name = match std::str::from_utf8(&trimmed[..colon]) {
            Ok(s) => s.to_string(),
            Err(_) => continue,
        };
        let mut value = trimmed[colon + 1..].to_vec();
        if value.first() == Some(&b' ') {
            value.remove(0);
        }
        headers.push((name, value));
    }
    headers
}

fn trim_eol(line: &[u8]) -> &[u8] {
    let mut end = line.len();
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

fn file_mtime(path: &Path) -> std::time::SystemTime {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}

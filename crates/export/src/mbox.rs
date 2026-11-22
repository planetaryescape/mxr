use std::time::SystemTime;

use mailbox_formats::{MboxVariant, MboxWriter, RawMessage};

use crate::ExportThread;

pub fn export_mbox(thread: &ExportThread) -> String {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut writer = MboxWriter::new(&mut buf, MboxVariant::Mboxrd);
        for msg in &thread.messages {
            let raw = export_message_to_raw(msg);
            writer
                .write_message(&raw)
                .expect("writing to Vec<u8> cannot fail");
        }
        writer.finish().expect("finish writes to Vec<u8>");
    }
    String::from_utf8(buf).expect("mbox output is ASCII-clean")
}

fn export_message_to_raw(msg: &crate::ExportMessage) -> RawMessage {
    let headers = if let Some(raw) = &msg.headers_raw {
        parse_raw_headers(raw)
    } else {
        reconstruct_headers(msg)
    };

    let body = msg
        .body_text
        .as_deref()
        .map(|s| {
            // Normalise to LF; the writer canonicalises to CRLF.
            s.replace("\r\n", "\n").into_bytes()
        })
        .unwrap_or_default();

    let timestamp: SystemTime = msg.date.into();

    RawMessage::new(headers, body)
        .with_envelope_from(msg.from_email.clone())
        .with_timestamp(timestamp)
}

fn parse_raw_headers(raw: &str) -> Vec<(String, Vec<u8>)> {
    let mut headers: Vec<(String, Vec<u8>)> = Vec::new();
    let normalised = raw.replace("\r\n", "\n");
    for line in normalised.split('\n') {
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some((_, value)) = headers.last_mut() {
                value.push(b' ');
                value.extend_from_slice(line.trim_start().as_bytes());
            }
            continue;
        }
        if let Some(colon) = line.find(':') {
            let name = line[..colon].to_string();
            let value = line[colon + 1..].trim_start().as_bytes().to_vec();
            headers.push((name, value));
        }
    }
    headers
}

fn reconstruct_headers(msg: &crate::ExportMessage) -> Vec<(String, Vec<u8>)> {
    let mut headers: Vec<(String, Vec<u8>)> = Vec::new();
    let from_value = match &msg.from_name {
        Some(name) => format!("{} <{}>", name, msg.from_email),
        None => msg.from_email.clone(),
    };
    headers.push(("From".to_string(), from_value.into_bytes()));
    headers.push(("Subject".to_string(), msg.subject.clone().into_bytes()));
    headers.push(("Date".to_string(), msg.date.to_rfc2822().into_bytes()));
    if !msg.to.is_empty() {
        headers.push(("To".to_string(), msg.to.join(", ").into_bytes()));
    }
    headers
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::tests::sample_thread;

    #[test]
    fn mbox_escapes_from_in_body() {
        let mut thread = sample_thread();
        thread.messages[0].body_text =
            Some("From the beginning of time\nNormal line\nFrom space comes light".into());
        let result = export_mbox(&thread);
        assert!(result.contains(">From the beginning of time"));
        assert!(result.contains("Normal line"));
        assert!(result.contains(">From space comes light"));
    }
}

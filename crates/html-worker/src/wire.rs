//! Length-prefixed frames between the supervisor and a helper process.
//!
//! A frame is a little-endian `u32` length and that many bytes. The HTML goes
//! as raw bytes, never escaped into JSON, so a 2 MiB document costs a copy and
//! not a re-encode.
//!
//! Request body: `id: u64`, `kind_len: u16`, `kind`, `params_len: u32`,
//! `params` (JSON), then the HTML to the end of the frame.
//! Response body: `id: u64`, `status: u8`, then the payload to the end.

use std::io::{self, Read, Write};

/// Largest frame either side accepts. A corrupt length must not make a reader
/// allocate gigabytes; 64 MiB is far above any real request (the HTML cap is
/// 2 MiB) or response (text, links, a rendered line list).
pub(crate) const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// Sent once by a helper after its limits are set, so a spawn that "worked"
/// but never became a helper is caught before the first request. Bump the
/// suffix when the frame layout changes: a helper left running from an older
/// binary then fails the handshake instead of misreading frames.
pub(crate) const READY: &[u8] = b"mxr-html-worker/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Status {
    Ok = 0,
    /// The job panicked or its params did not decode. The payload is a short
    /// message that names no content.
    JobError = 1,
    UnknownKind = 2,
    Ready = 3,
}

impl Status {
    fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Self::Ok,
            1 => Self::JobError,
            2 => Self::UnknownKind,
            3 => Self::Ready,
            _ => return None,
        })
    }
}

pub(crate) struct Request<'a> {
    pub id: u64,
    pub kind: &'a str,
    pub params: &'a [u8],
    pub html: &'a [u8],
}

pub(crate) struct Response<'a> {
    pub id: u64,
    pub status: Status,
    pub payload: &'a [u8],
}

fn malformed(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("malformed frame: {what}"),
    )
}

pub(crate) fn write_frame(out: &mut impl Write, body: &[u8]) -> io::Result<()> {
    let length = u32::try_from(body.len()).map_err(|_| malformed("frame too large"))?;
    // One buffered write per frame: a pipe write under PIPE_BUF is atomic and a
    // short frame is a single syscall.
    let mut framed = Vec::with_capacity(4 + body.len());
    framed.extend_from_slice(&length.to_le_bytes());
    framed.extend_from_slice(body);
    out.write_all(&framed)?;
    out.flush()
}

/// The next frame, or `None` on a clean end of input at a frame boundary.
pub(crate) fn read_frame(input: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0u8; 4];
    let mut filled = 0;
    while filled < header.len() {
        match input.read(&mut header[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    let length = u32::from_le_bytes(header) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(malformed("length over the frame cap"));
    }
    let mut body = vec![0u8; length];
    input.read_exact(&mut body)?;
    Ok(Some(body))
}

pub(crate) fn encode_request(id: u64, kind: &str, params: &[u8], html: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(8 + 2 + kind.len() + 4 + params.len() + html.len());
    body.extend_from_slice(&id.to_le_bytes());
    // Kinds are static identifiers written by us; saturating keeps this total
    // and a kind that long fails the helper's lookup rather than panicking.
    let kind_len = u16::try_from(kind.len()).unwrap_or(u16::MAX);
    body.extend_from_slice(&kind_len.to_le_bytes());
    body.extend_from_slice(&kind.as_bytes()[..usize::from(kind_len)]);
    let params_len = u32::try_from(params.len()).unwrap_or(u32::MAX);
    body.extend_from_slice(&params_len.to_le_bytes());
    body.extend_from_slice(params);
    body.extend_from_slice(html);
    body
}

pub(crate) fn decode_request(body: &[u8]) -> io::Result<Request<'_>> {
    let (id, rest) = take(body, 8)?;
    let (kind_len, rest) = take(rest, 2)?;
    let kind_len = usize::from(u16::from_le_bytes([kind_len[0], kind_len[1]]));
    let (kind, rest) = take(rest, kind_len)?;
    let (params_len, rest) = take(rest, 4)?;
    let params_len =
        u32::from_le_bytes([params_len[0], params_len[1], params_len[2], params_len[3]]) as usize;
    let (params, html) = take(rest, params_len)?;
    Ok(Request {
        id: u64_from(id),
        kind: std::str::from_utf8(kind).map_err(|_| malformed("kind is not UTF-8"))?,
        params,
        html,
    })
}

pub(crate) fn encode_response(id: u64, status: Status, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(9 + payload.len());
    body.extend_from_slice(&id.to_le_bytes());
    body.push(status as u8);
    body.extend_from_slice(payload);
    body
}

pub(crate) fn decode_response(body: &[u8]) -> io::Result<Response<'_>> {
    let (id, rest) = take(body, 8)?;
    let (status, payload) = take(rest, 1)?;
    Ok(Response {
        id: u64_from(id),
        status: Status::from_byte(status[0]).ok_or_else(|| malformed("unknown status"))?,
        payload,
    })
}

fn take(bytes: &[u8], count: usize) -> io::Result<(&[u8], &[u8])> {
    if bytes.len() < count {
        return Err(malformed("truncated"));
    }
    Ok(bytes.split_at(count))
}

fn u64_from(bytes: &[u8]) -> u64 {
    let mut array = [0u8; 8];
    array.copy_from_slice(bytes);
    u64::from_le_bytes(array)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_survives_a_frame_round_trip() {
        let body = encode_request(7, "reader.render", b"{\"width\":80}", b"<p>hi</p>");
        let mut wire = Vec::new();
        write_frame(&mut wire, &body).expect("write");
        let read = read_frame(&mut wire.as_slice())
            .expect("read")
            .expect("one frame");
        let request = decode_request(&read).expect("decode");
        assert_eq!(request.id, 7);
        assert_eq!(request.kind, "reader.render");
        assert_eq!(request.params, b"{\"width\":80}");
        assert_eq!(request.html, b"<p>hi</p>");
    }

    #[test]
    fn a_response_survives_a_round_trip() {
        let body = encode_response(9, Status::JobError, b"boom");
        let response = decode_response(&body).expect("decode");
        assert_eq!(response.id, 9);
        assert_eq!(response.status, Status::JobError);
        assert_eq!(response.payload, b"boom");
    }

    #[test]
    fn end_of_input_at_a_frame_boundary_is_not_an_error() {
        assert!(read_frame(&mut [].as_slice()).expect("clean eof").is_none());
    }

    #[test]
    fn a_length_cut_short_is_an_error() {
        assert!(read_frame(&mut [1u8, 0].as_slice()).is_err());
        let mut truncated = 10u32.to_le_bytes().to_vec();
        truncated.extend_from_slice(b"abc");
        assert!(read_frame(&mut truncated.as_slice()).is_err());
    }

    #[test]
    fn a_length_over_the_cap_is_refused_before_allocating() {
        let huge = u32::MAX.to_le_bytes();
        let error = read_frame(&mut huge.as_slice()).expect_err("over cap");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_request_cut_before_its_params_end_is_an_error_not_a_panic() {
        let body = encode_request(1, "k", b"{}", b"x");
        // id 8 + kind_len 2 + kind 1 + params_len 4 + params 2 = 17 bytes
        // before the HTML starts; any cut inside that prefix is malformed.
        for cut in 0..17 {
            assert!(decode_request(&body[..cut]).is_err(), "cut at {cut}");
        }
        let no_html = decode_request(&body[..17]).expect("empty HTML is valid");
        assert!(no_html.html.is_empty());
    }
}

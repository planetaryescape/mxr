//! The helper side: read requests, run jobs, write replies.
//!
//! Stdout carries frames and nothing else. A job that wrote to it would
//! corrupt the stream, so jobs must not print; the supervisor sends stderr to
//! `/dev/null`.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::ExitCode;

use crate::job::AnyJob;
use crate::limits::{self, Limits};
use crate::wire::{self, Status};

/// Stack for the thread that runs jobs. The tree builders recurse with the
/// document's nesting, and the default 8 MiB main stack is a deeper recursion
/// than an ordinary page needs but not than a hostile one can ask for; stack
/// is address space, not memory, until it is touched.
const JOB_STACK_BYTES: usize = 64 * 1024 * 1024;

/// Runs a helper process: applies the limits, then serves requests on stdin
/// and stdout until stdin closes. The `mxr` binary calls this for its hidden
/// helper mode, before it starts a runtime or logging.
pub fn run_helper(jobs: &[&dyn AnyJob]) -> ExitCode {
    let registry = match Registry::new(jobs) {
        Ok(registry) => registry,
        Err(message) => {
            eprintln!("html helper: {message}");
            return ExitCode::from(2);
        }
    };
    let limits = Limits::from_env();
    let outcome = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("html-helper".to_string())
            .stack_size(JOB_STACK_BYTES)
            .spawn_scoped(scope, || {
                // After the stack is mapped, so the address-space cap is
                // measured with it counted.
                limits::apply(limits);
                serve(&registry, io::stdin().lock(), io::stdout().lock(), || {
                    limits::rearm_cpu(limits);
                })
            })
            .and_then(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(io::Error::other("helper thread panicked")))
            })
    });
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("html helper: {error}");
            ExitCode::from(1)
        }
    }
}

pub(crate) struct Registry<'a> {
    by_kind: HashMap<&'static str, &'a dyn AnyJob>,
}

impl<'a> Registry<'a> {
    /// Two jobs with one kind would make one of them unreachable, so it is an
    /// error at start-up, not a quiet shadowing.
    pub(crate) fn new(jobs: &[&'a dyn AnyJob]) -> Result<Self, String> {
        let mut by_kind = HashMap::new();
        for job in jobs {
            if by_kind.insert(job.kind(), *job).is_some() {
                return Err(format!("two jobs are registered as {}", job.kind()));
            }
        }
        Ok(Self { by_kind })
    }
}

/// The request loop, separate from process setup so tests can drive it over
/// in-memory buffers.
pub(crate) fn serve(
    registry: &Registry<'_>,
    mut input: impl Read,
    mut output: impl Write,
    mut before_request: impl FnMut(),
) -> io::Result<()> {
    wire::write_frame(
        &mut output,
        &wire::encode_response(0, Status::Ready, wire::READY),
    )?;
    while let Some(frame) = wire::read_frame(&mut input)? {
        let request = wire::decode_request(&frame)?;
        before_request();
        let (status, payload) = handle(registry, &request);
        wire::write_frame(
            &mut output,
            &wire::encode_response(request.id, status, &payload),
        )?;
    }
    Ok(())
}

fn handle(registry: &Registry<'_>, request: &wire::Request<'_>) -> (Status, Vec<u8>) {
    let Some(job) = registry.by_kind.get(request.kind) else {
        return (
            Status::UnknownKind,
            b"no job is registered under that kind".to_vec(),
        );
    };
    let Ok(html) = std::str::from_utf8(request.html) else {
        return (Status::JobError, b"the document is not UTF-8".to_vec());
    };
    // html2text 0.17 panics on some malformed tables. Jobs guard what they
    // know about, and this catches the rest, so one bad document costs a
    // fallback and not the helper.
    match catch_unwind(AssertUnwindSafe(|| job.run_erased(html, request.params))) {
        Ok(Ok(bytes)) => (Status::Ok, bytes),
        Ok(Err(message)) => (Status::JobError, message.into_bytes()),
        Err(_) => (Status::JobError, b"the job panicked".to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::Job;

    static UPPER: Job<(), String> = Job::new("test.upper", |html, ()| html.to_uppercase());
    static PANICS: Job<(), String> = Job::new("test.panics", |_, ()| {
        std::panic::resume_unwind(Box::new("job failed"))
    });

    fn exchange(jobs: &[&dyn AnyJob], requests: &[Vec<u8>]) -> Vec<(u64, Status, Vec<u8>)> {
        let registry = Registry::new(jobs).expect("registry");
        let mut input = Vec::new();
        for request in requests {
            wire::write_frame(&mut input, request).expect("frame");
        }
        let mut output = Vec::new();
        let mut rearmed = 0;
        serve(&registry, input.as_slice(), &mut output, || rearmed += 1).expect("serve");
        assert_eq!(
            rearmed,
            requests.len(),
            "the CPU budget is re-armed per request"
        );
        let mut replies = Vec::new();
        let mut rest = output.as_slice();
        while let Some(frame) = wire::read_frame(&mut rest).expect("reply frame") {
            let reply = wire::decode_response(&frame).expect("decode");
            replies.push((reply.id, reply.status, reply.payload.to_vec()));
        }
        replies
    }

    #[test]
    fn the_helper_says_ready_then_answers_each_request_in_order() {
        let replies = exchange(
            &[&UPPER],
            &[
                wire::encode_request(1, "test.upper", b"null", b"ab"),
                wire::encode_request(2, "test.upper", b"null", b"cd"),
            ],
        );
        assert_eq!(replies[0].1, Status::Ready);
        assert_eq!(replies[0].2, wire::READY);
        assert_eq!((replies[1].0, replies[1].1), (1, Status::Ok));
        assert_eq!(replies[1].2, b"\"AB\"");
        assert_eq!((replies[2].0, replies[2].1), (2, Status::Ok));
        assert_eq!(replies[2].2, b"\"CD\"");
    }

    #[test]
    fn a_panicking_job_is_reported_and_the_next_request_still_runs() {
        let replies = exchange(
            &[&UPPER, &PANICS],
            &[
                wire::encode_request(1, "test.panics", b"null", b"x"),
                wire::encode_request(2, "test.upper", b"null", b"ok"),
            ],
        );
        assert_eq!((replies[1].0, replies[1].1), (1, Status::JobError));
        assert_eq!((replies[2].0, replies[2].1), (2, Status::Ok));
    }

    #[test]
    fn an_unknown_kind_is_reported_not_fatal() {
        let replies = exchange(
            &[&UPPER],
            &[wire::encode_request(5, "test.nothing", b"null", b"x")],
        );
        assert_eq!((replies[1].0, replies[1].1), (5, Status::UnknownKind));
    }

    #[test]
    fn a_duplicate_kind_is_refused_at_start_up() {
        let error = Registry::new(&[&UPPER, &UPPER])
            .err()
            .expect("duplicate refused");
        assert!(error.contains("test.upper"), "{error}");
    }

    #[test]
    fn a_document_that_is_not_utf8_is_a_job_error() {
        let replies = exchange(
            &[&UPPER],
            &[wire::encode_request(
                1,
                "test.upper",
                b"null",
                &[0xff, 0xfe],
            )],
        );
        assert_eq!(replies[1].1, Status::JobError);
    }
}

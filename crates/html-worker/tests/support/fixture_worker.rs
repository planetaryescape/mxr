//! A helper whose jobs misbehave on request. The supervisor's tests drive it
//! as a real child process.

use mxr_html_worker::{run_helper, AnyJob, Job};

static ECHO: Job<(), String> = Job::new("fixture.echo", |html, ()| html.to_string());

/// The helper's process id, so a test can tell one helper from its replacement.
static PID: Job<(), u32> = Job::new("fixture.pid", |_, ()| std::process::id());

/// Sleeps for the given milliseconds.
static SLEEP: Job<u64, String> = Job::new("fixture.sleep", |_, millis| {
    std::thread::sleep(std::time::Duration::from_millis(*millis));
    "slept".to_string()
});

/// Spins on the CPU until the OS stops the process.
static SPIN: Job<(), String> = Job::new("fixture.spin", |_, ()| {
    let mut counter = 0u64;
    loop {
        counter = std::hint::black_box(counter.wrapping_add(1));
    }
});

/// Burns CPU for the given milliseconds of wall time.
static BURN: Job<u64, String> = Job::new("fixture.burn", |_, millis| {
    let until = std::time::Instant::now() + std::time::Duration::from_millis(*millis);
    let mut counter = 0u64;
    while std::time::Instant::now() < until {
        counter = std::hint::black_box(counter.wrapping_add(1));
    }
    "burned".to_string()
});

/// Holds `mebibytes` resident, touching every page, growing at about 4 MiB per
/// millisecond like a parser that is running away, then keeps holding them for
/// `hold_ms` before it answers. The pace and the hold give the helper's
/// sampling memory watchdog a fair number of looks even on a loaded machine.
static ALLOCATE: Job<(usize, u64), usize> =
    Job::new("fixture.allocate", |_, (mebibytes, hold_ms)| {
        let mut held: Vec<Vec<u8>> = Vec::new();
        for mebibyte in 0..*mebibytes {
            held.push(vec![1u8; 1024 * 1024]);
            if mebibyte % 4 == 3 {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(*hold_ms));
        held.iter()
            .map(|block| usize::from(block[block.len() / 2]))
            .sum()
    });

/// Ends the process the way a failed allocation or a crash would.
static ABORT: Job<(), String> = Job::new("fixture.abort", |_, ()| std::process::abort());

static PANIC: Job<(), String> = Job::new("fixture.panic", |_, ()| {
    std::panic::resume_unwind(Box::new("fixture panic"))
});

/// Dies only on the document `die`, so a test can tell a bad input from a bad
/// helper.
static DIE_ON_INPUT: Job<(), String> = Job::new("fixture.die_on_input", |html, ()| {
    if html == "die" {
        std::process::abort();
    }
    html.to_string()
});

/// Answers the handshake the way a helper from another release would: the
/// right protocol, a different version. Written by hand because the frame
/// encoder is private to the crate. Then waits for its stdin to close.
fn pretend_to_be_another_release() -> std::process::ExitCode {
    use std::io::{Read, Write};
    let payload = b"mxr-html-worker/1 0.0.0-another-release";
    let mut body = Vec::new();
    body.extend_from_slice(&0u64.to_le_bytes());
    body.push(3); // Status::Ready
    body.extend_from_slice(payload);
    let mut out = std::io::stdout().lock();
    let length = u32::try_from(body.len()).unwrap_or(u32::MAX);
    let _ = out.write_all(&length.to_le_bytes());
    let _ = out.write_all(&body);
    let _ = out.flush();
    let _ = std::io::stdin().read_to_end(&mut Vec::new());
    std::process::ExitCode::SUCCESS
}

fn main() -> std::process::ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--another-release") {
        return pretend_to_be_another_release();
    }
    let jobs: [&dyn AnyJob; 10] = [
        &BURN,
        &ECHO,
        &PID,
        &SLEEP,
        &SPIN,
        &ALLOCATE,
        &ABORT,
        &PANIC,
        &DIE_ON_INPUT,
        &ECHO_UPPER,
    ];
    run_helper(&jobs)
}

static ECHO_UPPER: Job<(), String> = Job::new("fixture.upper", |html, ()| html.to_uppercase());

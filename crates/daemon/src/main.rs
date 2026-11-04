use mxr::cli_error::{json_output_requested, CliError};
use std::process::ExitCode;

/// Worker stack for the daemon's runtime. A debug build lays out every
/// request arm's locals in one `handler::dispatch` poll frame, measured at
/// 1.5 MB, which left an Updates let-go less than 0.5 MB of tokio's default
/// 2 MiB and overflowed it. Release frames are far smaller.
const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

/// Stack for the thread that drives the CLI, which `block_on` polls on. The
/// same debug-build layout puts every command arm's locals in one
/// `run_parsed_cli` poll frame, measured at 6.4 MB, which left the 8 MiB main
/// thread a few hundred bytes and overflowed it while `mxr demo` decoded a
/// response.
const CLI_STACK_BYTES: usize = 32 * 1024 * 1024;

fn main() -> anyhow::Result<ExitCode> {
    let args: Vec<String> = std::env::args().collect();
    // Helper mode comes first: no argument parsing, runtime or logging, and
    // stdout carries frames only.
    if args.get(1).map(String::as_str) == Some(mxr::html_helper::HELPER_ARG) {
        return Ok(mxr::html_helper::run_helper());
    }
    mxr::html_helper::install();
    let cli_args = args.clone();
    // Named `main` so panic messages read as they did on the process's thread.
    let result = std::thread::Builder::new()
        .name("main".to_string())
        .stack_size(CLI_STACK_BYTES)
        .spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_stack_size(WORKER_STACK_BYTES)
                .build()?
                .block_on(mxr::run_cli(cli_args))
        })?
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    let Err(error) = result else {
        return Ok(ExitCode::SUCCESS);
    };
    let Some(typed) = CliError::find(&error) else {
        return Err(error);
    };
    // Same text the `Termination` impl prints for an `Err` return.
    eprintln!("Error: {error:?}");
    if json_output_requested(&args) {
        eprintln!("{}", typed.json_line());
    }
    std::process::exit(i32::from(typed.exit_code()));
}

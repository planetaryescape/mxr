use mxr::cli_error::{json_output_requested, CliError};

/// Worker stack for the daemon's runtime. A debug build lays out every
/// request arm's locals in one `handler::dispatch` poll frame, measured at
/// 1.5 MB, which left an Updates let-go less than 0.5 MB of tokio's default
/// 2 MiB and overflowed it. Release frames are far smaller.
const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(WORKER_STACK_BYTES)
        .build()?
        .block_on(mxr::run_cli(args.clone()));
    let Err(error) = result else {
        return Ok(());
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

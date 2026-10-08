/// Worker stack for the daemon's runtime. A debug build lays out every
/// request arm's locals in one `handler::dispatch` poll frame, measured at
/// 1.5 MB, which left an Updates let-go less than 0.5 MB of tokio's default
/// 2 MiB and overflowed it. Release frames are far smaller.
const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(WORKER_STACK_BYTES)
        .build()?
        .block_on(mxr::run_cli(std::env::args().collect()))
}

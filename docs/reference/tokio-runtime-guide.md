# Tokio runtime guide for mxr

This is the house style for async Rust in `mxr`.

Read this before adding parallelism, background loops, or blocking work. The goal is simple: use Tokio for real throughput without starving the daemon, the TUI, or the sync engine.

Current in-repo examples:

- Runtime entry: [`crates/daemon/src/main.rs`](../../crates/daemon/src/main.rs)
- Background account loops: [`crates/daemon/src/loops.rs`](../../crates/daemon/src/loops.rs)
- Concurrent IPC request handling: [`crates/daemon/src/server.rs`](../../crates/daemon/src/server.rs)
- Blocking image decode offload: [`crates/tui/src/terminal_images.rs`](../../crates/tui/src/terminal_images.rs)

## Core mental model

- A `Future` is lazy. It does nothing until somebody polls it.
- `async fn` returns a future. Calling it starts nothing by itself.
- `.await` does not "run on another thread". It yields until the future can make progress.
- A Tokio `task` is a future the runtime schedules.
- A Tokio `worker thread` is an OS thread in the async scheduler.
- A Tokio `blocking thread` is a separate pool for `spawn_blocking`.

Concurrency and parallelism are related but not the same:

- Concurrency: one thread can make progress on many tasks by switching at `.await` points.
- Parallelism: multiple OS threads run tasks at the same time.

Tokio gives us both, but not from the same primitive.

## What `#[tokio::main]` actually does

Our daemon entry point is small:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    mxr::run_cli(std::env::args().collect()).await
}
```

That macro expands to the rough equivalent of:

```rust
fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            mxr::run_cli(std::env::args().collect()).await
        })
}
```

Important consequences:

- The default flavor is the multi-thread runtime.
- Worker thread count defaults to logical CPU count.
- `worker_threads = N` means the scheduler has `N` worker threads for spawned async tasks.
- The async function marked with `#[tokio::main]` is not itself a worker task.

That last point matters. It is tempting to count the calling thread as "one more worker". Tokio's own docs are more careful: the root future runs under `block_on`, while spawned tasks are what the scheduler moves across worker threads. For throughput planning in `mxr`, budget around worker threads, not "worker threads plus a bonus main worker".

If we need custom setup, use `tokio::runtime::Builder` directly. Typical reasons:

- custom worker thread count
- custom blocking thread cap
- thread names
- metrics / unstable runtime tuning

## Which primitive buys what

| Pattern | Use it for | What it does not buy |
|---|---|---|
| `a.await; b.await` | strict ordering, dependency chain | concurrency |
| `tokio::join!(a, b)` | fixed small set of independent async ops | automatic multi-core speedup |
| `tokio::try_join!(a, b)` | same as `join!`, but fail fast | automatic multi-core speedup |
| `tokio::spawn(...)` | independent task with separate scheduling/lifetime | guaranteed new thread |
| `JoinSet` | many spawned tasks, dynamic fan-out | back-pressure by itself |
| `buffer_unordered(n)` / `Semaphore` | bounded concurrency | task ownership / cancellation model |

Short version:

- Use `join!` for a small, fixed number of independent async operations when you want less overhead and one parent task.
- Use `spawn` when work should become its own task, may outlive the current stack frame, or should be schedulable across worker threads.
- Use `JoinSet` or bounded streams when fanning out over many items.

## `join!` is concurrent, not parallel

This is the most common confusion.

```rust
let a = fetch_a();
let b = fetch_b();
let (a, b) = tokio::join!(a, b);
```

This lets both futures make progress without waiting for one to fully finish before starting the other. But `join!` runs inside one parent task. At any instant, one thread is polling that task. When one branch hits `.await`, the runtime can poll the other branch.

That is great for:

- two HTTP requests
- a socket read and a timer
- a store read and a config fetch

It is not the right tool for CPU-heavy multicore work.

Rule:

- If the work is mostly waiting on async I/O, `join!` is often enough.
- If the work should become separately scheduled units that may run across worker threads, use `spawn`.

## `spawn` is where true task-level parallelism starts

`tokio::spawn` gives the runtime an independent task to schedule. On the multi-thread runtime, that task may run:

- on the same worker thread
- on a different worker thread
- on different worker threads over time

That is the lever that makes multicore async execution possible.

```rust
let a = tokio::spawn(async move { sync_account(account_a).await });
let b = tokio::spawn(async move { sync_account(account_b).await });

let a = a.await??;
let b = b.await??;
```

Two hard requirements follow:

- Spawned tasks must own their data: usually `async move`.
- Spawned tasks must be `Send + 'static` when using `tokio::spawn`.

If a task holds non-`Send` state across `.await`, it is not spawn-safe.

### Use `JoinSet` for dynamic fan-out

For "spawn one task per item" patterns, prefer `JoinSet` over a loose `Vec<JoinHandle<_>>`.

```rust
use tokio::task::JoinSet;

let mut set = JoinSet::new();

for account in accounts {
    set.spawn(async move { sync_one_account(account).await });
}

while let Some(result) = set.join_next().await {
    match result {
        Ok(Ok(summary)) => handle_summary(summary),
        Ok(Err(error)) => handle_task_error(error),
        Err(join_error) => handle_join_error(join_error),
    }
}
```

Why:

- cleaner error handling
- easier cancellation and shutdown
- better fit for "unknown number of tasks"

## Bounded concurrency first, unbounded spawn last

Most refactors should not turn into "spawn everything".

If 5,000 messages all kick off work at once, we can easily create:

- provider throttling
- lock contention
- queue blow-ups
- worse latency for interactive surfaces

Default to bounded fan-out:

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

let limit = Arc::new(Semaphore::new(16));
let mut set = JoinSet::new();

for item in items {
    let permit = limit.clone().acquire_owned().await?;
    set.spawn(async move {
        let _permit = permit;
        process_item(item).await
    });
}
```

Good places to bound concurrency in `mxr`:

- provider sync fan-out
- attachment extraction
- semantic chunking / embedding prep
- expensive reader-mode transforms
- parallel CLI batch mutations

## Blocking work: classify it before you touch it

Every piece of work should be classified into one of these buckets before refactoring.

### 1. Async I/O

Examples:

- socket reads/writes
- IPC over `tokio::net`
- `reqwest` HTTP calls
- async IMAP / SMTP / Gmail API requests
- async SQLite calls through `sqlx`

Do this in ordinary async tasks. Do not offload it.

### 2. Short, cheap CPU work

Examples:

- lightweight parsing
- small routing decisions
- tiny data transforms

Keep this inline in async code if it is truly small and bounded.

### 3. Bounded blocking or CPU-heavy work

Examples:

- image decode
- PDF/text extraction through sync libraries
- HTML cleanup with heavy CPU cost
- large MIME parsing through sync-only APIs
- compression / decompression

Use `tokio::task::spawn_blocking`.

We already do this correctly for terminal image decode in [`crates/tui/src/terminal_images.rs`](../../crates/tui/src/terminal_images.rs).

```rust
let decoded = tokio::task::spawn_blocking(move || decode_image(path)).await??;
```

### 4. Long-lived blocking loops

Examples:

- persistent worker thread
- long-running sync event loop
- background process that mostly blocks outside Tokio

Prefer `std::thread::spawn`, not `spawn_blocking`.

`spawn_blocking` is for bounded work that finishes. A long-lived loop can occupy a blocking-pool thread for too long and starve other blocking tasks.

### 5. Heavy pure CPU parallelism

Examples:

- big ranking jobs
- large batch transforms
- expensive indexing passes
- embedding prep if implemented as local CPU-bound compute

Do not blindly spray `spawn_blocking` everywhere. Tokio's docs explicitly warn that the blocking pool has a large upper limit because it is also used for blocking I/O. For heavy CPU parallelism:

- cap concurrency with a `Semaphore`, or
- use a dedicated CPU executor such as `rayon` if the workload is mostly pure compute

## `spawn_blocking` rules

Use `spawn_blocking` when all of these are true:

- the code is synchronous or CPU-bound
- it is bounded and should finish
- there is no async API available

Do not use `spawn_blocking` for:

- forever loops
- work that must be cheaply cancellable once started
- unbounded CPU fan-out without a limit

Operational facts that matter:

- it runs on the blocking thread pool, not the async worker pool
- once started, it cannot be meaningfully aborted
- runtime shutdown will wait for started blocking jobs to finish

That means every new `spawn_blocking` call is a lifecycle decision, not just a performance trick.

## Locks, shared state, and `.await`

Rule zero: never hold a lock across `.await` unless that is the specific design and we reviewed it on purpose.

### Which mutex to use

For `mxr`, the default order is:

1. message passing / owner task
2. `std::sync::Mutex` or `parking_lot::Mutex` for tiny, sync-only critical sections
3. `tokio::sync::Mutex` only when the guarded operation truly must cross `.await`

Tokio's own guidance is blunt here: using an async mutex everywhere is not automatically better. Async mutexes are more expensive. If the critical section is short and does not `.await`, a normal mutex is often the right tool.

### Scope guards so they drop before `.await`

Good:

```rust
{
    let mut state = shared.lock().unwrap();
    state.bump();
}

do_async_work().await;
```

Bad:

```rust
let mut state = shared.lock().unwrap();
state.bump();
do_async_work().await;
```

Why this matters:

- `tokio::spawn` needs `Send`
- many guard types are not `Send`
- even if a guard type is `Send`, holding it across `.await` can deadlock or kill throughput

### Prefer owner-task + channels for async resources

If the shared thing is itself async or stateful, a mutex is often the wrong abstraction.

Good candidates for an owner task:

- provider sessions
- long-lived clients
- mutable caches with async refresh logic
- pipelines that already have a natural command stream

Pattern:

- spawn one owner task
- send it commands over `mpsc`
- return results over `oneshot`

This avoids lock contention and makes task ownership clearer.

## Cancellation, shutdown, and error propagation

If we spawn a task, we need answers for all three:

- who owns it?
- how does it stop?
- where do errors go?

Rules:

- Keep `JoinHandle`s for important spawned work.
- Use `JoinSet` when managing a group.
- Use `tokio::select!` with a shutdown signal for long-running loops.
- Put timeouts around external I/O when "wait forever" is not acceptable.
- Log task failures with enough context to identify the unit of work.

For long-running loops, the shape should look like:

```rust
loop {
    tokio::select! {
        _ = shutdown.changed() => break,
        result = do_one_iteration() => handle(result)?,
    }
}
```

Detached fire-and-forget tasks should be rare. They are acceptable for true app-lifetime background services, but not for ordinary request work.

## What to parallelize in mxr

Good candidates:

- per-account sync work
- independent provider requests inside a sync batch
- attachment extraction
- expensive local rendering/decoding work
- semantic indexing sub-steps that do not need the same lock at once

Bad candidates:

- anything that immediately contends on the same mutex
- tiny operations where spawn overhead dominates
- code that still serializes on a single writer / single global lock
- work where correctness depends on strict ordering and we have not encoded that ordering explicitly

Parallelism only helps when the bottleneck is actually parallelizable.

## Practical refactor rules for this repo

### 1. One runtime boundary at the app edge

Use the main Tokio runtime for daemon / CLI async execution. Do not create nested runtimes inside async code.

Manual runtime creation is acceptable only at explicit sync boundaries, such as a sync-only CLI helper or bridge.

### 2. Do not spawn just to look "more async"

If the current task can simply `.await` the work and there is no need for separate lifetime or scheduling, just `.await` it.

### 3. Add back-pressure with the parallelism

Every new fan-out path should answer:

- what is the concurrency limit?
- where do results accumulate?
- what happens under provider throttling?

### 4. Keep hot locks small

If adding parallelism increases contention on `search`, `semantic`, or any shared state, first reduce lock scope. Parallel code that immediately waits on the same lock is usually fake parallelism.

### 5. Separate orchestration from heavy work

Async tasks should orchestrate I/O and scheduling. Heavy sync work should move to `spawn_blocking` or a dedicated CPU executor.

### 6. Instrument the spawned units

Parallel code without tracing is guesswork. Add spans around per-account, per-batch, or per-item work so we can see:

- queueing
- latency
- retries
- cancellations
- hotspots

## Common mistakes

- Expecting `join!` to use multiple cores by itself.
- Doing sync HTTP, `std::thread::sleep`, or heavy CPU work inside async tasks.
- Converting a loop to `spawn` without adding a concurrency limit.
- Holding any mutex guard across `.await`.
- Spawning detached tasks and dropping the handle with no shutdown plan.
- Assuming `spawn_blocking` is free or infinitely scalable.
- Moving shared mutable state into `Arc<Mutex<_>>` when an owner task would be simpler.
- Treating the `#[tokio::main]` root future as an extra worker for throughput math.

## Review checklist

Before shipping a Tokio refactor, answer these:

1. Is the work async I/O, short CPU, bounded blocking, long-lived blocking, or heavy CPU parallelism?
2. If operations are independent, should they stay sequential, use `join!`, or become spawned tasks?
3. If tasks are spawned, who owns the handles and shutdown path?
4. Is concurrency explicitly bounded?
5. Does any lock live across `.await`?
6. Are we parallelizing real work, or just increasing contention on one shared resource?
7. Should this use `spawn_blocking`, or is a dedicated thread / `rayon` the better fit?
8. Do traces and errors tell us which spawned unit failed?

## How we verify this in mxr

The repo now has a few concrete places to check whether a concurrency refactor is helping or hurting:

- Daemon status JSON includes semantic runtime metrics such as queue depth, in-flight work, queue wait, extract time, embedding prep time, and ingest time.
- Benchmarks live at the repo root:
  - `cargo bench --bench sync_overlap`
  - `cargo bench --bench semantic_ingest --features local,semantic-local`
  - `cargo bench --bench daemon_burst --features local,semantic-local`

Use them to validate the actual design goal:

- sync work overlaps instead of serializing
- background semantic ingest does not stall search/status paths
- CPU-heavy semantic prep stays off the async scheduler's hot path

## External references

- Tokio `#[tokio::main]` macro docs: <https://docs.rs/tokio-macros/latest/tokio_macros/attr.main.html>
- Tokio runtime docs: <https://docs.rs/tokio/latest/tokio/runtime/>
- Tokio spawning tutorial: <https://tokio.rs/tokio/tutorial/spawning>
- Tokio shared-state tutorial: <https://tokio.rs/tokio/tutorial/shared-state>
- Tokio `spawn_blocking` docs: <https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html>
- Tokio bridging guide: <https://tokio.rs/tokio/topics/bridging>

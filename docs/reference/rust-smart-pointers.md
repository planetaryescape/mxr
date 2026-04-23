# Rust smart pointer guide for mxr

This is the house guide for choosing pointer-like types in `mxr`.

Read this before adding `Box`, `Rc`, `Arc`, `RefCell`, `Mutex`, `RwLock`,
`Cow`, `Pin`, raw pointers, or trait-object pointers. The goal is simple:
make ownership and sharing explicit, keep public models boring, and avoid
turning local code into shared mutable state by accident.

Official references:

- [Rust Book: Smart Pointers](https://doc.rust-lang.org/book/ch15-00-smart-pointers.html)
- [Rust Reference: Pointer Types](https://doc.rust-lang.org/reference/types/pointer.html)
- [std::cell](https://doc.rust-lang.org/std/cell/index.html)
- [std::rc](https://doc.rust-lang.org/std/rc/index.html)
- [Box](https://doc.rust-lang.org/std/boxed/struct.Box.html)
- [Arc](https://doc.rust-lang.org/std/sync/struct.Arc.html)
- [Mutex](https://doc.rust-lang.org/std/sync/struct.Mutex.html)
- [RwLock](https://doc.rust-lang.org/std/sync/struct.RwLock.html)
- [Cow](https://doc.rust-lang.org/std/borrow/enum.Cow.html)
- [Pin](https://doc.rust-lang.org/std/pin/struct.Pin.html)

## Core rule

Prefer this order:

1. Plain ownership: `T`
2. Borrowing: `&T` / `&mut T`
3. A narrow smart pointer that encodes the missing capability
4. Shared mutable state only when the ownership graph demands it
5. Raw pointers only at FFI or low-level unsafe boundaries

Smart pointers are not a way to avoid designing ownership. They are the design.

## What a smart pointer is

A pointer stores or represents access to another value.

Rust has three broad pointer families:

- References: `&T`, `&mut T`
- Raw pointers: `*const T`, `*mut T`
- Smart pointers: library types that act pointer-like and add policy

Smart pointers usually implement one or both of:

- `Deref`: lets the wrapper behave like `&T`
- `Drop`: runs cleanup when the wrapper goes out of scope

The extra policy can be heap allocation, reference counting, locking, runtime
borrow checks, one-time initialization, clone-on-write, pinning, or custom
cleanup.

## Decision tree

Use this first. If the answer is uncertain, stop and simplify ownership before
adding a pointer.

```text
Start
|
|-- Can the value have one owner?
|   |
|   |-- Yes
|   |   |
|   |   |-- Do callers only need temporary read access?
|   |   |   `&T`
|   |   |
|   |   |-- Do callers need temporary exclusive mutation?
|   |   |   `&mut T`
|   |   |
|   |   |-- Does the value need heap allocation?
|   |   |   |
|   |   |   |-- Recursive/self-sized type?
|   |   |   |   `Box<T>`
|   |   |   |
|   |   |   |-- Large value moved often?
|   |   |   |   Consider `Box<T>`, but measure before optimizing
|   |   |   |
|   |   |   |-- Dynamic dispatch / trait object?
|   |   |   |   `Box<dyn Trait>` if one owner
|   |   |   |
|   |   |   `-- Otherwise
|   |   |       plain `T`
|   |   |
|   |   `-- Does the address need to stay stable?
|   |       `Pin<Box<T>>` only for address-sensitive or self-referential data
|   |
|   `-- No, multiple owners need to keep it alive
|       |
|       |-- Can ownership cross threads or spawned Tokio tasks?
|       |   |
|       |   |-- Yes
|       |   |   |
|       |   |   |-- Read-only shared value?
|       |   |   |   `Arc<T>`
|       |   |   |
|       |   |   |-- Shared mutation?
|       |   |   |   `Arc<Mutex<T>>`, `Arc<RwLock<T>>`, or atomics
|       |   |   |
|       |   |   |-- Async lock held across `.await`?
|       |   |   |   Prefer restructuring; otherwise use `tokio::sync` locks
|       |   |   |
|       |   |   `-- Back-reference that must not keep value alive?
|       |   |       `std::sync::Weak<T>`
|       |   |
|       `-- No, single-threaded ownership graph
|           |
|           |-- Read-only shared value?
|           |   `Rc<T>`
|           |
|           |-- Shared mutation?
|           |   `Rc<RefCell<T>>` or `Rc<Cell<T>>`
|           |
|           `-- Back-reference that must not keep value alive?
|               `std::rc::Weak<T>`
|
|-- Is mutation needed through `&self`?
|   |
|   |-- Small `Copy` value?
|   |   `Cell<T>`
|   |
|   |-- Single-thread runtime borrow checks acceptable?
|   |   `RefCell<T>`
|   |
|   |-- Value initialized once?
|   |   `OnceCell<T>` or `LazyCell<T, F>`
|   |
|   `-- Multi-thread shared mutation?
|       `Mutex<T>`, `RwLock<T>`, `OnceLock<T>`, `LazyLock<T, F>`, or atomics
|
|-- Is allocation usually avoidable but sometimes needed?
|   `Cow<'a, T>`
|
`-- Are you about to use raw pointers?
    Only for FFI, intrusive structures, or writing a safe abstraction.
    Document invariants beside the unsafe code.
```

## Quick table

| Type | Ownership | Mutability model | Threaded? | Main use |
|---|---|---|---|---|
| `&T` | borrowed | read-only | if `T: Sync` | temporary shared access |
| `&mut T` | borrowed | exclusive mutable | scoped only | temporary mutation |
| `Box<T>` | single owner | inherited | if `T` allows | heap allocation, recursion, trait objects |
| `Rc<T>` | shared owners | read-only by default | no | single-thread shared ownership |
| `Weak<T>` | non-owning | none | matches `Rc`/`Arc` family | break cycles, back-references |
| `Arc<T>` | shared owners | read-only by default | yes | shared ownership across threads/tasks |
| `Cell<T>` | owned container | replace/copy through `&self` | no | small copyable interior mutation |
| `RefCell<T>` | owned container | runtime borrow checks | no | single-thread shared mutation |
| `Mutex<T>` | owned container | exclusive lock guard | yes | cross-thread shared mutation |
| `RwLock<T>` | owned container | many readers or one writer | yes | read-heavy shared mutation |
| `OnceCell<T>` | owned container | set once | no | lazy cache under `&self` |
| `OnceLock<T>` | owned container | set once | yes | static/global one-time init |
| `LazyCell<T, F>` | owned container | init on deref | no | single-thread lazy value |
| `LazyLock<T, F>` | owned container | init on deref | yes | static/global lazy value |
| `Cow<'a, T>` | borrowed or owned | clone on write | depends on `T` | avoid clone until needed |
| `Pin<P>` | wrapper around pointer | prevents moving pointee | depends on `P` | async futures, self-referential data |
| `*const T` / `*mut T` | none | unsafe | manually proven | FFI and low-level internals |
| `NonNull<T>` | none | unsafe | manually proven | non-null raw pointer in abstractions |

## Baseline: use references first

Most Rust APIs should take borrowed data:

```rust
fn render_subject(subject: &str) -> String {
    subject.trim().to_owned()
}

fn sort_messages(messages: &mut [MessageSummary]) {
    messages.sort_by_key(|message| message.received_at);
}
```

Use `&str` instead of `&String`, `&[T]` instead of `&Vec<T>`, and `&Path`
instead of `&PathBuf` in parameters unless the concrete type is required.

Borrowing is best when:

- the callee does not need ownership
- the relationship is temporary
- the caller should keep controlling lifetime
- the value does not need to outlive the call

Do not add `Arc`, `Rc`, or `Box` to make a borrow checker error go away until
the ownership requirement is clear.

## `Box<T>`

Use `Box<T>` for one-owner heap allocation.

Good uses:

- recursive types
- large enum variants when enum size matters
- dynamic dispatch with `Box<dyn Trait>`
- owned unsized values: `Box<[T]>`, `Box<str>`
- pinning with `Pin<Box<T>>`
- reducing move cost for large values, only when it matters

Example: recursive query AST.

```rust
enum QueryNode {
    Text(String),
    And(Box<QueryNode>, Box<QueryNode>),
    Or(Box<QueryNode>, Box<QueryNode>),
    Not(Box<QueryNode>),
}
```

Why `Box` works here:

- enum variants must have a known size
- recursion would otherwise make the type infinitely sized
- `Box<QueryNode>` has a known pointer size
- ownership is still simple: one parent owns each child

Use `Box<dyn Trait>` when:

- callers should own a polymorphic implementation
- the trait is object-safe
- there is one owner
- generics would leak implementation choice into the API

```rust
trait SessionFactory {
    fn connect(&self) -> Result<Session>;
}

struct Provider {
    factory: Box<dyn SessionFactory + Send + Sync>,
}
```

Avoid `Box<T>` when:

- a plain `T` is fine
- you only want to shorten a lifetime error
- shared ownership is required
- the allocation is pure ceremony

## `Rc<T>`

Use `Rc<T>` for multiple owners in single-threaded code.

`Rc` means:

- the value lives until the last strong `Rc` is dropped
- cloning an `Rc` increments a non-atomic reference count
- the inner value is read-only by default
- it cannot be sent across threads

Good uses:

- immutable graph/tree nodes in one thread
- shared ownership in parser or UI-local structures
- cheap clones of read-only data where lifetimes would be awkward

```rust
use std::rc::Rc;

struct Node {
    label: String,
}

let root = Rc::new(Node { label: "inbox".to_owned() });
let left = Rc::clone(&root);
let right = Rc::clone(&root);
```

Use `Rc::clone(&value)` rather than `value.clone()` when you want the code to
say "this is another pointer to the same allocation".

Avoid `Rc<T>` in `mxr` daemon, sync, provider, or protocol surfaces. Those
parts are async and may cross task/thread boundaries. Prefer `Arc<T>` or plain
owned values there.

## `Arc<T>`

Use `Arc<T>` for multiple owners across threads or Tokio tasks.

`Arc` means:

- the value lives until the last strong `Arc` is dropped
- cloning increments an atomic reference count
- cloning is cheap, but not free
- the inner value is read-only by default

Good uses in `mxr`:

- daemon-wide shared handles
- store/search/service handles used by spawned tasks
- shared immutable configuration
- shared trait objects that must be `Send + Sync`
- test hooks shared across worker threads

```rust
use std::sync::Arc;

struct SyncEngine {
    store: Arc<Store>,
}

impl SyncEngine {
    fn new(store: Arc<Store>) -> Self {
        Self { store }
    }
}
```

`Arc<T>` does not make `T` mutable. If mutation is needed, choose the mutation
policy separately:

- `Arc<Mutex<T>>` for exclusive mutation
- `Arc<RwLock<T>>` for read-heavy shared state
- `Arc<AtomicUsize>` and other atomics for counters/flags
- `Arc<T>` with internal safe concurrency if `T` already provides it

Avoid `Arc<T>` when:

- one owner exists
- a borrowed parameter is enough
- the type is part of the core/protocol data model
- the real issue is an overly wide lifetime

## `Weak<T>`

Use `Weak<T>` for non-owning references into an `Rc` or `Arc` allocation.

Weak pointers do not keep the inner value alive. They must be upgraded:

```rust
if let Some(parent) = weak_parent.upgrade() {
    parent.handle_child_event();
}
```

Good uses:

- parent links in trees
- back-references in graphs
- observer lists where listeners may go away
- caches that should not extend object lifetime

Use `Weak` to break reference cycles. A cycle of strong `Rc` or `Arc` pointers
will not be deallocated.

Rule:

- child owns parent? Usually no. Use `Weak`.
- parent owns child? Usually yes. Use `Rc`/`Arc`/plain ownership.

## `Cell<T>`

Use `Cell<T>` for simple single-threaded interior mutability.

`Cell` lets you mutate through `&self` by moving values in and out. It does not
give out references to the inner value.

Good uses:

- booleans
- small counters
- flags
- `Copy` values
- implementation details of logically immutable methods

```rust
use std::cell::Cell;

struct RenderStats {
    frames: Cell<u64>,
}

impl RenderStats {
    fn record_frame(&self) {
        self.frames.set(self.frames.get() + 1);
    }
}
```

Prefer `Cell<T>` over `RefCell<T>` when the value is small and can be copied or
replaced cleanly.

Avoid `Cell<T>` when:

- references into the inner value are needed
- mutation must cross threads
- the value is large and replacing it is awkward

## `RefCell<T>`

Use `RefCell<T>` for single-threaded interior mutability with runtime borrow
checks.

`RefCell` enforces the same rule as Rust references, but at runtime:

- many immutable borrows, or
- one mutable borrow, but
- never both

Violating this panics unless using `try_borrow` / `try_borrow_mut`.

Good uses:

- mocks and test recorders
- single-thread UI-local state with aliasing
- caches behind `&self`
- `Rc<RefCell<T>>` when a single-thread ownership graph truly needs mutation

```rust
use std::cell::RefCell;

struct Recorder {
    events: RefCell<Vec<String>>,
}

impl Recorder {
    fn record(&self, event: String) {
        self.events.borrow_mut().push(event);
    }
}
```

Keep `Ref` and `RefMut` scopes short:

```rust
{
    let mut events = recorder.events.borrow_mut();
    events.push("synced".to_owned());
}

let count = recorder.events.borrow().len();
```

Avoid `RefCell<T>` when:

- `&mut self` would work
- the code is multi-threaded
- the borrow scope would cross complex call chains
- panics would turn ordinary state conflicts into crashes

In threaded code, use `Mutex<T>` or `RwLock<T>` instead.

## `Rc<RefCell<T>>`

Use only when both are true:

1. The value has multiple owners.
2. The value must be mutated through those owners.

This is common in examples and easy to overuse.

```rust
use std::cell::RefCell;
use std::rc::Rc;

let shared = Rc::new(RefCell::new(Vec::new()));
Rc::clone(&shared).borrow_mut().push("event".to_owned());
```

Costs:

- runtime borrow checks
- possible panic on invalid borrow
- harder reasoning about mutation sites
- easy reference cycles with `Rc`

In `mxr`, this should be rare. Prefer explicit ownership and message passing in
daemon/sync code, and plain owned app state in TUI code.

## `Mutex<T>`

Use `Mutex<T>` for thread-safe exclusive access.

`Mutex::lock()` returns a guard. The guard dereferences to `T` and unlocks when
dropped.

Good uses:

- test fakes recording calls from multiple threads
- short critical sections around in-memory state
- shared state where writes are common or reads are cheap

```rust
use std::sync::{Arc, Mutex};

let calls = Arc::new(Mutex::new(Vec::new()));

{
    let mut calls = calls.lock().expect("calls mutex poisoned");
    calls.push("sync");
}
```

Rules:

- keep lock scopes small
- do not call provider/network code while holding a lock
- do not hold `std::sync::MutexGuard` across `.await`
- handle poisoning deliberately

Poisoning means a thread panicked while holding the lock. In tests, `unwrap` or
`expect` is often fine. In daemon code, choose whether to recover, clear, or
surface an error.

Use `tokio::sync::Mutex` only when the guard must be held across `.await`.
Often the better fix is to take the needed data, drop the guard, then await.

See also: [Tokio runtime guide](tokio-runtime-guide.md).

## `RwLock<T>`

Use `RwLock<T>` when many readers and few writers need shared threaded access.

`RwLock` allows:

- many read guards at once, or
- one write guard

Good uses:

- read-heavy shared caches
- runtime state where reads dominate writes
- shared config snapshots that are occasionally replaced

Avoid `RwLock<T>` when:

- writes are frequent
- critical sections are tiny and simple
- fairness matters and the OS policy is not acceptable
- a plain `Mutex<T>` would be simpler

`RwLock` can be slower and more subtle than `Mutex` under write contention.
Choose it for a real read-heavy workload, not because it sounds more concurrent.

## Atomics

Use atomics for small thread-safe counters and flags.

Good uses:

- test concurrency counters
- cancellation flags
- progress counters
- "has work" booleans

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

let count = AtomicUsize::new(0);
count.fetch_add(1, Ordering::Relaxed);
```

Use the weakest ordering that is correct. For simple independent counters,
`Relaxed` is often enough. For coordination between data and flags, be more
careful; prefer channels, locks, or existing concurrency primitives if unsure.

Atomics are not general shared mutable state. They are low-level synchronization
tools.

## `OnceCell<T>` and `OnceLock<T>`

Use once cells for values initialized at most once.

Use `OnceCell<T>` for single-threaded code. Use `OnceLock<T>` for thread-safe
code and `static`s.

Good uses:

- compiled regexes in tests and parsers
- caches behind `&self`
- process-wide immutable configuration
- lazy expensive setup

```rust
use std::sync::OnceLock;

static MESSAGE_ID_RE: OnceLock<regex::Regex> = OnceLock::new();

fn message_id_re() -> &'static regex::Regex {
    MESSAGE_ID_RE.get_or_init(|| regex::Regex::new(r"<[^>]+>").unwrap())
}
```

Use `LazyCell<T, F>` or `LazyLock<T, F>` when the initializer is fixed and
should run on first deref.

Use `OnceCell` / `OnceLock` instead of `Mutex<Option<T>>` when the state is
write-once.

## `Cow<'a, T>`

Use `Cow` when data is usually borrowed but sometimes must become owned.

`Cow` means clone-on-write:

- `Cow::Borrowed(&T)` avoids allocation
- `Cow::Owned(T::Owned)` owns data
- `to_mut()` clones only if currently borrowed

Good uses:

- text normalization
- path normalization
- APIs that may return borrowed or transformed data
- avoiding unconditional clones in parsers/renderers

```rust
use std::borrow::Cow;

fn normalize_subject(subject: &str) -> Cow<'_, str> {
    let trimmed = subject.trim();
    if trimmed.contains('\r') {
        Cow::Owned(trimmed.replace('\r', ""))
    } else {
        Cow::Borrowed(trimmed)
    }
}
```

Avoid `Cow` when:

- ownership is always required
- borrowing is always enough
- the type would make public APIs harder to understand
- serialization/protocol boundaries should stay owned and simple

`Rc::make_mut` and `Arc::make_mut` are also clone-on-write tools when shared
ownership already exists.

## `Pin<P>`

Use `Pin` when a value must not move after being placed.

Pinning is about address stability, not immutability. `Pin<P>` wraps a pointer
and pins the pointee. Common forms:

- `Pin<Box<T>>`
- `Pin<&mut T>`
- `Pin<Box<dyn Future<Output = T> + Send>>`

Good uses:

- futures and async trait object adapters
- self-referential structs
- intrusive data structures
- APIs that require stable addresses

Example: boxed async callback.

```rust
use std::future::Future;
use std::pin::Pin;

type TokenFuture = Pin<Box<dyn Future<Output = Result<String, AuthError>> + Send>>;
type TokenFn = Box<dyn Fn() -> TokenFuture + Send + Sync>;
```

Avoid `Pin` when:

- you only need heap allocation
- you only need shared ownership
- the value can move safely
- you are not prepared to reason about projection and `Unpin`

If writing pinned structs, prefer a battle-tested projection crate such as
`pin-project` rather than hand-rolling unsafe projection.

## Trait object pointers

Trait objects combine dynamic dispatch with a pointer.

Common choices:

| Type | Use when |
|---|---|
| `&dyn Trait` | borrowed dynamic dispatch, no ownership |
| `Box<dyn Trait>` | one owner owns implementation |
| `Arc<dyn Trait + Send + Sync>` | shared implementation across threads/tasks |
| `Pin<Box<dyn Future<Output = T> + Send>>` | erased async future |

Use trait objects at boundaries where runtime polymorphism is real:

- provider adapter seams
- test doubles
- callback hooks
- erased future return types when generics are impractical

Prefer generics when:

- the implementation type is known by the caller
- performance-sensitive code benefits from static dispatch
- object safety would distort the trait
- only one concrete implementation exists in the real path

Do not use `Box<dyn Trait>` just to avoid naming a concrete type inside a small
function.

## Raw pointers and `NonNull<T>`

Raw pointers are not ordinary smart pointers. They have no automatic liveness,
aliasing, or cleanup guarantees.

Use raw pointers only for:

- FFI
- interfacing with C APIs
- implementing a safe abstraction
- intrusive data structures
- performance-critical low-level internals where references cannot express the
  invariant

Rules:

- keep unsafe blocks small
- document safety invariants beside the unsafe code
- expose a safe API around unsafe internals
- prefer `NonNull<T>` when null is invalid
- use `PhantomData<T>` when ownership/drop/lifetime semantics must be expressed
- never create references from raw pointers unless aliasing and validity are
  proven

If a safe standard-library type can express the ownership, use it.

## `UnsafeCell<T>`

`UnsafeCell<T>` is the primitive that makes interior mutability possible. Types
such as `Cell`, `RefCell`, `Mutex`, and atomics build safe APIs around it.

Do not use `UnsafeCell<T>` directly unless implementing a new synchronization or
interior-mutability abstraction. If you do, the type must clearly document:

- aliasing rules
- thread-safety rules
- when mutation can happen
- how data races are prevented
- what API keeps callers safe

Most `mxr` code should never mention `UnsafeCell`.

## Common combinations

| Combination | Meaning | Use |
|---|---|---|
| `Box<T>` | one owner, heap | recursive values, large variants |
| `Box<dyn Trait>` | one owner, dynamic dispatch | pluggable implementation |
| `Rc<T>` | many owners, one thread, read-only | local graph/shared immutable data |
| `Rc<RefCell<T>>` | many owners, one thread, mutable | rare local shared mutation |
| `Rc<Weak<T>>` pattern | non-owning back edges | trees/graphs |
| `Arc<T>` | many owners, threaded, read-only | daemon/service handles |
| `Arc<Mutex<T>>` | many owners, threaded, exclusive mutation | short shared critical sections |
| `Arc<RwLock<T>>` | many owners, threaded, read-heavy mutation | shared read-heavy caches |
| `Arc<dyn Trait + Send + Sync>` | shared polymorphic service | callbacks, shared adapters |
| `Arc<AtomicUsize>` | shared numeric state | counters/progress |
| `OnceLock<T>` | thread-safe write-once | statics, lazy globals |
| `Cow<'_, str>` | borrowed-or-owned text | parsers/renderers |
| `Pin<Box<dyn Future + Send>>` | erased async future | async callbacks/traits |

## mxr placement rules

### `crates/core`

Default:

- plain owned data
- typed IDs/newtypes
- `Vec`, `String`, `Option`, `Result`
- references in function parameters

Avoid:

- `Arc`/`Rc` in public model types
- provider-specific pointer policies
- `Mutex`/`RwLock` in core domain types
- `RefCell` in core business data

Reason:

`core` is the provider-agnostic language of the app. It should describe mail
truth, not runtime sharing mechanics.

### `crates/protocol`

Default:

- owned, serializable request/response payloads
- stable JSON-friendly shapes

Avoid:

- `Cow` in IPC payloads unless there is a measured reason
- `Arc`/`Rc`/locks in IPC types
- borrowed data in long-lived protocol values

Reason:

IPC is a boundary. Payloads should be clear to serialize, test, and evolve.

### Provider crates

Good uses:

- `Box<dyn Trait + Send + Sync>` for testable factories/clients
- `Arc<dyn Fn(...) + Send + Sync>` for shared hooks/password readers
- `Mutex` in fakes or test recorders
- `Pin<Box<dyn Future + Send>>` for erased async callback returns

Avoid:

- leaking provider pointer choices into `core`
- holding locks while doing network I/O
- `Rc` in provider code that may be used from daemon tasks

### `crates/sync`

Good uses:

- `Arc<Store>` or cloneable service handles shared by sync tasks
- atomics for concurrency tests/progress counters
- plain ownership inside one sync operation

Avoid:

- shared mutable state when a local accumulator works
- locks around database operations
- `Rc` in task-spawnable code

Reason:

Sync is orchestration. Prefer explicit dataflow over hidden shared mutation.

### `crates/daemon`

Good uses:

- `Arc` for daemon state shared across IPC handlers and loops
- `Arc<dyn Trait + Send + Sync>` for service seams
- short-lived locks for in-memory coordination

Avoid:

- `Rc` and `RefCell`
- holding `std::sync` guards across `.await`
- putting locks in request/response protocol types

Reason:

The daemon is concurrent. Anything shared across handlers must be `Send + Sync`
or deliberately kept local to one task.

### `crates/tui`

Default:

- one owned `App` state tree
- mutable methods taking `&mut self`
- borrowed render data

Use smart pointers only when:

- a local graph truly needs shared ownership
- a callback/lazy resource requires it
- data must be shared with a background task through a channel or `Arc`

Avoid:

- `Rc<RefCell<T>>` as a general state-management pattern
- sharing TUI state with daemon internals

Reason:

Ratatui rendering is already built around explicit state updates. Keep it
visible.

### Tests

Good uses:

- `OnceLock<Regex>` for compiled test regexes
- `Arc<Mutex<Vec<_>>>` for cross-thread captured calls
- `RefCell<Vec<_>>` for single-thread mocks
- `Box<dyn Trait>` for fakes behind trait boundaries

Avoid:

- introducing pointer patterns in production code only for tests
- broad `Arc<Mutex<Everything>>` fixtures

## Async-specific guidance

Tokio changes where pointer choices matter.

Spawned tasks usually require:

- owned data
- `Send`
- `'static`

That often means `Arc<T>`:

```rust
let store = Arc::clone(&state.store);
tokio::spawn(async move {
    store.run_maintenance().await
});
```

But do not reach for `Arc` automatically. If the task can receive an owned value
or a narrow command over a channel, prefer that.

Lock rules:

- use `std::sync::Mutex` for short, non-async critical sections
- use `tokio::sync::Mutex` only if a guard must live across `.await`
- prefer dropping the guard before `.await`
- prefer channels for ownership transfer and background workers

Bad:

```rust
let mut cache = cache.lock().unwrap();
provider.fetch().await?;
cache.insert(key, value);
```

Better:

```rust
let value = provider.fetch().await?;

let mut cache = cache.lock().unwrap();
cache.insert(key, value);
```

## Performance notes

Smart pointers are explicit trade-offs.

Costs:

- `Box<T>` allocates
- `Rc<T>` updates non-atomic ref counts
- `Arc<T>` updates atomic ref counts
- `RefCell<T>` checks borrows at runtime and can panic
- `Mutex<T>`/`RwLock<T>` can block and contend
- `Cow<'_, T>` can hide a clone at `to_mut`
- trait objects use dynamic dispatch
- `Pin` adds API complexity

None of these costs are automatically bad. They are bad when the capability is
not needed.

Do not micro-optimize pointer choices without evidence. Do avoid unnecessary
shared ownership in hot loops and protocol/domain types.

## API design rules

### Parameters

Prefer borrowed parameters:

```rust
fn index_message(message: &Message) -> Result<()>;
fn parse_labels(labels: &[Label]) -> Result<Vec<LabelId>>;
fn render_html(html: &str) -> String;
```

Accept smart pointers only when the function needs that policy:

```rust
fn spawn_sync(store: Arc<Store>) -> JoinHandle<Result<()>>;
fn install_callback(callback: Arc<dyn Fn(Event) + Send + Sync>);
```

Do not accept `&Arc<T>` unless the function specifically needs to clone or
inspect the `Arc`. Usually accept `&T`.

### Return values

Return owned values when crossing boundaries:

```rust
fn list_messages() -> Result<Vec<MessageSummary>>;
```

Return smart pointers when the caller needs the policy:

```rust
fn shared_store(&self) -> Arc<Store>;
fn boxed_provider(config: ProviderConfig) -> Box<dyn MailSyncProvider + Send>;
```

### Struct fields

Do not put smart pointers in fields by default. The field should answer a real
ownership question:

- `Box<T>`: this field owns heap data
- `Arc<T>`: this field shares a long-lived service
- `Mutex<T>`: this field protects mutable state
- `OnceLock<T>`: this field initializes once

If the field is just data, keep it as data.

## Anti-patterns

### `Arc<Mutex<T>>` as architecture

Bad sign:

- every component receives the same giant `Arc<Mutex<AppState>>`
- functions lock it, mutate unrelated parts, then call async work
- tests need lock choreography

Better:

- pass narrow handles
- split state by responsibility
- use channels for background ownership
- keep DB/search as the shared source of truth

### `Rc<RefCell<T>>` as a borrow-checker escape hatch

Bad sign:

- code works only because borrow conflicts moved to runtime
- panics are possible during ordinary flows
- ownership cycles are hard to see

Better:

- redesign ownership
- use explicit parent owns child
- use IDs instead of pointers
- use `Weak` for back-references

### Boxing to hide large types everywhere

Bad sign:

- `Box<T>` appears without recursion, trait object, pinning, or measured move cost

Better:

- keep `T`
- use modules/type aliases for readability
- measure before heap-allocating for move cost

### Public `Arc` in domain models

Bad sign:

- `Message`, `Thread`, `Account`, or IPC payloads expose runtime ownership

Better:

- store owned data
- pass IDs
- let daemon/sync choose sharing internally

### Lock held while calling unknown code

Bad sign:

- lock guard lives while calling provider, callback, script, network, or `.await`

Better:

- copy/take required data
- drop guard
- call unknown code
- reacquire only to write result

## Review checklist

Before approving a smart pointer change, answer:

1. What capability is missing from plain ownership or borrowing?
2. Is the pointer choice the narrowest type that provides it?
3. Does this pointer leak runtime policy across a crate boundary?
4. Can this introduce cycles, deadlocks, panics, or hidden clones?
5. Are lock/borrow guard scopes short and obvious?
6. Does async code avoid holding blocking guards across `.await`?
7. Would IDs, ownership transfer, or a channel be simpler?
8. Is this in `core` or `protocol` where smart pointers should be rare?
9. Is dynamic dispatch needed, or would generics be clearer?
10. Are unsafe pointer invariants documented if raw pointers appear?

## Short recommendations

- Use `T` until ownership says otherwise.
- Use `&T`/`&mut T` for temporary access.
- Use `Box<T>` for recursion, trait objects, pinning, or deliberate heap
  ownership.
- Use `Rc<T>` only in single-threaded local ownership graphs.
- Use `Arc<T>` for daemon/provider/sync sharing across tasks.
- Use `Weak<T>` for back-references.
- Use `Cell<T>` before `RefCell<T>` for small copyable state.
- Use `RefCell<T>` only when single-thread runtime borrow checks are acceptable.
- Use `Mutex<T>` for short exclusive threaded mutation.
- Use `RwLock<T>` only for real read-heavy shared state.
- Use `OnceLock<T>`/`LazyLock<T>` for thread-safe one-time initialization.
- Use `Cow<'_, T>` when allocation is conditional.
- Use `Pin<P>` only for address-sensitive values.
- Use raw pointers only behind safe abstractions or FFI.

# Idiomatic Rust Rubric for mxr

> The standard every crate in this workspace is audited and refactored against.
> Each dimension scores **0–3**. A crate is "prod-idiomatic" at **≥ 2** on every
> dimension with **no 0s**. The rubric is enforced mechanically where possible
> (see [Lint policy](#lint-policy)) and by review where it isn't.

This is a working spec, not generic advice. It is tuned to the facts of this
repo:

- The CI gate is `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
  **Any** warning fails CI. Therefore a lint is only enabled in
  `[workspace.lints]` once its violation count is driven to **zero** — that is
  what makes the rule self-enforcing instead of aspirational.
- Crate dependency rules in `.agents/skills/mxr-development/SKILL.md` are part of "idiomatic" here: `core` is a
  leaf, providers never import `store`/`sync`, clients (`tui`/`web`) never import
  `daemon`. Architectural seams are Cargo seams.
- `unwrap_used`, `panic`, `todo` are already `warn` (→ deny in CI). Convention:
  **no `.unwrap()` anywhere** — use `.expect("invariant: …")` for
  statically-impossible cases, `?`/`let-else`/combinators everywhere else.

---

## How we change code against this rubric (TDD discipline)

Per-finding loop, non-negotiable:

1. **Pin behavior.** Identify (or write) a test that exercises the behavior the
   refactor must preserve. For pure mechanical refactors (rustfix-class), the
   "test" is: existing crate tests + `cargo build` + `clippy` all green *before*.
2. **Green before.** Run the test(s); confirm green.
3. **Refactor.** Smallest change that satisfies the rule.
4. **Green after.** Re-run the same test(s); confirm still green. No behavior
   change ⇒ identical results.
5. **Lock it in.** Where the fix corresponds to a clippy lint, enable that lint
   in `[workspace.lints.clippy]` so it can't regress.

New behavior tests must pass the project test-quality gate (see
`test-quality-rubric` skill): assert exact spec-derived values, cover an error /
boundary / empty case, survive an implementation swap, and fail if the function
body is deleted. No tautologies, no mock-passthrough, no snapshot-only.

---

## The 12 dimensions

### 1. Error handling — *no panics in non-test code*
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| `.unwrap()`/`panic!`/`todo!` on fallible paths | `.expect()` without an invariant message; `Error(String)` for everything | `?` + typed errors at boundaries; `.expect("invariant: …")` only for impossible cases | `thiserror` enums with `#[from]`/`#[source]`, `#[non_exhaustive]`; error variants matchable; `Err` paths tested |
- Libraries/internal crates use `thiserror`; only the binary boundary (`daemon` top-level, CLI `main`) may use `anyhow`.
- A daemon panic kills background sync for **every** client. Hardening config/IO/parse paths to return `Result` is P1.
- Don't stringly-type (`.map_err(|e| e.to_string())`): it erases the source chain and downcasting. Prefer `#[error(transparent)]` + `#[from]` + `?`.

### 2. Ownership & borrowing — *no needless clones*
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| `.clone()` in hot/render/sync loops to dodge borrowck | scattered `.to_string()`/`.to_owned()` | borrows by default; clone only when ownership is genuinely needed | `Cow<'_, _>` for mostly-borrowed-occasionally-owned; `Arc<T>` for shared large structs cloned per-iteration |
- `&str` not `&String`; `&[T]` not `&Vec<T>`; `impl AsRef<Path>` for paths.
- Pass small `Copy` scalars by value, not `&u32`.

### 3. Iterators & functional style
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| index loops with manual accumulators | `for` + `push` where `map`/`filter` fits | iterator chains; `filter_map`/`find_map`/`flat_map` | no needless `collect`; `sum`/`fold`/`try_fold`; `collect::<Result<_,_>>()?` to fail-fast |

### 4. Pattern matching & control flow
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| nested `match` pyramids; `_ =>` hiding new enum variants | `match` returning `true`/`false` | `if let`/`let-else`/`matches!`; explicit arms for exhaustive domain enums | `Option`/`Result` combinators (`map_or`, `is_some_and`, `ok_or`, `zip`, `inspect_err`) over manual `match` |
- For domain enums (`Screen`, `Action`, `LabelKind`, `DeliveryStatus`) prefer **explicit arms over `_`** so adding a variant is a compile error, not a silent fall-through.

### 5. Type system & newtypes
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| `bool`/`String` for state; primitive IDs | stringly enums matched on `.as_str()` | typed IDs; enums for finite states | `From`/`TryFrom`; `Default` derived; primitive obsession removed (e.g. `WeekendDay` enum, not `"saturday"`) |

### 6. API design (Rust API Guidelines)
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| `pub` everywhere; `get_` getters; many-bool fns | owned params where borrow suffices | `&str`/`&[T]`/`impl IntoIterator` params; getters without `get_`; `as_`/`to_`/`into_` cost conventions | returns `impl Iterator`; `#[must_use]` on builders/previews; builder for many-optional-field ctors |

### 7. Async & concurrency (Tokio)
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| `std::Mutex` guard held across `.await`; blocking calls in async | lock in `match` scrutinee held across arms; sequential `.await` in a loop over independent items | guards scoped/dropped before await; `spawn_blocking` for blocking IO | heavy CPU on `rayon` via `oneshot`; `buffer_unordered`/`JoinSet` for fan-out; bounded channels; `select!` + cancellation |

### 8. Library idioms (sqlx / tantivy / ratatui / axum / reqwest)
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| `INSERT OR REPLACE` on rows with FK dependents; per-request `reqwest::Client`; state mutated in `draw` | per-doc tantivy `commit`; N+1 queries | `ON CONFLICT DO UPDATE`; one shared `Client`; `StatefulWidget` state out of draw | compile-time `query!`; batched commits; `Url`/`.query()` not string concat; no allocation in render path |

### 9. Module, visibility & feature hygiene
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| god modules; `pub` internals; `#[path]` pseudo-crates | broad `use foo::*`; deep import paths | `pub(crate)` by default; curated root re-exports | additive feature gates; crate-boundary rules upheld; focused `prelude` |

### 10. Naming & conventions
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| non-`snake_case`; `get_x`; misleading `to_`/`into_` | inconsistent | API-Guidelines naming throughout | iterator naming (`iter`/`iter_mut`/`into_iter`), predicate `is_`/`has_` |

### 11. Documentation
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| public API undocumented | sparse | `///` on public items, `//!` per module | examples on key APIs; `core`/`protocol` doc-complete |
- For an application, `missing_docs`/`missing_errors_doc` are **not** enforced
  (allowed). Doc-completeness is required only for the contract crates `core`
  and `protocol`.

### 12. Tests as specification (ties to `test-quality-rubric`)
| 0 | 1 | 2 | 3 |
|---|---|---|---|
| tautologies; mock-passthrough; snapshot-only | happy-path-only | behavior tests through public API | adversarial: boundary + error + empty; survives implementation swap; fails if body deleted |

---

## Lint policy

Two buckets. The split is the operational form of dimensions 1–10.

### Enforced (driven to zero, then `warn` in `[workspace.lints.clippy]` → deny in CI)

High-signal, low-false-positive, behavior-preserving idiom lints:

`uninlined_format_args`, `map_unwrap_or`, `redundant_closure_for_method_calls`,
`use_self`, `manual_string_new`, `default_trait_access`,
`semicolon_if_nothing_returned`, `manual_let_else`, `unnested_or_patterns`,
`needless_collect`, `or_fun_call`, `implicit_clone`, `match_same_arms`,
`derive_partial_eq_without_eq`, `redundant_clone`,
`significant_drop_in_scrutinee`.

Already enforced: `unwrap_used`, `panic`, `todo` (warn → deny in CI);
`unsafe_code`, `unused_must_use` (deny).

### Allowed (real noise for a binary, not defects)

`missing_errors_doc`, `missing_panics_doc`, `must_use_candidate`,
`missing_const_for_fn`, `doc_markdown`, `module_name_repetitions`,
`too_many_lines`, `too_long_first_doc_paragraph`, `similar_names`,
`cast_possible_truncation`, `cast_sign_loss`, `cast_precision_loss`,
`cast_lossless`, `cast_possible_wrap` (cast lints reviewed case-by-case;
terminal `u16` coords and tantivy score math legitimately trip them),
`wildcard_imports` (the handler/`pub(crate) use` re-export pattern is
deliberate), `redundant_pub_crate` (nursery, noisy).

The whole `pedantic`/`nursery` **groups are not enabled** — they are mined for
the high-signal lints above, then cherry-picked. (Per Rust/Clippy guidance the
nursery group is unstable and should never be enabled wholesale.)

---

## Deliberately deferred (documented, not executed in this pass)

These are real findings but are high-risk / high-churn and change shapes beyond
"behavior-preserving idiom cleanup." They warrant their own reviewed PRs:

- **Typed errors over stringly errors** (`.map_err(|e| e.to_string())` — 355+ in
  `daemon`, 45+ in `sync`; `core::error` String payloads). Changes the IPC error
  surface; needs a versioned, test-backed migration.
- **God-module / dispatch boilerplate** in `daemon` (`handler/mod.rs`,
  `cli/mod.rs` ~200-arm dispatch). Pure reorg; large diff, no behavior change,
  high merge-conflict cost.
- **`unused_async`** (20): removing `async` ripples to every `.await` call site.
- **TUI render-path caching** (`thread_message_blocks` recomputed per frame):
  perf, needs careful frame-state validation.
- **once_cell `Lazy` → std `LazyLock`** (`non_std_lazy_statics`, 29): modernization;
  touches many files, project currently standardizes on `once_cell`.

---

## Sources

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html)
- [Clippy lint reference](https://doc.rust-lang.org/stable/clippy/lints.html)
- [Tokio: shared state](https://tokio.rs/tokio/tutorial/shared-state) ·
  [Alice Ryhl: what is blocking](https://ryhl.io/blog/async-what-is-blocking/)
- [sqlx](https://github.com/launchbadge/sqlx) ·
  [SQLite UPSERT vs INSERT OR REPLACE](https://www.sqlite.org/lang_upsert.html)
- [ratatui StatefulWidget](https://docs.rs/ratatui/latest/ratatui/widgets/trait.StatefulWidget.html) ·
  [axum State](https://docs.rs/axum/latest/axum/extract/struct.State.html) ·
  [reqwest Client](https://docs.rs/reqwest/latest/reqwest/struct.Client.html)

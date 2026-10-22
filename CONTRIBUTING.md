# Contributing to mxr

## Dev setup

```bash
git clone https://github.com/planetaryescape/mxr
cd mxr
cargo build -p mxr
scripts/cargo-test -p mxr --test cli_help
```

If you need the docs site too:

```bash
cd site
npm ci
npm run build
```

## Non-negotiables

- local-first first
- SQLite is canonical state
- search index is rebuildable
- daemon is the system
- TUI and CLI are both clients
- web is also a client of daemon IPC
- provider-specific logic stays in adapter crates
- compose uses `$EDITOR`
- rules are deterministic before they are clever
- plain-text-first rendering wins over flashy rendering
- semantic search is an `mxr-platform` feature, not a core mail primitive
- semantic embeddings stay local
- semantic indexing uses real text extraction only; do not casually reintroduce OCR
- lexical exactness and semantic recall are different layers; do not blur them

## IPC boundaries

Classify every IPC addition before you add it:

1. `core-mail`
2. `mxr-platform`
3. `admin-maintenance`
4. `client-specific`

Rules:

- Core mail/runtime should stay boring and stable.
- mxr platform capabilities are first-class. Do not bury them as misc.
- Admin surfaces stay in IPC, but separate them mentally and in code from the mail contract.
- Client-specific shaping stays in clients.
- The daemon serves reusable truth/workflows, not screen payloads.

## Crate boundaries

Keep these intact:

1. `mxr-core` depends on nothing internal.
2. `mxr-protocol` depends only on `mxr-core`.
3. Provider crates depend on `mxr-core` plus shared mail utility crates only (`mail-parse`, `outbound`).
4. `mxr-store` and `mxr-search` depend only on `mxr-core`.
5. `mxr-sync` depends on `core + store + search`.
6. `mxr` (daemon crate) is the integration point.
7. `mxr-tui` and `mxr-web` are client crates. They may use local utility crates (`config`, `compose`, `reader`, `mail-parse`), but never daemon/store/search/sync/provider crates.
8. Do not use `#[path]` includes to simulate crate boundaries.

Semantic boundary:

- `mxr-semantic` is the local dense-retrieval/runtime layer
- sync may persist semantic chunk data even when semantic retrieval is disabled
- embeddings are generated only when semantic is enabled or explicitly reindexed
- field-aware hybrid search semantics are intentional
- OCR is out of scope for active semantic indexing

Lifecycle guardrails:

- sync itself guarantees SQLite persistence + lexical freshness
- daemon post-sync work ingests semantic chunks for changed messages
- lexical startup repair is mandatory and rebuilds from SQLite
- semantic readiness is optional derived state; do not make core mail depend on it

Repo reality:

- The product/install/package surface is the repo-root package `mxr`.
- Internal crates under `crates/` are real workspace crates and are private by default (`publish = false`).
- The IMAP adapter depends on the published `mxr-async-imap` fork from crates.io; vendored source is not part of the workspace boundary model.
- Provider paths are `provider-gmail`, `provider-imap`, `provider-smtp`, and `provider-fake`.
- `crates/web` is a current client/bridge surface.

## Required checks

Run checks for the surface you changed. For Rust changes, update the floating
stable toolchain, inspect the affected packages, and run the scoped gate:

```bash
rustup update stable
scripts/pre-pr-rust-gate --list
scripts/pre-pr-rust-gate
scripts/cargo-test -p <changed-crate> --tests
cargo build -p mxr
```

The gate checks formatting, Clippy for touched crates, architecture boundaries
and dependency policy. CI runs the workspace checks, tests, SQLx offline check,
Rustdoc and builds when the changed paths require them. Use `--full` only to
reproduce a CI failure that cannot be narrowed to one crate.

On a shared agent workstation, queue heavy commands with
`agent-work run -- <command>` and keep workers at four or fewer. The dotfiles
Cargo wrapper already queues Cargo; do not nest another queue around it.
Use a separate target directory for each worktree.

For test-quality and web-facing changes, also run:

```bash
scripts/pre-pr-test-gate
```

That gate validates the test-quality audit script, runs strict test-quality
audit, and runs the web typecheck/lint/unit test suite.

Test helpers may use narrow `#[expect(clippy::...)]` annotations when
panic/unwrap/default-mutation keeps fixture failures clearer. Keep the
annotation on the smallest test module or function that needs it, and include
the reason.

If you touch the docs site:

```bash
cd site
npm ci
npm run build
```

## Real-system verification

Green unit tests are not enough. For user-facing work, also test the real flow:

```bash
cargo run -- daemon --foreground
mxr status
mxr search "label:inbox"
mxr doctor --check
```

If you changed rules, exports, labels, notify, events, or logs, exercise the matching CLI surface too.

If local dev state gets messy, prefer the built-in reset flow over manual file hunting:

```bash
mxr reset --hard --dry-run
mxr burn --dry-run
```

That wipes rebuildable local runtime state only. It does not delete `config.toml` or system credentials by default.

## Measuring the classifier

`mxr classify eval` grades how mxr sorts mail against labelled scenarios
(blueprint 32). It is read-only and local, and changes no classification.
There are two sets:

| Set | Grades | Takes | Run it |
|---|---|---|---|
| `--set quick` (the default) | the 184 labelled emails of the hand-written demo mail and the near-miss twins | seconds | after every change to sorting |
| `--set full` | every labelled email in the mailbox: on the 50,000-message demo, the hand-written mail plus all of the generated mail (about 38,700 emails across both accounts) | tens of seconds once the demo is seeded | before you open a PR that can move an answer; CI runs it |

```bash
mxr demo
mxr classify eval --set quick
mxr classify eval --set full
mxr classify eval --set full --format json    # every mismatch
```

For every stored email that a scenario labels, it compares three answers with
the label: who sent it (`person`, `list`, `machine`, `screened_out`), its home
(`messages`, `updates`, `reading`, `spam`, `screened_out`) and whether a record
was made. It prints the accuracy per question.

What an email asks of you is not a yes or no, and it has no accuracy line:
nearly every email asks nothing, so accuracy would measure the true negatives.
Each label (label schema v2) lists the items the email should make, each with a
kind of ask from blueprint 37 (`pay`, `fix_payment`, `sign`, `verify`, `reply`,
`send`, `provide`, `upload`, `book`, `attend`, `renew_or_cancel`, `register`,
`contact`, `other`), a direction (`asks_you`, or `you_promised` for your sent
mail), the counterparty, the amount and the due words, plus whether a reply is
owed. The eval matches each email's to-do rows to its expected items by kind and
direction (the table that maps today's to-do kinds to these is
`translate` in `crates/daemon/src/handler/classify_eval_asks.rs`) and reports:

- true positives, false positives, false negatives, precision and recall per
  kind of ask, and for all kinds together;
- exact match for the amount (value and currency), the due date and the
  counterparty on the matched items, split into missing, wrong and spurious.
  A stated date is checked for presence, because dates in the corpus follow the
  seed clock; clock-free words ("Friday") are checked against the row's words;
- duplicates (two rows for one expected item) and merges (one row standing for
  two accounts of one sender, which today's dedup key does);
- twin-pair accuracy, where both members of a pair must be right on what they
  ask, and the same over the pairs whose members ask different things;
- reply owed, graded as its own question against `needs_reply`, the
  directed-request signal Your turn is built from. It reads one message, not the
  thread, so it cannot see that you answered;
- slices by sender kind (person, organisation, platform, list), locale (en-GB,
  en-ZA, en-US), template family and visible against held-out phrasing.

The quick set lists every mismatch with the scenario id, provider id, expected
answer, actual answer and the rule that decided it. The full set can be wrong
thousands of times, so its table groups mismatches into clusters (same
scenario, question, expected and actual answer), biggest first, with one
example each; `--format json` has every one. Home is the arrivals line's
destination under today's rules; mail with no arrivals row is placed live by
the same rules. Labels state what mxr should do, not what it does today, so a
gap shows up as a mismatch.

The full set also reports held-out accuracy on its own line, per question.
Each family of generated mail (below) keeps a fixed share of its phrasings out
of the mail you read while changing a judge, in
`crates/provider-fake/src/fixtures/scenarios/held_out.rs`. If held-out accuracy
falls behind the overall figure, the change taught a judge the generator's
wording, not the mail. Do not read `held_out.rs` while you change a judge, and
do not tune against it.

### Where the labels live

They are in `crates/provider-fake/src/fixtures/scenarios/`:

- `curated.rs` states the outcome the hand-written demo fixtures describe in
  prose.
- `twins.rs` builds near-miss pairs that differ in the one fact that should
  change the answer, and `twins_asks.rs` the pairs for every shape blueprint 37
  lists (marketing with pay words against an invoice, a Direct Debit offer
  against a notice, a decline resolved against a current one, a credit note
  against a balance, a final reminder with and without money owed, a platform
  announcement against a personal invitation, your request against your
  promise, a forwarded letter against a direct request, two accounts from one
  sender, pounds, euros and rand, a group ask against a copied one against an
  ask to you alone, a conditional ask against an unconditional one, a
  pleasantry against a question, an answered ask against an open one) and
  en-ZA phrasings. `held_out.rs` also holds two whole template families
  (a school's consent form, a vet's balance) that no other scenario shares,
  reported apart as held-out. The twins are not in the 50,000-email demo,
  because they would shift its message numbering and the counts its tests
  read, so the daemon test grades them (below).
- `leak_guard.rs` is the leak guard: a baseline that reads only the surface of
  an email (its subject words with digits masked, its sender and a length
  bucket), trained on every other labelled email, must not get both members of
  more than 40% of the twin pairs whose answers differ. If it does, the corpus
  gives its answer away and a judge could score well without reading the mail.
- `bulk/` builds the rest of the generated demo from scenario families. A
  family is a kind of mail (a colleague's thread, a thread you were only copied
  on, an order, a bill, a booking, a newsletter, spam...). It says who writes
  at each message position of its thread and what the answers are there, so one
  thread can mix answers, and it has several phrasings of the same mail. Which
  family and phrasing a thread uses is a function of its number alone, so a
  page, a fetch and a label for any of the 50,000 emails are all recomputed
  from the index and nothing is held in memory.

An item's counterparty, amount and due words are what the email says, never
what the code finds: Review any label that rests on a judgement call. A date
written from the seed clock cannot be a label, so an item says only that a
dated deadline is stated (`Due::Stated`), or names clock-free words
(`Due::Words("Friday")`).

Dated asks need care. A to-do is shown only while its deadline is open, so a
family that asks for something by a date puts one thread in thirty in the last
hours (the ask is live: a to-do) and the rest at least two months back (the
deadline passed before the mail was found: no to-do, as
`todo-event-already-over` says). The label follows the dating.

### Tests

```bash
scripts/cargo-test -p mxr-provider-fake --lib scenarios       # labels, families, seed clock
scripts/cargo-test -p mxr --lib -- handler::tests::classify_eval
```

The second runs the quick-set snapshot and a few full-set checks on a small
generated demo. It syncs the mail, runs the post-sync classification
(to-dos, records, deliveries, arrivals placement) and snapshots the
per-question counts and the sorted mismatch list. A change to sorting shows up
as a snapshot diff: review it and accept it only when every changed answer is
intended. The demo instance also seeds screener decisions and rules, so its
numbers can differ slightly from the snapshot.

The whole 50,000-message demo is one more test, marked `#[ignore]` so the
workspace test jobs skip it. CI runs it in the "Classifier Eval (full set)"
job, on pull requests that touch the generator, the classifier or the eval and
on every push to `main`, and snapshots the summary: per-question accuracy,
held-out accuracy, counts and the mismatch clusters (never one line per
email). Run it locally only when you need to, because it takes a few minutes
and a lot of CPU:

```bash
agent-work run -- scripts/cargo-test -p mxr --lib -- --ignored \
  handler::tests::classify_eval_full::the_full_set_is_graded_on_the_whole_demo --nocapture
```

Accept a changed snapshot with `INSTA_UPDATE=always` on that run, and read the
diff first: it is the list of answers your change moved.

### The per-kind floor

`crates/daemon/src/handler/tests/classify_eval_floor.json` holds the precision
and recall each kind of ask reached, for the quick and the full set. The quick
snapshot test and the full-set test (which CI runs) fail when any kind falls
below its floor, so a change cannot lose recall on one kind behind a gain on
another. A kind with no expected items, or nothing found, has no floor for the
number that is undefined.

The floor only goes up. After a change that improves a kind, raise it:

```bash
MXR_RAISE_CLASSIFY_FLOOR=1 scripts/cargo-test -p mxr --lib -- handler::tests::classify_eval::
MXR_RAISE_CLASSIFY_FLOOR=1 agent-work run -- scripts/cargo-test -p mxr --lib -- --ignored \
  handler::tests::classify_eval_full::the_full_set_is_graded_on_the_whole_demo
```

That rewrites the file with the better of the old and the measured value for
every kind. Review the diff. To lower a floor on purpose (a label was wrong, a
kind was retired), edit the file by hand and say why in the commit.

### Recorded model responses

No eval question reads a language model yet: the LLM is off by default. When
one does, every model call the eval makes goes through
`mxr_llm::recorded::RecordedProvider`, which wraps any `LlmProvider` and keys a
call by model, prompt version, schema version and a hash of the request
(messages, `max_tokens`, temperature). It is for evals and tests; the daemon
never builds it.

- Replay is the default, and what CI always does. A call with no recording is an
  error that names the key (`no recording for model / prompt p / schema s /
  input hash`); it never reaches a live model.
- Record is opt-in: set `MXR_LLM_RECORD=1` and build the provider with
  `RecordedProvider::from_env`, which then calls the live provider you give it
  and writes one file per key, in a fixed field order, into the recordings
  directory the test names. The same inputs give the same files.

Re-record only when a prompt, a schema or a model changes, and review the claim
diff: the recording holds the request beside the answer, so the diff shows which
claims moved. Check in the recordings with the change that needed them.

### Adding a scenario

State the answer a person would want, not today's answer, and never shape a
label to match what the code does. Put the reasoning in a `Review` if the
label is a judgement call. Change `LABEL_SCHEMA_VERSION` when the questions
themselves change. A new family needs a phrasing for each message position,
at least two held-out phrasings in `held_out.rs`, and no date written into its
text: write `{date}` or `{due}`, which come from the email's own date, because
`bulk_dates_follow_the_seed_clock` fails a date that stays fixed.

## Running the daemon

`mxr daemon --foreground` is the canonical manual-test entrypoint. Keep it running in one terminal, then use a second terminal for CLI smoke tests like `mxr status`, `mxr sync --status`, `mxr search`, and the mutation flow you changed.

## PR process

1. Fork the repo.
2. Create a focused branch from `main`.
3. Keep the diff surgical.
4. Run the required checks.
5. Open a PR with enough context to reproduce and verify.

CI must pass before review or merge.

## Rules for changes

- Keep blast radius small.
- Do not refactor adjacent code unless the task requires it.
- Add tests for new behavior.
- Prefer integration coverage over mock-heavy unit tests.
- Do not wire a daemon feature for only one client surface when both TUI and CLI need it.
- If you touch semantic search, update both code and docs. The behavior is subtle enough that stale docs become bugs.

## Adapter work

Adapter crates are replaceable by design.

When adding or changing an adapter:

1. Keep provider-specific code inside the adapter crate.
2. Map into the mxr internal model, not the other way around.
3. Validate against fake/conformance coverage.
4. Document any provider semantic mismatch honestly.

## Docs and release hygiene

- Update `README.md` for changed user-facing behavior.
- Update `site/` docs and sidebar navigation for new commands or workflows.
- Update architecture/blueprint docs when code changes invalidate older assumptions.
- Keep `.github/workflows/` aligned with the actual build and release process.
- Keep issue templates and bug-report flow current.

### Regenerate and verify the docs site

Use Node 24, matching the Docs Build CI job:

```bash
cd site
npm ci
npm run build
```

The build generates CLI pages from committed help snapshots, runs the docs
validator tests and content checks, builds the site, then creates `llms.txt`
exports. Edit `site/scripts/generate-cli-reference.mjs` for handwritten CLI
examples; never edit generated pages under `reference/cli/`.

The OpenAPI generator uses Cargo when available and otherwise retains
`site/public/openapi.json`. For prose-only work, that committed spec is enough.
When changing the HTTP API, build `dump_openapi_spec` and run the built binary
directly into the output file; queue status text must never enter the JSON.

Check `info.version` against the workspace version in `Cargo.toml` even when
the routes have not changed. A successful Node-only build can retain an older
spec. Reference generation also preserves handwritten examples: run those
against an isolated profile, including their selection semantics. For example,
`from:alice@example.com` matches that full address; `from:alice` does not search
sender display names.

Add user guides to `site/astro.config.mjs` and check their rendered links and
anchors. `npm run validate` checks known content errors and schema invariants;
it is not a general broken-link checker. After merging, verify the changed
pages on mxr.sh. A docs-only change deploys the site without a binary release.

## Architecture pointer

Start with [ARCHITECTURE.md](ARCHITECTURE.md), then use the blueprint and [implementation journey](docs/implementation-journey.md) for phased history and maintainer context.

## Good first issues

Look for the [`good first issue`](https://github.com/planetaryescape/mxr/labels/good%20first%20issue) label if you want a bounded starting point.

## Useful references

- [ARCHITECTURE.md](ARCHITECTURE.md)
- `docs/blueprint/`
- `docs/implementation-journey.md`
- `docs/blueprint/15-decision-log.md`
- `docs/blueprint/17-release-pipeline.md`
- `docs/blueprint/18-bug-reporting.md`

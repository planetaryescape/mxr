# Local builds must preserve other workflows

Status: fixed in this repository by the PR that removed the global reaper.

Run the runner regression check:

```sh
bash scripts/cargo_test_runner_test.sh
```

`scripts/cargo-test` forwards arguments and exit status, keeps `target-cli`
separate from rust-analyzer, and defaults Cargo and libtest to four workers.
It does not kill Cargo processes belonging to other worktrees. On BK's Mac,
the dotfiles Cargo proxy submits heavy commands to one Pueue queue, with two
jobs, four workers each, nice priority 10 and a macOS memory-pressure gate.
Other machines can use the runner without installing Pueue.

## Handover — 1 October 2026

The review branch `fix/shared-build-queue` is based on freshly fetched
`origin/main` at `5b42b6a8252983ecdde0e2d46d054357bace1291`.
The unsafe global reaper existed at that ref. The prepared change removes it
and adds `scripts/cargo_test_runner_test.sh`. The same runner change is
installed in the primary checkout and the observed `provenance` and `owed`
worktrees; their feature work remains intact. Other old checkouts may still
contain the reaper and should receive this fix before running their script.

Validation: the shell runner regression and ShellCheck pass. Dotfiles queue
integration checks prove two slots across three directories, quoting, cwd,
environment, nice priority, exit codes, queued/running cancellation,
client-death ownership, nested-call rejection and Cargo worker validation.
A live `cargo build -p mxr-core` on the review base completed through the
queue in 6.60 seconds. The full binary verification in the older primary
checkout was cancelled after confirming worker limits; no full-binary
build success is claimed. No Rust source changed. The host configuration
is documented in `~/.dotfiles/docs/agent-work.md`.

Next action: review the scoped diff, fetch/rebase, then commit and push the
MXR runner fix when repository publication is authorized. Do not include the
primary checkout's unrelated documentation or marketing work. The dotfiles
review worktree is `/tmp/build-resource-clones/dotfiles`; its changes are also
installed locally. No release or remote publication has occurred.

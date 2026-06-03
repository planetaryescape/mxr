---
id: task-004
title: Readability HTML-to-text in reader view (P0-3)
status: ready
phase: phase-002
depends_on: []
blocks: []
risk:
  level: medium
  blast_radius: medium
execution:
  executor_type: frontier_model
  lane: frontier
  preferred_model: pe-default-frontier
  skills:
    - build-and-fix
    - code-review
    - tdd
    - mxr
scope:
  allowed_paths:
    - crates/reader/**
    - crates/mail-parse/**
    - crates/daemon/src/handler/mailbox.rs
    - crates/daemon/src/commands/**
    - crates/tui/**
    - docs/implementation/triage-cli-gaps/**
  blocked_paths:
    - .env*
    - node_modules/**
    - .git/**
    - target/**
    - "*.db"
validation:
  test_commands:
    - cargo build -p mxr
    - scripts/cargo-test -p mxr-reader --lib
    - scripts/cargo-test -p mxr-reader --tests
    - scripts/cargo-test -p mxr-tui --tests
  success_criteria:
    - "`mxr cat <id>` (default reader view) on a message with text_plain=null renders readable plain text, not raw HTML."
    - "Raw markup remains available only via --view html / --raw; reader view never dumps unrendered HTML."
    - "Verified against the field-report repros: Tesco order email (was 50,309 chars raw HTML) and an HTML-only newsletter now return clean text."
    - "Uses a maintained HTML-to-text/readability crate rather than a hand-rolled stripper; tests cover an HTML-only fixture."
    - "SURFACE PARITY: the fix is in the shared `reader` crate, so CLI and TUI (crates/tui depends on reader) both render clean text — verify the TUI reader pane. The apps/web frontend renders HTML natively in-browser, so no apps/web change is needed (noted intentionally, not an omission)."
---

# Readability HTML-to-text in reader view

## Goal

Make "check contents" usable on modern mail. Verified in the field session: `mxr cat` reader
view returns raw HTML when `text_plain` is null (Tesco 50KB, Numan 209KB), which is unreadable
and contradicts the AGENTS.md "plain-text reader-first" invariant.

## Work

- In the reader path, when `text_plain` is absent, run an HTML->text readability pass to produce
  clean reader output.
- Prefer a battle-tested crate (e.g. an html2text/readability crate) over a custom stripper.
- Keep `--view html`/`--raw` returning original markup.
- Add a reader test with an HTML-only fixture asserting no tags / sane text.

## Out of scope

- Inline image/asset handling beyond current `--assets` behaviour.

## Source

docs/triage-session-feedback-2026-06-03.md — P0-3 (verified repro inline).

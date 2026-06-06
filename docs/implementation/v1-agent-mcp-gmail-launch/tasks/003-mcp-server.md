---
id: task-003
title: First-party MCP server on the daemon contract
status: accepted
phase: phase-002
depends_on:
  - task-001
blocks:
  - task-004
  - task-005
risk:
  level: high
  blast_radius: high
execution:
  executor_type: frontier_model
  lane: frontier
  preferred_model: pe-default-frontier
  skills:
    - build-and-fix
    - code-review
    - tdd
    - mxr
    - sdk
    - security-best-practices
scope:
  allowed_paths:
    - Cargo.toml
    - Cargo.lock
    - crates/mcp/**
    - crates/protocol/**
    - crates/daemon/src/**
    - crates/daemon/tests/**
    - crates/config/**
    - crates/test-support/**
    - docs/implementation/v1-agent-mcp-gmail-launch/**
  blocked_paths:
    - .env*
    - node_modules/**
    - .git/**
    - target/**
    - "*.db"
validation:
  test_commands:
    - cargo build -p mxr
    - scripts/cargo-test -p mxr --lib
    - scripts/cargo-test -p mxr-mcp --tests
    - scripts/cargo-test -p mxr --test cli_help
    - scripts/cargo-test -p mxr --test cli_journey
  success_criteria:
    - "A first-party MCP server ships as an mxr command/binary and speaks stdio MCP using the official rmcp Rust SDK unless impossible."
    - "MCP tools use daemon IPC/client code rather than provider-specific direct access."
    - "Tools cover v1 agent workflows: status, list/search, read message/thread, create draft/draft-assist, dry-run/preview mutations, and gated send."
    - "MCP requests are tagged as MCP source and pass through task-001 agent permission enforcement."
    - "MCP schemas/results are structured, stable, and avoid secrets/full bodies unless the tool explicitly reads body content."
    - "Tests or smoke fixtures prove tool listing and at least read, draft, mutation dry-run, and send-blocking behavior."
---

# First-party MCP server

## Goal

Ship a real mxr MCP server for v1.

## Library rule

Do not hand-roll MCP protocol handling. Prefer `rmcp`, the official Rust SDK:

- https://rust.sdk.modelcontextprotocol.io/
- https://github.com/modelcontextprotocol/rust-sdk
- https://modelcontextprotocol.io/docs/sdk

Use a crates.io dependency and Cargo.lock pin if available. Use git only if the published crate cannot support the required stdio server path.

## Shape

The MCP server should be a local stdio server that calls the same daemon surface humans use. It must not bypass daemon safety, activity, account scoping, dry-run, or provider boundaries.

The implementation can add a new `crates/mcp` crate and expose it via `mxr mcp serve` and/or an `mxr-mcp` binary, whichever fits the current CLI layout best. Keep the user-facing command documented for task 005.

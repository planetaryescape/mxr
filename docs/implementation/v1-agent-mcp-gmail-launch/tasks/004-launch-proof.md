---
id: task-004
title: End-to-end v1 launch proof across CLI, daemon, agent, MCP, and providers
status: accepted
phase: phase-003
depends_on:
  - task-001
  - task-002
  - task-003
blocks:
  - task-005
risk:
  level: high
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
    - deployment
scope:
  allowed_paths:
    - Cargo.toml
    - Cargo.lock
    - scripts/**
    - .github/workflows/**
    - crates/daemon/tests/**
    - crates/provider-gmail/tests/**
    - crates/provider-imap/tests/**
    - crates/provider-smtp/tests/**
    - crates/test-support/**
    - crates/mcp/**
    - docs/implementation/v1-agent-mcp-gmail-launch/**
  blocked_paths:
    - .env*
    - node_modules/**
    - .git/**
    - target/**
    - "*.db"
validation:
  test_commands:
    - bash scripts/release_version_gate_test.sh
    - bash scripts/release_gmail_oauth_gate_test.sh
    - bash scripts/provider_smoke_workflow_test.sh
    - cargo build -p mxr
    - scripts/cargo-test -p mxr --test cli_journey
    - scripts/cargo-test -p mxr --test live_gmail_e2e
  success_criteria:
    - "A deterministic v1 launch smoke proves install/binary invocation, configure fake account, daemon start, sync, search, read, draft, approve/gated send path, agent policy enforcement, and MCP tool invocation."
    - "Agent policy enforcement must be proven by a real daemon IPC request tagged `source=agent`, not just by unit tests or config presence. The proof must show an allowed agent read/draft-only path and a blocked agent send/destructive path."
    - "Optional live Gmail and IMAP/SMTP smoke paths are documented and CI-safe: when credentials exist, each provider path runs a real smoke check or emits explicit unavailable_no_live_smoke evidence; missing credentials produce skipped_missing_creds evidence, not silent success."
    - "Release/provider smoke workflows call or reference the v1 launch proof so v1 cannot be cut without visible proof status."
    - "Proof artifacts are machine-readable JSON/JSONL where practical and do not include secrets/full bodies."
    - "Validation docs explain how to run the proof locally from a release artifact or cargo-built binary."
---

# V1 launch proof

## Goal

Turn "we think this works" into a repeatable launch gate.

## Required journey

Deterministic no-network path:

1. create isolated temp `MXR_DATA_DIR`;
2. configure fake provider account;
3. start or auto-start daemon;
4. sync;
5. search/list;
6. read a message/thread;
7. draft/reply;
8. prove send is blocked without approval under agent/MCP policy;
9. prove approved/gated send path through fake provider;
10. exercise MCP tool listing and one read/draft/mutation path.

Live provider path:

- Gmail OAuth/API or Gmail-over-IMAP if env credentials exist;
- IMAP/SMTP if env credentials exist;
- missing credentials must be explicit skip evidence.

Do not require real secrets for the deterministic CI path.

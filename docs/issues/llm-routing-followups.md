# LLM routing follow-ups

Left open by the Claude routing change (Anthropic first, OpenRouter fallback).

- **Effort is fixed at `low`.** `output_config.effort` is `low` for every
  feature on the Anthropic route. That suits gists, commitments and
  deliveries; draft writing and humanizer rewrites may want `medium`. The
  provider is built per override, not per feature, so a per-feature effort
  needs `LlmFeature` plumbed into `RoutedProviderConfig`.
- **A `claude-*` model name always routes.** A user who points `llm.base_url`
  at their own proxy and names a model `claude-...` is now sent to Anthropic
  and OpenRouter instead. There is no opt-out; add one if anyone reports it.
- **Route health resets on any config mutation.** `LlmRuntime::replace` clears
  it, and `mutate_config` calls that for every config write, not only `[llm]`
  changes. Fingerprint the `[llm]` section if this costs a failed request too
  often during an outage.
- **TUI and MCP have no LLM status surface.** `mxr llm status` is CLI and web
  only; there was no existing TUI or MCP status screen to extend.
- **`scripts/pre-pr-rust-gate` skips the root crate.** `crates/daemon/src`
  belongs to the `mxr` package, but the path mapping only finds
  `crates/<name>/Cargo.toml`, so daemon-only changes get no clippy from the
  gate. Run `cargo clippy -p mxr --lib --tests -- -D warnings` by hand.
- **Keys are not mirrored to the OS keychain.** The keychain crate can't
  delete an entry, and a removed key must not return through the keychain
  fallback account passwords use.

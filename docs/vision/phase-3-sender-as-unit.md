# Phase 3 — Sender-as-unit (the unique bet)

> Goal: ship the feature reviewers tell their friends about. mxr is uniquely positioned because the relationship data is already in local SQLite.

See [01-delight-plan.md §Phase 3](./01-delight-plan.md#phase-3--sender-as-unit-the-unique-bet) for full specs.

## Tracker

### 3.1 Snippets with `;name` + `{var}` placeholders

**Store layer ✅**
- [x] Migration `016_snippets.sql` (table; renumbered from plan's 019 to fit existing sequence)
- [x] `crates/store/src/snippets.rs` with `Snippet` type + `upsert_snippet`, `delete_snippet`, `get_snippet`, `list_snippets`
- [x] RED+GREEN: `upsert_and_get_round_trips`
- [x] RED+GREEN: `upsert_replaces_existing_body_and_vars`
- [x] RED+GREEN: `delete_removes_existing_snippet`
- [x] RED+GREEN: `delete_returns_false_for_missing_snippet`
- [x] RED+GREEN: `list_returns_alphabetical_order`

**IPC + CLI ✅**
- [x] `Request::ListSnippets`, `Request::SetSnippet`, `Request::DeleteSnippet`
- [x] `ResponseData::Snippets`, `ResponseData::SnippetData` (with `SnippetData` shape)
- [x] Daemon handler `crates/daemon/src/handler/snippets.rs` (preserves `created_at` on update)
- [x] CLI `mxr snippets list|set <name> <body>|remove <name>`
- [x] CLI snapshot updated

**Still TBD**
- [ ] TUI: snippet manager modal
- [ ] Pre-editor expansion hook in `compose/src/lib.rs` (semicolon + name → body)
- [ ] Built-in var population (`{first_name}`, `{date}`, `{thread_subject}`)
- [ ] Post-edit `{var}` warning in `runtime.rs::SendDraft`

### 3.2 Sender view (`mxr sender <addr>`)

**Store + IPC + CLI ✅**
- [x] `crates/store/src/sender_profile.rs` with `get_sender_profile(account_id, email)`
- [x] Joins `contacts` (volume, cadence, replied count) with on-the-fly "open thread count" subquery
- [x] IPC: `Request::GetSenderProfile { account_id, email }` → `ResponseData::SenderProfile { profile: Option<SenderProfileData> }`
- [x] Handler `crates/daemon/src/handler/sender_view.rs`
- [x] CLI `mxr sender <addr> [--account <key>]` with table + JSON output
- [x] RED+GREEN: `get_sender_profile_returns_none_for_unknown_contact`

**Still TBD**
- [ ] Unanswered-question heuristic (last inbound has `?` + no outbound within cadence)
- [ ] Trend sparkline (messages-per-week)
- [ ] TUI: `Screen::SenderProfile` (full-screen page)
- [ ] Latency histogram from `reply_pairs`

### 3.3 LLM provider trait ✅

**Approach pivoted**: rather than embedding `mistral.rs` for native pure-Rust inference (heavy compile, gigabytes of artefacts), `mxr-llm` is an OpenAI-compatible HTTP client. One implementation covers **Ollama** (`http://localhost:11434/v1`), **LM Studio** (`http://localhost:1234/v1`), **OpenAI**, **Groq**, **OpenRouter**, **Together AI**, **Mistral La Plateforme**, etc. Inference runs in whichever local engine the user prefers — keeping the local-first stance intact while not bundling a model runtime.

- [x] New crate `crates/llm/` with `LlmProvider` trait, `ChatMessage`, `CompletionRequest`, `CompletionResponse`, `LlmCapabilities`, `LlmError`
- [x] `OpenAiCompatibleProvider` covers Ollama, LM Studio, OpenAI, Groq, OpenRouter, etc.
- [x] `OpenAiCompatibleProvider::ollama(model)` and `::lm_studio(model)` convenience constructors
- [x] `NoopProvider` for the disabled-by-default state
- [x] `LlmConfig` in `mxr-config` with `enabled`, `base_url`, `model`, `api_key_env`, `context_window`, `request_timeout_secs`
- [x] API key read from env (`api_key_env` names the env var; key never lives in the config file)
- [x] Error classification: `Disabled`, `Unreachable`, `RateLimited { retry_after_secs }`, `Timeout`, `Unauthorized`, `Empty`, `Other`
- [x] API key redaction in error strings (defensive)
- [x] Daemon LLM runtime always present (defaults to `NoopProvider` when disabled) and can rebuild the provider on config reload
- [x] IPC/CLI status surface: `mxr llm status` reports provider, runtime model, configured model, context window, timeout, and API-key env presence
- [x] RED+GREEN: `noop_provider_returns_disabled_error`
- [x] RED+GREEN: `redact_replaces_api_key_substring`
- [x] RED+GREEN: `redact_leaves_short_keys_alone_to_avoid_collateral_damage`
- [x] RED+GREEN: `ollama_defaults_to_localhost_with_no_api_key`
- [x] RED+GREEN: `lm_studio_defaults_to_localhost_with_no_api_key`
- [x] RED+GREEN: `capabilities_surface_context_window`

**Deferred to future sessions (large dependency surface)**
- [ ] Native pure-Rust inference via mistral.rs (would supplement HTTP, not replace)
- [ ] Streaming chunks (current is single-shot completions; thread summaries and short drafts don't need streaming)
- [ ] Live integration test against a running Ollama instance (httpmock-based unit-level tests would be next)

### 3.4 Thread summarize on demand ✅

- [x] IPC: `Request::SummarizeThread { thread_id }` → `ResponseData::ThreadSummary { text, model }`
- [x] Handler `crates/daemon/src/handler/summarize.rs` builds a chat-style prompt from the existing thread + bodies and asks the configured LLM for a 2-3 sentence summary
- [x] System prompt tuned for "actionable for a busy reader, no pleasantries"
- [x] 24KB prompt budget (truncates oldest messages first)
- [x] CLI `mxr summarize <thread-id>` (table or JSON output, model id surfaced)
- [x] Returns clear `LLM is disabled` error when LLM is off

**Deferred**
- [ ] Content-hash cache (re-summarising an unchanged thread re-prompts unnecessarily; small efficiency win but not correctness-critical)
- [ ] TUI `S` keybinding to invoke summarise on the focused thread

### 3.5 Draft assist grounded on sent corpus ✅

- [x] IPC: `Request::DraftAssist { thread_id, instruction }` → `ResponseData::DraftSuggestion { body, model }`
- [x] Handler `crates/daemon/src/handler/draft_assist.rs` constructs a prompt with thread transcript + the user's plain-language instruction
- [x] Semantic grounding retrieves similar prior outbound messages, excludes inbound/current-thread hits, and includes up to 3 examples as voice context when semantic search is enabled
- [x] System prompt tuned for "no greeting, no signature, match thread formality"
- [x] CLI `mxr draft-assist <thread-id> "<instruction>"`
- [x] Result printed to stdout for the user to pipe / edit / paste — never auto-sent
- [x] Gracefully falls back to thread-only prompting when semantic search is disabled or unavailable

**Deferred**
- [ ] Token-budget truncation that prioritises examples before truncating thread context

## Phase 3 acceptance

- [ ] Type `;thanks` in compose body; expanded with `{first_name}` filled
- [ ] `mxr sender alice@example.com` shows volume, response-time histogram, open commitments
- [ ] (LLM enabled) `mxr summarize <thread-id>` returns coherent summary
- [ ] (LLM enabled) `mxr draft-assist --reply <id> --instruct "decline"` returns a draft in user's voice
- [ ] (LLM disabled) Same commands return graceful `LlmDisabled` error

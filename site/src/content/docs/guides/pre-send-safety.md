---
title: Pre-send safety
description: Catch wrong recipients, missing attachments, leaked secrets, and unaddressed asks before the send button does. Runs on every send; gated, never silent.
---

mxr runs a deterministic safety pipeline against every draft before it
hits the provider. Six checks total — five run with zero LLM calls; one
(answer-coverage) uses your configured LLM. Each check produces an
issue with a severity:

- **Info** — the check ran but found nothing surprising (e.g. LLM
  disabled).
- **Warning** — worth a look; press send anyway if you mean it.
- **Blocker** — refuses to send until you fix it or supply a one-shot
  override token.

The pipeline runs on `mxr send`, `mxr compose --yes`, the TUI confirm
modal, and the scheduled-send flusher. The same `mxr send --check`
command runs it WITHOUT sending so you can dry-run any draft (or wire
it into a CI step / pre-commit hook).

:::tip[The one-line mental model]
Safety is a **gate**, not a rewriter. mxr will refuse to send a Blocker,
but it never edits your recipients or your body. You either fix the
draft, supply an override token, or kill the draft.
:::

## The six checks

| Code | Severity | Reads | LLM? |
|---|---|---|---|
| `WrongRecipient` | Warning or Blocker | `contacts`, `account_addresses`, your `internal_domains`/`sensitive_domains` config | No |
| `MissingAttachment` | Warning | subject + body regex | No |
| `ReplyAll` | Warning (never blocks) | thread participants, draft body | No |
| `PiiSecret` | Warning (cards/SSNs) or Blocker (private keys, common API tokens) | draft body | No |
| `ToneMismatch` | Warning | `contact_style` baseline (>= 3 prior messages) | No |
| `AnswerCoverage` | Warning (or `Info` if LLM disabled) | parent thread | Yes |

## Quick start

```bash
# Check a stored draft without sending. Exit 0 = ok / warnings only,
# Exit 2 = at least one Blocker.
mxr send DRAFT_ID --check --format json

# Check a draft built from CLI args (no daemon-side row created).
# Useful in scripts and CI: pipe a body in and assert the JSON.
mxr compose --to alice@example.com --body "see attached" --check

# Send a draft that hit a Blocker, with the token printed by --check.
mxr send DRAFT_ID --override-safety 01HXYZ...

# Skip the LLM-backed answer-coverage check (e.g. rate-limited model).
mxr send DRAFT_ID --check --no-llm
```

## Wrong recipient

Two failure modes:

1. **Typo distance** — you typed `alcie@example.com` but you have a
   strong history with `alice@example.com`. mxr warns and names the
   suggested address. Triggered only when the typed recipient has zero
   or weak prior history (`total_inbound + total_outbound < 3`).
2. **Sensitive / internal-leak domains** — recipients on a configured
   `sensitive_domains` list trigger a **Blocker**. So does an internal
   marker in the body (e.g. `INTERNAL`, `CONFIDENTIAL`) paired with an
   external recipient domain.

Configure in `config.toml`:

```toml
[safety.recipients]
internal_domains = ["company.com"]
sensitive_domains = ["competitor.com"]
warn_on_first_time_external = true   # warn on never-seen-before domains
```

**Use it like this:**

```bash
# Catch typos in a single ad-hoc send before it goes out.
mxr compose \
  --to alcie@example.com \
  --subject "Friday plan" \
  --body "$(cat plan.md)" \
  --check --format json | jq '.issues[] | select(.code == "wrong_recipient")'
```

```bash
# Block every send to competitor.com unless explicitly approved.
mxr send DRAFT_ID --check
# Reports:  [BLOCK] WrongRecipient: recipient ceo@competitor.com is on the
#                  configured sensitive-domain list
#           Override token: 01HXYZ-...

mxr send DRAFT_ID --override-safety 01HXYZ-...
```

## Missing attachment

Pure regex over subject + body. Matches `attached`, `see attached`,
`attachment`, `enclosed`, `I've attached`, `attaching`. Won't fire on
negations (`not attached`, `without attachment`) or quoted reply
context.

```bash
# Send only when the draft has actual files attached.
mxr compose --to alice@example.com --body "see attached for Q1" --check
# [warn] MissingAttachment: body mentions an attachment but the draft has none
```

**Use it like this:**

```bash
# CI / pre-commit hook: refuse to commit a draft that says "attached"
# without an actual file. Combine with --format json + jq -e.
mxr send DRAFT_ID --check --format json | jq -e '
  [.issues[].code] | contains(["missing_attachment"]) | not
'
```

## Reply-all sanity

Only fires when the intent is reply-all AND the visible recipient list
has more than two people. Reads the first paragraph of the draft body
and checks for:

- A single-person vocative greeting (`Hi Alice,`, `Hey Alice -`).
- The absence of group language (`Hi team`, `Hi everyone`, `Hi all`).

If exactly one person is named and they are not on the parent thread's
participant list (so it's not just quoted context), mxr warns. This
check is always a Warning, never a Blocker — the cost of a false
positive on a real group reply is too high.

**Use it like this:**

```bash
mxr send DRAFT_ID --check
# [warn] ReplyAll: draft body addresses only Alice, but reply-all sends
#        to 6 recipients
```

## PII and secrets

Local-only detectors. Nothing leaves the machine; raw secret material
is never written to logs, JSON output, or the audit table.

| Pattern | Severity |
|---|---|
| PEM private key header (`-----BEGIN ... PRIVATE KEY-----`) | Blocker |
| OpenAI-style key (`sk-...`) | Blocker |
| GitHub personal token (`ghp_...`) | Blocker |
| Slack bot token (`xoxb-...`) | Blocker |
| `AWS_ACCESS_KEY_ID=` / `AWS_SECRET_ACCESS_KEY=` | Blocker |
| `api_key=` / `client_secret=` | Blocker |
| Luhn-valid credit card number | Warning |
| SSN-shaped value (`###-##-####`) | Warning |

JSON output is always **redacted** — only a preview like `sk-...abcd`
or `**** **** **** 4242` ever appears.

**Use it like this:**

```bash
# Pre-commit hook: refuse to send any draft containing a private key.
mxr send "$DRAFT_ID" --check --format json | jq -e '
  [.issues[] | select(.severity == "blocker") | .code]
  | contains(["pii_secret"]) | not
'
```

```bash
# One-shot: check a tooling email you're about to send.
printf '%s\n' \
  "Here is the key to deploy:" \
  "<example PEM private key marker>" \
  "<redacted key bytes>" | \
  mxr compose --to deploys@example.com --body-stdin --check
# Exit 2.
# [BLOCK] PiiSecret: PEM private key detected in draft
#         redacted: -----BEGIN ... PRIVATE KEY-----
```

## Tone mismatch

Reads `mxr_relationship::contact_style` — the recipient's writing
baseline that the relationship engine maintains from prior outbound
mail. Triggers a Warning only when:

- The recipient has **>= 3 prior messages** to baseline against.
- The voice-match confidence is medium or high.
- The formality delta between draft and baseline exceeds the
  configured threshold.

```toml
[safety.tone]
formality_delta_threshold = 0.25   # 0.0 = always warn, 1.0 = never
```

The warning names the direction: "Tone is more formal than usual with
alice@example.com" or "more casual than usual". Useful when you'd
normally write breezily but the draft slipped into corporate-speak (or
vice versa).

This check **never calls the LLM**. The metrics are pure stylometry
(sentence length, contractions, opener tokens, formality score) and
live in the local `contact_style` table.

## Answer coverage

This is the only LLM-backed check. Runs against the parent thread when
the draft is a reply:

1. Loads inbound messages from the thread (reader-cleaned).
2. Asks the LLM to extract explicit asks and judge whether the draft
   addresses each one.
3. Requires the LLM to cite an `evidence_msg_id` from the loaded set —
   any ask citing an unknown message id is rejected (no hallucinated
   citations).
4. Warns on unaddressed asks, naming the first missing one.

If the LLM is disabled or unreachable, the check degrades to an `Info`
issue with the reason — never silently absent.

**Use it like this:**

```bash
# Compose a reply, then verify you didn't drop a question.
mxr reply MSG_ID --body "Yes — Q3, and we'll loop in legal."
mxr drafts --format json | jq -r '.[-1].id' | xargs -I{} \
  mxr send {} --check --format json | \
  jq '.issues[] | select(.code == "answer_coverage")'
# [warn] AnswerCoverage: draft does not address: ownership of rollout
```

```bash
# Skip the LLM (e.g. cloud rate limits, deterministic CI runs).
mxr send DRAFT_ID --check --no-llm
```

Configure the LLM in `[llm]`. Per-feature overrides:

```toml
[llm.overrides.answer_coverage]
model = "gpt-5-mini"
temperature = 0.0
```

## The override flow

When a check fires a Blocker, `--check` mints a single-use override
token and stamps it onto the issue. The token has these properties:

- **Single use.** Consumed on first send attempt; subsequent attempts
  with the same token are rejected.
- **Scoped.** Only valid for the draft it was minted against, and only
  bypasses the exact blocker kinds it was issued for. Editing the
  draft and adding a NEW blocker kind invalidates the token.
- **Auditable.** Every check run AND every override consumption is
  recorded in `draft_safety_runs` / `draft_safety_overrides`.

```bash
# Step 1: check, see the blocker, copy the token.
mxr send DRAFT_ID --check
# [BLOCK] PiiSecret: PEM private key detected in draft
#         Override token: OVERRIDE_TOKEN_FROM_CHECK

# Step 2: send with the token. mxr verifies it covers every active
# blocker before any provider call.
mxr send DRAFT_ID --override-safety OVERRIDE_TOKEN_FROM_CHECK
```

```bash
# JSON workflow: parse the token, store it briefly, send.
TOKEN=$(mxr send "$DRAFT_ID" --check --format json | \
  jq -r '.issues[] | select(.severity == "blocker") | .override_token | select(. != null)' | head -1)
[ -z "$TOKEN" ] && exit 0  # nothing to override; safe to send
mxr send "$DRAFT_ID" --override-safety "$TOKEN"
```

## Scheduled sends

When the scheduler fires a draft and the safety pipeline produces a
Blocker, mxr:

1. **Refuses the send.** No provider call is made.
2. **Clears the schedule.** Without this the scheduler would retry
   indefinitely on every tick.
3. **Logs an event.** Visible in `mxr events --type send --format jsonl`
   and the daemon log.
4. **Keeps the draft.** It stays in your drafts list for you to inspect.

Recover with:

```bash
mxr drafts --format json | jq -r '.[] | select(.status == "draft") | .id'
mxr send DRAFT_ID --check       # see the blocker
mxr send DRAFT_ID --override-safety 01HXYZ-...  # if you accept it
```

## TUI flow

The send-confirmation modal shows the verdict header (`Safety: SAFE` /
`WARN` / `BLOCKED`), each issue with its severity, and the override
token when present. Press `e` to edit the draft (which re-runs the
pipeline on save), or `s` to send. The `s` key is greyed out when the
verdict is BLOCKED and no override is in scope.

## In real life

- **Slack-paste audit.** You copied an env file into a thread by
  mistake. The PEM-key blocker and `sk-...` blocker stop the send
  before you can hit `Cmd-Enter`.
- **First-time-external check.** You're inside a regulated org with
  `internal_domains = ["company.com"]`. mxr warns when a recipient is
  not on company.com OR a known external counterparty you have a
  history with.
- **Reply-all on a 40-person list.** You started with "Hi Alice," and
  forgot to switch to reply. mxr warns, you switch to `mxr reply
  MSG_ID`, you're done.
- **Pre-commit hook for an autoresponder.** Your nightly job composes
  a digest and sends it; gate the send on `mxr send --check --format
  json` so a Luhn-valid card number in a digest never escapes.

## Agent prompts that work

```text
"Before sending DRAFT_ID, run `mxr send DRAFT_ID --check --format json`
and show me any Blocker issues. If there are none, send it. If there
is a Blocker for missing attachment, stop and ask me what file to attach.
For other Blockers, paste the override token and ask if I want to use it."
```

```text
"For every draft in `mxr drafts --format json`, run --check and report
a one-line summary: ID, verdict, top issue. Skip Info-only entries."
```

## See also

- [Compose](/guides/compose/) — the draft lifecycle the pipeline runs
  against
- [Crash-safe drafts](/guides/crash-safe-drafts/) — what happens to a
  draft when the daemon dies mid-send
- [LLM features](/guides/llm-features/) — configure the LLM that
  answer-coverage uses
- [Config](/reference/config/) — `[safety.recipients]`, `[safety.tone]`,
  `[llm.overrides.answer_coverage]`
- [CLI — `mxr send`](/reference/cli/send/) and [`mxr compose`](/reference/cli/compose/)

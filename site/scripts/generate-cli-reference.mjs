#!/usr/bin/env node
/**
 * Generates per-command Markdown pages for the mxr CLI reference,
 * driven by the captured `--help` snapshots in
 * `crates/daemon/tests/snapshots/cli_help__*.snap`.
 *
 * Output: site/src/content/docs/reference/cli/<command>.md (+ index.md).
 * Each page is structured: synopsis, description, arguments, options,
 * subcommands. Sub-subcommands (e.g. `accounts add`) become `### add`
 * sections under the parent page.
 *
 * Run as a `prebuild` step. Idempotent and pure: deletes the output
 * directory and rewrites it from scratch each invocation.
 */

import { readFileSync, readdirSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(__dirname, '..', '..');
const SNAPSHOT_DIR = join(REPO_ROOT, 'crates', 'daemon', 'tests', 'snapshots');
const OUT_DIR = join(REPO_ROOT, 'site', 'src', 'content', 'docs', 'reference', 'cli');
const PREFIX = 'cli_help__cli_help_';

const COMMAND_EXAMPLES = {
  search: {
    use: 'Find messages fast, then pipe IDs or JSON into the next step.',
    examples: [
      "mxr search 'from:alice is:unread' --format json",
      "mxr search 'has:attachment subject:invoice' --format ids | mxr label receipts --dry-run",
      "mxr search 'is:owed-reply' --format ids   # threads you're the bottleneck on",
    ],
  },
  cat: {
    use: 'Read one message body, or resolve a search to the first matching message.',
    examples: ["mxr cat MESSAGE_ID --view reader", "mxr cat --search 'from:boss is:unread' --first"],
  },
  thread: {
    use: 'Read the whole conversation around a message or thread.',
    examples: ["mxr thread MESSAGE_ID --format json", "mxr thread --search 'subject:launch' --first"],
  },
  archive: {
    use: 'Clear mail from the inbox without deleting it. Always dry-run bulk queries first.',
    examples: ["mxr archive MESSAGE_ID", "mxr archive --search 'label:newsletters older_than:30d' --dry-run"],
  },
  'read-archive': {
    use: 'Mark something done in one step: read it and remove it from the inbox.',
    examples: ["mxr read-archive MESSAGE_ID", "mxr read-archive --search 'from:no-reply older_than:7d' --dry-run"],
  },
  trash: {
    use: 'Move mail to trash after previewing the target set.',
    examples: ["mxr trash MESSAGE_ID --dry-run", "mxr trash --search 'from:spam@example.com' --yes"],
  },
  spam: {
    use: 'Report unwanted mail as spam through the provider-backed mutation path.',
    examples: ["mxr spam MESSAGE_ID --dry-run", "mxr spam --search 'from:bad.example' --yes"],
  },
  star: {
    use: 'Mark important messages for follow-up.',
    examples: ["mxr star MESSAGE_ID", "mxr star --search 'from:manager subject:review' --dry-run"],
  },
  unstar: { use: 'Remove stars from messages.', examples: ["mxr unstar MESSAGE_ID"] },
  read: { use: 'Mark messages read.', examples: ["mxr read --search 'label:alerts older_than:14d' --dry-run"] },
  unread: { use: 'Put messages back into an unread workflow.', examples: ["mxr unread MESSAGE_ID"] },
  label: {
    use: 'Apply a label/folder to messages, including piped IDs from search.',
    examples: ["mxr label receipts MESSAGE_ID", "mxr search 'from:billing' --format ids | mxr label receipts --yes"],
  },
  unlabel: { use: 'Remove a label from messages.', examples: ["mxr unlabel receipts MESSAGE_ID"] },
  move: { use: 'Move messages to a folder-like label.', examples: ["mxr move Archive MESSAGE_ID"] },
  snooze: {
    use: 'Hide mail until a useful time instead of leaving it as inbox noise.',
    examples: ["mxr snooze MESSAGE_ID --until 'monday 9am'", "mxr snooze --search 'from:newsletter is:unread' --until weekend --dry-run"],
  },
  unsnooze: { use: 'Wake snoozed messages early.', examples: ["mxr unsnooze MESSAGE_ID", "mxr unsnooze --all --dry-run"] },
  unsubscribe: {
    use: 'Unsubscribe from a single message id, or from every message by a given sender address. Passing an email address is shorthand for `--search "from:<addr>"` — the daemon picks the most recent match and uses its List-Unsubscribe header.',
    examples: [
      "mxr unsubscribe alice@example.com --dry-run",
      "mxr unsubscribe alice@example.com bob@example.com --yes",
      "mxr unsubscribe MESSAGE_ID --dry-run",
      "mxr unsubscribe --search 'list:<news.example.com>' --dry-run",
    ],
  },
  compose: {
    use: 'Write a new message in $EDITOR or from stdin in scripts. Add `--check` to run the [pre-send safety pipeline](/guides/pre-send-safety/) against a transient draft built from these flags — useful in CI / pre-commit hooks.',
    examples: [
      "mxr compose --to alice@example.com --subject 'Friday'",
      "printf 'Approved' | mxr compose --to alice@example.com --subject 'Re: plan' --body-stdin --dry-run",
      "mxr compose --to alice@example.com --body 'see attached' --check --format json   # warns: missing attachment",
    ],
  },
  reply: { use: 'Reply to one message, interactively or from an agent-approved body.', examples: ["mxr reply MESSAGE_ID", "mxr reply MESSAGE_ID --body 'On it.' --yes"] },
  'reply-all': { use: 'Reply to everyone on a thread.', examples: ["mxr reply-all MESSAGE_ID --body-stdin --dry-run"] },
  forward: { use: 'Forward a message with optional context.', examples: ["mxr forward MESSAGE_ID --to teammate@example.com"] },
  drafts: { use: 'List, recover, resume, or discard local drafts.', examples: ["mxr drafts --format json", "mxr drafts recover"] },
  send: {
    use: 'Send or schedule a saved draft. Add `--check` to run the [pre-send safety pipeline](/guides/pre-send-safety/) without sending; exit 2 on Blocker. Use `--override-safety <TOKEN>` to bypass a Blocker the previous `--check` minted.',
    examples: [
      "mxr send DRAFT_ID --dry-run",
      "mxr send DRAFT_ID --at 'tomorrow 9am'",
      "mxr send DRAFT_ID --check --format json   # safety report, no send",
      "mxr send DRAFT_ID --override-safety 01HXYZ-K4M2-...   # bypass minted Blocker",
    ],
  },
  unsend: { use: 'Cancel a scheduled send while keeping the draft.', examples: ["mxr unsend DRAFT_ID"] },
  accounts: { use: 'Add, test, repair, disable, or inspect accounts and aliases.', examples: ["mxr accounts add gmail", "mxr accounts addresses add --account work alias@example.com"] },
  sync: { use: 'Trigger sync or wait for sync completion in scripts.', examples: ["mxr sync --wait", "mxr sync --status --format json"] },
  status: { use: 'Check daemon health from a shell, status bar, or agent.', examples: ["mxr status --format json", "mxr status --watch"] },
  doctor: { use: 'Diagnose broken sync, search, semantic index, or daemon state.', examples: ["mxr doctor --format json", "mxr doctor --reindex --semantic-status"] },
  logs: {
    use: 'Inspect daemon logs without digging through runtime files. Combine `--level`, `--search`, and `--limit` to zero in on incidents.',
    examples: [
      "mxr logs --level warn --since 1h",
      "mxr logs --search 'timeout' --limit 200 --format jsonl",
      "mxr logs --level error --since 24h --search 'gmail'   # past errors mentioning Gmail",
    ],
  },
  events: { use: 'Watch live daemon events.', examples: ["mxr events --type sync --format jsonl"] },
  history: {
    use: 'Inspect persisted events after a mutation or sync run. Filter by category, level, time window, or free-text search.',
    examples: [
      "mxr history --category mutation --limit 5 --format json",
      "mxr history --search 'archive failed' --since 24h --level error",
      "mxr history --category-prefix sync --since 7d --limit 200 --format jsonl | jq -r '.summary'",
    ],
  },
  activity: {
    use: "Browse the local user-activity log — the git-reflog of your inbox. Strictly local; never transmitted off-device. See the [Activity Log guide](/guides/activity-log/) for the full design.",
    examples: [
      "mxr activity list --since 24h",
      "mxr activity stats --group-by action --since 7d --format json",
      "mxr activity export --format ndjson --out my-week.ndjson",
      "mxr activity clear --last 1h --yes   # tombstone the last hour",
    ],
  },
  notify: { use: 'Feed unread counts into status bars.', examples: ["mxr notify", "mxr notify --watch --format jsonl"] },
  labels: { use: 'List and manage labels/folders.', examples: ["mxr labels", "mxr labels create receipts --dry-run"] },
  saved: {
    use: 'Name searches you run every day. Saved searches appear in the TUI sidebar and the command palette.',
    examples: [
      "mxr saved add today 'is:unread newer_than:1d'",
      "mxr saved add owed 'is:owed-reply'   # threads you owe a reply on",
      "mxr saved run today --format ids",
    ],
  },
  rules: { use: 'Create deterministic filing/mutation rules and dry-run them before enabling.', examples: ["mxr rules validate --when 'from:billing' --then 'label:receipts'", "mxr rules dry-run --all --after 2026-01-01"] },
  snippets: {
    use: "Manage stock reply snippets. Bodies may reference built-in tokens: `{first_name}` / `{full_name}` (resolved from the draft's `to:`), `{thread_subject}` (the draft subject with `Re:` / `Fwd:` stripped), and `{today}` / `{date}` / `{year}` (local time at expansion).",
    examples: [
      "mxr snippets set hi 'Hi {first_name},'",
      "mxr snippets set followup 'Following up on {thread_subject} ({today}).'",
      "mxr snippets list --format json",
      "mxr snippets remove hi",
    ],
  },
  deliveries: {
    use: "Track packages and shipments detected in your mail. Detection runs automatically on sync (a local heuristic plus optional LLM confirmation); these commands browse and manage the results. See [Deliveries](/guides/deliveries/).",
    examples: [
      "mxr deliveries list",
      "mxr deliveries list --filter delivered --format json",
      "mxr deliveries scan --since-days 90 --dry-run",
      "mxr deliveries resolve DELIVERY_ID",
    ],
  },
  replies: { use: 'Manage the reply-later queue.', examples: ["mxr replies list --format ids", "mxr replies add MESSAGE_ID"] },
  remind: { use: 'Set reminders on outbound messages when you need a reply.', examples: ["mxr remind SENT_MESSAGE_ID --when 'friday 10am'"] },
  sender: { use: 'Inspect one sender before replying, unsubscribing, or writing an agent prompt.', examples: ["mxr sender alice@example.com --format json"] },
  senders: {
    use: "List top inbound senders by message volume. Use `--since` to restrict to a recent window (e.g. `--since 7d` for the last week, or an RFC-3339 timestamp). Pipe `--format ids` into `mxr unsubscribe` to clean house in two commands.",
    examples: [
      "mxr senders --top 20",
      "mxr senders --top 10 --since 7d",
      "mxr senders --since 2026-01-01T00:00:00Z --format json | jq '.[] | {who:.sender_email, count:.message_count}'",
    ],
  },
  screener: { use: 'Decide where first-time senders should go.', examples: ["mxr screener queue", "mxr screener feed newsletter@example.com --label feeds"] },
  summarize: { use: 'Summarize long threads through the configured LLM.', examples: ["mxr summarize THREAD_ID", "mxr summarize --search 'from:team newer_than:7d' --limit 5"] },
  'draft-assist': { use: 'Generate a draft body grounded in a thread, then review before sending.', examples: ["mxr draft-assist THREAD_ID 'Polite follow-up with next steps'", "mxr draft-assist --search 'from:sarah newer_than:14d' --first '1:1 agenda'"] },
  subscriptions: { use: 'Find noisy mailing lists and candidates for unsubscribe sweeps.', examples: ["mxr subscriptions --rank --format json"] },
  storage: { use: 'Find large senders, MIME types, labels, or individual messages consuming disk.', examples: ["mxr storage --by sender --limit 20", "mxr storage --by message --format ids | mxr archive --dry-run"] },
  contacts: { use: 'Surface relationship asymmetry or decay.', examples: ["mxr contacts asymmetry --limit 20", "mxr contacts decay --threshold-days 45"] },
  'response-time': { use: 'Measure reply latency with a counterparty or account.', examples: ["mxr response-time --theirs --counterparty alice@example.com --format json"] },
  stale: { use: 'Find threads where someone owes a reply.', examples: ["mxr stale --theirs --older-than-days 7", "mxr stale --mine --format ids | mxr snooze --until tomorrow --dry-run"] },
  owed: {
    use: "List threads where you're the bottleneck, ranked by overdue score (waiting days / recipient's typical cadence). Same set as `is:owed-reply` in search — pick whichever surface you prefer.",
    examples: [
      "mxr owed --format ids",
      "mxr owed --since 7 --format json | jq '.[] | {who:.counterparty_email, days:.waiting_days, score:.overdue_score}'",
      "mxr saved add owed 'is:owed-reply'   # persistent sidebar lens",
    ],
  },
  commitments: {
    use: 'List or resolve commitments mxr extracted from your sent mail. Set after every successful `mxr send`; persisted in `contact_commitments`. See the [forgotten-work guide](/guides/forgotten-work/) for the full draft → send → ledger flow.',
    examples: [
      "mxr commitments --status open --format json",
      "mxr commitments --contact alice@example.com",
      "mxr commitments resolve COMMITMENT_ID",
    ],
  },
  ask: {
    use: 'Synthesize a citation-backed answer over your local mail. Every claim cites a retrieved message id; uncited LLM output is rejected. See [archive intelligence](/guides/archive-intelligence/).',
    examples: [
      "mxr ask 'what did Alice and I decide about pricing in Q2?'",
      "mxr ask 'launch timeline' --from alice@example.com --after 2026-01-01 --format json",
      "mxr ask 'open infra questions' --mode lexical   # skip semantic when index is rebuilding",
      "mxr ask 'who owns the legal review' --format json | jq -r '.citations[].message_id' | xargs -I{} mxr cat {} --view reader",
    ],
  },
  decisions: {
    use: 'List or rebuild the citation-backed decision log. Idempotent on unchanged threads. See [archive intelligence](/guides/archive-intelligence/#the-decision-log).',
    examples: [
      "mxr decisions --topic pricing --format json",
      "mxr decisions rebuild --since 180",
      "mxr decisions --since 30 --format ids | xargs -I{} mxr decisions show {}",
    ],
  },
  briefing: {
    use: 'Re-enter old context fast: cached, citation-backed summaries of dormant threads or contacts after a long gap. See [briefings and loop-in](/guides/briefings-and-loop-in/).',
    examples: [
      "mxr briefing thread THREAD_ID",
      "mxr briefing thread THREAD_ID --refresh --format json",
      "mxr briefing recipient alice@example.com --format json",
    ],
  },
  cadence: {
    use: 'Watchlist for relationships you chose to maintain. Surfaces drift against an explicit expected interval — never auto-watches contacts. See [timing and cadence](/guides/timing-and-cadence/#cadence-drift).',
    examples: [
      "mxr cadence watch alice@example.com --every 14d",
      "mxr cadence list --format json",
      "mxr cadence drift --format ids | xargs -I{} mxr sender {}",
      "mxr cadence unwatch alice@example.com",
    ],
  },
  'send-time': {
    use: "Show the recipient's typical reply-time bucket from local `reply_pairs`. Statistical only — no LLM, no tracking pixels. See [timing and cadence](/guides/timing-and-cadence/).",
    examples: [
      "mxr send-time alice@example.com",
      "mxr send-time alice@example.com --at 'fri 19:00' --format json",
      "mxr send-time alice@example.com bob@example.com --format json   # worst meaningful delta wins",
    ],
  },
  expert: {
    use: 'Identify locally who has answered similar questions before. Ranks answerers above askers; citations point at answer messages, not just topic matches. See [briefings and loop-in](/guides/briefings-and-loop-in/#whos-the-expert).',
    examples: [
      "mxr expert MESSAGE_ID",
      "mxr expert --query 'DKIM setup' --format json",
      "mxr expert MESSAGE_ID --include-self --limit 10",
    ],
  },
  whois: {
    use: 'Explain a person, project, or jargon term using only locally cited mail evidence. No invented summaries when the corpus has nothing. See [briefings and loop-in](/guides/briefings-and-loop-in/#whois).',
    examples: [
      "mxr whois sam",
      "mxr whois 'Project Apollo' --format json",
      "mxr whois alice@example.com --limit 20",
    ],
  },
  'suggest-recipients': {
    use: 'Suggest "maybe include" Cc recipients for a draft based on co-participation in similar prior threads. Suggestions only, never auto-adds; never reveals Bcc. See [briefings and loop-in](/guides/briefings-and-loop-in/#maybe-include).',
    examples: [
      "mxr suggest-recipients --draft DRAFT_ID --format json",
      "echo 'rollout plan attached' | mxr suggest-recipients --subject 'pricing rollout' --body-stdin",
      "mxr suggest-recipients --draft DRAFT_ID --limit 3",
    ],
  },
  wrapped: { use: 'Generate year-to-date or yearly email analytics.', examples: ["mxr wrapped --ytd", "mxr wrapped --year 2025 --format json"] },
  export: { use: 'Turn threads into Markdown/JSON/LLM context.', examples: ["mxr export THREAD_ID --format markdown", "mxr export --search 'from:legal' --format markdown > legal.md"] },
  attachments: { use: 'List, open, or download message attachments.', examples: ["mxr attachments list MESSAGE_ID", "mxr attachments download MESSAGE_ID 1 --dir ~/Downloads/mxr"] },
  invite: {
    use: 'Inspect one email calendar invite and send an iMIP RSVP only after a dry-run preview. See [calendar invites](/guides/calendar-invites/) for the full flow.',
    examples: [
      "mxr invite show MESSAGE_ID --format json",
      "mxr invite reply MESSAGE_ID accept --dry-run --format json",
      "mxr invite reply MESSAGE_ID tentative --dry-run",
      "mxr invite reply MESSAGE_ID decline --dry-run",
    ],
  },
  invites: {
    use: 'List calendar invites found in stored mail, or backfill invite rows after upgrading.',
    examples: [
      "mxr invites list --limit 20",
      "mxr invites list --format jsonl | jq -r '.message_id'",
      "mxr invites backfill --format json",
    ],
  },
  config: { use: 'Inspect or edit resolved config without hunting for the file.', examples: ["mxr config path", "mxr config show --format json"] },
  llm: { use: 'Check which LLM provider the daemon is using.', examples: ["mxr llm status", "mxr llm status --format json"] },
  semantic: { use: 'Manage local semantic search profiles and indexing.', examples: ["mxr semantic status", "mxr semantic profile install bge-small-en-v1.5"] },
  open: { use: 'Open message URLs in your browser from CLI selection.', examples: ["mxr open MESSAGE_ID", "mxr open --search 'from:github subject:PR' --first"] },
  web: { use: 'Run the HTTP bridge directly for local app/API work.', examples: ["mxr web --print-url", "mxr web --auto-port --print-url"] },
  daemon: { use: 'Start the daemon explicitly for debugging or process managers.', examples: ["mxr daemon --foreground", "mxr daemon --no-bridge"] },
  demo: { use: 'Try mxr with an isolated two-account synthetic inbox and prewarmed demo analytics before connecting real mail.', examples: ["mxr demo", "mxr demo --reset", "mxr demo --no-tui"] },
  setup: { use: 'Run guided first setup, or use the legacy fake-account helper.', examples: ["mxr setup", "mxr demo"] },
  'bug-report': { use: 'Create a sanitized diagnostic bundle for issues.', examples: ["mxr bug-report --stdout", "mxr bug-report --output report.md"] },
  reset: { use: 'Preview or wipe local runtime state while preserving credentials by default.', examples: ["mxr reset --hard --dry-run"] },
  burn: { use: 'Alias for destructive local runtime reset.', examples: ["mxr burn --dry-run"] },
  completions: { use: 'Generate shell completions.', examples: ["mxr completions zsh > ~/.zfunc/_mxr"] },
  version: { use: 'Print the installed mxr version.', examples: ["mxr version"] },
};

function readSnapshotBody(file) {
  const raw = readFileSync(join(SNAPSHOT_DIR, file), 'utf8');
  const parts = raw.split(/^---\s*$/m);
  return parts.slice(2).join('---').replace(/^\n+/, '').trimEnd();
}

function snapshotKeyToTokens(name) {
  return name.replace(new RegExp('^' + PREFIX), '').replace(/\.snap$/, '').split('_');
}

function parseHelpText(body) {
  const lines = body.split('\n');
  let descriptionLines = [];
  let usage = '';
  let i = 0;

  while (i < lines.length && !lines[i].startsWith('Usage:')) {
    descriptionLines.push(lines[i]);
    i++;
  }
  if (i < lines.length) {
    usage = lines[i].replace(/^Usage:\s*/, '').trim();
    i++;
  }

  const sections = {};
  let current = null;
  let buffer = [];
  const sectionHeader = /^([A-Z][A-Za-z ]+):\s*$/;

  for (; i < lines.length; i++) {
    const line = lines[i];
    const m = line.match(sectionHeader);
    if (m) {
      if (current) sections[current] = buffer.join('\n');
      current = m[1];
      buffer = [];
    } else {
      buffer.push(line);
    }
  }
  if (current) sections[current] = buffer.join('\n');

  return {
    description: descriptionLines.join('\n').trim(),
    usage,
    sections,
  };
}

function parseFlagBlock(text) {
  const flags = [];
  const lines = text.split('\n');
  let current = null;

  for (const raw of lines) {
    if (!raw.trim()) {
      if (current && current.body.length === 0) continue;
      if (current) {
        current.body.push('');
      }
      continue;
    }

    const flagMatch = raw.match(/^( {2,6})(-[A-Za-z],\s+)?(--[A-Za-z][\w-]*(?:\s+<[^>]+>)?(?:,\s+--[A-Za-z][\w-]*)?)\s*(.*)$/);
    const argMatch = raw.match(/^( {2,6})(\[?<?[A-Z_][\w-]*>?\]?)\s*(.*)$/);

    if (flagMatch && raw.match(/^\s+(?:-\w,?\s+)?--/)) {
      if (current) flags.push(current);
      current = {
        token: (flagMatch[2] || '').trim() + (flagMatch[2] ? ' ' : '') + flagMatch[3].trim(),
        body: flagMatch[4] ? [flagMatch[4].trim()] : [],
      };
    } else if (argMatch && raw.match(/^\s{2,6}[<\[]/)) {
      if (current) flags.push(current);
      current = {
        token: argMatch[2].trim(),
        body: argMatch[3] ? [argMatch[3].trim()] : [],
      };
    } else if (current) {
      current.body.push(raw.trim());
    }
  }
  if (current) flags.push(current);

  return flags
    .map((f) => ({
      token: f.token,
      body: f.body.join(' ').replace(/\s+/g, ' ').trim(),
    }))
    .filter((f) => f.token);
}

function parseSubcommandBlock(text) {
  const subs = [];
  for (const line of text.split('\n')) {
    const m = line.match(/^\s{2,6}([\w-]+)\s+(.*)$/);
    if (m) subs.push({ name: m[1], summary: m[2].trim() });
  }
  return subs;
}

function escapePipes(s) {
  return s.replace(/\|/g, '\\|');
}

function flagsToTable(flags) {
  if (flags.length === 0) return '';
  let md = '| Flag | Description |\n|---|---|\n';
  for (const f of flags) {
    md += `| \`${escapePipes(f.token)}\` | ${escapePipes(f.body || '—')} |\n`;
  }
  return md;
}

function subsToTable(subs) {
  if (subs.length === 0) return '';
  let md = '| Subcommand | Purpose |\n|---|---|\n';
  for (const s of subs) {
    md += `| \`${escapePipes(s.name)}\` | ${escapePipes(s.summary || '—')} |\n`;
  }
  return md;
}

function renderSection(parsed, headingLevel = 2) {
  const h = '#'.repeat(headingLevel);
  let md = '';

  if (parsed.description) {
    md += parsed.description + '\n\n';
  }
  if (parsed.usage) {
    md += '```text\n' + parsed.usage + '\n```\n\n';
  }

  for (const [key, body] of Object.entries(parsed.sections)) {
    if (!body.trim()) continue;
    if (key === 'Arguments') {
      md += `${h} Arguments\n\n`;
      const flags = parseFlagBlock(body);
      md += flagsToTable(flags) + '\n';
    } else if (key === 'Options') {
      md += `${h} Options\n\n`;
      const flags = parseFlagBlock(body);
      md += flagsToTable(flags) + '\n';
    } else if (key === 'Commands') {
      md += `${h} Subcommands\n\n`;
      const subs = parseSubcommandBlock(body);
      md += subsToTable(subs) + '\n';
    } else {
      md += `${h} ${key}\n\n${body.trim()}\n\n`;
    }
  }

  return md;
}

function renderExamples(commandName) {
  const entry = COMMAND_EXAMPLES[commandName];
  if (!entry) return '';

  let md = `## Use When\n\n${entry.use}\n\n`;
  if (entry.examples?.length) {
    md += '## Everyday Examples\n\n```bash\n';
    md += entry.examples.join('\n');
    md += '\n```\n\n';
  }
  return md;
}

function tokensToSlug(tokens) {
  return tokens.join('-');
}

function tokensToCommandName(tokens) {
  return tokens.join(' ');
}

function main() {
  const files = readdirSync(SNAPSHOT_DIR).filter(
    (f) => f.startsWith(PREFIX) && f.endsWith('.snap'),
  );

  const rootFile = files.find((f) => f === `${PREFIX}root.snap`);
  if (!rootFile) {
    console.error('cli_help_root.snap missing — cannot derive top-level command list.');
    process.exit(1);
  }

  const rootBody = readSnapshotBody(rootFile);
  const rootParsed = parseHelpText(rootBody);
  const rootCommands = parseSubcommandBlock(rootParsed.sections.Commands || '');
  const topLevel = new Set(rootCommands.map((c) => c.name));

  // Group snapshots by top-level command. The first underscored token
  // is the parent (with `-` form preferred when it matches a known
  // top-level hyphenated command).
  const groups = new Map(); // commandName -> { own?: parsed, subs: { name, parsed }[] }

  for (const file of files) {
    if (file === rootFile) continue;
    const tokens = snapshotKeyToTokens(file);
    const body = readSnapshotBody(file);
    const parsed = parseHelpText(body);

    // Try progressively shorter joinings to find the parent top-level command.
    let parent = null;
    let tail = null;
    for (let n = tokens.length; n >= 1; n--) {
      const candidate = tokens.slice(0, n).join('-');
      if (topLevel.has(candidate)) {
        parent = candidate;
        tail = tokens.slice(n);
        break;
      }
    }

    if (!parent) {
      // Either a hyphenated command not snapshotted in root, or an orphan.
      // Fall back to first token.
      parent = tokens[0];
      tail = tokens.slice(1);
    }

    if (!groups.has(parent)) groups.set(parent, { own: null, subs: [] });
    const group = groups.get(parent);
    if (tail.length === 0) {
      group.own = parsed;
    } else {
      group.subs.push({ name: tail.join(' '), parsed });
    }
  }

  // Wipe and recreate the output directory, but preserve handwritten
  // sibling pages like `concepts.md` that aren't covered by snapshots.
  const PRESERVE = new Set(['concepts.md']);
  mkdirSync(OUT_DIR, { recursive: true });
  for (const f of readdirSync(OUT_DIR)) {
    if (PRESERVE.has(f)) continue;
    rmSync(join(OUT_DIR, f), { recursive: true, force: true });
  }

  // ------- Emit per-command pages -------
  const commandsForIndex = [];

  for (const cmd of rootCommands) {
    if (cmd.name === 'help') continue;

    const group = groups.get(cmd.name);
    const summary = cmd.summary;

    let md = `---
title: "mxr ${cmd.name}"
description: ${JSON.stringify(summary || `mxr ${cmd.name}`)}
---

> Generated from \`mxr ${cmd.name} --help\`. Edit the clap definitions in \`crates/daemon/src/cli/\` and re-run \`npm run build\` in \`site/\` to regenerate.

`;

    if (group?.own) {
      md += renderSection(group.own, 2);
    } else {
      md += `${summary || ''}\n\n`;
      md += '_No detailed `--help` snapshot is captured for this subcommand yet. See [`cli_help.rs`](https://github.com/planetaryescape/mxr/blob/main/crates/daemon/tests/cli_help.rs) to add one._\n\n';
    }

    md += renderExamples(cmd.name);

    if (group?.subs && group.subs.length > 0) {
      md += `## Sub-subcommands\n\n`;
      for (const sub of group.subs.sort((a, b) => a.name.localeCompare(b.name))) {
        md += `### \`mxr ${cmd.name} ${sub.name}\`\n\n`;
        md += renderSection(sub.parsed, 3);
      }
    }

    md += `\n## See also\n\n- [CLI overview](/reference/cli/) — full command index\n- [Concepts](/reference/cli/concepts/) — query operators, search modes, JSON shapes\n- [Automation contract](/guides/automation-contract/) — which commands support \`--format json\`, \`--dry-run\`, stdin\n`;

    const slug = cmd.name; // already in hyphenated form from root parse
    writeFileSync(join(OUT_DIR, `${slug}.md`), md);
    commandsForIndex.push({ name: cmd.name, summary });
  }

  // ------- Index page -------
  const groupsByLetter = new Map();
  for (const c of commandsForIndex) {
    const letter = c.name[0].toUpperCase();
    if (!groupsByLetter.has(letter)) groupsByLetter.set(letter, []);
    groupsByLetter.get(letter).push(c);
  }

  let index = `---
title: CLI reference
description: Every \`mxr\` subcommand. Generated from --help snapshots — never out of date with the binary.
---

> Generated from the captured \`--help\` snapshots at \`crates/daemon/tests/snapshots/cli_help__*.snap\`. To change a flag, change the clap definition in \`crates/daemon/src/cli/\` and the docs follow on rebuild.

mxr is a single binary with subcommands. Running \`mxr\` with no arguments launches the TUI.

For higher-level concepts (query operators, search modes, JSON output shapes), see [Concepts](/reference/cli/concepts/). For what's safe to script and pipe, see the [automation contract](/guides/automation-contract/).

## All commands

`;

  for (const letter of [...groupsByLetter.keys()].sort()) {
    index += `### ${letter}\n\n`;
    for (const c of groupsByLetter.get(letter).sort((a, b) => a.name.localeCompare(b.name))) {
      index += `- [\`mxr ${c.name}\`](/reference/cli/${c.name}/) — ${c.summary || '_no description_'}\n`;
    }
    index += '\n';
  }

  writeFileSync(join(OUT_DIR, 'index.md'), index);

  console.log(`Generated ${commandsForIndex.length} command pages in ${OUT_DIR}`);
}

main();

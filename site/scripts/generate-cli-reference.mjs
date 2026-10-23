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

import { existsSync, readFileSync, readdirSync, realpathSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(__dirname, '..', '..');
const SNAPSHOT_DIR = join(REPO_ROOT, 'crates', 'daemon', 'tests', 'snapshots');
const OUT_DIR = join(REPO_ROOT, 'site', 'src', 'content', 'docs', 'reference', 'cli');
const PREFIX = 'cli_help__cli_help_';
const SAVED_DRAFT_OWNER_GUIDANCE = 'For a saved draft, preview it with `mxr send DRAFT_ID --dry-run --format json`, then send with `--expected-revision REVISION_FROM_PREVIEW`, `--operation-id OPERATION_ID_FROM_PREVIEW` and `--expected-workspace \'<OWNER_JSON_FROM_PREVIEW>\'`. Use that same quoted owner for `mxr send-outcome` and for scheduling with `--at TIME`.';

const COMMAND_EXAMPLES = {
  // Shared with the clap-only example test; existing examples stay inline.
  ...JSON.parse(readFileSync(new URL('./cli-reference-examples.json', import.meta.url), 'utf8')),
  serve: {
    use: 'Inspect local pairing authority or run the foreground loopback gateway behind private HTTPS. Commands use the explicit local socket and ignore saved remote selection and MXR_DAEMON_ADDR. Approval and revocation preview by default; compare the pairing challenge before confirming.',
    examples: [
      'mxr serve --foreground --listen 127.0.0.1:18741 --dry-run',
      'mxr serve --foreground --listen 127.0.0.1:18741',
      'mxr serve status',
      'mxr serve pair list',
      'mxr serve pair approve REQUEST_ID --dry-run',
      'mxr serve pair approve REQUEST_ID --confirm',
      'mxr serve clients',
      'mxr serve clients revoke CLIENT_ID --dry-run',
      'mxr serve clients revoke CLIENT_ID --confirm',
    ],
  },
  connect: {
    use: 'Preview or save this installation\'s remote mailbox connection. Default pairing is read-only; --allow-send requests explicit host approval for portable compose and reviewed sending. Repeat the command with the same grant to claim it. Switching to local retires only this installation, with durable cleanup if the host is offline.',
    examples: [
      'mxr connect https://mail-host.example-tailnet.ts.net:8443 --name "My Mac" --dry-run',
      'mxr connect https://mail-host.example-tailnet.ts.net:8443 --name "My Mac" --confirm',
      'mxr connect https://mail-host.example-tailnet.ts.net:8443 --allow-send --dry-run',
      'mxr connect https://mail-host.example-tailnet.ts.net:8443 --allow-send --confirm',
      'mxr connect --status --format json',
      'mxr connect --local --dry-run',
      'mxr connect --local --confirm',
    ],
  },
  search: {
    use: 'Find messages fast, then pipe IDs or JSON into the next step.',
    examples: [
      "mxr search 'from:alice@example.com is:unread' --format json",
      "mxr search 'has:attachment subject:invoice' --format ids | mxr label receipts --dry-run",
      "mxr search 'is:owed-reply' --format ids   # threads you're the bottleneck on",
    ],
  },
  cat: {
    use: 'Read one message body, or resolve a search to the first matching message.',
    examples: ["mxr cat MESSAGE_ID --view reader", "mxr cat --search 'from:boss@example.com is:unread' --first"],
  },
  thread: {
    use: 'Read the whole conversation around a message or thread.',
    examples: ["mxr thread MESSAGE_ID --format json", "mxr thread --search 'subject:launch' --first"],
  },
  archive: {
    use: 'Clear mail from the inbox without deleting it. Always dry-run bulk queries first.',
    examples: ["mxr archive MESSAGE_ID", "mxr archive --search 'label:newsletters older_than:30d' --dry-run"],
  },
  'archive-operation': {
    use: 'Query a retained mailbox Archive operation or resume its undispatched items with the same operation ID. Unknown effects are never repeated. Supported formats are table, json, jsonl, and ids; CSV is rejected.',
    examples: [
      "mxr archive-operation --format json",
      "mxr archive-operation OPERATION_ID --format json",
      "mxr archive-operation OPERATION_ID --submit --format json",
    ],
  },
  'read-archive': {
    use: 'Mark something done in one step: read it and remove it from the inbox.',
    examples: ["mxr read-archive MESSAGE_ID", "mxr read-archive --search 'from:no-reply@example.com older_than:7d' --dry-run"],
  },
  trash: {
    use: 'Move mail to trash after previewing the target set.',
    examples: ["mxr trash MESSAGE_ID --dry-run", "mxr trash --search 'from:spam@example.com' --yes"],
  },
  spam: {
    use: 'Report unwanted mail as spam through the provider-backed mutation path.',
    examples: ["mxr spam MESSAGE_ID --dry-run", "mxr spam --search 'from:spam@bad.example' --yes"],
  },
  star: {
    use: 'Mark important messages for follow-up.',
    examples: ["mxr star MESSAGE_ID", "mxr star --search 'from:manager@example.com subject:review' --dry-run"],
  },
  unstar: { use: 'Remove stars from messages.', examples: ["mxr unstar MESSAGE_ID"] },
  read: { use: 'Mark messages read.', examples: ["mxr read --search 'label:alerts older_than:14d' --dry-run"] },
  unread: { use: 'Put messages back into an unread workflow.', examples: ["mxr unread MESSAGE_ID"] },
  label: {
    use: 'Apply a label/folder to messages, including piped IDs from search.',
    examples: ["mxr label receipts MESSAGE_ID", "mxr search 'from:billing@example.com' --format ids | mxr label receipts --yes"],
  },
  unlabel: { use: 'Remove a label from messages.', examples: ["mxr unlabel receipts MESSAGE_ID"] },
  move: { use: 'Move messages to a folder-like label.', examples: ["mxr move Archive MESSAGE_ID"] },
  snooze: {
    use: 'Hide mail until a useful time instead of leaving it as inbox noise.',
    examples: ["mxr snooze MESSAGE_ID --until 'monday 9am'", "mxr snooze --search 'from:newsletter@example.com is:unread' --until weekend --dry-run"],
  },
  unsnooze: { use: 'Wake snoozed messages early.', examples: ["mxr unsnooze MESSAGE_ID", "mxr unsnooze --all --dry-run"] },
  unsubscribe: {
    use: 'Unsubscribe from a single message id, or from every message by a given sender address. Passing an email address is shorthand for `--search "from:<addr>"`; the daemon picks the most recent match and uses its List-Unsubscribe header.',
    examples: [
      "mxr unsubscribe alice@example.com --dry-run",
      "mxr unsubscribe alice@example.com bob@example.com --yes",
      "mxr unsubscribe MESSAGE_ID --dry-run",
      "mxr unsubscribe --search 'list:<news.example.com>' --dry-run",
    ],
  },
  compose: {
    use: 'Write a new message in $EDITOR or from stdin in scripts. `--yes` sends; before submission it prints the operation ID and recovery query to stderr. Rerunning a one-shot command creates a new draft, so query that ID after an uncertain response. `--draft` saves to `mxr drafts` without sending (the two are mutually exclusive, so no added flag can turn a draft into a send). Add `--check` to run the [pre-send safety pipeline](/guides/pre-send-safety/) against a transient draft built from these flags, useful in CI or pre-commit hooks. ' + SAVED_DRAFT_OWNER_GUIDANCE,
    examples: [
      "mxr compose --to alice@example.com --subject 'Friday'",
      "printf 'Approved' | mxr compose --to alice@example.com --subject 'Re: plan' --body-stdin --dry-run",
      "mxr compose --to alice@example.com --body 'Draft it.' --draft   # saves, never sends",
      "mxr compose --to alice@example.com --body 'see attached' --check --format json   # warns: missing attachment",
    ],
  },
  reply: {
    use: `Reply to one message, interactively or from an agent-approved body. \`--yes\` prints the operation ID and recovery query to stderr before sending; query it after an uncertain response instead of rerunning, which creates a new draft. \`--draft\` saves a threaded draft. ${SAVED_DRAFT_OWNER_GUIDANCE}`,
    examples: ["mxr reply MESSAGE_ID", "mxr reply MESSAGE_ID --body 'On it.' --yes", "mxr reply MESSAGE_ID --body 'Draft it.' --draft   # saves, never sends"],
  },
  'reply-all': {
    use: `Reply to everyone on a thread. \`--yes\` prints the operation ID and recovery query to stderr before sending; query it after an uncertain response instead of rerunning, which creates a new draft. \`--draft\` saves a threaded draft. ${SAVED_DRAFT_OWNER_GUIDANCE}`,
    examples: ["mxr reply-all MESSAGE_ID --body-stdin --dry-run", "mxr reply-all MESSAGE_ID --body 'Thanks all.' --draft   # saves, never sends"],
  },
  forward: {
    use: `Forward a message with optional context. \`--yes\` prints the operation ID and recovery query to stderr before sending; query it after an uncertain response instead of rerunning, which creates a new draft. \`--draft\` saves the forward as a draft. ${SAVED_DRAFT_OWNER_GUIDANCE}`,
    examples: ["mxr forward MESSAGE_ID --to teammate@example.com", "mxr forward MESSAGE_ID --to teammate@example.com --body 'FYI' --draft   # saves, never sends"],
  },
  drafts: {
    use: 'List and edit drafts, preview linked deletion, or create/update one linked provider draft.',
    examples: [
      "mxr drafts --format json",
      "mxr drafts delete DRAFT_ID --dry-run",
      "mxr drafts push DRAFT_ID --dry-run",
    ],
  },
  send: {
    use: 'Preview a saved draft, then send or schedule the reviewed revision. Keep the `expected_workspace` JSON from that original preview and pass it unchanged, shell-quoted, with `--expected-workspace` on send, schedule and outcome queries. For an immediate send, keep the preview operation ID and query it with `mxr send-outcome` after a lost response; `unknown` is not permission to retry. The scheduler assigns a separate identity: query its latest outcome with `mxr send-outcome "draft:DRAFT_ID" --account ACCOUNT_ID --expected-workspace \'<OWNER_JSON_FROM_SCHEDULE_PREVIEW>\'`. Add `--check` to run the [pre-send safety pipeline](/guides/pre-send-safety/) without sending; exit 2 on Blocker. Use `--override-safety <TOKEN>` with the reviewed revision, original workspace and operation ID to bypass a Blocker the previous `--check` minted.',
    examples: [
      "mxr send DRAFT_ID --dry-run --format json   # retain the revision, operation ID and expected_workspace JSON",
      "mxr send DRAFT_ID --expected-revision REVISION_FROM_PREVIEW --expected-workspace '<OWNER_JSON_FROM_PREVIEW>' --operation-id OPERATION_ID_FROM_PREVIEW",
      "mxr send DRAFT_ID --at 'tomorrow 9am' --expected-revision REVISION_FROM_PREVIEW --expected-workspace '<OWNER_JSON_FROM_PREVIEW>'",
      "mxr send DRAFT_ID --check --format json   # safety report, no send",
      "mxr send DRAFT_ID --expected-revision REVISION_FROM_PREVIEW --expected-workspace '<OWNER_JSON_FROM_PREVIEW>' --operation-id OPERATION_ID_FROM_PREVIEW --override-safety OVERRIDE_TOKEN_FROM_CHECK   # after preview and --check",
    ],
  },
  'send-outcome': {
    use: 'Query the durable outcome with the workspace JSON retained from the original send preview. Do not infer a new owner after reconnect; `unknown` is not permission to retry.',
    examples: [
      "mxr send-outcome OPERATION_ID --account ACCOUNT_ID --expected-workspace '<OWNER_JSON_FROM_ORIGINAL_PREVIEW>' --format json",
      "mxr send-outcome 'draft:DRAFT_ID' --account ACCOUNT_ID --expected-workspace '<OWNER_JSON_FROM_SCHEDULE_PREVIEW>' --format json",
    ],
  },
  unsend: { use: 'Cancel a scheduled send while keeping the draft.', examples: ["mxr unsend DRAFT_ID"] },
  accounts: { use: 'Add, test, repair, disable, or inspect accounts and aliases.', examples: ["mxr accounts add gmail", "mxr accounts addresses add --account work alias@example.com"] },
  sync: {
    use: 'Trigger a sync and return, or hold until the account goes idle. The trigger acks as soon as the sync has started; `--wait` polls for idle and prints live progress. `--wait-timeout-secs` bounds the wait, not the sync.',
    examples: [
      "mxr sync",
      "mxr sync --wait --wait-timeout-secs 900   # a first backfill outlasts the 60s default",
      "mxr sync --status --format json | jq '.[] | {account_name, sync_in_progress, progress}'",
    ],
  },
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
    use: "Browse the local user-activity log: the git-reflog of your inbox. Strictly local; never transmitted off-device. See the [Activity Log guide](/guides/activity-log/) for the full design.",
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
  rules: { use: 'Create deterministic filing/mutation rules and dry-run them before enabling.', examples: ["mxr rules validate --when 'from:billing@example.com' --then 'label:receipts'", "mxr rules dry-run --all --after 2026-01-01"] },
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
  summarize: { use: 'Summarize long threads through the configured LLM.', examples: ["mxr summarize THREAD_ID", "mxr summarize --search 'from:team@example.com newer_than:7d' --limit 5"] },
  'draft-assist': { use: 'Generate a draft body grounded in a thread, then review before sending.', examples: ["mxr draft-assist THREAD_ID 'Polite follow-up with next steps'", "mxr draft-assist --search 'from:sarah@example.com newer_than:14d' --first '1:1 agenda'"] },
  subscriptions: { use: 'Find mailing-list candidates for unsubscribe review. The rank is a local READ-state proxy, not a distinct-open count; it sorts the bounded set returned by the query. Set `--account` to choose an account; when omitted, mxr uses the configured default account when present.', examples: ["mxr subscriptions --account personal --limit 200 --rank --format json"] },
  storage: { use: 'Find large senders, MIME types, labels, or individual messages consuming disk.', examples: ["mxr storage --by sender --limit 20", "mxr storage --by message --format ids | mxr archive --dry-run"] },
  contacts: { use: 'Surface relationship asymmetry or decay.', examples: ["mxr contacts asymmetry --limit 20", "mxr contacts decay --threshold-days 45"] },
  'response-time': { use: 'Measure reply latency with a counterparty or account.', examples: ["mxr response-time --theirs --counterparty alice@example.com --format json"] },
  stale: { use: 'Find threads where someone owes a reply.', examples: ["mxr stale --theirs --older-than-days 7", "mxr stale --mine --format ids | mxr snooze --until tomorrow --dry-run"] },
  owed: {
    use: "List the replies you owe: the desk's You owe lane in full for one account (people you have written to, mail still in the inbox), most overdue against your usual reply time first. `--all` lists the raw set instead, every thread whose latest inbound has no later reply from you, which is what `is:owed-reply` matches in search. See the [desk reference](/reference/desk-and-places/#lanes).",
    examples: [
      "mxr owed --format ids",
      "mxr owed --since 7 --format json | jq '.[] | {who:.from_email, days:.waiting_days, score:.overdue_score}'",
      "mxr owed --all --within 60 --format ids   # the raw list, archived and automated mail included",
    ],
  },
  commitments: {
    use: 'List, keep or resolve promises made in mail. `detect` finds the promises in a message you sent; `add` keeps one so it comes back under Due on the desk. See [promises on send](/guides/focus-and-reply/#keep-the-promises-you-make) and the [forgotten-work guide](/guides/forgotten-work/).',
    examples: [
      "mxr commitments --status open --format json",
      "mxr commitments detect SENT_MESSAGE_ID",
      "mxr commitments add SENT_MESSAGE_ID --what 'send the deck' --due 'fri 9am' --dry-run",
      "mxr commitments resolve COMMITMENT_ID",
    ],
  },
  messages: {
    use: "People you talk with, one row each, with your conversations inside as topics: Your turn, Pinned, Recent and Quiet. `person` shows one person's page with each message's new text; `ack` sends Got it (preview with `--dry-run`); `merge` makes several addresses one person, by hand. See [Messages](/guides/messages/).",
    examples: [
      "mxr messages list --turn mine",
      "mxr messages list --unread --past-pace",
      "mxr messages --format json | jq -r '.your_turn[] | .title'",
      "mxr messages person samir@launchpad.example",
      "mxr messages ack THREAD_ID --dry-run",
      "mxr messages merge --suggestions",
    ],
  },
  now: {
    use: 'Start with what needs you now: people waiting on you, due tasks, the Updates card and an evening Reading pick. Each section links to its mode. See [Now](/guides/now/).',
    examples: [
      'mxr now',
      'mxr now --format json',
      'mxr now --format ids',
    ],
  },
  arrivals: {
    use: 'See where arriving mail went, counted once per email, then inspect the messages behind a mode count. See [arrivals](/guides/now/#see-where-everything-that-arrived-went).',
    examples: [
      'mxr arrivals --format json',
      'mxr arrivals list --mode reading --format json',
      'mxr arrivals list --mode todo --limit 20',
    ],
  },
  reclassify: {
    use: 'Correct where an email belongs. A message move changes that email; --sender applies a sender preference to current and future mail. To do and Archive add membership rather than replacing every other mode. See [mode corrections](/guides/email-modes/#sorting-rules-and-categories-serve-different-jobs).',
    examples: [
      'mxr reclassify MESSAGE_ID reading --dry-run',
      'mxr reclassify MESSAGE_ID reading',
      'mxr reclassify MESSAGE_ID updates --sender --dry-run',
      'mxr reclassify MESSAGE_ID updates --sender',
      'mxr corrections --format json   # keep the correction id for undo',
    ],
  },
  corrections: {
    use: 'Inspect your mode moves and sender preferences, or undo a correction by its numeric id. Undoing the same correction twice changes nothing. See [mode corrections](/guides/email-modes/#sorting-rules-and-categories-serve-different-jobs).',
    examples: [
      'mxr corrections --format json',
      'mxr corrections undo CORRECTION_ID',
    ],
  },
  keys: {
    use: 'List every keyboard shortcut in the TUI and the web app from mxr\'s one key definition, or ask what a key does in a place. See [keybindings](/reference/keybindings/).',
    examples: [
      'mxr keys --client tui --context list',
      'mxr keys explain e --context todo',
      'mxr keys check',
      'mxr keys --client web --format json',
    ],
  },
  classify: {
    use: 'Grade stored sender, home and record answers against labelled scenarios, and inspect each mismatch. Measure asks with per-kind precision and recall and field exact-match. Read-only. Only the demo mailbox carries labels. See [Measure how well mxr sorts](/guides/email-modes/#measure-how-well-mxr-sorts).',
    examples: [
      'mxr demo --no-tui',
      'mxr classify report --limit 1000 --format json',
      'mxr classify eval',
      'mxr classify eval --set full',
      'mxr classify eval --compare POLICY --format json',
      'mxr classify eval --set full --format json   # every mismatch',
      'mxr demo stop',
    ],
  },
  'thread-preview': {
    use: 'Say what up to 100 threads are without reading them: subject, latest sender, date, snippet, message and unread counts and an attachment flag, with no bodies. Unknown thread ids are left out. See [JSON output](/reference/json-output/).',
    examples: [
      'mxr thread-preview THREAD_ID_1 THREAD_ID_2 --format json',
      'mxr thread-preview THREAD_ID_1 THREAD_ID_2 --format csv',
    ],
  },
  modes: {
    use: 'Look up a mode\'s purpose, keys and hints, inspect overlapping membership, or preview finishing a conversation in one mode. See [email modes](/guides/email-modes/).',
    examples: [
      'mxr modes explain',
      'mxr modes explain todo --format json',
      'mxr modes rail --format json',
      'mxr modes why MESSAGE_ID --format json',
      'mxr modes done THREAD_ID --mode messages --dry-run',
      'mxr modes done THREAD_ID --mode messages',
      'mxr modes hint todo.runway --show',
    ],
  },
  todo: {
    use: 'Act on tasks by their act-by dates, add or correct a task from mail, and review older work in the first-run catch-up. Preview changes before applying them. See [To do](/guides/todo/) and [later-evidence review](/guides/todo-resolution/).',
    examples: [
      'mxr todo --format json',
      'mxr todo why TODO_ID',
      'mxr todo add --from MESSAGE_ID --title "Send the signed form" --due "in 2d" --dry-run',
      'mxr todo edit TODO_ID "due=in 3d" --dry-run',
      'mxr todo catchup --format json',
      'mxr todo catchup --keep TODO_ID --dry-run',
      'mxr todo done TODO_ID --dry-run',
      'mxr todo done TODO_ID',
    ],
  },
  updates: {
    use: 'Read the twice-daily notification digest and the mail behind its facts. Without --dry-run, let-go previews and commits that exact selection in one invocation. See [Updates](/guides/updates/).',
    examples: [
      'mxr updates --format json',
      'mxr updates --expired --format json',
      'mxr updates let-go --dry-run --format json',
      'mxr updates let-go --format json',
    ],
  },
  records: {
    use: 'Find receipts, orders, bookings and documents filed in Archive. Inspect a field\'s source, ask for a reference, review recurring subscriptions or preview an export. Filing a record does not archive its source email at the provider. See [Archive](/guides/archive/).',
    examples: [
      'mxr records --format json',
      'mxr records show RECORD_ID --format json',
      'mxr records ask "lisbon booking ref" --no-fallback',
      'mxr records subscriptions --format json',
      'mxr records file MESSAGE_ID --dry-run',
      'mxr records export --year 2026 --out receipts.csv --dry-run',
      'mxr records export --year 2026 --out receipts.csv',
    ],
  },
  desk: {
    use: 'See what needs you: replies you owe, promises coming due, threads waiting on someone and new mail from people. `done` puts a row away; preview it with `--dry-run`. See [Clear the desk](/guides/now/) and the [desk reference](/reference/desk-and-places/).',
    examples: [
      "mxr desk",
      "mxr desk --format json | jq -r '.owed.rows[] | .subject'",
      "mxr desk done THREAD_ID --dry-run",
      "mxr desk done THREAD_ID",
    ],
  },
  reading: {
    use: 'Newsletters as an edition: since your last visit, earlier this week and fading, ranked by what you read, with a Later shelf. See [Read newsletters as an edition](/guides/reading/). Items can also be read in [Marginalia](/guides/marginalia/), which sends each item the first time and reads its highlights back.',
    examples: [
      "mxr reading",
      "mxr reading edition --peek --format json",
      "mxr reading open ITEM --article",
      "mxr reading later --add ITEM",
      "mxr reading let-go --all --dry-run",
      "mxr reading export --markdown",
      "mxr reading marginalia connect --url https://reader.example.com   # asks for the token, or reads it from stdin",
      "mxr reading marginalia send ITEM --dry-run",
      "mxr reading marginalia sync",
    ],
  },
  'paper-trail': {
    use: 'Receipts, notifications and other automated mail in your inbox, bundled by sender. See [Reading and Paper trail](/guides/reading-and-paper-trail/).',
    examples: ["mxr paper-trail --messages 5", "mxr paper-trail --format ids"],
  },
  sweep: {
    use: 'Archive everything unpinned in Reading or Paper trail, or one sender\'s bundle there. Preview first; the real sweep archives only what the preview listed.',
    examples: [
      "mxr sweep paper-trail --dry-run",
      "mxr sweep paper-trail --sender receipts@shop.example --yes",
      "mxr undo MUTATION_ID",
    ],
  },
  why: {
    use: 'Explain why a message is on the desk, in Reading, in Paper trail or screened out.',
    examples: ["mxr why MESSAGE_ID", "mxr why MESSAGE_ID --format json"],
  },
  pin: {
    use: 'Keep messages in place when you sweep Reading or Paper trail. Pins are local to this machine.',
    examples: ["mxr pin MESSAGE_ID", "mxr unpin MESSAGE_ID"],
  },
  unpin: { use: 'Unpin messages so the next sweep takes them too.', examples: ["mxr unpin MESSAGE_ID"] },
  time: {
    use: 'Preview how a time phrase resolves before a flag uses it. See [time phrases](/reference/time-phrases/).',
    examples: ["mxr time 'fri 3'", "mxr time 'in 2d' --format json"],
  },
  chimes: {
    use: 'Turn notification sounds on or off and pick the sound for each event. The web app shares this setting.',
    examples: ["mxr chimes status", "mxr chimes enable", "mxr chimes set archived pop"],
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
    use: 'List or rebuild the citation-backed decision log. Idempotent on unchanged threads. See [archive intelligence](/guides/archive-intelligence/#the-decision-log--mxr-decisions).',
    examples: [
      "mxr decisions --topic pricing --format json",
      "mxr decisions rebuild --since 180",
      "mxr decisions --since 30 --format ids | xargs -I{} mxr decisions show {}",
    ],
  },
  briefing: {
    use: 'Re-enter old context fast: what matters before reading a thread, and cached, citation-backed summaries of dormant threads or contacts after a long gap. See [briefings and loop-in](/guides/briefings-and-loop-in/).',
    examples: [
      "mxr briefing context THREAD_ID",
      "mxr briefing context THREAD_ID --no-ai --format json",
      "mxr briefing thread THREAD_ID",
      "mxr briefing thread THREAD_ID --refresh --format json",
      "mxr briefing recipient alice@example.com --format json",
    ],
  },
  cadence: {
    use: 'Watchlist for relationships you chose to maintain. Surfaces drift against an explicit expected interval and never auto-watches contacts. See [timing and cadence](/guides/timing-and-cadence/#cadence-drift-mxr-cadence).',
    examples: [
      "mxr cadence watch alice@example.com --every 14d",
      "mxr cadence list --format json",
      "mxr cadence drift --format ids | xargs -I{} mxr sender {}",
      "mxr cadence unwatch alice@example.com",
    ],
  },
  'send-time': {
    use: "Show the recipient's typical reply-time bucket from local `reply_pairs`. Statistical only: no LLM, no tracking pixels. See [timing and cadence](/guides/timing-and-cadence/).",
    examples: [
      "mxr send-time alice@example.com",
      "mxr send-time alice@example.com --at 'fri 19:00' --format json",
      "mxr send-time alice@example.com bob@example.com --format json   # worst meaningful delta wins",
    ],
  },
  expert: {
    use: 'Identify locally who has answered similar questions before. Ranks answerers above askers; citations point at answer messages, not just topic matches. See [briefings and loop-in](/guides/briefings-and-loop-in/#whos-the-expert-mxr-expert).',
    examples: [
      "mxr expert MESSAGE_ID",
      "mxr expert --query 'DKIM setup' --format json",
      "mxr expert MESSAGE_ID --include-self --limit 10",
    ],
  },
  whois: {
    use: 'Explain a person, project, or jargon term using only locally cited mail evidence. No invented summaries when the corpus has nothing. See [briefings and loop-in](/guides/briefings-and-loop-in/#personal-knowledge-graph-mxr-whois).',
    examples: [
      "mxr whois sam",
      "mxr whois 'Project Apollo' --format json",
      "mxr whois alice@example.com --limit 20",
    ],
  },
  'suggest-recipients': {
    use: 'Suggest "maybe include" Cc recipients for a draft based on co-participation in similar prior threads. Suggestions only, never auto-adds; never reveals Bcc. See [briefings and loop-in](/guides/briefings-and-loop-in/#maybe-include-mxr-suggest-recipients).',
    examples: [
      "mxr suggest-recipients --draft DRAFT_ID --format json",
      "echo 'rollout plan attached' | mxr suggest-recipients --subject 'pricing rollout' --body-stdin",
      "mxr suggest-recipients --draft DRAFT_ID --limit 3",
    ],
  },
  wrapped: { use: 'Generate year-to-date or yearly email analytics.', examples: ["mxr wrapped --ytd", "mxr wrapped --year 2025 --format json"] },
  export: { use: 'Turn threads into Markdown/JSON/LLM context.', examples: ["mxr export THREAD_ID --format markdown", "mxr export --search 'from:legal@example.com' --format markdown > legal.md"] },
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
  llm: { use: 'Check which LLM provider and model each feature uses, and manage the Anthropic and OpenRouter keys for Claude models. See [LLM features](/guides/llm-features/#use-claude-through-anthropic-with-openrouter-as-fallback).', examples: ["mxr llm status", "mxr llm status --format json", "op read \"op://<vault>/<item>/credential\" | mxr llm key set anthropic", "mxr llm key status", "mxr llm key remove anthropic --dry-run"] },
  semantic: {
    use: 'Manage local semantic search profiles and indexing. `reindex` is resumable and skips what is already embedded; `reindex --force` re-embeds everything.',
    examples: [
      "mxr semantic status",
      "mxr semantic profile install bge-small-en-v1.5",
      "mxr semantic reindex",
      "mxr semantic reindex --force   # recover a corrupt or half-migrated index",
    ],
  },
  open: { use: 'Open message URLs in your browser from CLI selection.', examples: ["mxr open MESSAGE_ID", "mxr open --search 'from:github@example.com subject:PR' --first"] },
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

function readSnapshotBody(snapshotDir, file) {
  const raw = readFileSync(join(snapshotDir, file), 'utf8');
  const parts = raw.split(/^---\s*$/m);
  return parts.slice(2).join('---').replace(/^\n+/, '').trimEnd();
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
    md += `| \`${escapePipes(f.token)}\` | ${escapePipes(f.body || '')} |\n`;
  }
  return md;
}

function subsToTable(subs) {
  if (subs.length === 0) return '';
  let md = '| Subcommand | Purpose |\n|---|---|\n';
  for (const s of subs) {
    md += `| \`${escapePipes(s.name)}\` | ${escapePipes(s.summary || '')} |\n`;
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

export function generateReference(snapshotDir = SNAPSHOT_DIR, outDir = OUT_DIR) {
  const files = readdirSync(snapshotDir).filter(
    (f) => f.startsWith(PREFIX) && f.endsWith('.snap'),
  );

  const rootFile = files.find((f) => f === `${PREFIX}root.snap`);
  if (!rootFile) {
    throw new Error('cli_help_root.snap missing; cannot derive top-level command list.');
  }

  const rootBody = readSnapshotBody(snapshotDir, rootFile);
  const rootParsed = parseHelpText(rootBody);
  const rootCommands = parseSubcommandBlock(rootParsed.sections.Commands || '');

  // Usage supplies canonical spelling even for legacy snapshot filenames such
  // as daemon_dial_stdio, or snapshots captured through a command alias.
  const groups = new Map(); // commandName -> { own?: parsed, subs: { name, parsed }[] }

  for (const file of files) {
    if (file === rootFile) continue;
    const body = readSnapshotBody(snapshotDir, file);
    const parsed = parseHelpText(body);
    const commandPath = parsed.usage.match(/^mxr ([a-z][a-z0-9-]*(?: [a-z][a-z0-9-]*)*)/);
    if (!commandPath) {
      throw new Error(`Cannot read command path from Usage in ${file}.`);
    }
    const [parent, ...tail] = commandPath[1].split(' ');

    if (!groups.has(parent)) groups.set(parent, { own: null, subs: [] });
    const group = groups.get(parent);
    if (tail.length === 0) {
      group.own = parsed;
    } else if (!group.subs.some((sub) => sub.name === tail.join(' '))) {
      group.subs.push({ name: tail.join(' '), parsed });
    }
  }

  // Validate the whole visible tree before touching the last generated output.
  // Commands sections contain canonical visible children; clap's generated
  // `help` command has no application reference page or dedicated snapshot.
  function requireHelp(parsed, path) {
    for (const child of parseSubcommandBlock(parsed.sections.Commands || '')) {
      if (child.name === 'help') continue;
      const childPath = [...path, child.name];
      const group = groups.get(childPath[0]);
      const help = childPath.length === 1
        ? group?.own
        : group?.subs.find((sub) => sub.name === childPath.slice(1).join(' '))?.parsed;
      if (!help?.usage) {
        throw new Error(`Missing help snapshot for mxr ${childPath.join(' ')}; add its --help case in crates/daemon/tests/cli_help.rs.`);
      }
      requireHelp(help, childPath);
    }
  }
  if (!rootParsed.usage) throw new Error('cli_help_root.snap has no Usage line.');
  requireHelp(rootParsed, []);

  // Wipe and recreate the output directory, but preserve handwritten
  // sibling pages like `concepts.md` that aren't covered by snapshots.
  const PRESERVE = new Set(['concepts.md']);
  mkdirSync(outDir, { recursive: true });
  for (const f of readdirSync(outDir)) {
    if (PRESERVE.has(f)) continue;
    rmSync(join(outDir, f), { recursive: true, force: true });
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

> Generated from \`mxr ${cmd.name} --help\`. After editing \`crates/daemon/src/cli/\`, update and review the snapshots with \`scripts/cargo-test -p mxr --test cli_help\`, then rebuild \`site/\`.

`;

    md += renderSection(group.own, 2);

    md += renderExamples(cmd.name);

    if (group?.subs && group.subs.length > 0) {
      md += `## Sub-subcommands\n\n`;
      for (const sub of group.subs.sort((a, b) => a.name.localeCompare(b.name))) {
        md += `### \`mxr ${cmd.name} ${sub.name}\`\n\n`;
        md += renderSection(sub.parsed, 3);
      }
    }

    md += `\n## See also\n\n- [CLI overview](/reference/cli/): full command index\n- [Concepts](/reference/cli/concepts/): query operators, search modes, JSON shapes\n- [Automation contract](/guides/automation-contract/): which commands support \`--format json\`, \`--dry-run\`, stdin\n`;

    const slug = cmd.name; // already in hyphenated form from root parse
    writeFileSync(join(outDir, `${slug}.md`), md);
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
description: Every \`mxr\` subcommand. Generated from --help snapshots, so it matches the binary.
---

> Generated from the captured \`--help\` snapshots at \`crates/daemon/tests/snapshots/cli_help__*.snap\`. After changing a clap definition in \`crates/daemon/src/cli/\`, update and review the snapshots with \`scripts/cargo-test -p mxr --test cli_help\`, then rebuild the site. Missing command-help inputs fail generation.

mxr is a single binary with subcommands. Running \`mxr\` with no arguments launches the TUI.

For higher-level concepts (query operators, search modes, JSON output shapes), see [Concepts](/reference/cli/concepts/). For what's safe to script and pipe, see the [automation contract](/guides/automation-contract/).

## All commands

`;

  for (const letter of [...groupsByLetter.keys()].sort()) {
    index += `### ${letter}\n\n`;
    for (const c of groupsByLetter.get(letter).sort((a, b) => a.name.localeCompare(b.name))) {
      index += `- [\`mxr ${c.name}\`](/reference/cli/${c.name}/): ${c.summary || '_no description_'}\n`;
    }
    index += '\n';
  }

  writeFileSync(join(outDir, 'index.md'), index);

  console.log(`Generated ${commandsForIndex.length} command pages in ${outDir}`);
}

if (process.argv[1] && existsSync(process.argv[1]) && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  generateReference();
}

import { HttpResponse, http } from "msw";
import { setupServer } from "msw/node";

const BASE_URL = "http://127.0.0.1:4010";
const ACCOUNT_ID = "11111111-1111-7111-8111-111111111111";

type DesktopMockOptions = {
  delayReadMutation?: Promise<void>;
  delayMailbox?: Promise<void>;
  delayMailboxLensKind?: string;
};

type MailboxState = {
  unreadByMessageId: Record<string, boolean>;
  starredByMessageId: Record<string, boolean>;
};

type MailboxRowFixture = {
  id: string;
  thread_id: string;
  provider_id: string;
  sender: string;
  sender_detail: string;
  subject: string;
  snippet: string;
  date_label: string;
  has_attachments: boolean;
};

type DesktopMockState = {
  mailboxState: MailboxState;
  inboxRows: MailboxRowFixture[];
  allMailRows: MailboxRowFixture[];
};

export type RecordedDesktopRequest = {
  method: string;
  url: string;
  path: string;
  body: string | null;
};

let currentOptions: DesktopMockOptions = {};
let currentState = createDesktopMockState();
let recordedRequests: RecordedDesktopRequest[] = [];

export const desktopMockServer = setupServer(
  http.all(new RegExp(`^${escapeRegExp(BASE_URL)}/.*$`), async ({ request }) => {
    const url = new URL(request.url);
    const path = url.pathname;
    const bodyText = await request.clone().text();

    recordedRequests.push({
      method: request.method,
      url: request.url,
      path,
      body: bodyText || null,
    });

    if (path === "/mailbox") {
      const lens = url.searchParams.get("lens_kind");
      const view = url.searchParams.get("view") ?? "threads";
      if (
        currentOptions.delayMailbox &&
        (!currentOptions.delayMailboxLensKind ||
          currentOptions.delayMailboxLensKind === lens)
      ) {
        await currentOptions.delayMailbox;
      }
      const isAllMail = lens === "all_mail";
      const isSubscriptions = lens === "subscription";
      const rows = (
        isSubscriptions ? currentState.allMailRows.slice(0, 2)
        : isAllMail ? currentState.allMailRows
        : currentState.inboxRows
      ).map((row) => rowWithFlags(currentState, row));
      const displayRows = view === "messages" ? rows : dedupeByThread(rows);
      return HttpResponse.json({
        mailbox: {
          lensLabel: isSubscriptions ? "Subscriptions" : isAllMail ? "All Mail" : "Inbox",
          view,
          counts: isSubscriptions
            ? { unread: 1, total: 2 }
            : isAllMail
              ? { unread: 24, total: 8124 }
              : { unread: unreadInboxCount(currentState), total: 144 },
          groups: [
            {
              id: isSubscriptions ? "subscriptions" : isAllMail ? "earlier" : "today",
              label: isSubscriptions ? "Subscriptions" : isAllMail ? "Earlier" : "Today",
              rows: displayRows.map((row) => ({
                ...row,
                kind: view === "messages" ? "message" : "thread",
                message_count:
                  view === "messages"
                    ? undefined
                    : rows.filter((candidate) => candidate.thread_id === row.thread_id).length,
              })),
            },
          ],
        },
        sidebar: {
          sections: [
            {
              id: "system",
              title: "System",
              items: [
                {
                  id: "inbox",
                  label: "Inbox",
                  unread: 12,
                  total: 144,
                  active: !isAllMail,
                  lens: { kind: "inbox" },
                },
                {
                  id: "all-mail",
                  label: "All Mail",
                  unread: 24,
                  total: 8124,
                  active: isAllMail && !isSubscriptions,
                  lens: { kind: "all_mail" },
                },
                {
                  id: "subscriptions",
                  label: "Subscriptions",
                  unread: 1,
                  total: 2,
                  active: isSubscriptions,
                  lens: { kind: "subscription" },
                },
              ],
            },
            {
              id: "labels",
              title: "Labels",
              items: [
                {
                  id: "follow-up",
                  label: "Follow Up",
                  unread: 3,
                  total: 42,
                  active: false,
                  lens: { kind: "label", labelId: "label-follow-up" },
                },
                {
                  id: "waiting",
                  label: "Waiting",
                  unread: 1,
                  total: 12,
                  active: false,
                  lens: { kind: "label", labelId: "label-waiting" },
                },
              ],
            },
            {
              id: "saved-searches",
              title: "Saved searches",
              items: [
                {
                  id: "saved-search-deploy",
                  label: "Deploys",
                  unread: 0,
                  total: 0,
                  active: false,
                  lens: { kind: "saved_search", savedSearch: "Deploys" },
                },
              ],
            },
          ],
        },
        shell: {
          accountLabel: "personal",
          syncLabel: "Synced",
          statusMessage: "Local-first and ready",
          commandHint: "Ctrl-p",
        },
      });
    }

    if (path === "/thread/thread-1/export") {
      return HttpResponse.json({ content: "# Deploy complete\n\nExport body" });
    }

    if (path === "/thread/thread-1") {
      return HttpResponse.json({
        thread: {
          id: "thread-1",
          subject: "Deploy complete",
          snippet: "Production deploy succeeded in 42 seconds.",
        },
        messages: [
          {
            ...rowWithFlags(currentState, currentState.inboxRows[0]),
            kind: "message",
          },
        ],
        bodies: [
          {
            message_id: "msg-1",
            text_plain: "Production deploy succeeded in 42 seconds.",
            text_html:
              '<p>Production deploy succeeded in <strong>42 seconds</strong>.</p><img src="https://cdn.example.com/deploy.png" alt="deploy chart" />',
            attachments: [
              {
                id: "attachment-1",
                filename: "deploy.log",
                mime_type: "text/plain",
                size_bytes: 1024,
                local_path: null,
              },
            ],
          },
        ],
        reader_mode: "reader",
        right_rail: {
          title: "Recent opens",
          items: ["Production deploy", "Billing alert"],
        },
      });
    }

    if (path === "/thread/thread-2") {
      return HttpResponse.json({
        thread: {
          id: "thread-2",
          subject: "Billing alert",
          snippet: "A customer payment needs manual review.",
        },
        messages: [
          {
            ...rowWithFlags(currentState, currentState.inboxRows[1]),
            kind: "message",
          },
        ],
        bodies: [
          {
            message_id: "msg-2",
            text_plain: "A customer payment needs manual review.",
            text_html: "<p>A customer payment needs <strong>manual review</strong>.</p>",
            attachments: [],
          },
        ],
        reader_mode: "reader",
        right_rail: {
          title: "Recent opens",
          items: ["Billing alert", "Production deploy"],
        },
      });
    }

    if (path === "/search") {
      const scope = url.searchParams.get("scope") ?? "threads";
      const mode = url.searchParams.get("mode") ?? "lexical";
      const sort = url.searchParams.get("sort") ?? "relevant";
      const explain = url.searchParams.get("explain") === "true";

      if (scope === "attachments") {
        return HttpResponse.json({
          scope,
          sort,
          mode,
          total: 1,
          groups: [
            {
              id: "results",
              label: "Results",
              rows: [
                {
                  ...rowWithFlags(currentState, currentState.inboxRows[0]),
                  kind: "attachment",
                  subject: "deploy.log",
                  snippet: "Deploy complete · Production deploy succeeded in 42 seconds.",
                  attachment_id: "attachment-1",
                  attachment_filename: "deploy.log",
                  attachment_size_bytes: 1024,
                },
              ],
            },
          ],
          explain: explain ? { mode, sort, query: url.searchParams.get("q") } : null,
        });
      }

      return HttpResponse.json({
        scope,
        sort,
        mode,
        total: 1,
        groups: [
          {
            id: "results",
            label: "Results",
            rows: [
              {
                ...rowWithFlags(currentState, currentState.inboxRows[0]),
                kind: scope === "messages" ? "message" : "thread",
                message_count: scope === "threads" ? 2 : undefined,
              },
            ],
          },
        ],
        explain: explain ? { mode, sort, query: url.searchParams.get("q") } : null,
      });
    }

    if (path === "/compose/session") {
      const payload = parseJsonBody<Record<string, unknown>>(bodyText);
      const kind = String(payload.kind ?? "new");
      return HttpResponse.json(composeSessionFor(kind));
    }

    if (path === "/compose/session/update") {
      const payload = parseJsonBody<Record<string, unknown>>(bodyText);
      return HttpResponse.json({
        session: {
          ...composeSessionFor("new").session,
          draftPath: String(payload.draft_path ?? "/tmp/new-draft.md"),
          frontmatter: {
            to: String(payload.to ?? ""),
            cc: String(payload.cc ?? ""),
            bcc: String(payload.bcc ?? ""),
            subject: String(payload.subject ?? ""),
            from: String(payload.from ?? "me@example.com"),
            attach: Array.isArray(payload.attach) ? payload.attach : [],
            references: [],
            in_reply_to: null,
          },
          previewHtml: `<p>${String(payload.subject ?? "")}</p>`,
          issues: payload.to
            ? []
            : [{ severity: "error", message: "No recipients (to: field is empty)" }],
        },
      });
    }

    if (path === "/compose/session/refresh") {
      return HttpResponse.json(composeSessionFor("new"));
    }

    if (path === "/compose/session/restore") {
      return HttpResponse.json({
        session: {
          ...composeSessionFor("new").session,
          draftPath: "/tmp/restored-draft.md",
          frontmatter: {
            to: "teammate@example.com",
            cc: "",
            bcc: "",
            subject: "Recovered draft",
            from: "me@example.com",
            attach: ["/tmp/wireframes.png"],
            references: [],
            in_reply_to: null,
          },
          bodyMarkdown: "Recovered body",
          previewHtml: "<p>Recovered body</p>",
          issues: [],
        },
      });
    }

    if (
      path === "/compose/session/send" ||
      path === "/compose/session/save" ||
      path === "/compose/session/discard"
    ) {
      return HttpResponse.json({ ok: true });
    }

    if (path === "/diagnostics/bug-report") {
      return HttpResponse.json({ content: "bug report body" });
    }

    if (path === "/diagnostics") {
      return HttpResponse.json({
        report: {
          healthy: true,
          health_class: "healthy",
          daemon_protocol_version: 1,
          daemon_version: "0.4.4",
          daemon_build_id: "build-123",
          lexical_index_freshness: "current",
          semantic_index_freshness: "disabled",
          database_path: "/tmp/mxr.db",
          index_path: "/tmp/mxr-index",
          log_path: "/tmp/mxr.log",
          log_size_bytes: 2048,
          recommended_next_steps: ["None"],
          recent_error_logs: [],
        },
      });
    }

    if (path === "/drafts") {
      return HttpResponse.json({
        drafts: [
          {
            id: "44444444-4444-7444-8444-444444444444",
            account_id: ACCOUNT_ID,
            subject: "Recovered draft",
            recipients: "teammate@example.com",
            updated_at: "2026-04-22T09:00:00Z",
            attachment_count: 1,
          },
        ],
      });
    }

    if (path === "/subscriptions") {
      return HttpResponse.json({
        subscriptions: [
          {
            account_id: ACCOUNT_ID,
            sender_name: "Vercel",
            sender_email: "notifications@vercel.com",
            message_count: 12,
            latest_message_id: "msg-1",
            latest_thread_id: "thread-1",
            latest_subject: "Deploy complete",
            latest_snippet: "Production deploy succeeded in 42 seconds.",
            latest_date: "2026-04-22T09:15:00Z",
            latest_has_attachments: true,
            unread: true,
          },
        ],
      });
    }

    if (path === "/snoozed") {
      return HttpResponse.json({
        snoozed: [
          {
            message_id: "msg-2",
            thread_id: "thread-2",
            sender: "Stripe",
            subject: "Billing alert",
            snippet: "A customer payment needs manual review.",
            wake_at: "2026-04-23T08:00:00Z",
            unread: true,
            has_attachments: true,
          },
        ],
      });
    }

    if (path === "/semantic/status") {
      return HttpResponse.json({
        status: {
          enabled: true,
          active_profile: "bge-small-en-v1.5",
          profiles: [],
          runtime: {
            queue_depth: 2,
            in_flight: 1,
          },
        },
      });
    }

    if (path === "/rules/detail") {
      return HttpResponse.json({
        rule: {
          id: "rule-1",
          name: "Archive receipts",
          condition: "from:stripe",
          action: "archive",
          priority: 100,
          enabled: true,
        },
      });
    }

    if (path === "/rules/history") {
      return HttpResponse.json({
        entries: [{ id: "hist-1", summary: "Matched 2 receipts" }],
      });
    }

    if (path === "/rules/dry-run") {
      return HttpResponse.json({
        results: [{ id: "dry-1", matched: 2, action: "archive" }],
      });
    }

    if (path === "/rules/form") {
      return HttpResponse.json({
        form: {
          id: "rule-1",
          name: "Archive receipts",
          condition: "from:stripe",
          action: "archive",
          priority: 100,
          enabled: true,
        },
      });
    }

    if (path === "/rules/upsert-form" || path === "/rules/upsert") {
      return HttpResponse.json({
        rule: {
          id: "rule-1",
          name: "Archive receipts",
          condition: "from:stripe",
          action: "archive",
          priority: 100,
          enabled: true,
        },
      });
    }

    if (path === "/rules") {
      return HttpResponse.json({
        rules: [
          {
            id: "rule-1",
            name: "Archive receipts",
            condition: "from:stripe",
            action: "archive",
            priority: 100,
            enabled: true,
          },
        ],
      });
    }

    if (path === "/accounts/default" || path === "/accounts/test" || path === "/accounts/upsert") {
      return HttpResponse.json({
        result: {
          ok: true,
          summary: "Account operation complete",
          save: { ok: true, detail: "Saved" },
          auth: null,
          sync: { ok: true, detail: "Sync ok" },
          send: { ok: true, detail: "Send ok" },
        },
      });
    }

    if (path === "/accounts") {
      return HttpResponse.json({
        accounts: [
          {
            account_id: ACCOUNT_ID,
            key: "personal",
            name: "Personal",
            email: "me@example.com",
            provider_kind: "gmail",
            sync_kind: "gmail",
            send_kind: "gmail",
            enabled: true,
            is_default: true,
            source: "both",
            editable: "full",
            sync: {
              type: "gmail",
              credential_source: "bundled",
              client_id: "",
              client_secret: null,
              token_ref: "gmail:personal",
            },
            send: {
              type: "gmail",
            },
          },
        ],
      });
    }

    if (path === "/attachments/open" || path === "/attachments/download") {
      return HttpResponse.json({
        file: {
          attachment_id: "attachment-1",
          filename: "deploy.log",
          path: "/tmp/deploy.log",
        },
      });
    }

    if (path === "/actions/snooze" || path === "/actions/unsubscribe") {
      return HttpResponse.json({ ok: true });
    }

    if (path === "/actions/snooze/presets") {
      return HttpResponse.json({
        presets: [
          { id: "tomorrow", label: "Tomorrow morning", wakeAt: "2026-03-23T08:00:00Z" },
          { id: "tonight", label: "Tonight", wakeAt: "2026-03-22T19:00:00Z" },
        ],
      });
    }

    if (path === "/mutations/read-and-archive") {
      const payload = parseJsonBody<{ message_ids?: string[] }>(bodyText);
      for (const messageId of payload.message_ids ?? []) {
        currentState.mailboxState.unreadByMessageId[messageId] = false;
      }
      if (currentOptions.delayReadMutation) {
        await currentOptions.delayReadMutation;
      }
      return HttpResponse.json({ ok: true });
    }

    if (path === "/mutations/read") {
      const payload = parseJsonBody<{ message_ids?: string[]; read?: boolean }>(bodyText);
      for (const messageId of payload.message_ids ?? []) {
        currentState.mailboxState.unreadByMessageId[messageId] = payload.read === false;
      }
      if (currentOptions.delayReadMutation) {
        await currentOptions.delayReadMutation;
      }
      return HttpResponse.json({ ok: true });
    }

    if (
      path === "/mutations/archive" ||
      path === "/mutations/trash" ||
      path === "/mutations/spam" ||
      path === "/mutations/labels" ||
      path === "/mutations/move" ||
      path === "/semantic/reindex" ||
      path === "/saved-searches/delete" ||
      path === "/labels/create" ||
      path === "/labels/rename" ||
      path === "/labels/delete"
    ) {
      return HttpResponse.json({ ok: true });
    }

    if (path === "/mutations/star") {
      const payload = parseJsonBody<{ message_ids?: string[]; starred?: boolean }>(bodyText);
      for (const messageId of payload.message_ids ?? []) {
        currentState.mailboxState.starredByMessageId[messageId] = Boolean(payload.starred);
      }
      return HttpResponse.json({ ok: true });
    }

    // Fallback: return ok for unhandled paths to prevent test hangs.
    // In production, these would be real API calls.
    return HttpResponse.json({ ok: true });
  }),
);

export function configureDesktopMockServer(options: DesktopMockOptions = {}) {
  currentOptions = options;
  currentState = createDesktopMockState();
  recordedRequests = [];
}

export function resetDesktopMockServer() {
  currentOptions = {};
  currentState = createDesktopMockState();
  recordedRequests = [];
}

export function getRecordedDesktopRequests() {
  return recordedRequests;
}

function createDesktopMockState(): DesktopMockState {
  return {
    mailboxState: {
      unreadByMessageId: {
        "msg-1": true,
        "msg-2": true,
        "msg-4": false,
        "msg-3": false,
      },
      starredByMessageId: {
        "msg-1": false,
        "msg-2": false,
        "msg-4": false,
        "msg-3": true,
      },
    },
    inboxRows: [
      {
        id: "msg-1",
        thread_id: "thread-1",
        provider_id: "gmail-msg-1",
        sender: "Vercel",
        sender_detail: "notifications@vercel.com",
        subject: "Deploy complete",
        snippet: "Production deploy succeeded in 42 seconds.",
        date_label: "2m",
        has_attachments: false,
      },
      {
        id: "msg-2",
        thread_id: "thread-2",
        provider_id: "gmail-msg-2",
        sender: "Stripe",
        sender_detail: "support@stripe.com",
        subject: "Billing alert",
        snippet: "A customer payment needs manual review.",
        date_label: "9m",
        has_attachments: true,
      },
      {
        id: "msg-4",
        thread_id: "thread-1",
        provider_id: "gmail-msg-4",
        sender: "Vercel",
        sender_detail: "notifications@vercel.com",
        subject: "Deploy follow-up",
        snippet: "Logs and rollout metrics attached.",
        date_label: "3m",
        has_attachments: true,
      },
    ],
    allMailRows: [
      {
        id: "msg-3",
        thread_id: "thread-3",
        provider_id: "gmail-msg-3",
        sender: "GitHub",
        sender_detail: "notifications@github.com",
        subject: "Review requested",
        snippet: "A pull request is waiting on your review.",
        date_label: "1d",
        has_attachments: false,
      },
    ],
  };
}

function unreadInboxCount(state: DesktopMockState) {
  return 10 + state.inboxRows.filter((row) => state.mailboxState.unreadByMessageId[row.id]).length;
}

function rowWithFlags(state: DesktopMockState, row: MailboxRowFixture) {
  return {
    ...row,
    unread: state.mailboxState.unreadByMessageId[row.id],
    starred: state.mailboxState.starredByMessageId[row.id],
  };
}

function dedupeByThread<T extends { thread_id: string }>(rows: T[]) {
  const seen = new Set<string>();
  return rows.filter((row) => {
    if (seen.has(row.thread_id)) {
      return false;
    }
    seen.add(row.thread_id);
    return true;
  });
}

function composeSessionFor(kind: string) {
  return {
    session: {
      draftPath: `/tmp/${kind}-draft.md`,
      rawContent:
        "---\n" +
        "to: reply@example.com\n" +
        "cc: ''\n" +
        "bcc: ''\n" +
        `subject: ${kind === "new" ? "Fresh draft" : "Re: Deploy complete"}\n` +
        "from: me@example.com\n" +
        "---\n\nDraft body",
      frontmatter: {
        to: kind === "new" ? "" : "reply@example.com",
        cc: "",
        bcc: "",
        subject: kind === "new" ? "Fresh draft" : "Re: Deploy complete",
        from: "me@example.com",
        attach: [],
        references: [],
        in_reply_to: null,
      },
      bodyMarkdown: "Draft body",
      previewHtml: `<p>${kind} preview</p>`,
      issues:
        kind === "new"
          ? [{ severity: "error", message: "No recipients (to: field is empty)" }]
          : [],
      cursorLine: 7,
      accountId: ACCOUNT_ID,
      kind,
      editorCommand: "nvim",
    },
  };
}

function parseJsonBody<T>(body: string) {
  if (!body) {
    return {} as T;
  }
  return JSON.parse(body) as T;
}

function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

import type { MailPane } from "@/state/mailboxPaneStore";

export interface ShortcutHint {
  key: string;
  label: string;
}

export interface ShortcutSection {
  title: string;
  hints: ShortcutHint[];
}

interface ShortcutContext {
  path: string;
  activePane: MailPane;
}

const globalHints: ShortcutHint[] = [
  { key: "⌘K", label: "Palette" },
  { key: "/", label: "Search" },
  { key: "?", label: "Help" },
  { key: "c", label: "Compose" },
];

export function shortcutSections({ path, activePane }: ShortcutContext): ShortcutSection[] {
  if (path.startsWith("/compose")) {
    return [
      {
        title: "Compose shortcuts",
        hints: [
          { key: "⌘K", label: "Palette" },
          { key: "Esc", label: "Leave dialogs" },
          { key: "Tab", label: "Move fields" },
          { key: "⌘Enter", label: "Send from confirmation" },
        ],
      },
    ];
  }

  if (path.startsWith("/search")) {
    return [
      {
        title: "Search shortcuts",
        hints: [
          { key: "/", label: "Focus query" },
          { key: "Enter", label: "Open result" },
          { key: "g i", label: "Inbox" },
          ...globalHints,
        ],
      },
    ];
  }

  if (path.startsWith("/analytics")) {
    return [
      {
        title: "Analytics shortcuts",
        hints: [
          { key: "1-0", label: "Sidebar nav" },
          { key: "g i", label: "Inbox" },
          ...globalHints,
        ],
      },
    ];
  }

  if (path.startsWith("/subscriptions")) {
    return [
      {
        title: "Subscriptions shortcuts",
        hints: [
          { key: "g u", label: "Subscriptions" },
          { key: "g i", label: "Inbox" },
          ...globalHints,
        ],
      },
    ];
  }

  if (path.startsWith("/reply-queue")) {
    return [
      {
        title: "Reply queue shortcuts",
        hints: [
          { key: "g l", label: "Reply queue" },
          { key: "g i", label: "Inbox" },
          ...globalHints,
        ],
      },
    ];
  }

  if (path.startsWith("/m/") && activePane === "reader") {
    return [
      {
        title: "Reader shortcuts",
        hints: [
          { key: "j/k", label: "Scroll" },
          { key: "h", label: "Mail list" },
          { key: "r", label: "Reply" },
          { key: "a", label: "Reply all" },
          { key: "f", label: "Forward" },
          { key: "s", label: "Star" },
          { key: "m", label: "Read/unread" },
          { key: "l", label: "Labels" },
          { key: "L", label: "Context" },
          { key: "A", label: "Attachments" },
          { key: "y", label: "Summary" },
          { key: "p", label: "Sender" },
          { key: "e", label: "Archive" },
          { key: "!", label: "Spam" },
          { key: "Del", label: "Trash" },
        ],
      },
      { title: "Global", hints: globalHints },
    ];
  }

  if (path.startsWith("/m/") && activePane === "sidebar") {
    return [
      {
        title: "Sidebar shortcuts",
        hints: [
          { key: "j/k", label: "Move" },
          { key: "l/o", label: "Open lens" },
          { key: "1-0", label: "Sidebar nav" },
          { key: "g i", label: "Inbox" },
          ...globalHints,
        ],
      },
    ];
  }

  if (path.startsWith("/m/")) {
    return [
      {
        title: "Mailbox shortcuts",
        hints: [
          { key: "j/k", label: "Move" },
          { key: "o", label: "Open" },
          { key: "x", label: "Select" },
          { key: "h", label: "Sidebar" },
          { key: "e", label: "Archive" },
          { key: "s", label: "Star" },
          { key: "m", label: "Read/unread" },
          ...globalHints,
        ],
      },
    ];
  }

  return [
    {
      title: "Global shortcuts",
      hints: [{ key: "1-0", label: "Sidebar nav" }, { key: "g i", label: "Inbox" }, ...globalHints],
    },
  ];
}

export function primaryShortcutHints(context: ShortcutContext, limit = 5): ShortcutHint[] {
  return shortcutSections(context)[0]?.hints.slice(0, limit) ?? globalHints;
}

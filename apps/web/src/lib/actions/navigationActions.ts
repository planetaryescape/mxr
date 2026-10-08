/*
 * Global navigation and shell actions. Chords follow the TUI's live key
 * handler (crates/tui/src/input.rs): `g` + letter for views, digits for the
 * TUI's tabs. Web-only destinations use letters the TUI leaves free, and
 * every deliberate difference carries a `tuiNote` that help displays.
 */

import {
  Activity,
  Archive,
  BarChart3,
  Bell,
  CalendarDays,
  Clock,
  FileText,
  FolderArchive,
  Hourglass,
  Inbox,
  LampDesk,
  ListTodo,
  Mail,
  MailX,
  MessagesSquare,
  Newspaper,
  Package,
  Reply,
  Search,
  Send,
  Settings as SettingsIcon,
  Shield,
  ShieldAlert,
  Star,
  Stethoscope,
  Sun,
  Tag,
  Timer,
  Trash2,
  Undo2,
  Users,
  Workflow,
} from "lucide-react";
import { toast } from "sonner";

import { newMessageIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { holdTypeAhead } from "@/lib/keys/typeAhead";
import { useModals } from "@/state/modalStore";
import { runLatestUndo } from "@/state/undoStore";

import { getRuntimeNavigate } from "./runtime";
import type { Action } from "./types";

function go(to: string): () => void {
  return () => getRuntimeNavigate().navigate(to);
}

function undoLast(): void {
  useModals.getState().setCommandPaletteOpen(false);
  if (!runLatestUndo()) toast.info("Nothing to undo");
}

export const navigationActions: Action[] = [
  {
    id: "shell.command-palette",
    label: "Command palette",
    description: "Run any action by name",
    group: "Navigate",
    shortcut: "Mod+k",
    aliases: [":", "Ctrl+p"],
    tuiNote: "Ctrl+P works on macOS; elsewhere it stays the browser's Print",
    hideInPalette: true,
    run: () => {
      holdTypeAhead();
      useModals.getState().setCommandPaletteOpen(true);
    },
  },
  {
    id: "shell.search-palette",
    label: "Search mail",
    description: "Search the whole local archive",
    group: "Search",
    icon: Search,
    shortcut: "/",
    run: () => {
      holdTypeAhead();
      useModals.getState().setSearchPaletteOpen(true);
    },
  },
  {
    id: "shell.help",
    label: "Keyboard help",
    description: "Every shortcut for the current view",
    group: "Navigate",
    shortcut: "?",
    run: () => {
      const modals = useModals.getState();
      modals.setHelpOpen(!modals.helpOpen);
    },
  },
  {
    id: "shell.compose",
    label: "Compose",
    description: "Start a new message",
    group: "Compose",
    icon: Mail,
    shortcut: "c",
    run: () => useComposeUi.getState().openCompose(newMessageIntent(), "overlay"),
  },
  {
    id: "mail.undo",
    label: "Undo last action",
    description: "Reverse the last archive, trash, move, label change or send (about 60 seconds)",
    group: "Mail",
    icon: Undo2,
    shortcut: "u",
    aliases: ["z"],
    tuiNote: "z also undoes, for Gmail muscle memory",
    run: undoLast,
  },

  // Now and the modes, in rail order. g + letter, as in the TUI.
  {
    id: "nav.now",
    label: "Go to Now",
    description: "The few things that need you now, from every mode",
    group: "Navigate",
    icon: Sun,
    shortcut: "g h",
    run: go("/now"),
  },
  {
    id: "nav.messages",
    label: "Go to Messages",
    description: "People you talk with: whose turn it is, and who you're waiting on",
    group: "Navigate",
    icon: MessagesSquare,
    shortcut: "g m",
    run: go("/messages"),
  },
  {
    id: "nav.updates",
    label: "Go to Updates",
    description: "Notifications gathered twice a day: read the digest, then let go",
    group: "Navigate",
    icon: Bell,
    shortcut: "g u",
    // g p opened Paper trail, which is Updates now (blueprint 22).
    aliases: ["g p"],
    retiredAliases: ["g P"],
    run: go("/updates"),
  },
  {
    id: "nav.archive-mode",
    label: "Go to Archive",
    description: "Receipts, orders, bookings and documents, filed as records",
    group: "Navigate",
    icon: FolderArchive,
    shortcut: "g e",
    run: go("/archive"),
  },
  {
    id: "nav.desk",
    label: "Go to the desk",
    description: "The desk as it was before Now: every lane of what needs you",
    group: "Navigate",
    icon: LampDesk,
    paletteOnly: true,
    run: go("/desk"),
  },
  {
    id: "nav.waiting",
    label: "Waiting on",
    description: "Threads where you wrote last and are waiting on a reply",
    group: "Triage",
    icon: Timer,
    shortcut: "g w",
    tuiNote: "A lane of the desk lens in the TUI",
    run: go("/messages?turn=theirs"),
  },
  {
    id: "nav.inbox",
    label: "Go to Inbox",
    group: "Navigate",
    icon: Inbox,
    shortcut: "g i",
    aliases: ["g 0"],
    run: go("/m/inbox"),
  },
  {
    id: "nav.starred",
    label: "Go to Starred",
    group: "Navigate",
    icon: Star,
    shortcut: "g s",
    run: go("/m/starred"),
  },
  {
    id: "nav.sent",
    label: "Go to Sent",
    group: "Navigate",
    icon: Send,
    shortcut: "g t",
    run: go("/m/sent"),
  },
  {
    id: "nav.drafts",
    label: "Go to Drafts",
    group: "Navigate",
    icon: FileText,
    shortcut: "g d",
    aliases: ["g E"],
    tuiNote: "One page here; in the TUI g d is the Drafts mailbox and g E its local drafts",
    run: go("/drafts"),
  },
  {
    id: "nav.archive",
    label: "Go to All Mail",
    group: "Navigate",
    icon: Archive,
    shortcut: "g a",
    run: go("/m/archive"),
  },
  {
    id: "nav.labels",
    label: "Go to label",
    description: "Jump to any label or saved search",
    group: "Navigate",
    icon: Tag,
    shortcut: "g l",
    run: () => {
      holdTypeAhead();
      useModals.getState().openCommandPaletteAt("lens");
    },
  },
  {
    id: "nav.analytics",
    label: "Analytics",
    group: "Analytics",
    icon: BarChart3,
    shortcut: "g A",
    run: go("/analytics"),
  },
  {
    id: "nav.activity",
    label: "Activity log",
    group: "Diagnostics",
    icon: Activity,
    shortcut: "g y",
    run: go("/activity"),
  },
  {
    id: "nav.logs",
    label: "Daemon logs",
    group: "Diagnostics",
    icon: Stethoscope,
    shortcut: "g L",
    run: go("/diagnostics?panel=logs"),
  },
  {
    id: "nav.settings",
    label: "Settings",
    group: "Settings",
    icon: SettingsIcon,
    shortcut: "g c",
    aliases: ["0"],
    tuiNote: "The TUI opens config.toml in $EDITOR; the web opens Settings",
    run: go("/settings/theme"),
  },
  // Web-only views, on letters the TUI does not use.
  {
    id: "nav.snoozed",
    label: "Go to Snoozed",
    group: "Navigate",
    icon: Clock,
    shortcut: "g n",
    run: go("/m/snoozed"),
    tuiNote: "Web only",
  },
  {
    id: "nav.trash",
    label: "Go to Trash",
    group: "Navigate",
    icon: Trash2,
    shortcut: "g #",
    run: go("/m/trash"),
    tuiNote: "Web only",
  },
  {
    id: "nav.spam",
    label: "Go to Spam",
    group: "Navigate",
    icon: ShieldAlert,
    shortcut: "g !",
    run: go("/m/spam"),
    tuiNote: "Web only",
  },
  // Triage views. The same chords open the TUI's lenses and modals.
  {
    id: "nav.reply-queue",
    label: "Reply queue",
    group: "Triage",
    icon: Reply,
    shortcut: "g q",
    run: go("/reply-queue"),
  },
  {
    id: "nav.owed",
    label: "Owed replies",
    group: "Triage",
    icon: Hourglass,
    shortcut: "g o",
    run: go("/owed"),
  },
  {
    id: "nav.invites",
    label: "Calendar invites",
    group: "Triage",
    icon: CalendarDays,
    shortcut: "g v",
    aliases: ["9"],
    run: go("/invites"),
  },
  {
    id: "nav.reading",
    label: "Reading",
    description: "Newsletters you chose, as an edition; nothing here is owed",
    group: "Triage",
    icon: Newspaper,
    shortcut: "g r",
    // The web used g R and g P up to 0.6.40, while g r opened Rules.
    // Drop these retired aliases after 0.6.41.
    retiredAliases: ["g R"],
    run: go("/reading"),
  },
  {
    // g u opens Updates now; Subscriptions lives under More and the palette.
    id: "nav.subscriptions",
    label: "Subscriptions",
    group: "Triage",
    icon: MailX,
    paletteOnly: true,
    run: go("/subscriptions"),
  },
  {
    id: "nav.screener",
    label: "Screener",
    group: "Triage",
    icon: Shield,
    shortcut: "g S",
    aliases: ["8"],
    run: go("/screener"),
  },
  {
    id: "nav.jobs",
    label: "Background jobs",
    group: "Diagnostics",
    icon: ListTodo,
    paletteOnly: true,
    run: go("/jobs"),
  },

  // Tabs. 1 to 7 match the TUI's tab bar.
  {
    id: "nav.tab-mail",
    label: "Mail",
    group: "Navigate",
    icon: Inbox,
    shortcut: "1",
    hideInPalette: true,
    run: go("/m/inbox"),
  },
  {
    id: "nav.search-page",
    label: "Search page",
    group: "Search",
    icon: Search,
    shortcut: "2",
    run: go("/search"),
  },
  {
    id: "nav.rules",
    label: "Rules",
    group: "Rules",
    icon: Workflow,
    shortcut: "3",
    run: go("/rules"),
  },
  {
    id: "nav.accounts",
    label: "Accounts",
    group: "Accounts",
    icon: Users,
    shortcut: "4",
    run: go("/accounts"),
  },
  {
    id: "nav.diagnostics",
    label: "Diagnostics",
    group: "Diagnostics",
    icon: Stethoscope,
    shortcut: "5",
    run: go("/diagnostics"),
  },
  {
    id: "nav.tab-analytics",
    label: "Analytics (tab)",
    group: "Analytics",
    shortcut: "6",
    hideInPalette: true,
    run: go("/analytics"),
  },
  {
    id: "nav.deliveries",
    label: "Deliveries",
    group: "Triage",
    icon: Package,
    shortcut: "7",
    run: go("/deliveries"),
  },
];

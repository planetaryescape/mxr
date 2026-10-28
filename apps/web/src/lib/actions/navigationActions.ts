/*
 * Global navigation and shell actions. Their chords, and why each is what
 * it is, live in crates/keymap/keymap.toml.
 */

import {
  Activity,
  Archive,
  BarChart3,
  CalendarDays,
  Clock,
  FileText,
  Hourglass,
  Inbox,
  LampDesk,
  ListTodo,
  Mail,
  MailX,
  Package,
  Reply,
  Search,
  Send,
  Settings as SettingsIcon,
  Shield,
  ShieldAlert,
  Star,
  Stethoscope,
  Tag,
  Timer,
  Trash2,
  Undo2,
  Users,
  Workflow,
} from "lucide-react";
import { toast } from "sonner";

import { MODE_GLYPHS } from "@/components/ModeMark";
import { newMessageIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { holdTypeAhead } from "@/lib/keys/typeAhead";
import { useModals } from "@/state/modalStore";
import { runLatestUndo } from "@/state/undoStore";

import { getRuntimeNavigate } from "./runtime";
import type { ActionBehavior } from "./types";

function go(to: string): () => void {
  return () => getRuntimeNavigate().navigate(to);
}

function undoLast(): void {
  useModals.getState().setCommandPaletteOpen(false);
  if (!runLatestUndo()) toast.info("Nothing to undo");
}

export const navigationActions: ActionBehavior[] = [
  {
    id: "shell.command-palette",
    run: () => {
      holdTypeAhead();
      useModals.getState().setCommandPaletteOpen(true);
    },
  },
  {
    id: "shell.search-palette",
    icon: Search,
    run: () => {
      holdTypeAhead();
      useModals.getState().setSearchPaletteOpen(true);
    },
  },
  {
    id: "shell.help",
    run: () => {
      const modals = useModals.getState();
      modals.setHelpOpen(!modals.helpOpen);
    },
  },
  {
    id: "shell.compose",
    icon: Mail,
    run: () => useComposeUi.getState().openCompose(newMessageIntent(), "overlay"),
  },
  { id: "mail.undo", icon: Undo2, run: undoLast },
  { id: "nav.now", icon: MODE_GLYPHS.now, run: go("/now") },
  { id: "nav.messages", icon: MODE_GLYPHS.messages, run: go("/messages") },
  { id: "nav.updates", icon: MODE_GLYPHS.updates, run: go("/updates") },
  { id: "nav.archive-mode", icon: MODE_GLYPHS.archive, run: go("/archive") },
  { id: "nav.desk", icon: LampDesk, run: go("/desk") },
  { id: "nav.waiting", icon: Timer, run: go("/messages?turn=theirs") },
  { id: "nav.inbox", icon: Inbox, run: go("/m/inbox") },
  { id: "nav.starred", icon: Star, run: go("/m/starred") },
  { id: "nav.sent", icon: Send, run: go("/m/sent") },
  { id: "nav.drafts", icon: FileText, run: go("/drafts") },
  { id: "nav.archive", icon: Archive, run: go("/m/archive") },
  {
    id: "nav.labels",
    icon: Tag,
    run: () => {
      holdTypeAhead();
      useModals.getState().openCommandPaletteAt("lens");
    },
  },
  { id: "nav.analytics", icon: BarChart3, run: go("/analytics") },
  { id: "nav.activity", icon: Activity, run: go("/activity") },
  { id: "nav.logs", icon: Stethoscope, run: go("/diagnostics?panel=logs") },
  { id: "nav.settings", icon: SettingsIcon, run: go("/settings/theme") },
  { id: "nav.snoozed", icon: Clock, run: go("/m/snoozed") },
  { id: "nav.trash", icon: Trash2, run: go("/m/trash") },
  { id: "nav.spam", icon: ShieldAlert, run: go("/m/spam") },
  { id: "nav.reply-queue", icon: Reply, run: go("/reply-queue") },
  { id: "nav.owed", icon: Hourglass, run: go("/owed") },
  { id: "nav.invites", icon: CalendarDays, run: go("/invites") },
  { id: "nav.reading", icon: MODE_GLYPHS.reading, run: go("/reading") },
  { id: "nav.subscriptions", icon: MailX, run: go("/subscriptions") },
  { id: "nav.screener", icon: Shield, run: go("/screener") },
  { id: "nav.jobs", icon: ListTodo, run: go("/jobs") },
  { id: "nav.search-page", icon: Search, run: go("/search") },
  { id: "nav.rules", icon: Workflow, run: go("/rules") },
  { id: "nav.accounts", icon: Users, run: go("/accounts") },
  { id: "nav.diagnostics", icon: Stethoscope, run: go("/diagnostics") },
  { id: "nav.deliveries", icon: Package, run: go("/deliveries") },
];

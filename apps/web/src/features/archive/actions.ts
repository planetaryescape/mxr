/*
 * Archive's keys, as in the TUI's records lens and blueprint 22's key
 * table. The mounted Archive registers what each command does
 * (`useScopeController`); the reader keeps its own keys while it has
 * focus. `g e` opens Archive from anywhere; `X` then `e` on a
 * conversation files it, and Pass to a mode in the palette previews the
 * card first.
 */

import { BadgeCheck, Copy, Download, FileText, Filter, Mail, PenLine, X } from "lucide-react";

import type { ActionBehavior } from "@/lib/actions/types";

export const archiveActions: ActionBehavior[] = [
  { id: "archive.down", command: "down" },
  { id: "archive.up", command: "up" },
  { id: "archive.ask", command: "ask" },
  { id: "archive.copy-reference", command: "copyReference", icon: Copy },
  { id: "archive.copy-amount", command: "copyAmount", icon: Copy },
  { id: "archive.open-document", command: "openDocument", icon: FileText },
  { id: "archive.open-email", command: "openEmail", icon: Mail },
  { id: "archive.issuer", command: "issuer" },
  { id: "archive.prev-year", command: "prevYear" },
  { id: "archive.next-year", command: "nextYear" },
  { id: "archive.edit", command: "edit", icon: PenLine },
  { id: "archive.check", command: "check", icon: BadgeCheck },
  { id: "archive.dismiss", command: "dismiss", icon: X },
  { id: "archive.export", command: "export", icon: Download },
  { id: "archive.make-todo", command: "makeTodo" },
  { id: "archive.filter", command: "filter", icon: Filter },
  { id: "archive.close", command: "close" },
];

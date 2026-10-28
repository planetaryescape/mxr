/*
 * Reading's keys, per blueprint 35's verb table: Enter read, L the linked
 * article, b later, e let go, D unsubscribe, R the sender's layout, E let
 * go of everything shown, O open in Marginalia, and the tabs 1 Edition,
 * 2 Later, 3 Highlights. The mounted edition and reader register what each
 * command does (`useScopeController`); `g r` opens Reading from anywhere.
 */

import {
  Bookmark,
  BookOpenText,
  Check,
  ExternalLink,
  Highlighter,
  Layout,
  MailX,
} from "lucide-react";

import type { ActionBehavior } from "@/lib/actions/types";

export const readingActions: ActionBehavior[] = [
  { id: "reading.down", command: "down" },
  { id: "reading.up", command: "up" },
  { id: "reading.read", command: "read" },
  { id: "reading.article", command: "article", icon: ExternalLink },
  { id: "reading.later", command: "later", icon: Bookmark },
  { id: "reading.let-go", command: "letGo", icon: Check },
  { id: "reading.unsubscribe", command: "unsubscribe", icon: MailX },
  { id: "reading.original", command: "original", icon: Layout },
  { id: "reading.let-go-all", command: "letGoAll" },
  { id: "reading.open-email", command: "openEmail" },
  { id: "reading.move-sender", command: "moveSender" },
  { id: "reading.edition", command: "edition" },
  { id: "reading.later-shelf", command: "laterShelf" },
  { id: "reading.highlights", command: "highlights", icon: Highlighter },
  { id: "reading.marginalia", command: "marginalia", icon: BookOpenText },
  { id: "reading.close-hint", command: "closeHint" },
  // The reader.
  { id: "reading.reader-down", command: "down" },
  { id: "reading.reader-up", command: "up" },
  { id: "reading.reader-article", command: "article", icon: ExternalLink },
  { id: "reading.reader-issue", command: "issue" },
  { id: "reading.reader-later", command: "later", icon: Bookmark },
  { id: "reading.reader-let-go", command: "letGo", icon: Check },
  { id: "reading.reader-next", command: "next" },
  { id: "reading.reader-original", command: "original", icon: Layout },
  { id: "reading.reader-highlight", command: "highlight", icon: Highlighter },
  { id: "reading.reader-unsubscribe", command: "unsubscribe", icon: MailX },
  { id: "reading.reader-open-email", command: "openEmail" },
  { id: "reading.reader-marginalia", command: "marginalia", icon: BookOpenText },
  { id: "reading.reader-back", command: "back" },
];

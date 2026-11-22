import { Maximize2, Shuffle, SquareStack } from "lucide-react";

import { useMailboxPane } from "@/state/mailboxPaneStore";

import type { ActionBehavior } from "./types";

export const sidebarActions: ActionBehavior[] = [
  { id: "sidebar.down", command: "down" },
  { id: "sidebar.up", command: "up" },
  { id: "sidebar.top", command: "top" },
  { id: "sidebar.bottom", command: "bottom" },
  { id: "sidebar.open", command: "open" },
  { id: "sidebar.collapse", command: "collapse" },
  { id: "sidebar.expand", command: "expand" },
  { id: "sidebar.prev-section", command: "prevSection" },
  { id: "sidebar.next-section", command: "nextSection" },
];

export const listActions: ActionBehavior[] = [
  { id: "page.controls", command: "controls" },
  { id: "list.down", command: "down" },
  { id: "list.up", command: "up" },
  { id: "list.top", command: "top" },
  { id: "list.bottom", command: "bottom" },
  { id: "list.page-down", command: "pageDown" },
  { id: "list.page-up", command: "pageUp" },
  { id: "list.viewport-top", command: "viewportTop" },
  { id: "list.viewport-middle", command: "viewportMiddle" },
  { id: "list.open", command: "open" },
  { id: "list.sidebar", run: () => useMailboxPane.getState().setActivePane("sidebar") },
  { id: "list.row-action", command: "rowAction" },
  { id: "list.toggle-select", command: "toggleSelect" },
  { id: "list.visual", command: "visual" },
  { id: "list.select-all", command: "selectAll" },
  { id: "list.select-all-chord", command: "selectAll" },
  { id: "list.select-none", command: "selectNone" },
  { id: "list.select-read", command: "selectRead" },
  { id: "list.select-unread", command: "selectUnread" },
  { id: "list.select-starred", command: "selectStarred" },
  { id: "list.escape", command: "escape" },
  { id: "list.filter", command: "filter" },
  { id: "list.toggle-threads", command: "toggleThreads", icon: SquareStack },
];

export const readerActions: ActionBehavior[] = [
  { id: "reader.scroll-down", command: "scrollDown" },
  { id: "reader.scroll-up", command: "scrollUp" },
  { id: "reader.page-down", command: "pageDown" },
  { id: "reader.page-up", command: "pageUp" },
  { id: "reader.top", command: "top" },
  { id: "reader.bottom", command: "bottom" },
  { id: "reader.next-message", command: "nextMessage" },
  { id: "reader.prev-message", command: "prevMessage" },
  { id: "reader.toggle-message", command: "toggleMessage" },
  { id: "reader.expand-all", command: "expandAll" },
  { id: "reader.next-thread", command: "nextThread" },
  { id: "reader.prev-thread", command: "prevThread" },
  { id: "reader.archive-next", command: "archiveNext" },
  { id: "reader.archive-prev", command: "archivePrev" },
  { id: "reader.list", command: "focusList" },
  { id: "reader.close", command: "close" },
  { id: "reader.view-reader", command: "viewReader" },
  { id: "reader.view-html", command: "viewHtml" },
  { id: "reader.view-plain", command: "viewPlain" },
  { id: "reader.remote", command: "toggleRemote" },
  { id: "reader.signature", command: "toggleSignature" },
  { id: "reader.quotes", command: "toggleQuotes" },
  { id: "reader.headers", command: "headers" },
  { id: "reader.open-original", command: "openOriginal" },
  { id: "reader.summary", command: "summarize" },
  { id: "reader.fullscreen", command: "fullscreen", icon: Maximize2 },
  { id: "reader.move-sender", command: "moveSender", icon: Shuffle },
];

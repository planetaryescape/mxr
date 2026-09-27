import { useQuery } from "@tanstack/react-query";
import { Link, useRouterState } from "@tanstack/react-router";
import { ChevronRight, PenSquare, Search } from "lucide-react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { newMessageIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { fetchAdminStatus } from "@/features/diagnostics/api";
import { lensesFromShell, resolveLens } from "@/features/mailbox/lenses";
import { useShellQuery } from "@/features/mailbox/useMailboxQuery";
import { fetchThread } from "@/features/mailbox/api";
import { formatChord } from "@/lib/keys/chord";
import { useModals } from "@/state/modalStore";

interface Crumb {
  label: string;
  to?: string;
}

const PAGE_TITLES: Record<string, string> = {
  desk: "Desk",
  search: "Search",
  drafts: "Drafts",
  "reply-queue": "Reply queue",
  owed: "Owed replies",
  focus: "Focus & reply",
  snoozed: "Snoozed",
  screener: "Screener",
  subscriptions: "Subscriptions",
  analytics: "Analytics",
  rules: "Rules",
  accounts: "Accounts",
  onboarding: "Welcome",
  settings: "Settings",
  diagnostics: "Diagnostics",
  activity: "Activity",
  jobs: "Jobs",
  deliveries: "Deliveries",
  invites: "Calendar invites",
  sender: "Sender",
  compose: "Compose",
};

function threadIdInPath(parts: string[]): string | undefined {
  if (parts[0] === "m") return parts[1] === "label" || parts[1] === "saved" ? parts[3] : parts[2];
  if (parts[0] === "search" || parts[0] === "desk") return parts[1];
  return undefined;
}

function useBreadcrumb(path: string, search: string): Crumb[] {
  const shell = useShellQuery();
  const parts = path
    .split("/")
    .filter(Boolean)
    .map((part) => decodeURIComponent(part));
  const openThreadId = threadIdInPath(parts);
  // Passive observer: the reader fetches; the crumb updates when it lands.
  const thread = useQuery({
    queryKey: ["thread", openThreadId ?? ""],
    queryFn: () => fetchThread(openThreadId ?? ""),
    enabled: false,
  });
  const subject = thread.data?.thread.subject || "Conversation";
  if (parts[0] === "m") {
    const lenses = lensesFromShell(shell.data);
    const route =
      parts[1] === "label"
        ? { kind: "label" as const, name: parts[2] ?? "" }
        : parts[1] === "saved"
          ? { kind: "saved" as const, slug: parts[2] ?? "" }
          : { kind: "system" as const, mailbox: parts[1] ?? "inbox" };
    const lens = resolveLens(route, lenses);
    const crumbs: Crumb[] = [{ label: lens?.label ?? "Mail", to: lens?.path }];
    if (openThreadId) crumbs.push({ label: subject });
    return crumbs;
  }
  const title = PAGE_TITLES[parts[0] ?? ""] ?? (parts[0] ? parts[0] : "mxr");
  if (parts[0] === "search") {
    const q = new URLSearchParams(search).get("q");
    const crumbs: Crumb[] = [
      { label: "Search", to: q ? `/search?q=${encodeURIComponent(q)}` : "/search" },
    ];
    if (q) crumbs.push({ label: `“${q}”`, to: `/search?q=${encodeURIComponent(q)}` });
    if (openThreadId) crumbs.push({ label: subject });
    return crumbs;
  }
  if (parts[0] === "desk") {
    const crumbs: Crumb[] = [{ label: title, to: "/desk" }];
    if (openThreadId) crumbs.push({ label: subject });
    return crumbs;
  }
  if (parts.length > 1 && parts[0] !== "sender") {
    return [{ label: title, to: `/${parts[0]}` }, { label: humanize(parts[1] ?? "") }];
  }
  if (parts[0] === "sender" && parts[1]) return [{ label: title }, { label: parts[1] }];
  return [{ label: title }];
}

// Path segments that are acronyms, not words ("llm" → "LLM", not "Llm").
const ACRONYMS = new Set(["llm", "mcp", "api", "imap", "smtp", "ai"]);

function humanize(value: string): string {
  const words = value
    .split(/[-_]/)
    .map((word) => (ACRONYMS.has(word.toLowerCase()) ? word.toUpperCase() : word));
  const text = words.join(" ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

export function Topbar() {
  const location = useRouterState({ select: (s) => s.location });
  const setSearchOpen = useModals((state) => state.setSearchPaletteOpen);
  const crumbs = useBreadcrumb(location.pathname, location.searchStr ?? "");
  const { data: status } = useQuery({
    queryKey: ["admin-status-is-demo"],
    queryFn: fetchAdminStatus,
    staleTime: 60_000,
    refetchOnWindowFocus: false,
  });
  const isDemo = Boolean((status as { is_demo?: boolean } | undefined)?.is_demo);

  return (
    <div className="flex w-full min-w-0 items-center gap-3">
      <nav aria-label="Breadcrumb" className="flex min-w-0 flex-1 items-center gap-1 text-[13px]">
        {crumbs.map((crumb, index) => {
          const last = index === crumbs.length - 1;
          return (
            // oxlint-disable-next-line react/no-array-index-key -- crumbs are positional
            <span key={`${crumb.label}-${index}`} className="flex min-w-0 items-center gap-1">
              {index > 0 ? <ChevronRight className="size-3.5 shrink-0 text-faint" /> : null}
              {crumb.to && !last ? (
                <Link
                  to={crumb.to}
                  className="shrink-0 truncate text-muted-foreground hover:text-foreground"
                >
                  {crumb.label}
                </Link>
              ) : (
                <span
                  className={
                    last
                      ? "truncate font-semibold text-foreground"
                      : "truncate text-muted-foreground"
                  }
                  aria-current={last ? "page" : undefined}
                >
                  {crumb.label}
                </span>
              )}
            </span>
          );
        })}
        {isDemo ? (
          <span
            className="ml-2 shrink-0 rounded-sm border border-warning/50 px-1.5 font-mono text-2xs uppercase tracking-wider text-warning"
            title="Demo profile: no real mail is touched"
          >
            demo
          </span>
        ) : null}
      </nav>

      <Button
        type="button"
        variant="outline"
        className="h-8 w-[min(360px,34vw)] shrink justify-start gap-2 px-2.5 text-left text-[13px] font-normal text-muted-foreground"
        onClick={() => setSearchOpen(true)}
        aria-label="Search mail"
      >
        <Search className="size-3.5" />
        <span className="min-w-0 flex-1 truncate">Search mail</span>
        <KeyChip>/</KeyChip>
      </Button>

      <Button
        size="sm"
        onClick={() => useComposeUi.getState().openCompose(newMessageIntent(), "overlay")}
        aria-label="Compose new email"
        title={`Compose (${formatChord("c")})`}
      >
        <PenSquare className="size-3.5" />
        <span className="hidden md:inline">Compose</span>
      </Button>
    </div>
  );
}

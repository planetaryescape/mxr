import { useMutation, useQuery } from "@tanstack/react-query";
import { useNavigate, useRouterState } from "@tanstack/react-router";
import {
  Archive,
  BarChart3,
  Command as CommandIcon,
  FileText,
  Inbox,
  Mail,
  Send,
  Palette,
  Plus,
  Search,
  Settings,
  Shield,
  UserCog,
} from "lucide-react";
import { useMemo } from "react";
import { toast } from "sonner";

import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
} from "@/components/ui/command";
import { fetchAccounts } from "@/features/accounts/api";
import {
  backfillSemantic,
  fetchSemanticStatus,
  installSemanticProfile,
  reindexSemantic,
  semanticProfiles,
  semanticSnapshot,
  setSemanticEnabled,
  useSemanticProfile,
  type SemanticProfile,
} from "@/features/diagnostics/api";
import { fetchShell, listCommitments } from "@/features/mailbox/api";
import type { SidebarItem } from "@/features/mailbox/types";
import { useModals } from "@/state/modalStore";

interface ActionItem {
  id: string;
  label: string;
  description: string;
  keybinding?: string;
  run: () => void;
}

const commandActionIds = new Set([
  "compose",
  "draft-to",
  "show-commitments",
  "semantic-backfill",
  "rules-new",
  "accounts-new",
]);

export function CommandPaletteMount() {
  const navigate = useNavigate();
  const path = useRouterState({ select: (state) => state.location.pathname });
  const open = useModals((state) => state.commandPaletteOpen);
  const setOpen = useModals((state) => state.setCommandPaletteOpen);
  const setSearchOpen = useModals((state) => state.setSearchPaletteOpen);
  const setComposeOpen = useModals((state) => state.setComposeLauncherOpen);
  const openRail = useModals((state) => state.openRightRail);
  const shell = useQuery({
    queryKey: ["shell"],
    queryFn: fetchShell,
    staleTime: 60_000,
    enabled: open,
  });
  const accounts = useQuery({
    queryKey: ["accounts"],
    queryFn: fetchAccounts,
    staleTime: 60_000,
    enabled: open,
  });
  const semantic = useQuery({
    queryKey: ["diagnostics", "semantic"],
    queryFn: fetchSemanticStatus,
    staleTime: 30_000,
    enabled: open,
  });
  const semanticStatus = semanticSnapshot(semantic.data);
  const semanticBackfill = useMutation({
    mutationFn: backfillSemantic,
    onSuccess: () => toast.success("Semantic backfill queued"),
    onError: (error) => toast.error("Semantic backfill failed", { description: error.message }),
  });
  const semanticEnable = useMutation({
    mutationFn: setSemanticEnabled,
    onSuccess: (_, enabled) =>
      toast.success(enabled ? "Semantic search enabled" : "Semantic search disabled"),
    onError: (error) => toast.error("Semantic update failed", { description: error.message }),
  });
  const semanticReindex = useMutation({
    mutationFn: reindexSemantic,
    onSuccess: () => toast.success("Semantic reindex queued"),
    onError: (error) => toast.error("Semantic reindex failed", { description: error.message }),
  });
  const semanticInstall = useMutation({
    mutationFn: installSemanticProfile,
    onSuccess: (_, profile) => toast.success(`${profile} install queued`),
    onError: (error) =>
      toast.error("Semantic profile install failed", { description: error.message }),
  });
  const semanticUse = useMutation({
    mutationFn: useSemanticProfile,
    onSuccess: (_, profile) => toast.success(`${profile} selected`),
    onError: (error) =>
      toast.error("Semantic profile switch failed", { description: error.message }),
  });
  const commitments = useMutation({
    mutationFn: (accountId: string) => listCommitments({ accountId, status: "open" }),
    onSuccess: (result) => openRail("commitments", result),
    onError: (error) => toast.error("Commitments unavailable", { description: error.message }),
  });
  const defaultAccount =
    accounts.data?.accounts.find((account) => account.enabled && account.is_default) ??
    accounts.data?.accounts.find((account) => account.enabled) ??
    accounts.data?.accounts[0];

  const actions = useMemo<ActionItem[]>(() => {
    const go = (to: string) => () => {
      setOpen(false);
      void navigate({ to });
    };
    const items: ActionItem[] = [
      {
        id: "compose",
        label: "Compose",
        description: "Write a new message",
        keybinding: "c",
        run: () => {
          setOpen(false);
          setComposeOpen(true);
        },
      },
      {
        id: "draft-to",
        label: "Draft to...",
        description: "Pick a recipient, then use Draft for me",
        run: () => {
          setOpen(false);
          setComposeOpen(true);
        },
      },
      {
        id: "show-commitments",
        label: "Show commitments...",
        description: "Open unresolved relationship commitments",
        run: () => {
          setOpen(false);
          if (!defaultAccount?.account_id) {
            toast.error("No account available");
            return;
          }
          commitments.mutate(defaultAccount.account_id);
        },
      },
      {
        id: "semantic-backfill",
        label: "Backfill semantic now",
        description: "Queue local semantic chunk and embedding repair",
        run: () => {
          setOpen(false);
          semanticBackfill.mutate();
        },
      },
      {
        id: semanticStatus?.enabled ? "semantic-disable" : "semantic-enable",
        label: semanticStatus?.enabled ? "Disable semantic search" : "Enable semantic search",
        description: "Toggle hybrid and semantic retrieval locally",
        run: () => {
          setOpen(false);
          semanticEnable.mutate(!(semanticStatus?.enabled ?? false));
        },
      },
      {
        id: "semantic-reindex",
        label: "Reindex semantic now",
        description: "Rebuild embeddings for the active semantic profile",
        run: () => {
          setOpen(false);
          semanticReindex.mutate();
        },
      },
      {
        id: "inbox",
        label: "Go to inbox",
        description: "Open the inbox lens",
        keybinding: "g i",
        run: go("/m/inbox"),
      },
      {
        id: "starred",
        label: "Go to starred",
        description: "Open starred mail",
        keybinding: "g s",
        run: go("/m/starred"),
      },
      {
        id: "sent",
        label: "Go to sent",
        description: "Open sent mail",
        run: go("/m/sent"),
      },
      {
        id: "drafts",
        label: "Go to drafts",
        description: "Open saved drafts",
        keybinding: "g d",
        run: go("/m/drafts"),
      },
      {
        id: "search",
        label: "Search",
        description: "Search local mail",
        keybinding: "/",
        run: () => {
          setOpen(false);
          setSearchOpen(true);
        },
      },
      {
        id: "analytics",
        label: "Analytics",
        description: "Open dashboards",
        keybinding: "g a",
        run: go("/analytics"),
      },
      {
        id: "rules",
        label: "Rules",
        description: "Manage deterministic mail rules",
        keybinding: "g r",
        run: go("/rules"),
      },
      {
        id: "rules-new",
        label: "New rule",
        description: "Open the deterministic rule builder",
        run: go("/rules/new"),
      },
      { id: "accounts", label: "Accounts", description: "Manage providers", run: go("/accounts") },
      {
        id: "accounts-new",
        label: "Add account",
        description: "Open account onboarding",
        run: go("/accounts/new"),
      },
      {
        id: "screener",
        label: "Screener",
        description: "Triage unknown senders",
        run: go("/screener"),
      },
      {
        id: "settings",
        label: "Settings",
        description: "Theme, density, notifications",
        run: go("/settings/theme"),
      },
      {
        id: "diagnostics",
        label: "Diagnostics",
        description: "Bridge and daemon health",
        run: go("/diagnostics"),
      },
    ];
    const threadId = path.match(/^\/m\/[^/]+\/([^/]+)/)?.[1];
    if (threadId) {
      items.unshift({
        id: "thread-inbox",
        label: "Back to inbox",
        description: "Return to the inbox list",
        keybinding: "g i",
        run: go("/m/inbox"),
      });
    }
    for (const profile of semanticProfiles) {
      const installed =
        semanticStatus?.profiles.some((record) => record.profile === profile) ?? false;
      items.push(
        semanticProfileAction(
          profile,
          installed,
          semanticInstall.mutate,
          semanticUse.mutate,
          setOpen,
        ),
      );
    }
    return items;
  }, [
    commitments,
    defaultAccount?.account_id,
    navigate,
    path,
    semanticBackfill,
    semanticEnable,
    semanticInstall,
    semanticReindex,
    semanticStatus?.enabled,
    semanticStatus?.profiles,
    semanticUse,
    setComposeOpen,
    setOpen,
    setSearchOpen,
  ]);

  const sidebarItems = shell.data?.sidebar?.sections?.flatMap((section) => section.items) ?? [];
  const commandActions = actions.filter(isCommandAction);
  const navigationActions = actions.filter((action) => !isCommandAction(action));
  const settings = settingsActions((to) => () => {
    setOpen(false);
    void navigate({ to });
  });

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <CommandInput placeholder="Type a command or destination..." />
      <CommandList>
        <CommandEmpty>No command found.</CommandEmpty>
        <CommandGroup heading="Commands">
          {commandActions.map((action) => (
            <CommandItem
              key={action.id}
              value={`${action.label} ${action.description}`}
              onSelect={action.run}
            >
              {iconForAction(action.id)}
              <div>
                <div>{action.label}</div>
                <div className="text-2xs text-muted-foreground">{action.description}</div>
              </div>
              {action.keybinding ? <CommandShortcut>{action.keybinding}</CommandShortcut> : null}
            </CommandItem>
          ))}
        </CommandGroup>
        <CommandSeparator />
        <CommandGroup heading="Navigate">
          {navigationActions.map((action) => (
            <CommandItem
              key={action.id}
              value={`${action.label} ${action.description}`}
              onSelect={action.run}
            >
              {iconForAction(action.id)}
              <div>
                <div>{action.label}</div>
                <div className="text-2xs text-muted-foreground">{action.description}</div>
              </div>
              {action.keybinding ? <CommandShortcut>{action.keybinding}</CommandShortcut> : null}
            </CommandItem>
          ))}
        </CommandGroup>
        <CommandSeparator />
        <CommandGroup heading="Lenses">
          {sidebarItems.map((item) => (
            <CommandItem
              key={item.id}
              value={item.label}
              onSelect={() => runSidebarItem(item, navigate, setOpen)}
            >
              <Inbox className="size-3.5" />
              <span>{item.label}</span>
              {item.unread ? <CommandShortcut>{item.unread}</CommandShortcut> : null}
            </CommandItem>
          ))}
        </CommandGroup>
        <CommandSeparator />
        <CommandGroup heading="Settings">
          {settings.map((action) => (
            <CommandItem
              key={action.id}
              value={`${action.label} ${action.description}`}
              onSelect={action.run}
            >
              <Settings className="size-3.5" />
              <div>
                <div>{action.label}</div>
                <div className="text-2xs text-muted-foreground">{action.description}</div>
              </div>
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}

function isCommandAction(action: ActionItem): boolean {
  return commandActionIds.has(action.id) || action.id.startsWith("semantic-");
}

function semanticProfileAction(
  profile: SemanticProfile,
  installed: boolean,
  install: (profile: SemanticProfile) => void,
  use: (profile: SemanticProfile) => void,
  setOpen: (open: boolean) => void,
): ActionItem {
  return {
    id: installed ? `semantic-profile-use-${profile}` : `semantic-profile-install-${profile}`,
    label: installed ? `Use semantic profile: ${profile}` : `Install semantic profile: ${profile}`,
    description: installed
      ? "Switch the active local embedding profile"
      : "Install a local embedding profile",
    run: () => {
      setOpen(false);
      if (installed) {
        use(profile);
      } else {
        install(profile);
      }
    },
  };
}

function iconForAction(id: string) {
  if (id.includes("new")) return <Plus className="size-3.5" />;
  if (id.includes("compose")) return <Mail className="size-3.5" />;
  if (id.includes("draft")) return <Mail className="size-3.5" />;
  if (id.includes("commitments")) return <FileText className="size-3.5" />;
  if (id.includes("semantic")) return <Shield className="size-3.5" />;
  if (id.includes("search")) return <Search className="size-3.5" />;
  if (id.includes("sent")) return <Send className="size-3.5" />;
  if (id.includes("analytics")) return <BarChart3 className="size-3.5" />;
  if (id.includes("rules")) return <FileText className="size-3.5" />;
  if (id.includes("accounts")) return <UserCog className="size-3.5" />;
  if (id.includes("settings")) return <Palette className="size-3.5" />;
  if (id.includes("diagnostics")) return <Shield className="size-3.5" />;
  if (id.includes("archive")) return <Archive className="size-3.5" />;
  return <CommandIcon className="size-3.5" />;
}

function settingsActions(go: (to: string) => () => void): ActionItem[] {
  const actions: Array<[string, string, string, string]> = [
    ["settings-theme", "Theme settings", "Switch color theme", "/settings/theme"],
    ["settings-density", "Density settings", "Change row density", "/settings/density"],
    ["settings-keybindings", "Keybinding help", "View keyboard shortcuts", "/settings/keybindings"],
    [
      "settings-notifications",
      "Notification settings",
      "Configure browser alerts and VIPs",
      "/settings/notifications",
    ],
    ["settings-compose", "Compose settings", "Choose editor preference", "/settings/compose"],
    ["settings-voice", "Voice settings", "Inspect local voice profile", "/settings/voice"],
    ["settings-llm", "LLM settings", "Configure summaries and draft assist", "/settings/llm"],
    ["settings-snippets", "Snippets", "Manage compose snippets", "/settings/snippets"],
    ["settings-token", "Bridge token", "Paste or inspect the bridge token", "/settings/token"],
  ];
  return actions.map(([id, label, description, to]) => ({ id, label, description, run: go(to) }));
}

function runSidebarItem(
  item: SidebarItem,
  navigate: ReturnType<typeof useNavigate>,
  setOpen: (open: boolean) => void,
) {
  setOpen(false);
  if (item.lens?.kind === "saved_search") {
    void navigate({
      to: "/m/saved/$slug",
      params: { slug: item.id.replace(/^saved-search-/, "") },
    });
    return;
  }
  if (item.lens?.kind === "label") {
    void navigate({ to: "/m/label/$name", params: { name: item.id } });
    return;
  }
  void navigate({
    to: "/m/$mailbox",
    params: { mailbox: item.id.replace(/^mailbox-/, "") || "inbox" },
  });
}

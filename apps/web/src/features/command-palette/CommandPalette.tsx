import { useQuery } from "@tanstack/react-query";
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
import { fetchShell } from "@/features/mailbox/api";
import type { SidebarItem } from "@/features/mailbox/types";
import { useModals } from "@/state/modalStore";

interface ActionItem {
  id: string;
  label: string;
  description: string;
  keybinding?: string;
  run: () => void;
}

export function CommandPaletteMount() {
  const navigate = useNavigate();
  const path = useRouterState({ select: (state) => state.location.pathname });
  const open = useModals((state) => state.commandPaletteOpen);
  const setOpen = useModals((state) => state.setCommandPaletteOpen);
  const setSearchOpen = useModals((state) => state.setSearchPaletteOpen);
  const setComposeOpen = useModals((state) => state.setComposeLauncherOpen);
  const shell = useQuery({
    queryKey: ["shell"],
    queryFn: fetchShell,
    staleTime: 60_000,
    enabled: open,
  });

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
    return items;
  }, [navigate, path, setComposeOpen, setOpen, setSearchOpen]);

  const sidebarItems = shell.data?.sidebar?.sections?.flatMap((section) => section.items) ?? [];
  const commandActions = actions.filter((action) =>
    ["compose", "rules-new", "accounts-new"].includes(action.id),
  );
  const navigationActions = actions.filter(
    (action) => !["compose", "rules-new", "accounts-new"].includes(action.id),
  );
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

function iconForAction(id: string) {
  if (id.includes("new")) return <Plus className="size-3.5" />;
  if (id.includes("compose")) return <Mail className="size-3.5" />;
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

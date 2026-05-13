import { useMutation, useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Inbox } from "lucide-react";
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
  fetchSemanticStatus,
  installSemanticProfile,
  semanticProfiles,
  semanticSnapshot,
  useSemanticProfile,
  type SemanticProfile,
} from "@/features/diagnostics/api";
import { fetchShell } from "@/features/mailbox/api";
import type { SidebarItem } from "@/features/mailbox/types";
import {
  formatChord,
  useActionContext,
  useActionsByGroup,
  type Action,
  type ActionGroup,
} from "@/lib/actions";
import { useModals } from "@/state/modalStore";

const GROUP_ORDER: ActionGroup[] = [
  "Compose",
  "Navigate",
  "Search",
  "Triage",
  "Analytics",
  "Rules",
  "Accounts",
  "Semantic",
  "Diagnostics",
  "Settings",
  "View",
  "Mail",
];

export function CommandPaletteMount() {
  const navigate = useNavigate();
  const open = useModals((state) => state.commandPaletteOpen);
  const setOpen = useModals((state) => state.setCommandPaletteOpen);

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

  const ctx = useActionContext({ accountCount: accounts.data?.accounts.length ?? 0 });
  const grouped = useActionsByGroup(ctx);

  const semanticInstall = useMutation({
    mutationFn: installSemanticProfile,
    onSuccess: (_, profile) => toast.success(`${profile} install queued`),
    onError: (error) =>
      toast.error("Semantic profile install failed", { description: error.message }),
  });
  const semanticUseMutation = useMutation({
    mutationFn: useSemanticProfile,
    onSuccess: (_, profile) => toast.success(`${profile} selected`),
    onError: (error) =>
      toast.error("Semantic profile switch failed", { description: error.message }),
  });

  const semanticProfileItems = useMemo(
    () => buildSemanticProfileActions(semanticStatus, semanticInstall.mutate, semanticUseMutation.mutate, setOpen),
    [semanticStatus, semanticInstall.mutate, semanticUseMutation.mutate, setOpen],
  );

  const sidebarItems = shell.data?.sidebar?.sections?.flatMap((section) => section.items) ?? [];

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <CommandInput placeholder="Type a command or destination..." />
      <CommandList>
        <CommandEmpty>No command found.</CommandEmpty>
        {GROUP_ORDER.flatMap((group) => {
          const items = grouped.get(group);
          if (!items || items.length === 0) return [];
          return [
            <CommandGroup key={group} heading={group}>
              {items.map((action) => (
                <CommandItem
                  key={action.id}
                  value={`${action.label} ${action.description ?? ""}`}
                  onSelect={() => {
                    setOpen(false);
                    void action.run(ctx);
                  }}
                >
                  {renderIcon(action)}
                  <div>
                    <div>{action.label}</div>
                    {action.description ? (
                      <div className="text-2xs text-muted-foreground">{action.description}</div>
                    ) : null}
                  </div>
                  {action.shortcut ? (
                    <CommandShortcut>{formatChord(action.shortcut)}</CommandShortcut>
                  ) : null}
                </CommandItem>
              ))}
            </CommandGroup>,
            <CommandSeparator key={`${group}-sep`} />,
          ];
        })}
        {semanticProfileItems.length > 0 ? (
          <>
            <CommandGroup heading="Semantic profiles">
              {semanticProfileItems.map((action) => (
                <CommandItem
                  key={action.id}
                  value={`${action.label} ${action.description ?? ""}`}
                  onSelect={() => action.run({} as never)}
                >
                  <div>
                    <div>{action.label}</div>
                    <div className="text-2xs text-muted-foreground">{action.description}</div>
                  </div>
                </CommandItem>
              ))}
            </CommandGroup>
            <CommandSeparator />
          </>
        ) : null}
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
      </CommandList>
    </CommandDialog>
  );
}

function renderIcon(action: Action) {
  const Icon = action.icon;
  return Icon ? <Icon className="size-3.5" /> : null;
}

function buildSemanticProfileActions(
  status: ReturnType<typeof semanticSnapshot>,
  install: (profile: SemanticProfile) => void,
  use: (profile: SemanticProfile) => void,
  setOpen: (open: boolean) => void,
): Action[] {
  const installed = new Set<SemanticProfile>();
  if (status && typeof status === "object" && "profiles" in status) {
    const profiles = (status as { profiles?: Array<{ profile: SemanticProfile }> }).profiles ?? [];
    for (const record of profiles) installed.add(record.profile);
  }
  return semanticProfiles.map<Action>((profile) => {
    const isInstalled = installed.has(profile);
    return {
      id: isInstalled
        ? `semantic.profile.use.${profile}`
        : `semantic.profile.install.${profile}`,
      label: isInstalled
        ? `Use semantic profile: ${profile}`
        : `Install semantic profile: ${profile}`,
      description: isInstalled
        ? "Switch the active local embedding profile"
        : "Install a local embedding profile",
      group: "Semantic",
      paletteOnly: true,
      run: () => {
        setOpen(false);
        if (isInstalled) {
          use(profile);
        } else {
          install(profile);
        }
      },
    };
  });
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

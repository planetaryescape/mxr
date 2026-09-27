import { useMutation, useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Bookmark, Inbox, Tag } from "lucide-react";
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
import { lensesFromShell, type MailLens } from "@/features/mailbox/lenses";
import {
  formatChord,
  invokeAction,
  useActionContext,
  useActionsByGroup,
  type Action,
  type ActionGroup,
} from "@/lib/actions";
import { useModals } from "@/state/modalStore";

const GROUP_ORDER: ActionGroup[] = [
  "Mail",
  "Compose",
  "Read",
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
  "Select",
  "Move",
];

export function CommandPaletteMount() {
  const navigate = useNavigate();
  const open = useModals((state) => state.commandPaletteOpen);
  const mode = useModals((state) => state.commandPaletteMode);
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
    () =>
      buildSemanticProfileActions(
        semanticStatus,
        semanticInstall.mutate,
        semanticUseMutation.mutate,
        setOpen,
      ),
    [semanticStatus, semanticInstall.mutate, semanticUseMutation.mutate, setOpen],
  );

  const lenses = useMemo(() => lensesFromShell(shell.data), [shell.data]);
  const lensGroup = (
    <CommandGroup heading="Mailboxes, labels and saved searches">
      {lenses.map((lens) => (
        <CommandItem
          key={lens.key}
          value={`${lens.label} ${lens.section}`}
          onSelect={() => openLens(lens, navigate, setOpen)}
        >
          <LensIcon lens={lens} />
          <span>{lens.label}</span>
          {lens.unread > 0 ? (
            <CommandShortcut>{lens.unread.toLocaleString()}</CommandShortcut>
          ) : null}
        </CommandItem>
      ))}
    </CommandGroup>
  );

  if (mode === "lens") {
    return (
      <CommandDialog open={open} onOpenChange={setOpen}>
        <CommandInput placeholder="Jump to a mailbox, label or saved search…" />
        <CommandList>
          <CommandEmpty>No mailbox or label by that name.</CommandEmpty>
          {lensGroup}
        </CommandList>
      </CommandDialog>
    );
  }

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <CommandInput placeholder="Run a command or jump somewhere…" />
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
                    invokeAction(action, ctx);
                  }}
                >
                  {renderIcon(action)}
                  <div>
                    <div>{action.label}</div>
                    {action.description ? (
                      <div className="text-2xs text-muted-foreground">{action.description}</div>
                    ) : null}
                  </div>
                  {action.shortcut && !action.paletteOnly ? (
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
                  onSelect={() => invokeAction(action, ctx)}
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
        {lensGroup}
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
      id: isInstalled ? `semantic.profile.use.${profile}` : `semantic.profile.install.${profile}`,
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

function openLens(
  lens: MailLens,
  navigate: ReturnType<typeof useNavigate>,
  setOpen: (open: boolean) => void,
) {
  setOpen(false);
  void navigate({ to: lens.path });
}

function LensIcon({ lens }: { lens: MailLens }) {
  if (lens.section === "labels") return <Tag className="size-3.5" />;
  if (lens.section === "saved") return <Bookmark className="size-3.5" />;
  return <Inbox className="size-3.5" />;
}

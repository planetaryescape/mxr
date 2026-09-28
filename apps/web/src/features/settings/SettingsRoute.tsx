import { Link, useParams } from "@tanstack/react-router";
import {
  Bell,
  BookOpen,
  Bot,
  Code2,
  Info,
  KeyRound,
  Keyboard,
  Mic,
  Palette,
  Pencil,
  SearchX,
  Volume2,
} from "lucide-react";
import type { ComponentType, ReactNode } from "react";

import { AppearanceSection } from "./AppearanceSection";
import { ComposeSettingsSection } from "./ComposeSettingsSection";
import { KeybindingsSection } from "./KeybindingsSection";
import { LlmSettingsSection } from "./LlmSettingsSection";
import { NotificationsSection } from "./NotificationsSection";
import { SoundSection } from "./SoundSection";
import { ReaderSection } from "./ReaderSection";
import { SnippetsSection } from "./SnippetsSection";
import { TokenSection } from "./TokenSection";
import { VoiceSection } from "./VoiceSection";
import { FactList, PageEmpty } from "@/components/PageParts";
import { Page } from "@/components/Page";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface SectionDef {
  id: string;
  label: string;
  description: string;
  icon: ComponentType<{ className?: string }>;
  render: () => ReactNode;
  /** Old ids that still land here (palette actions, bookmarks). */
  aliases?: string[];
}

const SECTIONS: SectionDef[] = [
  {
    id: "theme",
    label: "Appearance",
    description: "Theme, density and motion.",
    icon: Palette,
    render: () => <AppearanceSection />,
    aliases: ["density", "appearance"],
  },
  {
    id: "reader",
    label: "Reader",
    description: "How mail lists and conversations open and render.",
    icon: BookOpen,
    render: () => <ReaderSection />,
  },
  {
    id: "keybindings",
    label: "Keyboard",
    description: "Every key, grouped by where it works.",
    icon: Keyboard,
    render: () => <KeybindingsSection />,
  },
  {
    id: "notifications",
    label: "Notifications",
    description: "Browser alerts for new mail.",
    icon: Bell,
    render: () => <NotificationsSection />,
  },
  {
    id: "sound",
    label: "Sound",
    description:
      "Soft tones for send, archive, snooze and a cleared desk. Off unless you turn it on.",
    icon: Volume2,
    render: () => <SoundSection />,
  },
  {
    id: "compose",
    label: "Compose",
    description: "Editor, undo send, default account and signatures.",
    icon: Pencil,
    render: () => <ComposeSettingsSection />,
  },
  {
    id: "snippets",
    label: "Snippets",
    description: "Reusable text, expanded in compose with ;name.",
    icon: Code2,
    render: () => <SnippetsSection />,
  },
  {
    id: "llm",
    label: "Language model",
    description: "The model behind summaries, briefings, draft assist and Ask your archive.",
    icon: Bot,
    render: () => <LlmSettingsSection />,
  },
  {
    id: "voice",
    label: "Voice",
    description: "How you write, learned from sent mail so drafts sound like you.",
    icon: Mic,
    render: () => <VoiceSection />,
  },
  {
    id: "token",
    label: "Bridge token",
    description: "How this page authenticates to the local daemon.",
    icon: KeyRound,
    render: () => <TokenSection />,
  },
  {
    id: "about",
    label: "About",
    description: "Versions and where this app gets its data.",
    icon: Info,
    render: () => (
      <FactList
        columns={1}
        facts={[
          ["App", "mxr web"],
          ["Version", import.meta.env.PACKAGE_VERSION ?? "dev"],
          ["Data", "local daemon over HTTP and WebSocket"],
        ]}
      />
    ),
  },
];

function findSection(id: string): SectionDef | undefined {
  return SECTIONS.find((section) => section.id === id || section.aliases?.includes(id));
}

export function SettingsRoute() {
  const { section: requested } = useParams({ from: "/settings/$section" });
  const section = findSection(requested);

  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <nav
        aria-label="Settings sections"
        className="hidden w-52 shrink-0 overflow-y-auto border-r border-border px-2 py-5 md:block"
      >
        <div className="mb-2 px-2 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
          Settings
        </div>
        <ul className="space-y-0.5">
          {SECTIONS.map(({ id, label, icon: Icon }) => {
            const active = section?.id === id;
            return (
              <li key={id}>
                <Link
                  to="/settings/$section"
                  params={{ section: id }}
                  aria-current={active ? "page" : undefined}
                  className={cn(
                    "flex items-center gap-2 rounded-md px-2 py-1.5 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    active
                      ? "bg-primary-muted text-foreground"
                      : "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
                  )}
                >
                  <Icon className="size-3.5" />
                  {label}
                </Link>
              </li>
            );
          })}
        </ul>
      </nav>
      {section ? (
        <Page
          eyebrow="Settings"
          title={section.label}
          description={section.description}
          width="narrow"
          tabs={<NarrowSectionPicker current={section.id} />}
        >
          {section.render()}
        </Page>
      ) : (
        <Page eyebrow="Settings" title="Not found" width="narrow">
          <PageEmpty
            icon={<SearchX className="size-5" />}
            title={`There is no "${requested}" settings page`}
            body="It may have moved. Pick a section from the list."
            action={
              <Button size="sm" variant="outline" asChild>
                <Link to="/settings/$section" params={{ section: "theme" }}>
                  Open appearance settings
                </Link>
              </Button>
            }
          />
        </Page>
      )}
    </div>
  );
}

/** Below md the side nav is hidden; a compact section list takes its place. */
function NarrowSectionPicker({ current }: { current: string }) {
  return (
    <ul aria-label="Settings sections" className="-mx-1 flex gap-1 overflow-x-auto pb-2 md:hidden">
      {SECTIONS.map(({ id, label }) => (
        <li key={id}>
          <Link
            to="/settings/$section"
            params={{ section: id }}
            aria-current={id === current ? "page" : undefined}
            className={cn(
              "block whitespace-nowrap rounded-md px-2 py-1 text-xs",
              id === current ? "bg-primary-muted text-foreground" : "text-muted-foreground",
            )}
          >
            {label}
          </Link>
        </li>
      ))}
    </ul>
  );
}

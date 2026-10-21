import { useQuery } from "@tanstack/react-query";
import { Link, Navigate, useParams } from "@tanstack/react-router";
import {
  Bell,
  BookOpen,
  BookOpenText,
  Bot,
  ChevronLeft,
  ChevronRight,
  Code2,
  Info,
  KeyRound,
  Keyboard,
  Mic,
  Palette,
  Pencil,
  Volume2,
  Users,
} from "lucide-react";
import { useEffect, useRef, type ComponentType, type ReactNode } from "react";

import { AppearanceSection } from "./AppearanceSection";
import { ComposeSettingsSection } from "./ComposeSettingsSection";
import { KeybindingsSection } from "./KeybindingsSection";
import { LlmSettingsSection } from "./LlmSettingsSection";
import { MarginaliaSection } from "./MarginaliaSection";
import { NotificationsSection } from "./NotificationsSection";
import { SoundSection } from "./SoundSection";
import { ReaderSection } from "./ReaderSection";
import { SnippetsSection } from "./SnippetsSection";
import { TokenSection } from "./TokenSection";
import { VoiceSection } from "./VoiceSection";
import { FactList, PageEmpty } from "@/components/PageParts";
import { Page, pageTabClass } from "@/components/Page";
import { Row } from "@/components/Row";
import { Button } from "@/components/ui/button";
import { fetchAdminStatus } from "@/features/diagnostics/api";
import { useMediaQuery } from "@/hooks/useMediaQuery";
import { useScrollFade } from "@/hooks/useScrollFade";
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
    id: "marginalia",
    label: "Marginalia",
    description: "The reading app Reading items open in with M.",
    icon: BookOpenText,
    render: () => <MarginaliaSection />,
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
    id: "accounts",
    label: "Accounts",
    description: "Mailboxes connected to mxr.",
    icon: Users,
    render: () => (
      <Button size="sm" variant="outline" asChild>
        <Link to="/accounts">Manage accounts</Link>
      </Button>
    ),
  },
  {
    id: "about",
    label: "About",
    description: "Versions and where this app gets its data.",
    icon: Info,
    render: () => <AboutSection />,
  },
];

function findSection(id: string): SectionDef | undefined {
  return SECTIONS.find((section) => section.id === id || section.aliases?.includes(id));
}

/** The phone's index: the list that drills into a section. Wide screens never show it. */
export const SETTINGS_INDEX = "all";

export function SettingsRoute() {
  const { section: requested } = useParams({ from: "/settings/$section" });
  // md is where the sections become the page's tabs; below it they are a list
  // that drills in. The app's rail is the only left navigation either way.
  const narrow = useMediaQuery("(max-width: 767px)");
  const section = findSection(requested);

  if (requested === SETTINGS_INDEX) {
    return narrow ? (
      <SectionIndex />
    ) : (
      <Navigate to="/settings/$section" params={{ section: SECTIONS[0]!.id }} replace />
    );
  }

  return section ? (
    <Page
      eyebrow={narrow ? <BackToIndex /> : "Settings"}
      title={section.label}
      description={section.description}
      width="narrow"
      tabs={narrow ? undefined : <SectionTabs current={section.id} />}
    >
      {section.render()}
    </Page>
  ) : (
    <Page eyebrow="Settings" title="Not found" width="narrow">
      <PageEmpty
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
  );
}

/**
 * Wide screens: the sections are the page's tabs, the selected one with the
 * underline. Thirteen tabs outrun the column at laptop widths, so the strip
 * scrolls, keeps the current tab in view and fades the edge it runs off.
 */
function SectionTabs({ current }: { current: string }) {
  const strip = useRef<HTMLUListElement>(null);
  const tabs = useScrollFade<HTMLElement>();
  useEffect(() => {
    strip.current
      ?.querySelector('[aria-current="page"]')
      ?.scrollIntoView({ inline: "center", block: "nearest" });
  }, [current]);
  return (
    <nav
      ref={tabs.ref}
      aria-label="Settings sections"
      className={cn(
        "overflow-x-auto [scrollbar-width:none]",
        tabs.fade && "[mask-image:linear-gradient(to_right,black_calc(100%-2.5rem),transparent)]",
      )}
    >
      <ul ref={strip} className="-mb-px flex h-8 gap-6">
        {SECTIONS.map(({ id, label }) => (
          <li key={id} className="shrink-0">
            <Link
              to="/settings/$section"
              params={{ section: id }}
              aria-current={id === current ? "page" : undefined}
              className={pageTabClass(id === current)}
            >
              {label}
            </Link>
          </li>
        ))}
      </ul>
    </nav>
  );
}

function BackToIndex() {
  return (
    <Link
      to="/settings/$section"
      params={{ section: SETTINGS_INDEX }}
      className="inline-flex items-center gap-1 hover:text-text-1"
    >
      <ChevronLeft aria-hidden className="size-3" />
      Settings
    </Link>
  );
}

/** Phones: one list of sections; each drills in and the eyebrow comes back. */
function SectionIndex() {
  return (
    <Page title="Settings" description="Pick a section." width="narrow" className="px-0">
      <ul aria-label="Settings sections">
        {SECTIONS.map(({ id, label, description, icon: Icon }) => (
          <Row
            key={id}
            lit={false}
            lead={<Icon aria-hidden className="size-4 shrink-0 text-text-3" />}
            primary={label}
            secondary={description}
            trail={<ChevronRight aria-hidden className="size-4" />}
            renderMain={(content, className) => (
              <Link to="/settings/$section" params={{ section: id }} className={className}>
                {content}
              </Link>
            )}
          />
        ))}
      </ul>
    </Page>
  );
}

function AboutSection() {
  const status = useQuery({
    queryKey: ["admin-status"],
    queryFn: fetchAdminStatus,
    staleTime: 60_000,
  });
  const version = status.isPending
    ? "Checking…"
    : status.isError
      ? "Unavailable"
      : typeof status.data.daemon_version === "string" && status.data.daemon_version.trim()
        ? status.data.daemon_version
        : "Unavailable";
  return (
    <>
      <FactList
        columns={1}
        facts={[
          ["App", "mxr web"],
          ["Connected daemon version", version],
          ["Mail storage", "On the computer running mxr"],
        ]}
      />
      <Button className="mt-4" size="sm" variant="outline" asChild>
        <Link to="/diagnostics">Open diagnostics</Link>
      </Button>
    </>
  );
}

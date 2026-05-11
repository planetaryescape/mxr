import { useConnectionStore } from "@/state/connectionStore";
import { useRouterState } from "@tanstack/react-router";

import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { KeyChip } from "@/components/KeyChip";
import { primaryShortcutHints, shortcutSections } from "@/lib/shortcutHints";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useModals } from "@/state/modalStore";

export function StatusBar() {
  const sync = useConnectionStore((s) => s.syncProgress);
  const reindex = useConnectionStore((s) => s.semanticReindexProgress);
  const state = useConnectionStore((s) => s.state);
  const path = useRouterState({ select: (routerState) => routerState.location.pathname });
  const activePane = useMailboxPane((s) => s.activePane);
  const helpOpen = useModals((s) => s.helpOpen);
  const setHelpOpen = useModals((s) => s.setHelpOpen);
  const context = { path, activePane };
  const primaryHints = primaryShortcutHints(context);
  const sections = shortcutSections(context);

  return (
    <>
      <ShortcutHelpPanel open={helpOpen} sections={sections} onOpenChange={setHelpOpen} />
      <span>mxr</span>
      <span>·</span>
      <span>
        bridge: <span className="text-foreground">{state}</span>
      </span>
      {sync ? (
        <>
          <span>·</span>
          <span>
            sync {sync.current}/{sync.total}
          </span>
        </>
      ) : null}
      {reindex ? (
        <>
          <span>·</span>
          <span>semantic {Math.round((reindex.current / Math.max(1, reindex.total)) * 100)}%</span>
        </>
      ) : null}
      <span className="ml-auto flex items-center gap-2">
        {primaryHints.map((hint) => (
          <span
            key={`${hint.key}-${hint.label}`}
            className="hidden items-center gap-1 md:inline-flex"
          >
            <KeyChip>{hint.key}</KeyChip>
            <span>{hint.label}</span>
          </span>
        ))}
      </span>
    </>
  );
}

function ShortcutHelpPanel({
  open,
  sections,
  onOpenChange,
}: {
  open: boolean;
  sections: ReturnType<typeof shortcutSections>;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[min(520px,calc(100vw-2rem))]">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
        </DialogHeader>
        <div className="grid gap-3 sm:grid-cols-2">
          {sections.map((section) => (
            <section key={section.title}>
              <h3 className="mb-1 text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
                {section.title}
              </h3>
              <div className="grid gap-1">
                {section.hints.map((hint) => (
                  <div
                    key={`${section.title}-${hint.key}-${hint.label}`}
                    className="flex items-center justify-between gap-3 text-xs"
                  >
                    <span className="text-muted-foreground">{hint.label}</span>
                    <KeyChip>{hint.key}</KeyChip>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}

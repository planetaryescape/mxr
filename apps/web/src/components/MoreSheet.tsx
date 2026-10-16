import * as DialogPrimitive from "@radix-ui/react-dialog";
import { Link, useRouter, useRouterState } from "@tanstack/react-router";
import { X } from "lucide-react";
import { useEffect } from "react";

import { isEntryActive, type NavEntry } from "@/components/navMap";
import { NavMark, useNavSections } from "@/components/Sidebar";
import { Button } from "@/components/ui/button";
import { overlayMotion } from "@/components/ui/motion";
import { cn } from "@/lib/utils";
import { useModals } from "@/state/modalStore";

/** The modes that have a phone tab of their own; the sheet lists the rest. */
const HAS_TAB = new Set(["now", "messages", "todo", "reading"]);

/**
 * The More sheet: every destination the phone's four tabs leave out
 * (Updates, Archive, Inbox, then Mail, Views, Tools and Labels), on the
 * overlay ink over a scrim (D133). The narrow rail opens the same sheet, so
 * no width has an unreachable page. A Radix dialog, so focus is trapped,
 * Esc closes it and the page's keys stay under it while it is open.
 */
export function MoreSheet() {
  const open = useModals((s) => s.moreSheetOpen);
  const setOpen = useModals((s) => s.setMoreSheetOpen);
  const sections = useNavSections();
  const path = useRouterState({ select: (s) => s.location.pathname });
  const lane = useRouterState({
    select: (s) => ("turn" in s.location.search ? s.location.search.turn : undefined),
  });

  // Any navigation, from a link here or from outside (back, a key), closes the
  // sheet. Subscribing at the start of a navigation, not watching the path,
  // means a sheet opened while a route is still settling isn't closed by it.
  const router = useRouter();
  useEffect(() => router.subscribe("onBeforeNavigate", () => setOpen(false)), [router, setOpen]);

  return (
    <DialogPrimitive.Root open={open} onOpenChange={setOpen}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          className={cn(
            "fixed inset-0 z-50 bg-scrim data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
            overlayMotion,
          )}
        />
        <DialogPrimitive.Content
          data-testid="more-sheet"
          aria-describedby={undefined}
          onCloseAutoFocus={(event) => {
            // Back to whatever opened it: the More tab or the rail's button.
            const trigger = [...document.querySelectorAll<HTMLElement>("[data-more-trigger]")].find(
              (node) => node.offsetParent !== null,
            );
            if (trigger) {
              event.preventDefault();
              trigger.focus();
            }
          }}
          className={cn(
            "fixed z-50 flex flex-col bg-ink-overlay text-text-1 outline-none",
            "inset-x-0 bottom-0 max-h-[85dvh] border-t border-rule",
            "sm:inset-y-0 sm:left-0 sm:right-auto sm:max-h-none sm:w-80 sm:border-r sm:border-t-0",
            "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
            "data-[state=open]:slide-in-from-bottom-2 data-[state=closed]:slide-out-to-bottom-2",
            overlayMotion,
          )}
        >
          <div className="flex h-12 shrink-0 items-center justify-between px-4">
            <DialogPrimitive.Title className="type-title">More</DialogPrimitive.Title>
            <DialogPrimitive.Close asChild>
              <Button variant="ghost" size="icon-sm" aria-label="Close" className="text-text-2">
                <X className="size-4" />
              </Button>
            </DialogPrimitive.Close>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-2 pb-4 [padding-bottom:max(1rem,env(safe-area-inset-bottom))]">
            {sections.map((section) => {
              const entries = section.entries.filter((entry) => !HAS_TAB.has(entry.key));
              if (entries.length === 0) return null;
              return (
                <nav
                  key={section.id}
                  aria-label={section.title ?? "Modes"}
                  className={section.id === "places" ? undefined : "mt-3"}
                >
                  {section.title ? (
                    <h2 className="type-eyebrow px-3 py-1.5 text-text-3">{section.title}</h2>
                  ) : null}
                  <ul>
                    {entries.map((entry) => (
                      <li key={entry.key}>
                        <SheetLink
                          entry={entry}
                          active={isEntryActive(path, lane, entry)}
                          onNavigate={() => setOpen(false)}
                        />
                      </li>
                    ))}
                  </ul>
                </nav>
              );
            })}
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

function SheetLink({
  entry,
  active,
  onNavigate,
}: {
  entry: NavEntry;
  active: boolean;
  onNavigate: () => void;
}) {
  const count = entry.count && entry.count > 0 ? entry.count : null;
  return (
    <Link
      to={entry.to}
      search={entry.search}
      // The router would call Messages current on Waiting on; the nav decides.
      activeOptions={{ exact: true }}
      onClick={onNavigate}
      aria-current={active ? "page" : undefined}
      className={cn(
        "type-body relative flex h-11 items-center gap-3 px-3",
        active ? "bg-ink-lamp text-text-1" : "text-text-2 active:bg-ink-lamp",
      )}
    >
      {active ? <span aria-hidden className="absolute inset-y-0 left-0 w-0.5 bg-signal" /> : null}
      <NavMark entry={entry} className={active ? "text-text-2" : "text-text-3"} />
      <span className="min-w-0 flex-1 truncate">{entry.label}</span>
      {count ? (
        <span className="type-meta text-text-3">
          {count > 9999 ? "9999+" : count.toLocaleString()}
        </span>
      ) : null}
    </Link>
  );
}

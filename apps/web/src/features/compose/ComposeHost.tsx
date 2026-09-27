/*
 * The one compose surface (inline / overlay / fullscreen). Every entry
 * point opens it: `c`, the compose launcher, reply/forward, drafts, and the
 * /compose/* deep links. Mounted once in AppShell so the compose session
 * survives surface switches and route changes. The inline surface portals the
 * editor into the thread reader's slot (#inline-composer-slot); when no
 * slot exists (user navigated away) it falls back to the overlay so the
 * draft is never hidden.
 */

import { useRouterState } from "@tanstack/react-router";
import { Maximize2, Minimize2, PictureInPicture2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { ComposeEditorPanel } from "./ComposeEditorPanel";
import { htmlDraftRefusal, HtmlDraftNotice } from "./HtmlDraftNotice";
import { useComposeUi, type ComposeSurface } from "./composeUiStore";
import { useComposeSession, type ComposeIntent } from "./useComposeSession";

export function ComposeHost() {
  const intent = useComposeUi((s) => s.intent);
  if (!intent) return null;
  // Key by intent so a new reply target starts a fresh session while the
  // host itself stays mounted.
  return <ComposeHostInner key={intent.key} intent={intent} />;
}

function ComposeHostInner({ intent }: { intent: ComposeIntent }) {
  const surface = useComposeUi((s) => s.surface);
  const setSurface = useComposeUi((s) => s.setSurface);
  const closeCompose = useComposeUi((s) => s.closeCompose);
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const [slot, setSlot] = useState<HTMLElement | null>(null);

  // A send finishes after its undo window, possibly once the user has moved
  // on to another composer; only close the surface if it still shows this
  // session.
  const closeThisSession = () => {
    if (useComposeUi.getState().intent?.key === intent.key) closeCompose();
  };
  const controller = useComposeSession(intent, {
    onSent: closeThisSession,
    onDiscarded: closeThisSession,
    onClose: closeThisSession,
  });

  // The inline slot lives at the bottom of the thread reader; re-resolve
  // it whenever the route changes.
  useEffect(() => {
    if (surface !== "inline") {
      setSlot(null);
      return;
    }
    setSlot(document.getElementById("inline-composer-slot"));
  }, [surface, pathname]);

  // Bring the inline composer into view once it has content: the slot is
  // hidden while empty, so scrolling to it any earlier does nothing.
  const sessionLoading = controller.sessionLoading;
  useEffect(() => {
    if (!slot || sessionLoading) return;
    const frame = requestAnimationFrame(() => slot.scrollIntoView({ block: "nearest" }));
    return () => cancelAnimationFrame(frame);
  }, [slot, sessionLoading]);

  // An HTML-bodied draft is a permanent refusal, not a transient failure:
  // retrying can only fail again. Show the document instead.
  const htmlDraft = htmlDraftRefusal(controller.sessionError);
  const body = controller.sessionLoading ? (
    <div className="flex h-40 items-center justify-center text-xs text-muted-foreground">
      Opening {intent.title.toLowerCase()}…
    </div>
  ) : htmlDraft ? (
    <HtmlDraftNotice refusal={htmlDraft} />
  ) : controller.sessionError ? (
    <div className="flex h-40 flex-col items-center justify-center gap-2 text-xs">
      <span className="text-destructive">{controller.sessionError.message}</span>
      <Button size="sm" variant="outline" onClick={controller.retrySession}>
        Retry
      </Button>
    </div>
  ) : controller.draft ? (
    <ComposeEditorPanel controller={controller} />
  ) : null;

  const chrome = (
    <div
      data-compose-surface
      className={cn(
        "flex min-h-0 flex-col overflow-hidden border border-border-strong bg-background",
        surface === "inline"
          ? "max-h-[70vh] min-h-[320px] rounded-lg"
          : "h-full rounded-xl shadow-2xl",
      )}
      onKeyDown={(event) => {
        if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "f") {
          event.preventDefault();
          setSurface(surface === "fullscreen" ? "inline" : "fullscreen");
        }
      }}
    >
      <div className="flex h-8 shrink-0 items-center justify-between border-b border-border bg-card/40 px-2">
        <span className="px-1 font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          {intent.title}
        </span>
        <span className="flex items-center gap-1">
          <SurfaceButton
            label="Inline"
            active={surface === "inline"}
            onClick={() => setSurface("inline")}
            icon={<Minimize2 className="size-3" />}
          />
          <SurfaceButton
            label="Popout"
            active={surface === "overlay"}
            onClick={() => setSurface("overlay")}
            icon={<PictureInPicture2 className="size-3" />}
          />
          <SurfaceButton
            label="Fullscreen (⇧⌘F)"
            active={surface === "fullscreen"}
            onClick={() => setSurface("fullscreen")}
            icon={<Maximize2 className="size-3" />}
          />
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Close composer (saves draft)"
            title="Close composer (saves draft)"
            onClick={() => void controller.requestClose()}
          >
            <X className="size-3.5" />
          </Button>
        </span>
      </div>
      <div className="flex min-h-0 flex-1 flex-col">{body}</div>
    </div>
  );

  if (surface === "inline" && slot) {
    return createPortal(chrome, slot);
  }

  const fullscreen = surface === "fullscreen";
  return (
    <div
      role="dialog"
      aria-modal="false"
      aria-label={intent.title}
      className={cn(
        "fixed z-40 flex flex-col",
        fullscreen
          ? "inset-4"
          : "bottom-4 right-4 h-[min(640px,calc(100vh-6rem))] w-[min(680px,calc(100vw-3rem))]",
      )}
    >
      {chrome}
    </div>
  );
}

function SurfaceButton({
  label,
  active,
  onClick,
  icon,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
}) {
  return (
    <Button
      variant={active ? "secondary" : "ghost"}
      size="icon-sm"
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      {icon}
    </Button>
  );
}

/** Surface switch helper for ComposeSurface validation at call sites. */
export type { ComposeSurface };

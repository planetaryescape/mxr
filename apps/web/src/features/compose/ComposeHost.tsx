/*
 * Single host for the surface-based composer (inline / overlay /
 * fullscreen). Mounted once in AppShell so the compose session survives
 * surface switches and route changes. The inline surface portals the
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

  const controller = useComposeSession(intent, {
    onSent: closeCompose,
    onDiscarded: closeCompose,
  });

  // The inline slot lives at the bottom of the thread reader; re-resolve
  // it whenever the route changes.
  useEffect(() => {
    if (surface !== "inline") {
      setSlot(null);
      return;
    }
    const element = document.getElementById("inline-composer-slot");
    setSlot(element);
    if (element) {
      requestAnimationFrame(() => element.scrollIntoView({ block: "nearest" }));
    }
  }, [surface, pathname]);

  const body = controller.sessionLoading ? (
    <div className="flex h-40 items-center justify-center text-xs text-muted-foreground">
      Opening {intent.title.toLowerCase()}…
    </div>
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
      className={cn(
        "flex min-h-0 flex-col overflow-hidden border border-border bg-background",
        surface === "inline" ? "max-h-[60vh] rounded-lg" : "h-full rounded-xl shadow-2xl",
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
            aria-label="Close composer (draft is saved)"
            onClick={closeCompose}
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
